// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound in the song (decision 0017): audio tracks, importing files,
//! recording takes from the input, and shaping clips (split, trim, fade,
//! volume, moving between tracks).
//!
//! An audio track can be in focus in the song view (`audio_focus`) while
//! the instrument track whose pattern is open stays `track`: notes and the
//! keyboard keep going to an instrument, and Record records sound into the
//! audio track in focus.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use daw_dsp::audio::Peaks;
use daw_model::{AudioFile, AudioId, ClipContent, ClipId, Ticks, TrackId};

use super::audio_store::{Done, Entry, ImportPlace};
use super::recorder::{Finished, Recorder};
use super::{Session, View};
use crate::text;

/// How long the input stays open after a take, so the next one starts at
/// once.
const INPUT_KEPT: Duration = Duration::from_secs(30);

/// A take of sound: asked for, recording, or being finished.
#[derive(Clone, Debug)]
pub(super) struct AudioTake {
    /// The engine's number for it, once recording started.
    pub number: Option<u32>,
    /// The track it goes on.
    pub track: TrackId,
    /// Where in the song it started.
    pub at: Ticks,
    /// The delay to make up for, in seconds.
    pub latency: f32,
    /// The file it is written to, once recording started.
    pub file: Option<String>,
}

/// A take of sound being recorded, as autosave notes it so a crash does
/// not lose it (decision 0021).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TakeNote {
    /// The track it goes on.
    pub track: TrackId,
    /// Where in the song it started.
    pub at: Ticks,
    /// The file it is written to, in the song's audio folder.
    pub file: String,
    /// Its name ("Recording 3").
    pub name: String,
    /// The delay to make up for, in seconds.
    pub latency: f32,
}

/// What is known about the sound of an audio file, for drawing it.
#[derive(Clone, Debug)]
pub enum SoundState {
    /// Being read.
    Loading,
    /// Ready: its loudness summary.
    Ready(Arc<Peaks>),
    /// It could not be read: why.
    Failed(String),
}

/// The take being recorded, for drawing it as it grows.
#[derive(Clone)]
pub struct LiveTake {
    /// Its track.
    pub track: TrackId,
    /// Where it started.
    pub start: Ticks,
    /// Its sound so far, summed up (at the input's rate).
    pub peaks: Option<Peaks>,
}

impl Session {
    // ----- Focus

    /// The track in focus: an audio track chosen in the song view, else the
    /// instrument track whose pattern is open.
    pub fn focused_track(&self) -> TrackId {
        self.audio_focus.unwrap_or(self.track)
    }

    /// Whether a track plays audio clips.
    pub fn is_audio_track(&self, track: TrackId) -> bool {
        self.project()
            .track(track)
            .is_some_and(|t| t.instrument.is_audio())
    }

    /// Whether the track in focus is an audio track.
    pub fn audio_track_in_focus(&self) -> bool {
        self.audio_focus.is_some()
    }

    pub(super) fn focus_audio_track(&mut self, track: TrackId) {
        self.audio_focus = Some(track);
    }

    /// Whether a track is the song's last instrument track (it stays: notes
    /// and the keyboard need one).
    pub fn is_last_instrument(&self, track: TrackId) -> bool {
        let project = self.project();
        !self.is_audio_track(track)
            && project
                .tracks
                .iter()
                .filter(|t| !t.instrument.is_audio())
                .count()
                <= 1
    }

    // ----- Tracks

    /// Adds an audio track at the bottom and puts it in focus.
    pub fn add_audio_track(&mut self) -> TrackId {
        let name = self.unique_track_name(text::AUDIO_TRACK);
        let track = self
            .doc
            .edit(text::EDIT_ADD_AUDIO_TRACK, |p| p.add_audio_track(&name));
        self.audio_focus = Some(track);
        track
    }

    // ----- The audio folder

    /// Keeps the song's audio files in `folder` from now on (the program's
    /// recovery folder, so a crash keeps them).
    pub fn set_audio_folder(&mut self, folder: PathBuf) {
        self.audio
            .set_folder(folder, self.engine.config().sample_rate);
        self.load_song_audio();
    }

    /// Where the song's audio files are kept while it is open.
    pub fn audio_folder(&self) -> &Path {
        self.audio.folder()
    }

    /// Starts reading the song's audio files the engine does not have yet.
    pub(super) fn load_song_audio(&mut self) {
        let names: Vec<String> = self
            .project()
            .audio
            .iter()
            .filter(|f| !self.engine.has_audio(&f.file))
            .map(|f| f.file.clone())
            .collect();
        self.audio.load_missing(names.iter().map(String::as_str));
    }

    /// Whether audio files are still being imported or read.
    pub fn is_loading_audio(&self) -> bool {
        self.audio.is_busy()
    }

    /// What is known about the sound of an audio file (by stored name).
    pub fn sound_state(&self, file: &str) -> SoundState {
        match self.audio.entry(file) {
            Some(Entry::Ready(peaks)) => SoundState::Ready(Arc::clone(peaks)),
            Some(Entry::Failed(reason)) => SoundState::Failed(reason.clone()),
            Some(Entry::Loading) | None => SoundState::Loading,
        }
    }

    // ----- Importing

    /// Imports sound files: each goes at `at`, on `track` if that is an
    /// audio track, else on a new audio track of its own (so stems stay
    /// lined up).
    pub fn import_audio(&mut self, files: Vec<PathBuf>, track: Option<TrackId>, at: Ticks) {
        let track = track.filter(|&t| self.is_audio_track(t));
        let at = at.max(Ticks::ZERO);
        for (index, file) in files.into_iter().enumerate() {
            let name = file
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            self.set_status(text::importing(&name));
            // The first file may go on the chosen track; the others get
            // tracks of their own.
            let place = ImportPlace {
                track: if index == 0 { track } else { None },
                at,
                pad_of: None,
            };
            self.audio.import(file, place);
        }
    }

    // ----- Recording sound

    /// Starts recording sound into an audio track, after the count-in.
    pub(super) fn record_audio(&mut self, track: TrackId) {
        if self.view != View::Song {
            self.show_song();
        }
        self.audio_focus = Some(track);
        self.audio_take = Some(AudioTake {
            number: None,
            track,
            at: Ticks::ZERO,
            latency: 0.0,
            file: None,
        });
        self.input_until = None;
        self.sync_engine();
        self.engine.record_audio(self.count_in_bars);
        self.record_requested = Some(Instant::now());
    }

    /// Whether a take of sound is asked for, counting in or recording.
    pub fn is_recording_audio(&self) -> bool {
        self.audio_take.is_some()
    }

    /// Whether the program wants the input device open: while recording
    /// sound, and for a while after.
    pub fn wants_input(&self) -> bool {
        self.audio_take.is_some() || self.input_until.is_some_and(|until| Instant::now() < until)
    }

    /// How loud the input is (0 to 1 and over), for its meter: the loudest
    /// sample since the last frame, falling back slowly.
    pub fn input_level(&mut self) -> f32 {
        let peak = self.engine.take_input_peak();
        self.input_meter = (self.input_meter * 0.85).max(peak);
        self.input_meter
    }

    /// The take of sound being recorded, for autosave (none before the
    /// count-in ends).
    pub fn take_in_progress(&self) -> Option<TakeNote> {
        let take = self.audio_take.as_ref()?;
        take.number?;
        Some(TakeNote {
            track: take.track,
            at: take.at,
            file: take.file.clone()?,
            name: self.next_recording_name(),
            latency: take.latency,
        })
    }

    /// The take being recorded, for drawing it.
    pub fn live_take(&self) -> Option<LiveTake> {
        let take = self.audio_take.as_ref()?;
        take.number?;
        Some(LiveTake {
            track: take.track,
            start: take.at,
            peaks: self
                .recorder
                .as_ref()
                .and_then(Recorder::live)
                .filter(|live| Some(live.take) == take.number)
                .map(|live| live.peaks),
        })
    }

    pub(super) fn audio_recording_started(&mut self, number: u32, at: Ticks, latency: f32) {
        let Some(take) = self.audio_take.as_mut() else {
            return;
        };
        take.number = Some(number);
        take.at = at;
        take.latency = latency;
        let name = self.audio.fresh_name("wav");
        take.file = Some(name.clone());
        let path = self.audio.folder().join(name);
        if let Some(recorder) = &self.recorder {
            recorder.begin(number, path);
        }
    }

    pub(super) fn audio_recording_stopped(&mut self, number: u32, frames: u64, dropped: u64) {
        if let Some(recorder) = &self.recorder {
            recorder.end(number, frames);
        }
        if let Some(take) = self.audio_take.take()
            && take.number == Some(number)
        {
            self.finishing.push(take);
        }
        self.input_until = Some(Instant::now() + INPUT_KEPT);
        if dropped > 0 {
            self.set_status(text::RECORDING_SKIPPED);
        }
    }

    /// Recording stopped: a take asked for that never started (stopped
    /// during the count-in) is let go.
    pub(super) fn audio_recording_ended(&mut self) {
        if self.audio_take.as_ref().is_some_and(|t| t.number.is_none()) {
            self.audio_take = None;
            self.input_until = Some(Instant::now() + INPUT_KEPT);
        }
    }

    /// Brings finished imports and takes into the song, and gives the
    /// engine the sound of files that finished loading.
    pub(super) fn poll_audio(&mut self) {
        for done in self.audio.poll() {
            match done {
                Done::Loaded { name, result } => match result {
                    Ok(ready) => self.engine.register_audio(&name, ready.sound),
                    Err(reason) => {
                        let title = self
                            .project()
                            .audio
                            .iter()
                            .find(|f| f.file == name)
                            .map_or(name.clone(), |f| f.name.clone());
                        self.set_status(text::unreadable_sound(&title, &reason));
                    }
                },
                Done::Imported {
                    source,
                    place,
                    result,
                } => {
                    let file_name = source
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    match result {
                        Ok(imported) => {
                            self.engine
                                .register_audio(&imported.name, Arc::clone(&imported.ready.sound));
                            let file = AudioFile {
                                id: AudioId(0),
                                name: imported.title.clone(),
                                file: imported.name.clone(),
                                frames: imported.frames,
                                sample_rate: imported.sample_rate,
                                channels: imported.channels,
                            };
                            if let Some(kit) = place.pad_of {
                                self.add_imported_pad(kit, &file, &file_name);
                            } else {
                                self.place_sound(&file, place, 0, text::EDIT_IMPORT);
                                self.set_status(text::imported(&file_name));
                            }
                        }
                        Err(reason) => {
                            self.set_status(text::could_not_import(&file_name, &reason));
                        }
                    }
                }
            }
        }
        let finished: Vec<Finished> = self
            .recorder
            .as_ref()
            .map_or_else(Vec::new, |r| std::iter::from_fn(|| r.poll()).collect());
        for Finished { take, result } in finished {
            let Some(index) = self.finishing.iter().position(|t| t.number == Some(take)) else {
                continue;
            };
            let info = self.finishing.remove(index);
            match result {
                Ok(sound) => {
                    let Some(name) = sound
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                    else {
                        continue;
                    };
                    self.engine.register_audio(&name, Arc::clone(&sound.sound));
                    self.audio.insert_ready(&name, Arc::clone(&sound.peaks));
                    let file = AudioFile {
                        id: AudioId(0),
                        name: self.next_recording_name(),
                        file: name,
                        frames: sound.frames,
                        sample_rate: sound.sample_rate,
                        channels: sound.channels,
                    };
                    // The take starts late by the delay out and back in.
                    let delay = (f64::from(info.latency) * f64::from(sound.sample_rate)).round();
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a positive frame count"
                    )]
                    let offset = (delay.max(0.0) as u64).min(sound.frames.saturating_sub(1));
                    let place = ImportPlace {
                        track: Some(info.track),
                        at: info.at,
                        pad_of: None,
                    };
                    let seconds = file.seconds_in(sound.frames - offset);
                    self.place_sound(&file, place, offset, text::EDIT_RECORD_SOUND);
                    self.set_status(text::recorded_sound(seconds));
                }
                Err(reason) if reason.is_empty() => self.set_status(text::NO_INPUT),
                Err(reason) => self.set_status(text::could_not_record(&reason)),
            }
        }
    }

    /// Puts an audio file in the song as one undo step: from frame
    /// `offset`, at the place asked for (a new track named after it if
    /// none), replacing what it covers there. Selects the new clip.
    fn place_sound(&mut self, file: &AudioFile, place: ImportPlace, offset: u64, label: &str) {
        let new_track_name = place.track.is_none().then(|| {
            let base = if file.name.trim().is_empty() {
                text::AUDIO_TRACK
            } else {
                file.name.trim()
            };
            self.unique_track_name(base)
        });
        let at = place.at;
        let seconds = file.seconds_in(file.frames.saturating_sub(offset));
        let result = self.doc.edit(label, |p| {
            let track = match (place.track, &new_track_name) {
                (Some(track), _) if p.track(track).is_some() => track,
                (_, Some(name)) => p.add_audio_track(name),
                _ => p.add_audio_track(text::AUDIO_TRACK),
            };
            let audio = p.add_audio_file(file.clone());
            let length = p.ticks_for(at, seconds);
            let clip = p.add_audio_clip(track, at, length, audio, offset);
            if let Some(clip) = clip {
                p.clear_span(track, at, at + length, Some(clip));
            }
            p.prune_audio();
            clip.map(|clip| (track, clip))
        });
        if let Some((track, clip)) = result {
            self.audio_focus = Some(track);
            self.selected_clip = Some(clip);
        }
    }

    /// "Recording 3": the first number no audio file of the song has.
    fn next_recording_name(&self) -> String {
        let taken = |name: &str| self.project().audio.iter().any(|f| f.name == name);
        (1..10_000)
            .map(text::recording_name)
            .find(|name| !taken(name))
            .unwrap_or_else(|| text::recording_name(1))
    }

    // ----- Shaping clips

    /// Splits a clip in two at `at` (a moment inside it). Returns the new
    /// second part, which becomes selected.
    pub fn split_clip(&mut self, clip: ClipId, at: Ticks) -> Option<ClipId> {
        let (_, c) = self.project().clip(clip)?;
        if at <= c.start || at >= c.end() {
            return None;
        }
        let second = self
            .doc
            .edit(text::EDIT_SPLIT_CLIP, |p| p.split_clip(clip, at))?;
        self.selected_clip = Some(second);
        Some(second)
    }

    /// Splits the selected clip at the play position, if it is inside it.
    pub fn split_at_play_position(&mut self) -> Option<ClipId> {
        let clip = self.selected_clip?;
        self.split_clip(clip, self.song_position())
    }

    /// Moves a clip's start, keeping what it plays where it is (trimming its
    /// beginning away, or uncovering it).
    pub fn trim_clip_start(&mut self, clip: ClipId, start: Ticks) {
        if self
            .project()
            .clip(clip)
            .is_none_or(|(_, c)| c.start == start)
        {
            return;
        }
        self.doc.edit(text::EDIT_TRIM_CLIP, |p| {
            p.trim_clip_start(clip, start);
        });
    }

    /// How long an audio clip takes to fade in and out, in seconds.
    pub fn clip_fades(&self, clip: ClipId) -> Option<(f32, f32)> {
        match self.project().clip(clip)?.1.content {
            ClipContent::Audio {
                fade_in, fade_out, ..
            } => Some((fade_in, fade_out)),
            ClipContent::Pattern { .. } => None,
        }
    }

    /// Sets an audio clip's fades, in seconds (together no longer than the
    /// clip).
    pub fn set_clip_fades(&mut self, clip: ClipId, fade_in: f32, fade_out: f32) {
        let Some((_, c)) = self.project().clip(clip).map(|(t, c)| (t, *c)) else {
            return;
        };
        #[allow(clippy::cast_possible_truncation, reason = "seconds")]
        let length = self.project().seconds_between(c.start, c.end()) as f32;
        let fade_in = fade_in.clamp(0.0, length);
        let fade_out = fade_out.clamp(0.0, (length - fade_in).max(0.0));
        if self.clip_fades(clip) == Some((fade_in, fade_out)) {
            return;
        }
        self.doc.edit(text::EDIT_FADE, |p| {
            p.update_clip(clip, |c| {
                if let ClipContent::Audio {
                    fade_in: a,
                    fade_out: b,
                    ..
                } = &mut c.content
                {
                    *a = fade_in;
                    *b = fade_out;
                }
            });
        });
    }

    /// An audio clip's volume (1 is unchanged).
    pub fn clip_gain(&self, clip: ClipId) -> Option<f32> {
        match self.project().clip(clip)?.1.content {
            ClipContent::Audio { gain, .. } => Some(gain),
            ClipContent::Pattern { .. } => None,
        }
    }

    /// Sets an audio clip's volume (from silence to four times, +12 dB).
    pub fn set_clip_gain(&mut self, clip: ClipId, gain: f32) {
        let gain = if gain.is_finite() {
            gain.clamp(0.0, 4.0)
        } else {
            1.0
        };
        if self
            .clip_gain(clip)
            .is_none_or(|g| (g - gain).abs() < f32::EPSILON)
        {
            return;
        }
        self.doc.edit(text::EDIT_CLIP_VOLUME, |p| {
            p.update_clip(clip, |c| {
                if let ClipContent::Audio { gain: g, .. } = &mut c.content {
                    *g = gain;
                }
            });
        });
    }

    /// Moves an audio clip to another audio track, at the same time.
    /// Returns whether it moved.
    pub fn move_clip_to_track(&mut self, clip: ClipId, track: TrackId) -> bool {
        let movable = self
            .project()
            .clip(clip)
            .is_some_and(|(from, c)| from != track && c.is_audio())
            && self.is_audio_track(track);
        if !movable {
            return false;
        }
        let moved = self
            .doc
            .edit(text::EDIT_MOVE_CLIP, |p| p.move_clip_to_track(clip, track));
        if moved {
            self.audio_focus = Some(track);
        }
        moved
    }
}

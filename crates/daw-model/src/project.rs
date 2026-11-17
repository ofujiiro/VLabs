// SPDX-License-Identifier: GPL-3.0-or-later

//! The project: everything that makes up one song.

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::automation::{AutomationLane, AutomationTarget};
use crate::mixer::{ChainPlace, Effect, EffectKind, LimiterSettings, MasterBus};
use crate::song::{
    AudioFile, AudioId, ChannelStrip, Clip, ClipContent, ClipId, EffectId, Instrument, MAX_KEY,
    Note, NoteId, Pattern, PatternId, Track, TrackId, default_step,
};
use crate::time::{MeterMap, Region, TempoMap, Ticks, TimeSignature};

/// One song. This is the single source of truth: the interface shows it and
/// edits it (through [`crate::Document`]), the engine plays a compiled copy.
///
/// Tracks and patterns are reference-counted so that copying a project (as
/// the undo history does after every edit) shares everything that did not
/// change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// The song's title.
    pub title: String,
    /// Tempo over the whole song.
    pub tempo: TempoMap,
    /// Time signatures over the whole song.
    pub meter: MeterMap,
    /// The tracks, top to bottom.
    pub tracks: Vec<Arc<Track>>,
    /// Every pattern, whether placed on the timeline or not.
    pub patterns: Vec<Arc<Pattern>>,
    /// The part of the song that plays in a loop, if looping is on.
    pub loop_region: Option<Region>,
    /// The master bus: master volume and effects.
    #[serde(default)]
    pub master: MasterBus,
    /// The audio files the song's audio clips play (decision 0017).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<AudioFile>,
    /// The next unused identifier.
    next_id: u64,
}

impl Project {
    /// An empty project with no tracks.
    pub fn empty() -> Self {
        Self {
            title: String::new(),
            tempo: TempoMap::default(),
            meter: MeterMap::default(),
            tracks: Vec::new(),
            patterns: Vec::new(),
            loop_region: None,
            master: MasterBus::default(),
            audio: Vec::new(),
            next_id: 1,
        }
    }

    /// The project a new user starts with: one piano track with an empty
    /// four-bar pattern, looping, at 120 BPM in 4/4.
    pub fn starter(pattern_name: &str, track_name: &str) -> Self {
        let mut project = Self::empty();
        let bars = 4;
        let length = TimeSignature::FOUR_FOUR.bar_length() * bars;
        let track = project.add_track(track_name, Instrument::builtin_piano());
        let pattern = project.add_pattern_for(track, pattern_name, length);
        project.add_clip(track, Ticks::ZERO, length, pattern);
        project.loop_region = Region::new(Ticks::ZERO, length);
        // A transparent limiter on the master keeps a busy mix from
        // distorting.
        project.add_effect(
            ChainPlace::Master,
            EffectKind::Limiter(LimiterSettings::default()),
        );
        project
    }

    /// Adds a drum track at the top with an empty pattern of `bars` bars,
    /// repeated over the song's first `song_bars` bars.
    pub fn add_drums_with_pattern(
        &mut self,
        track_name: &str,
        pattern_name: &str,
        kit: &str,
        bars: u32,
        song_bars: u32,
    ) -> (TrackId, PatternId) {
        let track = self.add_track(track_name, Instrument::drums(kit));
        if let Some(index) = self.tracks.iter().position(|t| t.id == track) {
            let moved = self.tracks.remove(index);
            self.tracks.insert(0, moved);
        }
        let bar = self.meter.initial().bar_length();
        let pattern = self.add_pattern_for(track, pattern_name, bar * i64::from(bars.max(1)));
        self.add_clip(
            track,
            Ticks::ZERO,
            self.meter.bar_start(song_bars.max(1)),
            pattern,
        );
        (track, pattern)
    }

    /// A new, unique identifier.
    pub fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Makes sure future identifiers are above every one in use (after
    /// loading a file written by hand or by another program).
    pub fn repair_id_counter(&mut self) {
        let mut highest = 0;
        for pattern in &self.patterns {
            highest = highest.max(pattern.id.0);
            for note in &pattern.notes {
                highest = highest.max(note.id.0);
            }
        }
        for track in &self.tracks {
            highest = highest.max(track.id.0);
            for clip in &track.clips {
                highest = highest.max(clip.id.0);
            }
            for effect in &track.effects {
                highest = highest.max(effect.id.0);
            }
        }
        for effect in &self.master.effects {
            highest = highest.max(effect.id.0);
        }
        for file in &self.audio {
            highest = highest.max(file.id.0);
        }
        self.next_id = self.next_id.max(highest + 1);
    }

    // ----- Patterns

    /// Adds an empty pattern that belongs to no track yet.
    pub fn add_pattern(&mut self, name: &str, length: Ticks) -> PatternId {
        let id = PatternId(self.allocate_id());
        self.patterns.push(Arc::new(Pattern {
            id,
            name: name.to_owned(),
            length,
            notes: Vec::new(),
            track: None,
            step: default_step(),
        }));
        id
    }

    /// Adds an empty pattern for a track.
    pub fn add_pattern_for(&mut self, track: TrackId, name: &str, length: Ticks) -> PatternId {
        let id = self.add_pattern(name, length);
        if let Some(pattern) = self.pattern_mut(id) {
            pattern.track = Some(track);
        }
        id
    }

    /// The patterns of a track, in the order they were made.
    pub fn track_patterns(&self, track: TrackId) -> impl Iterator<Item = &Pattern> {
        self.patterns
            .iter()
            .map(AsRef::as_ref)
            .filter(move |p| p.track == Some(track))
    }

    /// Copies a pattern (notes and all) under a new name; returns the copy.
    pub fn duplicate_pattern(&mut self, id: PatternId, name: &str) -> Option<PatternId> {
        let original = self.pattern(id)?.clone();
        let copy = PatternId(self.allocate_id());
        let notes: Vec<Note> = original
            .notes
            .iter()
            .map(|n| Note {
                id: NoteId(self.allocate_id()),
                ..*n
            })
            .collect();
        self.patterns.push(Arc::new(Pattern {
            id: copy,
            name: name.to_owned(),
            notes,
            ..original
        }));
        Some(copy)
    }

    /// Removes a pattern and every clip that plays it.
    pub fn remove_pattern(&mut self, id: PatternId) {
        self.patterns.retain(|p| p.id != id);
        for track in &mut self.tracks {
            if track.clips.iter().any(|c| c.pattern() == Some(id)) {
                Arc::make_mut(track)
                    .clips
                    .retain(|c| c.pattern() != Some(id));
            }
        }
    }

    /// A pattern by id.
    pub fn pattern(&self, id: PatternId) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id).map(AsRef::as_ref)
    }

    /// A pattern by id, for changing it (copied first if shared with the
    /// undo history).
    pub fn pattern_mut(&mut self, id: PatternId) -> Option<&mut Pattern> {
        self.patterns
            .iter_mut()
            .find(|p| p.id == id)
            .map(Arc::make_mut)
    }

    /// Adds a note to a pattern; returns its id. The note's own id is
    /// ignored: a fresh one is assigned.
    pub fn add_note(&mut self, pattern: PatternId, note: Note) -> Option<NoteId> {
        let id = NoteId(self.allocate_id());
        let pattern = self.pattern_mut(pattern)?;
        let note = Note {
            id,
            ..sanitized(note)
        };
        let index = pattern
            .notes
            .partition_point(|n| (n.start, n.key, n.id) <= (note.start, note.key, note.id));
        pattern.notes.insert(index, note);
        Some(id)
    }

    /// Adds several notes at once; returns their ids in the same order.
    pub fn add_notes(&mut self, pattern: PatternId, notes: &[Note]) -> Vec<NoteId> {
        let ids: Vec<NoteId> = notes.iter().map(|_| NoteId(self.allocate_id())).collect();
        if let Some(target) = self.pattern_mut(pattern) {
            target
                .notes
                .extend(notes.iter().zip(&ids).map(|(n, &id)| Note {
                    id,
                    ..sanitized(*n)
                }));
            target.sort_notes();
        }
        ids
    }

    /// Removes notes by id.
    pub fn remove_notes(&mut self, pattern: PatternId, ids: &[NoteId]) {
        let wanted = IdSet::new(ids);
        if let Some(pattern) = self.pattern_mut(pattern) {
            pattern.notes.retain(|n| !wanted.contains(n.id));
        }
    }

    /// Changes notes in place; `change` sees each selected note. Keeps the
    /// notes sorted and valid afterwards.
    pub fn update_notes(
        &mut self,
        pattern: PatternId,
        ids: &[NoteId],
        mut change: impl FnMut(&mut Note),
    ) {
        let wanted = IdSet::new(ids);
        if let Some(pattern) = self.pattern_mut(pattern) {
            for note in pattern.notes.iter_mut().filter(|n| wanted.contains(n.id)) {
                change(note);
                *note = sanitized(*note);
            }
            pattern.sort_notes();
        }
    }

    // ----- Tracks and clips

    /// Adds a track at the bottom.
    pub fn add_track(&mut self, name: &str, instrument: Instrument) -> TrackId {
        let id = TrackId(self.allocate_id());
        self.tracks.push(Arc::new(Track {
            id,
            name: name.to_owned(),
            instrument,
            clips: Vec::new(),
            strip: ChannelStrip::default(),
            effects: Vec::new(),
            automation: Vec::new(),
        }));
        id
    }

    /// Removes a track with its clips and its patterns.
    pub fn remove_track(&mut self, id: TrackId) {
        self.tracks.retain(|t| t.id != id);
        self.patterns.retain(|p| p.track != Some(id));
        self.prune_audio();
    }

    /// A clip by id, with its track.
    pub fn clip(&self, id: ClipId) -> Option<(TrackId, &Clip)> {
        self.tracks
            .iter()
            .find_map(|t| t.clips.iter().find(|c| c.id == id).map(|c| (t.id, c)))
    }

    /// Changes a clip in place (its track's clips stay sorted).
    pub fn update_clip(&mut self, id: ClipId, change: impl FnOnce(&mut Clip)) {
        let Some((track, _)) = self.clip(id) else {
            return;
        };
        if let Some(track) = self.track_mut(track)
            && let Some(clip) = track.clips.iter_mut().find(|c| c.id == id)
        {
            change(clip);
            clip.start = clip.start.max(Ticks::ZERO);
            clip.length = clip.length.max(Ticks(1));
            track.clips.sort_by_key(|c| (c.start, c.id));
        }
    }

    /// Removes a clip (and its audio file, if no other clip plays it).
    pub fn remove_clip(&mut self, id: ClipId) {
        let Some((track, _)) = self.clip(id) else {
            return;
        };
        if let Some(track) = self.track_mut(track) {
            track.clips.retain(|c| c.id != id);
        }
        self.prune_audio();
    }

    /// Changes a clip's length; an audio clip cannot outlast its sound.
    pub fn set_clip_length(&mut self, id: ClipId, length: Ticks) {
        let Some((_, clip)) = self.clip(id).map(|(t, c)| (t, *c)) else {
            return;
        };
        let mut length = length.max(Ticks(1));
        if let Some(end) = self.audio_end(&clip) {
            length = length.min((end - clip.start).max(Ticks(1)));
        }
        self.update_clip(id, |c| c.length = length);
    }

    /// Moves a clip's start to `start` while what it plays stays where it
    /// is in the song: its beginning is trimmed away, or uncovered again.
    /// Its end stays put. An audio clip cannot start before its file does.
    pub fn trim_clip_start(&mut self, id: ClipId, start: Ticks) {
        let Some((_, clip)) = self.clip(id).map(|(t, c)| (t, *c)) else {
            return;
        };
        let mut start = start.max(Ticks::ZERO).min(clip.end() - Ticks(1));
        let content = match clip.content {
            ClipContent::Pattern { pattern, offset } => {
                let length = self.pattern(pattern).map_or(Ticks::ZERO, |p| p.length);
                let moved = offset + (start - clip.start);
                ClipContent::Pattern {
                    pattern,
                    offset: if length > Ticks::ZERO {
                        moved % length
                    } else {
                        Ticks::ZERO
                    },
                }
            }
            ClipContent::Audio {
                audio,
                offset,
                gain,
                fade_in,
                fade_out,
            } => {
                let Some(file) = self.audio_file(audio) else {
                    return;
                };
                if start < clip.start {
                    // No earlier than the file's first frame.
                    let first = self.tempo.seconds_at(clip.start) - file.seconds_in(offset);
                    #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
                    let earliest = Ticks(self.tempo.ticks_at(first.max(0.0)).ceil() as i64);
                    start = start.max(earliest).min(clip.start);
                }
                let moved = self.seconds_between(clip.start, start);
                let frames = file.frames_in(moved.abs());
                ClipContent::Audio {
                    audio,
                    offset: if moved >= 0.0 {
                        offset + frames
                    } else {
                        offset.saturating_sub(frames)
                    },
                    gain,
                    fade_in,
                    fade_out,
                }
            }
        };
        self.update_clip(id, |c| {
            let end = c.end();
            c.start = start;
            c.length = end - start;
            c.content = content;
        });
    }

    /// Splits a clip in two at `at`, which must be inside it: the clip
    /// keeps the part before, and a new clip plays the part after, carrying
    /// on exactly where the first stops. Returns the new clip.
    pub fn split_clip(&mut self, id: ClipId, at: Ticks) -> Option<ClipId> {
        let (track, clip) = self.clip(id).map(|(t, c)| (t, *c))?;
        if at <= clip.start || at >= clip.end() {
            return None;
        }
        let (before, after) = match clip.content {
            ClipContent::Pattern { pattern, offset } => {
                let length = self.pattern(pattern).map_or(Ticks::ZERO, |p| p.length);
                let moved = offset + (at - clip.start);
                let offset = if length > Ticks::ZERO {
                    moved % length
                } else {
                    Ticks::ZERO
                };
                (clip.content, ClipContent::Pattern { pattern, offset })
            }
            ClipContent::Audio {
                audio,
                offset,
                gain,
                fade_in,
                fade_out,
            } => {
                let file = self.audio_file(audio)?;
                let first = self.seconds_between(clip.start, at);
                let second = self.seconds_between(at, clip.end());
                #[allow(clippy::cast_possible_truncation, reason = "seconds")]
                let parts = (
                    ClipContent::Audio {
                        audio,
                        offset,
                        gain,
                        fade_in: fade_in.min(first as f32),
                        fade_out: 0.0,
                    },
                    ClipContent::Audio {
                        audio,
                        offset: offset + file.frames_in(first),
                        gain,
                        fade_in: 0.0,
                        fade_out: fade_out.min(second as f32),
                    },
                );
                parts
            }
        };
        let new = ClipId(self.allocate_id());
        let track = self.track_mut(track)?;
        if let Some(first) = track.clips.iter_mut().find(|c| c.id == id) {
            first.length = at - clip.start;
            first.content = before;
        }
        track.clips.push(Clip {
            id: new,
            start: at,
            length: clip.end() - at,
            content: after,
        });
        track.clips.sort_by_key(|c| (c.start, c.id));
        Some(new)
    }

    /// Makes room on a track from `start` to `end`: the parts of its clips
    /// there are cut away (clips wholly inside go, clips across an edge are
    /// trimmed, a clip across the whole span is split). `keep` is left
    /// alone (the clip being placed there).
    pub fn clear_span(&mut self, track: TrackId, start: Ticks, end: Ticks, keep: Option<ClipId>) {
        if end <= start {
            return;
        }
        let covered: Vec<Clip> = self.track(track).map_or_else(Vec::new, |t| {
            t.clips
                .iter()
                .filter(|c| Some(c.id) != keep && c.start < end && start < c.end())
                .copied()
                .collect()
        });
        for clip in covered {
            if clip.start >= start && clip.end() <= end {
                if let Some(t) = self.track_mut(track) {
                    t.clips.retain(|c| c.id != clip.id);
                }
            } else if clip.start < start && clip.end() > end {
                self.split_clip(clip.id, end);
                self.set_clip_length(clip.id, start - clip.start);
            } else if clip.start < start {
                self.set_clip_length(clip.id, start - clip.start);
            } else {
                self.trim_clip_start(clip.id, end);
            }
        }
        self.prune_audio();
    }

    /// Moves an audio clip to another audio track, at the same time.
    /// Returns whether it moved.
    pub fn move_clip_to_track(&mut self, id: ClipId, to: TrackId) -> bool {
        let Some((from, clip)) = self.clip(id).map(|(t, c)| (t, *c)) else {
            return false;
        };
        let allowed = from != to
            && clip.is_audio()
            && self.track(to).is_some_and(|t| t.instrument.is_audio());
        if !allowed {
            return false;
        }
        if let Some(t) = self.track_mut(from) {
            t.clips.retain(|c| c.id != id);
        }
        if let Some(t) = self.track_mut(to) {
            t.clips.push(clip);
            t.clips.sort_by_key(|c| (c.start, c.id));
        }
        true
    }

    // ----- Audio

    /// Adds an audio track at the bottom: it plays audio clips.
    pub fn add_audio_track(&mut self, name: &str) -> TrackId {
        self.add_track(name, Instrument::Audio)
    }

    /// Adds an audio file to the project; returns its id (the file's own
    /// id is ignored). Its sound is kept outside the model.
    pub fn add_audio_file(&mut self, file: AudioFile) -> AudioId {
        let id = AudioId(self.allocate_id());
        self.audio.push(AudioFile {
            id,
            sample_rate: file.sample_rate.max(1),
            channels: file.channels.clamp(1, 2),
            ..file
        });
        id
    }

    /// An audio file by id.
    pub fn audio_file(&self, id: AudioId) -> Option<&AudioFile> {
        self.audio.iter().find(|f| f.id == id)
    }

    /// Places part of an audio file on a track: from frame `offset` of the
    /// file, for `length`.
    pub fn add_audio_clip(
        &mut self,
        track: TrackId,
        start: Ticks,
        length: Ticks,
        audio: AudioId,
        offset: u64,
    ) -> Option<ClipId> {
        self.audio_file(audio)?;
        let id = ClipId(self.allocate_id());
        let track = self.track_mut(track)?;
        track.clips.push(Clip {
            id,
            start: start.max(Ticks::ZERO),
            length: length.max(Ticks(1)),
            content: ClipContent::audio(audio, offset),
        });
        track.clips.sort_by_key(|c| (c.start, c.id));
        Some(id)
    }

    /// Forgets the audio files no clip plays any more.
    pub fn prune_audio(&mut self) {
        let used: HashSet<AudioId> = self
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter().filter_map(Clip::audio))
            .collect();
        self.audio.retain(|f| used.contains(&f.id));
    }

    /// How many seconds pass from `start` to `end` (negative if `end` is
    /// earlier).
    pub fn seconds_between(&self, start: Ticks, end: Ticks) -> f64 {
        self.tempo.seconds_at(end) - self.tempo.seconds_at(start)
    }

    /// How long `seconds` of sound starting at `start` lasts on the
    /// timeline (at least one tick).
    pub fn ticks_for(&self, start: Ticks, seconds: f64) -> Ticks {
        let from = self.tempo.seconds_at(start);
        #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
        let end = self.tempo.ticks_at(from + seconds.max(0.0)).round() as i64;
        Ticks((end - start.0).max(1))
    }

    /// Where an audio clip's sound runs out: the furthest its end can go.
    pub fn audio_end(&self, clip: &Clip) -> Option<Ticks> {
        let ClipContent::Audio { audio, offset, .. } = clip.content else {
            return None;
        };
        let file = self.audio_file(audio)?;
        let left = file.seconds() - file.seconds_in(offset);
        Some(clip.start + self.ticks_for(clip.start, left))
    }

    /// Sets the tempo from `at` onwards (decision 0017: audio clips keep
    /// their length in seconds, since their sound is not stretched; their
    /// starts stay on their beats).
    pub fn set_tempo(&mut self, at: Ticks, bpm: f64) {
        let before = self.tempo.clone();
        self.tempo.set(at, bpm);
        if before == self.tempo {
            return;
        }
        let tempo = self.tempo.clone();
        for track in &mut self.tracks {
            if !track.clips.iter().any(Clip::is_audio) {
                continue;
            }
            for clip in Arc::make_mut(track)
                .clips
                .iter_mut()
                .filter(|c| c.is_audio())
            {
                let seconds = before.seconds_at(clip.end()) - before.seconds_at(clip.start);
                #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
                let end = tempo
                    .ticks_at(tempo.seconds_at(clip.start) + seconds)
                    .round() as i64;
                clip.length = Ticks((end - clip.start.0).max(1));
            }
        }
    }

    // ----- Effects

    /// The effect chain at a place.
    pub fn chain(&self, place: ChainPlace) -> &[Effect] {
        match place {
            ChainPlace::Master => &self.master.effects,
            ChainPlace::Track(track) => self.track(track).map_or(&[], |t| t.effects.as_slice()),
        }
    }

    fn chain_mut(&mut self, place: ChainPlace) -> Option<&mut Vec<Effect>> {
        match place {
            ChainPlace::Master => Some(&mut self.master.effects),
            ChainPlace::Track(track) => self.track_mut(track).map(|t| &mut t.effects),
        }
    }

    /// Adds an effect at the end of a chain; returns its id.
    pub fn add_effect(&mut self, place: ChainPlace, kind: EffectKind) -> Option<EffectId> {
        let id = EffectId(self.allocate_id());
        self.chain_mut(place)?.push(Effect {
            id,
            enabled: true,
            kind,
        });
        Some(id)
    }

    /// Where an effect is, and its position in that chain.
    pub fn effect_place(&self, id: EffectId) -> Option<(ChainPlace, usize)> {
        if let Some(index) = self.master.effects.iter().position(|e| e.id == id) {
            return Some((ChainPlace::Master, index));
        }
        self.tracks.iter().find_map(|t| {
            t.effects
                .iter()
                .position(|e| e.id == id)
                .map(|index| (ChainPlace::Track(t.id), index))
        })
    }

    /// An effect by id.
    pub fn effect(&self, id: EffectId) -> Option<&Effect> {
        let (place, index) = self.effect_place(id)?;
        self.chain(place).get(index)
    }

    /// Changes an effect in place.
    pub fn update_effect(&mut self, id: EffectId, change: impl FnOnce(&mut Effect)) {
        if let Some((place, index)) = self.effect_place(id)
            && let Some(effect) = self.chain_mut(place).and_then(|c| c.get_mut(index))
        {
            change(effect);
            effect.id = id;
        }
    }

    /// Removes an effect (and the automation of its settings).
    pub fn remove_effect(&mut self, id: EffectId) {
        if let Some((place, _)) = self.effect_place(id)
            && let Some(chain) = self.chain_mut(place)
        {
            chain.retain(|e| e.id != id);
            if let ChainPlace::Track(track) = place
                && let Some(track) = self.track_mut(track)
            {
                track.automation.retain(
                    |lane| !matches!(lane.target, AutomationTarget::Effect { effect, .. } if effect == id),
                );
            }
        }
    }

    /// Moves an effect to another position in its chain.
    pub fn move_effect(&mut self, id: EffectId, to: usize) {
        if let Some((place, from)) = self.effect_place(id)
            && let Some(chain) = self.chain_mut(place)
        {
            let effect = chain.remove(from);
            let to = to.min(chain.len());
            chain.insert(to, effect);
        }
    }

    // ----- Automation

    /// A track's lane for a setting, if it has one.
    pub fn lane(&self, track: TrackId, target: AutomationTarget) -> Option<&AutomationLane> {
        self.track(track)?
            .automation
            .iter()
            .find(|lane| lane.target == target)
    }

    /// Gives a track a lane for a setting (nothing changes if it has one).
    pub fn add_lane(&mut self, track: TrackId, target: AutomationTarget) {
        if self.track(track).is_some()
            && self.lane(track, target).is_none()
            && let Some(track) = self.track_mut(track)
        {
            track.automation.push(AutomationLane::new(target));
        }
    }

    /// Takes a lane, with its points, away from a track.
    pub fn remove_lane(&mut self, track: TrackId, target: AutomationTarget) {
        if self.lane(track, target).is_some()
            && let Some(track) = self.track_mut(track)
        {
            track.automation.retain(|lane| lane.target != target);
        }
    }

    /// Changes a lane's points (they are put back in time order after).
    pub fn update_lane(
        &mut self,
        track: TrackId,
        target: AutomationTarget,
        change: impl FnOnce(&mut AutomationLane),
    ) {
        if self.lane(track, target).is_some()
            && let Some(track) = self.track_mut(track)
            && let Some(lane) = track.automation.iter_mut().find(|l| l.target == target)
        {
            change(lane);
            lane.sort();
        }
    }

    /// A track by id.
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id).map(AsRef::as_ref)
    }

    /// A track by id, for changing it.
    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|t| t.id == id)
            .map(Arc::make_mut)
    }

    /// Places a pattern on a track.
    pub fn add_clip(
        &mut self,
        track: TrackId,
        start: Ticks,
        length: Ticks,
        pattern: PatternId,
    ) -> Option<ClipId> {
        let id = ClipId(self.allocate_id());
        let track = self.track_mut(track)?;
        track.clips.push(Clip {
            id,
            start,
            length,
            content: ClipContent::Pattern {
                pattern,
                offset: Ticks::ZERO,
            },
        });
        track.clips.sort_by_key(|c| (c.start, c.id));
        Some(id)
    }

    /// The first track that plays a pattern, and the pattern of its first
    /// pattern clip: what the editor shows when a project opens.
    pub fn first_pattern(&self) -> Option<(TrackId, PatternId)> {
        self.tracks.iter().find_map(|track| {
            track
                .clips
                .iter()
                .find_map(Clip::pattern)
                .map(|pattern| (track.id, pattern))
        })
    }

    /// The end of the last clip: how long the song is.
    pub fn song_end(&self) -> Ticks {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(Clip::end))
            .max()
            .unwrap_or(Ticks::ZERO)
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::empty()
    }
}

/// A set of note ids that stays fast for large selections.
enum IdSet<'a> {
    Few(&'a [NoteId]),
    Many(HashSet<NoteId>),
}

impl<'a> IdSet<'a> {
    fn new(ids: &'a [NoteId]) -> Self {
        if ids.len() <= 16 {
            Self::Few(ids)
        } else {
            Self::Many(ids.iter().copied().collect())
        }
    }

    fn contains(&self, id: NoteId) -> bool {
        match self {
            Self::Few(ids) => ids.contains(&id),
            Self::Many(ids) => ids.contains(&id),
        }
    }
}

/// Keeps a note within valid bounds: inside the key range, positive length,
/// velocity between 0 and 1, start not before zero.
fn sanitized(mut note: Note) -> Note {
    note.key = note.key.min(MAX_KEY);
    note.start = note.start.max(Ticks::ZERO);
    if note.length <= Ticks::ZERO {
        note.length = Ticks(1);
    }
    note.velocity = if note.velocity.is_finite() {
        note.velocity.clamp(0.0, 1.0)
    } else {
        crate::DEFAULT_VELOCITY
    };
    if !note.pitch_offset.is_finite() {
        note.pitch_offset = 0.0;
    }
    note
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_VELOCITY;

    fn note(start: Ticks, key: u8) -> Note {
        Note {
            id: NoteId(0),
            start,
            length: Ticks::QUARTER,
            key,
            velocity: DEFAULT_VELOCITY,
            pitch_offset: 0.0,
        }
    }

    #[test]
    fn the_starter_project_has_a_looping_piano_pattern() {
        let project = Project::starter("Pattern 1", "Piano");
        let (track, pattern) = project.first_pattern().unwrap();
        assert_eq!(
            project.track(track).unwrap().instrument,
            Instrument::builtin_piano()
        );
        assert_eq!(project.pattern(pattern).unwrap().length, Ticks::WHOLE * 4);
        assert_eq!(
            project.loop_region,
            Region::new(Ticks::ZERO, Ticks::WHOLE * 4)
        );
        assert_eq!(project.song_end(), Ticks::WHOLE * 4);
    }

    #[test]
    fn notes_get_unique_ids_and_stay_sorted() {
        let mut project = Project::starter("P", "T");
        let (_, pattern) = project.first_pattern().unwrap();
        let a = project.add_note(pattern, note(Ticks::QUARTER, 64)).unwrap();
        let b = project.add_note(pattern, note(Ticks::ZERO, 60)).unwrap();
        let more = project.add_notes(pattern, &[note(Ticks::QUARTER, 60), note(Ticks::ZERO, 67)]);
        let mut ids = vec![a, b];
        ids.extend(more);
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 4, "every note has its own id");
        let starts: Vec<(Ticks, u8)> = project
            .pattern(pattern)
            .unwrap()
            .notes
            .iter()
            .map(|n| (n.start, n.key))
            .collect();
        let mut sorted = starts.clone();
        sorted.sort();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn invalid_notes_are_repaired() {
        let mut project = Project::starter("P", "T");
        let (_, pattern) = project.first_pattern().unwrap();
        let id = project
            .add_note(
                pattern,
                Note {
                    length: Ticks(-5),
                    key: 200,
                    velocity: 3.0,
                    start: Ticks(-10),
                    ..note(Ticks::ZERO, 0)
                },
            )
            .unwrap();
        let stored = *project.pattern(pattern).unwrap().note(id).unwrap();
        assert_eq!(stored.key, 127);
        assert_eq!(stored.start, Ticks::ZERO);
        assert!(stored.length > Ticks::ZERO);
        assert_eq!(stored.velocity, 1.0);
    }

    #[test]
    fn moving_and_removing_notes() {
        let mut project = Project::starter("P", "T");
        let (_, pattern) = project.first_pattern().unwrap();
        let a = project.add_note(pattern, note(Ticks::ZERO, 60)).unwrap();
        let b = project.add_note(pattern, note(Ticks::QUARTER, 62)).unwrap();
        project.update_notes(pattern, &[a], |n| {
            n.start = Ticks::QUARTER * 2;
            n.key += 12;
        });
        let notes = &project.pattern(pattern).unwrap().notes;
        assert_eq!(notes[0].id, b, "the moved note now sorts after the other");
        assert_eq!((notes[1].start, notes[1].key), (Ticks::QUARTER * 2, 72));
        project.remove_notes(pattern, &[b]);
        assert_eq!(project.pattern(pattern).unwrap().notes.len(), 1);
    }

    #[test]
    fn copies_share_unchanged_parts() {
        let mut project = Project::starter("P", "T");
        let (_, pattern) = project.first_pattern().unwrap();
        let before = project.clone();
        project.add_note(pattern, note(Ticks::ZERO, 60));
        assert!(
            Arc::ptr_eq(&before.tracks[0], &project.tracks[0]),
            "the track did not change"
        );
        assert!(
            !Arc::ptr_eq(&before.patterns[0], &project.patterns[0]),
            "the pattern did"
        );
    }

    #[test]
    fn the_id_counter_is_repaired_after_loading() {
        let mut project = Project::starter("P", "T");
        project.next_id = 1;
        project.repair_id_counter();
        let fresh = project.allocate_id();
        assert!(project.patterns.iter().all(|p| p.id.0 < fresh));
        assert!(project.tracks.iter().all(|t| t.id.0 < fresh));
    }

    #[test]
    fn patterns_belong_to_tracks_and_go_with_them() {
        let mut project = Project::starter("P", "Piano");
        let (piano, first) = project.first_pattern().unwrap();
        assert_eq!(project.pattern(first).unwrap().track, Some(piano));
        let (drums, beat) =
            project.add_drums_with_pattern("Drums", "Beat", crate::ELECTRONIC_KIT, 1, 4);
        assert_eq!(project.tracks[0].id, drums, "drums go on top");
        assert_eq!(project.pattern(beat).unwrap().length, Ticks::WHOLE);
        assert_eq!(
            project.track(drums).unwrap().clips[0].length,
            Ticks::WHOLE * 4,
            "repeated over 4 bars"
        );
        assert_eq!(project.track_patterns(drums).count(), 1);

        project.add_note(beat, note(Ticks::ZERO, 36));
        let copy = project.duplicate_pattern(beat, "Beat 2").unwrap();
        let (a, b) = (
            project.pattern(beat).unwrap(),
            project.pattern(copy).unwrap(),
        );
        assert_eq!(a.notes.len(), b.notes.len());
        assert_ne!(
            a.notes[0].id, b.notes[0].id,
            "the copy's notes are new notes"
        );
        assert_eq!(b.track, Some(drums));

        project.remove_pattern(beat);
        assert!(
            project.track(drums).unwrap().clips.is_empty(),
            "its clips went with it"
        );
        project.remove_track(drums);
        assert!(
            project.pattern(copy).is_none(),
            "a track's patterns go with it"
        );
        assert_eq!(project.tracks.len(), 1);
    }

    #[test]
    fn clips_move_and_resize_and_stay_sorted() {
        let mut project = Project::starter("P", "Piano");
        let (track, pattern) = project.first_pattern().unwrap();
        let second = project
            .add_clip(track, Ticks::WHOLE * 8, Ticks::WHOLE, pattern)
            .unwrap();
        project.update_clip(second, |c| c.start = Ticks::ZERO - Ticks::WHOLE);
        assert_eq!(
            project.clip(second).unwrap().1.start,
            Ticks::ZERO,
            "clamped at zero"
        );
        project.update_clip(second, |c| c.start = Ticks::WHOLE * 2);
        project.update_clip(second, |c| c.start = Ticks::ZERO);
        let starts: Vec<Ticks> = project
            .track(track)
            .unwrap()
            .clips
            .iter()
            .map(|c| c.start)
            .collect();
        assert!(starts.is_sorted(), "clips stay in order: {starts:?}");
        project.update_clip(second, |c| c.length = Ticks::ZERO);
        assert_eq!(
            project.clip(second).unwrap().1.length,
            Ticks(1),
            "never empty"
        );
        project.remove_clip(second);
        assert!(project.clip(second).is_none());
    }

    #[test]
    fn effect_chains_add_change_move_and_remove() {
        use crate::mixer::{ChainPlace, EffectKind, EqSettings, ReverbSettings};
        let mut project = Project::starter("P", "Piano");
        assert_eq!(
            project.chain(ChainPlace::Master).len(),
            1,
            "a new song has a master limiter"
        );
        let (track, _) = project.first_pattern().unwrap();
        let place = ChainPlace::Track(track);
        let eq = project
            .add_effect(place, EffectKind::Eq(EqSettings::default()))
            .unwrap();
        let reverb = project
            .add_effect(place, EffectKind::Reverb(ReverbSettings::default()))
            .unwrap();
        assert_eq!(project.effect_place(reverb), Some((place, 1)));
        project.update_effect(eq, |e| e.enabled = false);
        assert!(!project.effect(eq).unwrap().enabled);
        project.move_effect(reverb, 0);
        assert_eq!(project.chain(place)[0].id, reverb);
        project.remove_effect(eq);
        assert_eq!(project.chain(place).len(), 1);

        // Identifiers stay unique after loading a file written elsewhere.
        let mut loaded = project.clone();
        loaded.next_id = 1;
        loaded.repair_id_counter();
        assert!(loaded.allocate_id() > reverb.0);
    }

    #[test]
    fn drum_pads_keep_only_changed_settings() {
        use crate::song::PadSetting;
        let mut drums = Instrument::drums(crate::ACOUSTIC_KIT);
        let mut kick = drums.pad(36);
        assert_eq!(kick, PadSetting::new(36));
        kick.tune = -2.0;
        drums.set_pad(kick);
        assert_eq!(drums.pad(36).tune, -2.0);
        assert!(
            drums.same_source(&Instrument::drums(crate::ACOUSTIC_KIT)),
            "settings are not the source"
        );
        assert!(!drums.same_source(&Instrument::drums(crate::ELECTRONIC_KIT)));
        drums.set_pad(PadSetting::new(36));
        assert_eq!(
            drums,
            Instrument::drums(crate::ACOUSTIC_KIT),
            "back to defaults stores nothing"
        );
    }

    /// A song with an audio track holding one clip of a ten-second mono
    /// file at 48 kHz, starting at bar 2 (the tempo is 120: a bar lasts two
    /// seconds).
    fn audio_song() -> (Project, TrackId, AudioId, ClipId) {
        let mut project = Project::starter("P", "Piano");
        let track = project.add_audio_track("Audio");
        let audio = project.add_audio_file(AudioFile {
            id: AudioId(0),
            name: "Take".to_owned(),
            file: "take.wav".to_owned(),
            frames: 480_000,
            sample_rate: 48_000,
            channels: 1,
        });
        let start = Ticks::WHOLE;
        let length = project.ticks_for(start, 10.0);
        let clip = project
            .add_audio_clip(track, start, length, audio, 0)
            .unwrap();
        (project, track, audio, clip)
    }

    fn audio_offset(project: &Project, clip: ClipId) -> u64 {
        match project.clip(clip).unwrap().1.content {
            ClipContent::Audio { offset, .. } => offset,
            ClipContent::Pattern { .. } => panic!("not an audio clip"),
        }
    }

    #[test]
    fn audio_clips_last_as_long_as_their_sound() {
        let (project, track, audio, clip) = audio_song();
        let (_, c) = project.clip(clip).unwrap();
        // Ten seconds at 120 BPM are 20 quarter notes.
        assert_eq!(c.length, Ticks::quarters(20));
        assert_eq!(
            project.audio_end(c),
            Some(Ticks::WHOLE + Ticks::quarters(20))
        );
        assert_eq!(project.song_end(), Ticks::WHOLE * 6);
        assert!(project.track(track).unwrap().instrument.is_audio());
        assert_eq!(project.audio_file(audio).unwrap().seconds(), 10.0);
        assert_eq!(
            project.first_pattern().map(|(t, _)| t),
            Some(project.tracks[0].id),
            "an audio track is never what opens first"
        );
    }

    #[test]
    fn splitting_an_audio_clip_carries_on_where_the_first_part_stops() {
        let (mut project, _, _, clip) = audio_song();
        project.update_clip(clip, |c| {
            if let ClipContent::Audio {
                fade_in, fade_out, ..
            } = &mut c.content
            {
                *fade_in = 0.5;
                *fade_out = 1.0;
            }
        });
        let at = Ticks::WHOLE * 2; // One bar in: two seconds of sound.
        let second = project.split_clip(clip, at).unwrap();
        let (_, a) = project.clip(clip).map(|(t, c)| (t, *c)).unwrap();
        let (_, b) = project.clip(second).map(|(t, c)| (t, *c)).unwrap();
        assert_eq!((a.start, a.end()), (Ticks::WHOLE, at));
        assert_eq!((b.start, b.end()), (at, Ticks::WHOLE + Ticks::quarters(20)));
        assert_eq!(audio_offset(&project, second), 96_000, "two seconds in");
        let fades = |c: &Clip| match c.content {
            ClipContent::Audio {
                fade_in, fade_out, ..
            } => (fade_in, fade_out),
            ClipContent::Pattern { .. } => (-1.0, -1.0),
        };
        assert_eq!(fades(&a), (0.5, 0.0), "the fade in stays at the start");
        assert_eq!(fades(&b), (0.0, 1.0), "the fade out stays at the end");
        assert!(
            project.split_clip(clip, Ticks::WHOLE).is_none(),
            "not inside"
        );
        assert_eq!(project.audio.len(), 1, "both parts play the same file");
    }

    #[test]
    fn splitting_a_pattern_clip_keeps_the_pattern_in_place() {
        let mut project = Project::starter("P", "Piano");
        let (track, _) = project.first_pattern().unwrap();
        let clip = project.track(track).unwrap().clips[0].id;
        // The four-bar clip of a four-bar pattern, split after bar 1.
        let second = project.split_clip(clip, Ticks::WHOLE).unwrap();
        let (_, b) = project.clip(second).map(|(t, c)| (t, *c)).unwrap();
        assert_eq!(
            b.content,
            ClipContent::Pattern {
                pattern: project.first_pattern().unwrap().1,
                offset: Ticks::WHOLE
            }
        );
        // Trimming the second part's start back uncovers the pattern again.
        project.trim_clip_start(second, Ticks::ZERO);
        let (_, b) = project.clip(second).map(|(t, c)| (t, *c)).unwrap();
        assert_eq!(b.start, Ticks::ZERO);
        assert!(matches!(
            b.content,
            ClipContent::Pattern { offset, .. } if offset == Ticks::ZERO
        ));
    }

    #[test]
    fn trimming_an_audio_clip_moves_its_ends_but_not_its_sound() {
        let (mut project, _, _, clip) = audio_song();
        // The start one bar later: two seconds of sound are trimmed away.
        project.trim_clip_start(clip, Ticks::WHOLE * 2);
        let (_, c) = project.clip(clip).map(|(t, c)| (t, *c)).unwrap();
        assert_eq!(c.start, Ticks::WHOLE * 2);
        assert_eq!(c.end(), Ticks::WHOLE + Ticks::quarters(20), "the end stays");
        assert_eq!(audio_offset(&project, clip), 96_000);
        // Back to the very start of the song: no earlier than the file's start.
        project.trim_clip_start(clip, Ticks::ZERO);
        let (_, c) = project.clip(clip).map(|(t, c)| (t, *c)).unwrap();
        assert_eq!(c.start, Ticks::WHOLE);
        assert_eq!(audio_offset(&project, clip), 0);
        // The end cannot go past the end of the sound.
        project.set_clip_length(clip, Ticks::WHOLE * 100);
        let (_, c) = project.clip(clip).map(|(t, c)| (t, *c)).unwrap();
        assert_eq!(c.end(), Ticks::WHOLE + Ticks::quarters(20));
        project.set_clip_length(clip, Ticks::WHOLE);
        assert_eq!(project.clip(clip).unwrap().1.length, Ticks::WHOLE);
    }

    #[test]
    fn a_new_take_replaces_what_it_covers() {
        let (mut project, track, audio, clip) = audio_song();
        // Bars 2 to 7 hold the clip; a take over bars 3 and 4 splits it.
        let take = project
            .add_audio_clip(track, Ticks::WHOLE * 2, Ticks::WHOLE * 2, audio, 0)
            .unwrap();
        project.clear_span(track, Ticks::WHOLE * 2, Ticks::WHOLE * 4, Some(take));
        let spans: Vec<(Ticks, Ticks)> = project
            .track(track)
            .unwrap()
            .clips
            .iter()
            .map(|c| (c.start, c.end()))
            .collect();
        assert_eq!(
            spans,
            vec![
                (Ticks::WHOLE, Ticks::WHOLE * 2),
                (Ticks::WHOLE * 2, Ticks::WHOLE * 4),
                (Ticks::WHOLE * 4, Ticks::WHOLE + Ticks::quarters(20)),
            ]
        );
        let last = project.track(track).unwrap().clips[2].id;
        assert_eq!(
            audio_offset(&project, last),
            6 * 48_000,
            "the old sound carries on"
        );
        // A take over everything removes the old clips.
        project.clear_span(track, Ticks::ZERO, Ticks::WHOLE * 10, Some(take));
        assert_eq!(project.track(track).unwrap().clips.len(), 1);
        assert!(project.clip(clip).is_none());
    }

    #[test]
    fn audio_files_go_when_no_clip_plays_them() {
        let (mut project, track, audio, clip) = audio_song();
        let copy = project
            .add_audio_clip(track, Ticks::WHOLE * 8, Ticks::WHOLE, audio, 0)
            .unwrap();
        project.remove_clip(clip);
        assert!(
            project.audio_file(audio).is_some(),
            "still played by the copy"
        );
        project.remove_clip(copy);
        assert!(project.audio_file(audio).is_none());

        let (mut project, track, audio, _) = audio_song();
        project.remove_track(track);
        assert!(project.audio_file(audio).is_none());
        // Ids stay unique after loading.
        let (mut loaded, _, audio, _) = audio_song();
        loaded.next_id = 1;
        loaded.repair_id_counter();
        assert!(loaded.allocate_id() > audio.0);
    }

    #[test]
    fn audio_clips_move_only_between_audio_tracks() {
        let (mut project, track, _, clip) = audio_song();
        let other = project.add_audio_track("Audio 2");
        let piano = project.first_pattern().unwrap().0;
        assert!(
            !project.move_clip_to_track(clip, piano),
            "not onto an instrument"
        );
        assert!(project.move_clip_to_track(clip, other));
        assert_eq!(project.clip(clip).unwrap().0, other);
        assert!(
            project.track(track).unwrap().clips.is_empty(),
            "it left its old track"
        );
        let pattern_clip = project.track(piano).unwrap().clips[0].id;
        assert!(
            !project.move_clip_to_track(pattern_clip, track),
            "patterns stay home"
        );
    }

    #[test]
    fn a_tempo_change_keeps_audio_clips_in_seconds() {
        let (mut project, _, _, clip) = audio_song();
        let pattern_clip = project.tracks[0].clips[0];
        project.set_tempo(Ticks::ZERO, 60.0);
        let (_, c) = project.clip(clip).map(|(t, c)| (t, *c)).unwrap();
        assert_eq!(c.start, Ticks::WHOLE, "the start stays on its beat");
        assert_eq!(c.length, Ticks::quarters(10), "ten seconds at 60 BPM");
        assert!((project.seconds_between(c.start, c.end()) - 10.0).abs() < 1e-9);
        assert_eq!(
            project.tracks[0].clips[0], pattern_clip,
            "patterns follow the beat"
        );
    }
}

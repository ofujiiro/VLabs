// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound library's side of the song (decision 0023): playing a sound
//! once to hear it, and SoundFont files as instruments.

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use daw_dsp::PresetInfo;
use daw_dsp::audio::AudioBuffer;
use daw_model::{Instrument, SoundSource, Ticks, TrackId};

use super::Session;
use super::arrange::PIANO_BARS;
use crate::text;

/// The largest file the library plays to be heard (a longer one can still
/// be dragged onto the song).
const MAX_PREVIEW_BYTES: u64 = 256 * 1024 * 1024;

/// The library's preview: the sound playing, or being read to play.
#[derive(Default)]
pub(super) struct Preview {
    playing: Option<PathBuf>,
    loading: Option<(PathBuf, mpsc::Receiver<Result<AudioBuffer, String>>)>,
}

impl Session {
    /// Plays a sound file once, over the song, to hear it (it is read in
    /// the background first). Any sound playing this way stops.
    pub fn preview_file(&mut self, path: &Path) {
        self.stop_preview();
        let (send, ready) = mpsc::channel();
        let rate = self.engine.config().sample_rate;
        let file = path.to_path_buf();
        let _ = std::thread::Builder::new()
            .name("preview".to_owned())
            .spawn(move || {
                let too_large = std::fs::metadata(&file).is_ok_and(|m| m.len() > MAX_PREVIEW_BYTES);
                let result = if too_large {
                    Err(text::PREVIEW_TOO_LARGE.to_owned())
                } else {
                    daw_formats::audio::read_file(&file)
                        .and_then(|bytes| daw_formats::audio::decode(&bytes))
                        .map(|decoded| {
                            AudioBuffer::new(decoded.sample_rate, decoded.channels).resampled(rate)
                        })
                        .map_err(|e| e.to_string())
                };
                let _ = send.send(result);
            });
        self.preview.loading = Some((path.to_path_buf(), ready));
    }

    /// Stops the sound the library is playing.
    pub fn stop_preview(&mut self) {
        self.preview.loading = None;
        if self.preview.playing.take().is_some() {
            self.engine.preview(None);
        }
    }

    /// The sound the library is playing (or reading to play), if any.
    pub fn previewing(&self) -> Option<&Path> {
        if let Some((path, _)) = &self.preview.loading {
            return Some(path);
        }
        self.preview
            .playing
            .as_deref()
            .filter(|_| self.engine.is_previewing())
    }

    /// Passes a sound that has been read on to the engine; notices when it
    /// has finished.
    pub(super) fn poll_preview(&mut self) {
        if let Some((path, ready)) = &self.preview.loading {
            match ready.try_recv() {
                Ok(Ok(sound)) => {
                    let path = path.clone();
                    self.preview.loading = None;
                    self.engine.preview(Some(Arc::new(sound)));
                    self.preview.playing = Some(path);
                }
                Ok(Err(why)) => {
                    let message = text::cannot_preview(&file_name(path), &why);
                    self.preview.loading = None;
                    self.set_status(&message);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.preview.loading = None,
            }
        }
        if self.preview.playing.is_some() && !self.engine.is_previewing() {
            self.preview.playing = None;
        }
    }

    /// Whether a preview is being read or played (to draw again soon).
    pub fn is_previewing(&self) -> bool {
        self.preview.loading.is_some() || self.preview.playing.is_some()
    }

    // ----- SoundFonts

    /// Adds a track playing a SoundFont file (its first preset) with an
    /// empty four-bar pattern at the start of the song, and puts it in
    /// focus. The song refers to the file where it is.
    pub fn add_soundfont_track(&mut self, file: &Path) -> TrackId {
        let source = SoundSource::File(file.to_string_lossy().into_owned());
        let (bank, program) = self
            .engine
            .soundfont_presets(&source)
            .ok()
            .and_then(|presets| presets.first().map(|p| (p.bank, p.program)))
            .unwrap_or((0, 0));
        let title = file.file_stem().map_or_else(
            || text::SOUNDFONT.to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let name = self.unique_track_name(&title);
        let track = self.doc.edit(text::EDIT_ADD_SOUNDFONT, |p| {
            let track = p.add_track(
                &name,
                Instrument::SoundFont {
                    source,
                    bank,
                    program,
                },
            );
            let length = p.meter.bar_start(PIANO_BARS);
            let pattern = p.add_pattern_for(track, &text::pattern_name(1), length);
            p.add_clip(track, Ticks::ZERO, length, pattern);
            track
        });
        self.focus_track(track);
        track
    }

    /// The SoundFont file a track plays, if it plays one chosen by the
    /// person (not the bundled piano).
    pub fn soundfont_file(&self, track: TrackId) -> Option<&str> {
        match &self.project().track(track)?.instrument {
            Instrument::SoundFont {
                source: SoundSource::File(path),
                ..
            } => Some(path),
            _ => None,
        }
    }

    /// The presets of the SoundFont file a track plays (none if it cannot
    /// be read).
    pub fn soundfont_presets(&mut self, track: TrackId) -> Vec<PresetInfo> {
        let Some(path) = self.soundfont_file(track) else {
            return Vec::new();
        };
        let source = SoundSource::File(path.to_owned());
        self.engine.soundfont_presets(&source).unwrap_or_default()
    }

    /// The preset (bank and program) a SoundFont track plays.
    pub fn soundfont_preset(&self, track: TrackId) -> Option<(u16, u8)> {
        match &self.project().track(track)?.instrument {
            Instrument::SoundFont { bank, program, .. } => Some((*bank, *program)),
            _ => None,
        }
    }

    /// Changes the preset a SoundFont track plays (undoable).
    pub fn set_soundfont_preset(&mut self, track: TrackId, bank: u16, program: u8) {
        if self
            .soundfont_preset(track)
            .is_none_or(|current| current == (bank, program))
        {
            return;
        }
        self.doc.edit(text::EDIT_SOUNDFONT_PRESET, |p| {
            if let Some(t) = p.track_mut(track)
                && let Instrument::SoundFont {
                    bank: b,
                    program: n,
                    ..
                } = &mut t.instrument
            {
                *b = bank;
                *n = program;
            }
        });
    }
}

/// A file's name, for messages.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

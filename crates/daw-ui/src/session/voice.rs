// SPDX-License-Identifier: GPL-3.0-or-later

//! Singing voices (decision 0027): adding a voice track, the lyrics of its
//! notes (typed one at a time or as a phrase that goes to the notes in
//! turn, every change undoable), whether its voicebank has each lyric, and
//! whether its singing is still being rendered.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_dsp::voice::VoiceBank;
use daw_model::lyrics::{DEFAULT_LYRIC, alias_candidates, is_rest};
use daw_model::{Instrument, NoteId, Ticks, TrackId};

use super::Session;
use super::arrange::PIANO_BARS;
use crate::text;

/// The voicebanks the piano roll checks lyrics against, by name, read the
/// first time (their `oto.ini` only: no sound is read).
#[derive(Default)]
pub(crate) struct VoicebankCache {
    root: Option<PathBuf>,
    banks: HashMap<String, Option<Arc<VoiceBank>>>,
}

impl std::fmt::Debug for VoicebankCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoicebankCache")
            .field("root", &self.root)
            .field("banks", &self.banks.len())
            .finish()
    }
}

impl VoicebankCache {
    fn bank(&mut self, name: &str) -> Option<Arc<VoiceBank>> {
        let root = self.root.as_ref()?;
        self.banks
            .entry(name.to_owned())
            .or_insert_with(|| {
                daw_formats::utau::read_voicebank(&root.join(name))
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }
}

impl Session {
    /// Where the installed voicebanks are (the program's own folder).
    pub fn set_voicebanks_folder(&mut self, root: Option<&Path>) {
        self.voicebanks = VoicebankCache {
            root: root.map(Path::to_path_buf),
            banks: HashMap::new(),
        };
    }

    /// Forgets what is known of a voicebank (it was added again, or
    /// removed).
    pub fn forget_voicebank(&mut self, name: &str) {
        self.voicebanks.banks.remove(name);
    }

    /// Adds a track sung by the voicebank in the folder `voicebank` (shown as
    /// `name`), with an empty four-bar pattern at the start of the song, and
    /// puts it in focus.
    pub fn add_voice_track(&mut self, voicebank: &str, name: &str) -> TrackId {
        let track_name = self.unique_track_name(name);
        let voicebank = voicebank.to_owned();
        let track = self.doc.edit(text::EDIT_ADD_VOICE, |p| {
            let track = p.add_track(&track_name, Instrument::Voice { voicebank });
            let length = p.meter.bar_start(PIANO_BARS);
            let pattern = p.add_pattern_for(track, &text::pattern_name(1), length);
            p.add_clip(track, Ticks::ZERO, length, pattern);
            track
        });
        self.focus_track(track);
        track
    }

    /// The voicebank of the track in focus, if it is a voice.
    pub fn voicebank(&self) -> Option<&str> {
        self.project()
            .track(self.track)
            .and_then(|t| t.instrument.voicebank())
    }

    /// Whether the open pattern is sung.
    pub fn is_voice_track(&self) -> bool {
        self.voicebank().is_some()
    }

    /// What a note of the open pattern sings: its own lyric, or "a".
    pub fn lyric(&self, note: NoteId) -> &str {
        self.pattern().lyric(note).unwrap_or(DEFAULT_LYRIC)
    }

    /// Gives lyrics to the notes of the open pattern from `note` on: the
    /// words of `phrase` (separated by spaces) go to it and to the notes
    /// after it, in order (by start, then from the highest key, as the
    /// melody is usually on top). One undo step. Returns the note after the
    /// last one given a word, to go on typing there.
    pub fn sing_from(&mut self, note: NoteId, phrase: &str) -> Option<NoteId> {
        let mut order: Vec<(Ticks, u8, NoteId)> = self
            .pattern()
            .notes
            .iter()
            .map(|n| (n.start, n.key, n.id))
            .collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let first = order.iter().position(|(_, _, id)| *id == note)?;
        let words: Vec<&str> = phrase.split_whitespace().collect();
        let lyrics: Vec<(NoteId, String)> = if words.is_empty() {
            // Nothing typed: the note sings the default again.
            vec![(note, String::new())]
        } else {
            order[first..]
                .iter()
                .zip(&words)
                .map(|((_, _, id), word)| (*id, (*word).to_owned()))
                .collect()
        };
        let given = lyrics.len();
        let pattern = self.pattern;
        self.doc
            .edit(text::EDIT_LYRICS, |p| p.set_lyrics(pattern, &lyrics));
        order.get(first + given).map(|(_, _, id)| *id)
    }

    /// Whether the voicebank of the track in focus has a recording for a
    /// note's lyric, sung after the note before it (true when there is no
    /// voicebank to ask, or the lyric is a rest).
    pub fn lyric_found(&mut self, note: NoteId) -> bool {
        let Some(name) = self.voicebank().map(str::to_owned) else {
            return true;
        };
        let pattern = self.pattern();
        let Some(this) = pattern.note(note).copied() else {
            return true;
        };
        let lyric = self.lyric(note).to_owned();
        if is_rest(&lyric) {
            return true;
        }
        // The note sounding just before (touching it), for VCV.
        let previous = pattern
            .notes
            .iter()
            .filter(|n| {
                n.id != note
                    && n.end() <= this.start
                    && this.start - n.end() < Ticks::note_value(1, 16)
            })
            .max_by_key(|n| n.end())
            .map(|n| self.lyric(n.id).to_owned());
        let Some(bank) = self.voicebanks.bank(&name) else {
            return true;
        };
        bank.find(&alias_candidates(&lyric, previous.as_deref()), this.key)
            .is_some()
    }

    /// Whether voice tracks' singing is still being rendered (silent where
    /// it is not ready).
    pub fn is_rendering_voices(&self) -> bool {
        self.engine.is_rendering_voices()
    }
}

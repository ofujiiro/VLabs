// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing in the key (decision 0018): the song's key where the open
//! pattern is, keeping notes in it, chord stamps, inversions, transposing
//! by intervals, the pattern's swing and its length in steps.

use std::collections::BTreeMap;

use daw_model::{ChordKind, ChordShape, MAX_KEY, Note, NoteId, Scale, Swing, Ticks, invert_once};

use super::{Session, grow_to_fit, set_length};
use crate::text;

/// What a click on an empty spot of the piano roll places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draw {
    /// One note.
    Note,
    /// A whole chord, on the clicked note.
    Chord(ChordShape),
}

impl Session {
    /// The key in effect where the open pattern is placed in the song.
    pub fn pattern_key(&self) -> Option<Scale> {
        let project = self.project();
        let bar = project
            .meter
            .position(self.placement().start.max(Ticks::ZERO))
            .bar;
        project.key.at_bar(bar)
    }

    /// Whether notes are kept in the key right now: asked for, and the
    /// pattern has a key.
    pub fn keeps_in_key(&self) -> bool {
        self.keep_in_key && self.pattern_key().is_some()
    }

    /// Where a note put on `key` lands: the nearest note of the key when
    /// notes are kept in it.
    pub fn placed_key(&self, key: u8) -> u8 {
        match self.pattern_key() {
            Some(scale) if self.keep_in_key => scale.nearest(key),
            _ => key,
        }
    }

    /// How a group of notes moves when the one at `grabbed` is dragged to
    /// the row of `pointer`: each key's new key, or `None` if one would
    /// leave the keyboard. Kept in the key, every note moves by the same
    /// number of steps of the scale, so a chord of the key stays one;
    /// otherwise by semitones.
    pub fn moved_keys(&self, keys: &[u8], grabbed: u8, pointer: u8) -> Option<Vec<u8>> {
        match self.pattern_key() {
            Some(scale) if self.keep_in_key => {
                let steps = scale.index(pointer) - scale.index(grabbed);
                keys.iter()
                    .map(|&k| scale.key_at_index(scale.index(k) + steps))
                    .collect()
            }
            _ => {
                let semitones = i32::from(pointer) - i32::from(grabbed);
                keys.iter()
                    .map(|&k| {
                        u8::try_from(i32::from(k) + semitones)
                            .ok()
                            .filter(|&k| k <= MAX_KEY)
                    })
                    .collect()
            }
        }
    }

    // ----- Chord stamps

    /// The keys a click on `root` places now: one note, or the chosen
    /// chord (in the key where the pattern is, in the chosen inversion).
    pub fn chord_keys(&self, root: u8) -> Vec<u8> {
        let root = self.placed_key(root);
        match self.draw {
            Draw::Note => vec![root],
            Draw::Chord(shape) => shape.keys(root, self.pattern_key(), self.chord_inversion),
        }
    }

    /// The name of the chord a click on `root` places ("D minor"), if a
    /// chord is chosen.
    pub fn chord_name(&self, root: u8) -> Option<String> {
        let Draw::Chord(shape) = self.draw else {
            return None;
        };
        let root = match (shape, self.pattern_key()) {
            (ChordShape::InKey(_), Some(scale)) => scale.nearest(self.placed_key(root)),
            _ => self.placed_key(root),
        };
        let keys = shape.keys(root, self.pattern_key(), 0);
        Some(match ChordKind::recognise(root, &keys) {
            Some(kind) => text::chord_name(root, kind),
            None => text::note_name(root),
        })
    }

    /// Places the current note or chord at `start` on `root`, as one undo
    /// step (or part of a gesture), selects it and plays it. Returns the
    /// new notes.
    pub fn add_chord(&mut self, start: Ticks, root: u8, velocity: f32) -> Vec<NoteId> {
        let keys = self.chord_keys(root);
        let length = self.note_length.max(Ticks(1));
        let notes: Vec<Note> = keys
            .iter()
            .map(|&key| Note {
                id: NoteId(0),
                start: start.max(Ticks::ZERO),
                length,
                key,
                velocity,
                pitch_offset: 0.0,
            })
            .collect();
        let (track, pattern) = (self.track, self.pattern);
        let end = start.max(Ticks::ZERO) + length;
        let label = if keys.len() > 1 {
            text::EDIT_ADD_CHORD
        } else {
            text::EDIT_ADD_NOTE
        };
        let ids = self.doc.edit(label, |p| {
            let ids = p.add_notes(pattern, &notes);
            grow_to_fit(p, track, pattern, end);
            ids
        });
        self.select_only(ids.iter().copied());
        if let Some(name) = self.chord_name(root) {
            self.set_status(text::placed(&name));
        }
        match keys.as_slice() {
            [key] => self.pointer_on(*key, velocity),
            _ => self.chord_on(&keys, velocity),
        }
        ids
    }

    /// Plays several notes while the mouse holds them (a chord just
    /// placed); [`Self::pointer_off`] releases them.
    pub fn chord_on(&mut self, keys: &[u8], velocity: f32) {
        self.pointer_off();
        for &key in keys {
            self.engine.note_on(self.track, key, velocity);
        }
        self.chord_held = keys.to_vec();
    }

    // ----- Inversions and intervals

    /// Inverts each chord of the selection (the selected notes that start
    /// together): up moves its lowest note above the others, down its
    /// highest below. One undo step.
    pub fn invert_selection(&mut self, up: bool) {
        let mut chords: BTreeMap<Ticks, Vec<Note>> = BTreeMap::new();
        for note in self.selected_notes() {
            chords.entry(note.start).or_default().push(note);
        }
        let mut changes: Vec<(NoteId, u8)> = Vec::new();
        for notes in chords.values().filter(|n| n.len() > 1) {
            let keys: Vec<u8> = notes.iter().map(|n| n.key).collect();
            if let Some(inverted) = invert_once(&keys, up) {
                changes.extend(
                    notes
                        .iter()
                        .zip(inverted)
                        .filter(|(n, key)| n.key != *key)
                        .map(|(n, key)| (n.id, key)),
                );
            }
        }
        if changes.is_empty() {
            self.set_status(text::NOTHING_TO_INVERT);
            return;
        }
        let ids: Vec<NoteId> = changes.iter().map(|(id, _)| *id).collect();
        let pattern = self.pattern;
        self.doc.edit(text::EDIT_INVERT, |p| {
            p.update_notes(pattern, &ids, |n| {
                if let Some((_, key)) = changes.iter().find(|(id, _)| *id == n.id) {
                    n.key = *key;
                }
            });
        });
    }

    /// Moves the selected notes by an interval (semitones; negative is
    /// down), as one undo step of its own, unless one would leave the
    /// keyboard.
    pub fn transpose_by(&mut self, semitones: i32) {
        let notes = self.selected_notes();
        let moved: Option<Vec<u8>> = notes
            .iter()
            .map(|n| {
                u8::try_from(i32::from(n.key) + semitones)
                    .ok()
                    .filter(|&k| k <= MAX_KEY)
            })
            .collect();
        self.apply_keys(&notes, moved);
    }

    /// Moves the selected notes by steps of the key where the pattern is
    /// (a note outside the key first goes to its nearest note of the key
    /// in that direction), as one undo step.
    pub fn transpose_in_key(&mut self, steps: i32) {
        let Some(scale) = self.pattern_key() else {
            return;
        };
        let notes = self.selected_notes();
        let moved: Option<Vec<u8>> = notes.iter().map(|n| scale.step(n.key, steps)).collect();
        self.apply_keys(&notes, moved);
    }

    /// The up and down arrows: a semitone, or a step of the key when notes
    /// are kept in it (merged with the steps before it into one undo step).
    pub fn step_selection(&mut self, up: bool) {
        let steps = if up { 1 } else { -1 };
        match self.pattern_key() {
            Some(scale) if self.keep_in_key => {
                let notes = self.selected_notes();
                let moved: Option<Vec<u8>> =
                    notes.iter().map(|n| scale.step(n.key, steps)).collect();
                let (Some(moved), false) = (moved, notes.is_empty()) else {
                    return;
                };
                let changes: Vec<(NoteId, u8)> = notes.iter().map(|n| n.id).zip(moved).collect();
                let ids: Vec<NoteId> = notes.iter().map(|n| n.id).collect();
                let pattern = self.pattern;
                self.doc
                    .edit_merged(text::EDIT_TRANSPOSE, super::MERGE_TRANSPOSE, |p| {
                        p.update_notes(pattern, &ids, |n| {
                            if let Some((_, key)) = changes.iter().find(|(id, _)| *id == n.id) {
                                n.key = *key;
                            }
                        });
                    });
            }
            _ => self.transpose(steps),
        }
    }

    fn apply_keys(&mut self, notes: &[Note], moved: Option<Vec<u8>>) {
        let Some(moved) = moved else {
            self.set_status(text::WOULD_LEAVE_KEYBOARD);
            return;
        };
        if notes.is_empty() {
            return;
        }
        let changes: Vec<(NoteId, u8)> = notes.iter().map(|n| n.id).zip(moved).collect();
        let ids: Vec<NoteId> = notes.iter().map(|n| n.id).collect();
        let pattern = self.pattern;
        self.doc.edit(text::EDIT_TRANSPOSE, |p| {
            p.update_notes(pattern, &ids, |n| {
                if let Some((_, key)) = changes.iter().find(|(id, _)| *id == n.id) {
                    n.key = *key;
                }
            });
        });
    }

    // ----- Feel and length

    /// The open pattern's swing.
    pub fn swing(&self) -> Swing {
        self.pattern().swing
    }

    /// Changes the open pattern's swing (one undo step, or part of a
    /// gesture while a slider is dragged).
    pub fn set_swing(&mut self, swing: Swing) {
        if swing == self.swing() {
            return;
        }
        let pattern = self.pattern;
        self.doc.edit(text::EDIT_SWING, |p| {
            if let Some(pattern) = p.pattern_mut(pattern) {
                pattern.swing = swing;
            }
        });
    }

    /// Makes the open pattern exactly `length` long (at least a 1/64 note),
    /// even if that is not whole bars, so patterns of different lengths can
    /// turn against each other (decision 0018). Clips that played it once
    /// follow; clips that repeat it keep their length. Notes after the end
    /// stay, silent, until the pattern grows again.
    pub fn set_pattern_length(&mut self, length: Ticks) {
        let length = length.max(Ticks::note_value(1, 64));
        if length == self.pattern().length {
            return;
        }
        let (track, pattern) = (self.track, self.pattern);
        let place = super::placement(self.project(), track, pattern);
        self.doc.edit(text::EDIT_PATTERN_LENGTH, |p| match place {
            Some(place) if length > place.offset => {
                set_length(p, track, pattern, place, length - place.offset);
            }
            _ => {
                if let Some(pattern) = p.pattern_mut(pattern) {
                    pattern.length = length;
                }
            }
        });
    }
}

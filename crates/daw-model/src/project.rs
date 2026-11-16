// SPDX-License-Identifier: GPL-3.0-or-later

//! The project: everything that makes up one song.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::song::{
    ChannelStrip, Clip, ClipContent, ClipId, Instrument, MAX_KEY, Note, NoteId, Pattern, PatternId,
    Track, TrackId,
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
            next_id: 1,
        }
    }

    /// The project a new user starts with: one piano track with an empty
    /// four-bar pattern, looping, at 120 BPM in 4/4.
    pub fn starter(pattern_name: &str, track_name: &str) -> Self {
        let mut project = Self::empty();
        let bars = 4;
        let length = TimeSignature::FOUR_FOUR.bar_length() * bars;
        let pattern = project.add_pattern(pattern_name, length);
        let track = project.add_track(track_name, Instrument::builtin_piano());
        project.add_clip(track, Ticks::ZERO, length, pattern);
        project.loop_region = Region::new(Ticks::ZERO, length);
        project
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
        }
        self.next_id = self.next_id.max(highest + 1);
    }

    // ----- Patterns

    /// Adds an empty pattern.
    pub fn add_pattern(&mut self, name: &str, length: Ticks) -> PatternId {
        let id = PatternId(self.allocate_id());
        self.patterns.push(Arc::new(Pattern {
            id,
            name: name.to_owned(),
            length,
            notes: Vec::new(),
        }));
        id
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
        if let Some(pattern) = self.pattern_mut(pattern) {
            pattern.notes.retain(|n| !ids.contains(&n.id));
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
        if let Some(pattern) = self.pattern_mut(pattern) {
            for note in pattern.notes.iter_mut().filter(|n| ids.contains(&n.id)) {
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
        }));
        id
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

    /// The first track and the pattern of its first clip: what the editor
    /// shows when a project opens.
    pub fn first_pattern(&self) -> Option<(TrackId, PatternId)> {
        self.tracks.iter().find_map(|track| {
            track
                .clips
                .iter()
                .map(|clip| match clip.content {
                    ClipContent::Pattern { pattern, .. } => (track.id, pattern),
                })
                .next()
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
}

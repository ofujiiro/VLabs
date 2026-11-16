// SPDX-License-Identifier: GPL-3.0-or-later

//! The project: everything that makes up one song.

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::mixer::{ChainPlace, Effect, EffectKind, LimiterSettings, MasterBus};
use crate::song::{
    ChannelStrip, Clip, ClipContent, ClipId, EffectId, Instrument, MAX_KEY, Note, NoteId, Pattern,
    PatternId, Track, TrackId, default_step,
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
        }));
        id
    }

    /// Removes a track with its clips and its patterns.
    pub fn remove_track(&mut self, id: TrackId) {
        self.tracks.retain(|t| t.id != id);
        self.patterns.retain(|p| p.track != Some(id));
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

    /// Removes a clip.
    pub fn remove_clip(&mut self, id: ClipId) {
        let Some((track, _)) = self.clip(id) else {
            return;
        };
        if let Some(track) = self.track_mut(track) {
            track.clips.retain(|c| c.id != id);
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

    /// Removes an effect.
    pub fn remove_effect(&mut self, id: EffectId) {
        if let Some((place, _)) = self.effect_place(id)
            && let Some(chain) = self.chain_mut(place)
        {
            chain.retain(|e| e.id != id);
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
}

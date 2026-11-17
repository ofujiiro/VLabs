// SPDX-License-Identifier: GPL-3.0-or-later

//! The song as a whole: which view is open, tracks, patterns, clips on the
//! timeline, and drum hits on the step grid. Every change is one undo step
//! (a drag in progress merges into one through the document's gestures).

use daw_model::{
    ACOUSTIC_KIT, Clip, ClipId, Instrument, Note, NoteId, PatternId, Project, Ticks, TrackId,
};

use super::{Session, View, first_focus, placement};
use crate::text;

/// Bars a new piano pattern lasts.
pub(super) const PIANO_BARS: u32 = 4;
/// The shortest song the timeline shows, in bars.
const MIN_SONG_BARS: u32 = 4;

impl Session {
    // ----- Views

    /// What the large surface shows.
    pub fn view(&self) -> View {
        self.view
    }

    /// Shows the whole song.
    pub fn show_song(&mut self) {
        if self.view == View::Song {
            return;
        }
        self.view = View::Song;
        self.selection.clear();
        self.sync_engine();
    }

    /// Opens a pattern of a track in the editor.
    pub fn open_pattern(&mut self, track: TrackId, pattern: PatternId) {
        let project = self.project();
        if project.track(track).is_none() || project.pattern(pattern).is_none() {
            return;
        }
        if track != self.track {
            self.release_keys();
        }
        if (track, pattern) != (self.track, self.pattern) {
            self.selection.clear();
            self.paste_at = None;
        }
        self.track = track;
        self.pattern = pattern;
        self.current.insert(track, pattern);
        self.view = View::Pattern;
        self.sync_engine();
    }

    /// Opens the pattern a track last showed (making one if it has none).
    pub fn open_track(&mut self, track: TrackId) {
        let pattern = match self.current_pattern(track) {
            Some(pattern) => pattern,
            None => self.make_pattern(track),
        };
        self.open_pattern(track, pattern);
    }

    /// Puts a track in focus without changing the view (in the song view):
    /// the computer keyboard plays it and Record records into it.
    pub fn focus_track(&mut self, track: TrackId) {
        if track == self.track || self.project().track(track).is_none() {
            return;
        }
        let pattern = match self.current_pattern(track) {
            Some(pattern) => pattern,
            None => self.make_pattern(track),
        };
        self.release_keys();
        self.track = track;
        self.pattern = pattern;
        self.selection.clear();
        self.sync_engine();
    }

    /// The pattern a track shows when it is opened: the one last opened,
    /// else the one its first clip plays, else its first.
    pub fn current_pattern(&self, track: TrackId) -> Option<PatternId> {
        let project = self.project();
        let owned = |id: PatternId| {
            project
                .pattern(id)
                .is_some_and(|p| p.track.is_none_or(|t| t == track))
        };
        self.current
            .get(&track)
            .copied()
            .filter(|&id| owned(id))
            .or_else(|| {
                project
                    .track(track)?
                    .clips
                    .iter()
                    .find_map(Clip::pattern)
                    .filter(|&id| owned(id))
            })
            .or_else(|| project.track_patterns(track).next().map(|p| p.id))
    }

    /// Whether the track in focus plays drums.
    pub fn is_drum_track(&self) -> bool {
        self.is_drums(self.track)
    }

    /// Whether a track plays drums.
    pub fn is_drums(&self, track: TrackId) -> bool {
        self.project()
            .track(track)
            .is_some_and(|t| t.instrument.is_drums())
    }

    /// Keeps the focus on things that exist (after undo, for instance).
    pub(super) fn keep_focus_valid(&mut self) {
        if let Some(clip) = self.selected_clip
            && self.project().clip(clip).is_none()
        {
            self.selected_clip = None;
        }
        let project = self.project();
        let track_ok = project.track(self.track).is_some();
        let pattern_ok = project
            .pattern(self.pattern)
            .is_some_and(|p| p.track.is_none_or(|t| t == self.track));
        if track_ok && pattern_ok {
            return;
        }
        let focus = if track_ok {
            self.current_pattern(self.track).map(|p| (self.track, p))
        } else {
            None
        };
        let (track, pattern) = focus.unwrap_or_else(|| first_focus(self.project()));
        if track != self.track {
            self.release_keys();
        }
        self.track = track;
        self.pattern = pattern;
        if self.project().pattern(pattern).is_none() {
            self.view = View::Song;
        }
    }

    // ----- Tracks

    /// Adds a drum track at the bottom, with an empty one-bar beat repeated
    /// over the whole song, and puts it in focus.
    pub fn add_drum_track(&mut self) -> TrackId {
        let name = self.unique_track_name(text::DRUMS_TRACK);
        let song = self.song_length();
        let track = self.doc.edit(text::EDIT_ADD_TRACK, |p| {
            let track = p.add_track(&name, Instrument::drums(ACOUSTIC_KIT));
            let bar = p.meter.initial().bar_length();
            let pattern = p.add_pattern_for(track, &text::beat_name(1), bar);
            p.add_clip(track, Ticks::ZERO, song, pattern);
            track
        });
        self.focus_track(track);
        track
    }

    /// Adds a piano track at the bottom with an empty four-bar pattern at
    /// the start of the song, and puts it in focus.
    pub fn add_piano_track(&mut self) -> TrackId {
        let name = self.unique_track_name(text::FIRST_TRACK);
        let track = self.doc.edit(text::EDIT_ADD_TRACK, |p| {
            let track = p.add_track(&name, Instrument::builtin_piano());
            let length = p.meter.bar_start(PIANO_BARS);
            let pattern = p.add_pattern_for(track, &text::pattern_name(1), length);
            p.add_clip(track, Ticks::ZERO, length, pattern);
            track
        });
        self.focus_track(track);
        track
    }

    /// `base`, or `base 2`, `base 3`… whichever no track uses yet.
    pub(super) fn unique_track_name(&self, base: &str) -> String {
        let taken = |name: &str| self.project().tracks.iter().any(|t| t.name == name);
        if !taken(base) {
            return base.to_owned();
        }
        (2..10_000)
            .map(|n| text::numbered(base, n))
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_owned())
    }

    /// Renames a track (an empty name is ignored).
    pub fn rename_track(&mut self, track: TrackId, name: &str) {
        let name = name.trim();
        let current = self.project().track(track).map(|t| t.name.as_str());
        if name.is_empty() || current.is_none_or(|n| n == name) {
            return;
        }
        self.doc.edit(text::EDIT_RENAME_TRACK, |p| {
            if let Some(t) = p.track_mut(track) {
                name.clone_into(&mut t.name);
            }
        });
    }

    /// Removes a track with its clips and patterns (never the last one).
    pub fn remove_track(&mut self, track: TrackId) {
        if self.project().tracks.len() <= 1 || self.project().track(track).is_none() {
            return;
        }
        // Undoing brings its plugin back with the settings it has now.
        self.capture_plugin_states();
        self.doc
            .edit(text::EDIT_REMOVE_TRACK, |p| p.remove_track(track));
        self.current.remove(&track);
        self.keep_focus_valid();
        self.sync_engine();
    }

    /// Moves a track up (negative) or down the list.
    pub fn move_track(&mut self, track: TrackId, by: isize) {
        let Some(from) = self.project().tracks.iter().position(|t| t.id == track) else {
            return;
        };
        let last = self.project().tracks.len() - 1;
        let to = from.saturating_add_signed(by).min(last);
        if to == from {
            return;
        }
        self.doc.edit(text::EDIT_MOVE_TRACK, |p| {
            let moved = p.tracks.remove(from);
            p.tracks.insert(to, moved);
        });
    }

    /// Changes the kit a drum track plays (its pad settings stay).
    pub fn set_kit(&mut self, track: TrackId, kit: &str) {
        let current = self
            .project()
            .track(track)
            .and_then(|t| match &t.instrument {
                Instrument::Drums { kit, .. } => Some(kit.as_str()),
                Instrument::SoundFont { .. } | Instrument::Plugin(_) | Instrument::Audio => None,
            });
        if current.is_none_or(|k| k == kit) {
            return;
        }
        self.doc.edit(text::EDIT_KIT, |p| {
            if let Some(t) = p.track_mut(track)
                && let Instrument::Drums { kit: k, .. } = &mut t.instrument
            {
                kit.clone_into(k);
            }
        });
    }

    /// The kit a drum track plays.
    pub fn kit(&self, track: TrackId) -> Option<&str> {
        match &self.project().track(track)?.instrument {
            Instrument::Drums { kit, .. } => Some(kit),
            Instrument::SoundFont { .. } | Instrument::Plugin(_) | Instrument::Audio => None,
        }
    }

    // ----- Patterns

    /// Makes an empty pattern on the track in focus and opens it. It is not
    /// in the song until it is placed on the timeline.
    pub fn new_pattern(&mut self) -> PatternId {
        let track = self.track;
        let pattern = self.make_pattern(track);
        self.open_pattern(track, pattern);
        pattern
    }

    /// Adds an empty pattern to a track: one bar of drums, or four bars.
    fn make_pattern(&mut self, track: TrackId) -> PatternId {
        let name = self.next_pattern_name(track);
        let bars = if self.is_drums(track) { 1 } else { PIANO_BARS };
        self.doc.edit(text::EDIT_NEW_PATTERN, |p| {
            let length = p.meter.bar_start(bars);
            p.add_pattern_for(track, &name, length)
        })
    }

    /// Copies the open pattern, notes and all, and opens the copy.
    pub fn duplicate_pattern(&mut self) -> Option<PatternId> {
        let (track, original) = (self.track, self.pattern);
        let name = self.next_pattern_name(track);
        let copy = self.doc.edit(text::EDIT_DUPLICATE_PATTERN, |p| {
            let copy = p.duplicate_pattern(original, &name)?;
            if let Some(pattern) = p.pattern_mut(copy) {
                pattern.track = Some(track);
            }
            Some(copy)
        })?;
        self.open_pattern(track, copy);
        Some(copy)
    }

    /// "Beat 2", "Pattern 3"…: the first number no pattern of the track has.
    fn next_pattern_name(&self, track: TrackId) -> String {
        let drums = self.is_drums(track);
        let name = |n| {
            if drums {
                text::beat_name(n)
            } else {
                text::pattern_name(n)
            }
        };
        let taken = |candidate: &str| {
            self.project()
                .track_patterns(track)
                .any(|p| p.name == candidate)
        };
        (1..10_000)
            .map(name)
            .find(|candidate| !taken(candidate))
            .unwrap_or_default()
    }

    /// Renames a pattern (an empty name is ignored).
    pub fn rename_pattern(&mut self, pattern: PatternId, name: &str) {
        let name = name.trim();
        let current = self.project().pattern(pattern).map(|p| p.name.as_str());
        if name.is_empty() || current.is_none_or(|n| n == name) {
            return;
        }
        self.doc.edit(text::EDIT_RENAME_PATTERN, |p| {
            if let Some(pattern) = p.pattern_mut(pattern) {
                name.clone_into(&mut pattern.name);
            }
        });
    }

    /// Removes a pattern and every clip that plays it. A track always keeps
    /// one pattern: removing its last leaves an empty one in its place.
    pub fn remove_pattern(&mut self, pattern: PatternId) {
        let Some(track) = self.project().pattern(pattern).and_then(|p| p.track) else {
            return;
        };
        let last = self.project().track_patterns(track).count() <= 1;
        let replacement = last.then(|| self.next_pattern_name(track));
        let bars = if self.is_drums(track) { 1 } else { PIANO_BARS };
        let fresh = self.doc.edit(text::EDIT_REMOVE_PATTERN, |p| {
            p.remove_pattern(pattern);
            replacement.map(|name| {
                let length = p.meter.bar_start(bars);
                p.add_pattern_for(track, &name, length)
            })
        });
        if self.pattern == pattern {
            let next = fresh.or_else(|| self.current_pattern(track));
            if let Some(next) = next {
                self.current.insert(track, next);
                if self.view == View::Pattern {
                    self.open_pattern(track, next);
                } else {
                    self.pattern = next;
                }
            }
        }
        self.keep_focus_valid();
        self.sync_engine();
    }

    /// The step size of the open pattern's grid (drums).
    pub fn step(&self) -> Ticks {
        self.pattern().step.max(Ticks(1))
    }

    /// Changes the step size of the open pattern's grid.
    pub fn set_step(&mut self, step: Ticks) {
        if step <= Ticks::ZERO || step == self.pattern().step {
            return;
        }
        let pattern = self.pattern;
        self.doc.edit(text::EDIT_STEPS, |p| {
            if let Some(pattern) = p.pattern_mut(pattern) {
                pattern.step = step;
            }
        });
    }

    // ----- Drum hits

    /// Adds a drum hit one step long at `at`; selects nothing. Returns its id.
    pub fn add_hit(&mut self, key: u8, at: Ticks, velocity: f32) -> Option<NoteId> {
        let note = Note {
            id: NoteId(0),
            start: at.max(Ticks::ZERO),
            length: self.step(),
            key,
            velocity: velocity.clamp(0.01, 1.0),
            pitch_offset: 0.0,
        };
        let pattern = self.pattern;
        self.doc
            .edit(text::EDIT_ADD_HIT, |p| p.add_note(pattern, note))
    }

    /// Removes notes of the open pattern.
    pub fn remove_notes(&mut self, ids: &[NoteId]) {
        if ids.is_empty() {
            return;
        }
        let pattern = self.pattern;
        self.doc
            .edit(text::EDIT_REMOVE_HIT, |p| p.remove_notes(pattern, ids));
        self.selection.retain(|id| !ids.contains(id));
    }

    /// Sets how hard notes of the open pattern play.
    pub fn set_velocities(&mut self, ids: &[NoteId], velocity: f32) {
        if ids.is_empty() {
            return;
        }
        let velocity = velocity.clamp(0.01, 1.0);
        let pattern = self.pattern;
        self.doc.edit(text::EDIT_VELOCITY, |p| {
            p.update_notes(pattern, ids, |n| n.velocity = velocity);
        });
    }

    // ----- Clips

    /// Places a track's current pattern on the timeline at `start`, as long
    /// as the pattern (shortened to stop at the next clip). Returns the new
    /// clip, now selected.
    pub fn place_pattern(&mut self, track: TrackId, start: Ticks) -> Option<ClipId> {
        let start = start.max(Ticks::ZERO);
        let project = self.project();
        let clips = &project.track(track)?.clips;
        if clips.iter().any(|c| c.start <= start && start < c.end()) {
            return None;
        }
        let pattern = self.current_pattern(track)?;
        let next = clips.iter().map(|c| c.start).filter(|&s| s > start).min();
        let mut length = self.project().pattern(pattern)?.length;
        if let Some(next) = next {
            length = length.min(next - start);
        }
        let clip = self.doc.edit(text::EDIT_PLACE_PATTERN, |p| {
            p.add_clip(track, start, length, pattern)
        })?;
        self.current.insert(track, pattern);
        self.selected_clip = Some(clip);
        Some(clip)
    }

    /// Moves a clip to start at `start`.
    pub fn move_clip(&mut self, clip: ClipId, start: Ticks) {
        if self
            .project()
            .clip(clip)
            .is_none_or(|(_, c)| c.start == start)
        {
            return;
        }
        self.doc.edit(text::EDIT_MOVE_CLIP, |p| {
            p.update_clip(clip, |c| c.start = start.max(Ticks::ZERO));
        });
    }

    /// Makes a clip last `length` (repeating its pattern to fill it).
    pub fn resize_clip(&mut self, clip: ClipId, length: Ticks) {
        let length = length.max(Ticks(1));
        if self
            .project()
            .clip(clip)
            .is_none_or(|(_, c)| c.length == length)
        {
            return;
        }
        self.doc.edit(text::EDIT_RESIZE_CLIP, |p| {
            p.update_clip(clip, |c| c.length = length);
        });
    }

    /// Removes a clip from the timeline (its pattern stays, to place again).
    pub fn remove_clip(&mut self, clip: ClipId) {
        if self.project().clip(clip).is_none() {
            return;
        }
        self.doc
            .edit(text::EDIT_REMOVE_CLIP, |p| p.remove_clip(clip));
        if self.selected_clip == Some(clip) {
            self.selected_clip = None;
        }
    }

    /// Places a copy of a clip right after it; returns the copy, selected.
    pub fn duplicate_clip(&mut self, clip: ClipId) -> Option<ClipId> {
        let (track, original) = self.project().clip(clip).map(|(t, c)| (t, *c))?;
        let pattern = original.pattern()?;
        let copy = self.doc.edit(text::EDIT_DUPLICATE_CLIP, |p| {
            let copy = p.add_clip(track, original.end(), original.length, pattern)?;
            p.update_clip(copy, |c| c.content = original.content);
            Some(copy)
        })?;
        self.selected_clip = Some(copy);
        Some(copy)
    }

    /// The clip selected on the timeline.
    pub fn selected_clip(&self) -> Option<ClipId> {
        self.selected_clip
    }

    /// Selects a clip (or none); its track comes into focus.
    pub fn select_clip(&mut self, clip: Option<ClipId>) {
        self.selected_clip = clip;
        let Some((track, pattern)) = clip
            .and_then(|id| self.project().clip(id))
            .and_then(|(track, c)| c.pattern().map(|p| (track, p)))
        else {
            return;
        };
        self.focus_track(track);
        self.current.insert(track, pattern);
        self.pattern = pattern;
    }

    /// Moves the selected clip by whole bars (negative is earlier).
    pub fn nudge_clip(&mut self, bars: i64) {
        let Some((_, clip)) = self.selected_clip.and_then(|id| self.project().clip(id)) else {
            return;
        };
        let (id, start) = (clip.id, clip.start);
        let meter = &self.project().meter;
        let bar = meter.position(start).bar;
        let target = i64::from(bar) + bars;
        let Ok(target) = u32::try_from(target) else {
            return;
        };
        let start = meter.bar_start(target);
        self.doc
            .edit_merged(text::EDIT_MOVE_CLIP, MERGE_CLIP_NUDGE, |p| {
                p.update_clip(id, |c| c.start = start);
            });
    }

    /// How long the song is, in whole bars, but never less than four bars.
    pub fn song_length(&self) -> Ticks {
        let project = self.project();
        project.meter.bar_start(song_bars(project))
    }

    /// Where a pattern is placed: its clips, as (start, length).
    pub fn clips_of(&self, pattern: PatternId) -> Vec<(Ticks, Ticks)> {
        let Some(track) = self.project().pattern(pattern).and_then(|p| p.track) else {
            return Vec::new();
        };
        self.project().track(track).map_or_else(Vec::new, |t| {
            t.clips
                .iter()
                .filter(|c| c.pattern() == Some(pattern))
                .map(|c| (c.start, c.length))
                .collect()
        })
    }

    /// Whether the open pattern is placed somewhere in the song.
    pub fn is_placed(&self) -> bool {
        placement(self.project(), self.track, self.pattern).is_some()
    }
}

/// Joins repeated clip nudges into one undo step.
const MERGE_CLIP_NUDGE: u64 = 3;

/// The song's length in whole bars (at least four).
fn song_bars(project: &Project) -> u32 {
    let end = project.song_end();
    let at = project.meter.position(end);
    let partial = u32::from(at.beat > 0 || at.ticks > Ticks::ZERO);
    (at.bar + partial).max(MIN_SONG_BARS)
}

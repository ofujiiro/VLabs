// SPDX-License-Identifier: GPL-3.0-or-later

//! Selecting several tracks or clips in the song view, and moving them
//! together (decision 0032).

use daw_model::{ClipContent, ClipId, Ticks, TrackId};

use super::Session;
use crate::text;

/// How a click changes a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Only this one (a plain click).
    Only,
    /// This one in or out, the others staying (Ctrl+click).
    Toggle,
    /// Everything from the one picked last to this one (Shift+click).
    Range,
}

impl Session {
    // ----- Tracks

    /// The tracks selected in the song view's headers (at least the track
    /// in focus, once one was clicked), in the song's order.
    pub fn selected_tracks(&self) -> Vec<TrackId> {
        self.project()
            .tracks
            .iter()
            .map(|t| t.id)
            .filter(|id| self.selected_tracks.contains(id))
            .collect()
    }

    /// Whether a track is selected.
    pub fn is_track_selected(&self, track: TrackId) -> bool {
        self.selected_tracks.contains(&track)
    }

    /// Changes the track selection by a click on a track's header; the
    /// track clicked comes into focus.
    pub fn pick_track(&mut self, track: TrackId, pick: Pick) {
        if self.project().track(track).is_none() {
            return;
        }
        match pick {
            Pick::Only => self.selected_tracks = vec![track],
            Pick::Toggle => {
                if let Some(at) = self.selected_tracks.iter().position(|&t| t == track) {
                    self.selected_tracks.remove(at);
                } else {
                    self.selected_tracks.push(track);
                }
            }
            Pick::Range => {
                let order: Vec<TrackId> = self.project().tracks.iter().map(|t| t.id).collect();
                let anchor = self
                    .selected_tracks
                    .last()
                    .copied()
                    .unwrap_or_else(|| self.focused_track());
                let index = |id: TrackId| order.iter().position(|&t| t == id);
                if let (Some(a), Some(b)) = (index(anchor), index(track)) {
                    let (from, to) = (a.min(b), a.max(b));
                    for &id in &order[from..=to] {
                        if !self.selected_tracks.contains(&id) {
                            self.selected_tracks.push(id);
                        }
                    }
                    // The one clicked is the one picked last.
                    self.selected_tracks.retain(|&t| t != track);
                    self.selected_tracks.push(track);
                }
            }
        }
        if self.selected_tracks.contains(&track) {
            self.focus_track(track);
        }
    }

    /// Moves the selected tracks together (keeping their order) so the
    /// first of them is at `to` among the other tracks. Within a gesture,
    /// every move of a drag is one undo step.
    pub fn move_selected_tracks(&mut self, to: usize) {
        let tracks = self.selected_tracks();
        if tracks.is_empty() {
            return;
        }
        let mut moved = self.project().clone();
        if !moved.move_tracks(&tracks, to) {
            return;
        }
        self.doc.edit(text::EDIT_MOVE_TRACKS, |p| {
            p.move_tracks(&tracks, to);
        });
    }

    // ----- Clips

    /// The clips selected on the timeline.
    pub fn selected_clips(&self) -> &[ClipId] {
        &self.selected_clips
    }

    /// Whether a clip is selected.
    pub fn is_clip_selected(&self, clip: ClipId) -> bool {
        self.selected_clips.contains(&clip)
    }

    /// Puts a clip in or out of the selection, the others staying
    /// (Ctrl+click); a clip put in becomes the one picked last.
    pub fn toggle_clip(&mut self, clip: ClipId) {
        if self.project().clip(clip).is_none() {
            return;
        }
        if let Some(at) = self.selected_clips.iter().position(|&c| c == clip) {
            self.selected_clips.remove(at);
        } else {
            self.selected_clips.push(clip);
        }
    }

    /// Selects exactly these clips (the selection box).
    pub fn select_clips(&mut self, clips: Vec<ClipId>) {
        let project = self.project();
        let mut kept: Vec<ClipId> = Vec::with_capacity(clips.len());
        for clip in clips {
            if project.clip(clip).is_some() && !kept.contains(&clip) {
                kept.push(clip);
            }
        }
        self.selected_clips = kept;
    }

    /// Selects every clip of the song.
    pub fn select_all_clips(&mut self) {
        self.selected_clips = self
            .project()
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.id))
            .collect();
    }

    /// Moves clips to new starts, all as one step (one step per gesture
    /// while dragging).
    pub fn move_clips(&mut self, moves: &[(ClipId, Ticks)]) {
        let project = self.project();
        let changed = moves.iter().any(|&(clip, start)| {
            project
                .clip(clip)
                .is_some_and(|(_, c)| c.start != start.max(Ticks::ZERO))
        });
        if !changed {
            return;
        }
        self.doc.edit(text::EDIT_MOVE_CLIPS, |p| {
            for &(clip, start) in moves {
                p.update_clip(clip, |c| c.start = start.max(Ticks::ZERO));
            }
        });
    }

    /// Removes the selected clips from the timeline (one step).
    pub fn remove_selected_clips(&mut self) {
        let clips = std::mem::take(&mut self.selected_clips);
        match clips.as_slice() {
            [] => {}
            [one] => self.remove_clip(*one),
            _ => {
                self.doc.edit(text::EDIT_REMOVE_CLIPS, |p| {
                    for &clip in &clips {
                        p.remove_clip(clip);
                    }
                });
            }
        }
    }

    /// Places a copy of the selected clips right after them, as a block
    /// (one step); the copies become the selection.
    pub fn duplicate_selected_clips(&mut self) {
        let clips: Vec<(TrackId, daw_model::Clip)> = self
            .selected_clips
            .iter()
            .filter_map(|&id| self.project().clip(id).map(|(t, c)| (t, *c)))
            .collect();
        match clips.as_slice() {
            [] => {}
            [(_, one)] => {
                self.duplicate_clip(one.id);
            }
            _ => {
                let first = clips.iter().map(|(_, c)| c.start).min().unwrap_or_default();
                let last = clips.iter().map(|(_, c)| c.end()).max().unwrap_or_default();
                let by = last - first;
                let copies = self.doc.edit(text::EDIT_DUPLICATE_CLIPS, |p| {
                    let mut copies = Vec::with_capacity(clips.len());
                    for (track, clip) in &clips {
                        let start = clip.start + by;
                        let copy = match clip.content {
                            ClipContent::Pattern { pattern, .. } => {
                                p.add_clip(*track, start, clip.length, pattern)
                            }
                            ClipContent::Audio { audio, offset, .. } => {
                                p.add_audio_clip(*track, start, clip.length, audio, offset)
                            }
                        };
                        if let Some(copy) = copy {
                            p.update_clip(copy, |c| c.content = clip.content);
                            copies.push(copy);
                        }
                    }
                    copies
                });
                self.selected_clips = copies;
            }
        }
    }

    /// Moves the selected clips together by whole bars (negative is
    /// earlier): the one picked last lands on a bar line, the others keep
    /// their distance from it.
    pub fn nudge_selected_clips(&mut self, bars: i64) {
        if self.selected_clips.len() <= 1 {
            self.nudge_clip(bars);
            return;
        }
        let Some((_, lead)) = self.selected_clip().and_then(|id| self.project().clip(id)) else {
            return;
        };
        let meter = &self.project().meter;
        let Ok(bar) = u32::try_from(i64::from(meter.position(lead.start).bar) + bars) else {
            return;
        };
        let by = meter.bar_start(bar) - lead.start;
        let moves: Vec<(ClipId, Ticks)> = self
            .selected_clips
            .iter()
            .filter_map(|&id| self.project().clip(id).map(|(_, c)| (id, c.start + by)))
            .collect();
        if moves.iter().any(|&(_, start)| start < Ticks::ZERO) {
            return;
        }
        self.move_clips(&moves);
    }
}

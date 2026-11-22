// SPDX-License-Identifier: GPL-3.0-or-later

//! Note shapes (decision 0031): how a track's notes start and end, for the
//! instruments that play recorded sounds.

use daw_model::{NoteShape, TrackId};

use super::Session;
use crate::text;

impl Session {
    /// A track's note shape, if its instrument takes one (it plays recorded
    /// sounds: a SoundFont, an SFZ instrument, a drum kit or a custom kit).
    pub fn track_shape(&self, track: TrackId) -> Option<NoteShape> {
        let track = self.project().track(track)?;
        track.instrument.takes_shape().then_some(track.shape)
    }

    /// Changes a track's note shape (its times are kept in their ranges).
    pub fn set_track_shape(&mut self, track: TrackId, shape: NoteShape) {
        let shape = shape.clamped();
        if self.track_shape(track).is_none_or(|s| s == shape) {
            return;
        }
        self.doc.edit(text::EDIT_NOTE_SHAPE, |p| {
            if let Some(t) = p.track_mut(track) {
                t.shape = shape;
            }
        });
    }
}

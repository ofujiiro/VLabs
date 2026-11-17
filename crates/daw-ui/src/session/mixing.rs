// SPDX-License-Identifier: GPL-3.0-or-later

//! The mixer: each track's volume, position, mute and solo, the master
//! volume, effect chains, drum pad settings, and the meters. Every change
//! is one undo step; a fader drag is merged into one through the document's
//! gestures.

use daw_model::{ChainPlace, EffectId, EffectKind, PadSetting, TrackId};

use super::Session;
use crate::text;

/// The loudest a track or the master can be turned up: twice as loud
/// (+6 dB).
pub const MAX_GAIN: f32 = 2.0;

impl Session {
    // ----- Channel strips

    /// Sets a track's volume (linear gain, 0 to 2).
    pub fn set_track_gain(&mut self, track: TrackId, gain: f32) {
        let gain = gain.clamp(0.0, MAX_GAIN);
        if self
            .project()
            .track(track)
            .is_none_or(|t| t.strip.gain.to_bits() == gain.to_bits())
        {
            return;
        }
        self.doc.edit(text::EDIT_VOLUME, |p| {
            if let Some(t) = p.track_mut(track) {
                t.strip.gain = gain;
            }
        });
    }

    /// Sets a track's stereo position (-1 left to 1 right).
    pub fn set_track_pan(&mut self, track: TrackId, pan: f32) {
        let pan = pan.clamp(-1.0, 1.0);
        if self
            .project()
            .track(track)
            .is_none_or(|t| t.strip.pan.to_bits() == pan.to_bits())
        {
            return;
        }
        self.doc.edit(text::EDIT_PAN, |p| {
            if let Some(t) = p.track_mut(track) {
                t.strip.pan = pan;
            }
        });
    }

    /// Silences a track, or lets it be heard again.
    pub fn toggle_mute(&mut self, track: TrackId) {
        self.doc.edit(text::EDIT_MUTE, |p| {
            if let Some(t) = p.track_mut(track) {
                t.strip.mute = !t.strip.mute;
            }
        });
    }

    /// Hears a track alone (with any other soloed tracks), or not.
    pub fn toggle_solo(&mut self, track: TrackId) {
        self.doc.edit(text::EDIT_SOLO, |p| {
            if let Some(t) = p.track_mut(track) {
                t.strip.solo = !t.strip.solo;
            }
        });
    }

    /// Sets the volume of the whole mix (linear gain, 0 to 2).
    pub fn set_master_gain(&mut self, gain: f32) {
        let gain = gain.clamp(0.0, MAX_GAIN);
        if self.project().master.gain.to_bits() == gain.to_bits() {
            return;
        }
        self.doc
            .edit(text::EDIT_MASTER_VOLUME, |p| p.master.gain = gain);
    }

    // ----- Effects

    /// Adds an effect, with its usual settings, at the end of a chain.
    pub fn add_effect(&mut self, place: ChainPlace, kind: EffectKind) -> Option<EffectId> {
        self.doc
            .edit(text::EDIT_ADD_EFFECT, |p| p.add_effect(place, kind))
    }

    /// Removes an effect.
    pub fn remove_effect(&mut self, id: EffectId) {
        if self.project().effect(id).is_none() {
            return;
        }
        if self.effect_plugin(id).is_some() {
            // Undoing brings the plugin back with the settings it has now.
            self.capture_plugin_states();
        }
        self.doc
            .edit(text::EDIT_REMOVE_EFFECT, |p| p.remove_effect(id));
    }

    /// Moves an effect to another position in its chain.
    pub fn move_effect(&mut self, id: EffectId, to: usize) {
        let Some((_, from)) = self.project().effect_place(id) else {
            return;
        };
        if from == to {
            return;
        }
        self.doc
            .edit(text::EDIT_MOVE_EFFECT, |p| p.move_effect(id, to));
    }

    /// Switches an effect on, or bypasses it.
    pub fn set_effect_enabled(&mut self, id: EffectId, enabled: bool) {
        if self
            .project()
            .effect(id)
            .is_none_or(|e| e.enabled == enabled)
        {
            return;
        }
        let label = if enabled {
            text::EDIT_EFFECT_ON
        } else {
            text::EDIT_EFFECT_OFF
        };
        self.doc
            .edit(label, |p| p.update_effect(id, |e| e.enabled = enabled));
    }

    /// Changes an effect's settings (of the same kind).
    pub fn set_effect(&mut self, id: EffectId, kind: EffectKind) {
        let same = self
            .project()
            .effect(id)
            .is_none_or(|e| e.kind == kind || !e.kind.same_kind(&kind));
        if same {
            return;
        }
        self.doc.edit(text::EDIT_CHANGE_EFFECT, |p| {
            p.update_effect(id, |e| e.kind = kind);
        });
    }

    // ----- Drum pads

    /// The settings of a drum track's pad.
    pub fn pad(&self, track: TrackId, key: u8) -> PadSetting {
        self.project()
            .track(track)
            .map_or_else(|| PadSetting::new(key), |t| t.instrument.pad(key))
    }

    /// Changes a drum pad's sound: volume, position, tune, length, mute.
    pub fn set_pad(&mut self, track: TrackId, setting: PadSetting) {
        if self.pad(track, setting.key) == setting {
            return;
        }
        self.doc.edit(text::EDIT_PAD, |p| {
            if let Some(t) = p.track_mut(track) {
                t.instrument.set_pad(setting);
            }
        });
    }

    // ----- Meters

    /// How loud a track is right now: its loudest recent sample (1.0 is
    /// full scale).
    pub fn track_level(&self, track: TrackId) -> f32 {
        self.engine.track_peak(track)
    }

    /// How loud the whole mix is right now (above 1.0 would distort).
    pub fn master_level(&self) -> f32 {
        self.engine.peak()
    }
}

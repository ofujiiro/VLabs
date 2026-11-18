// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in synth (decision 0019): adding a synth track, changing its
//! settings and presets (every change undoable), and which synth windows
//! are open.

use daw_model::{Instrument, SynthPreset, SynthSettings, Ticks, TrackId};

use super::Session;
use super::arrange::PIANO_BARS;
use crate::text;

impl Session {
    /// Adds a synth track at the bottom (the Soft keys preset) with an
    /// empty four-bar pattern at the start of the song, and puts it in
    /// focus.
    pub fn add_synth_track(&mut self) -> TrackId {
        let name = self.unique_track_name(text::SYNTH_TRACK);
        let track = self.doc.edit(text::EDIT_ADD_SYNTH, |p| {
            let track = p.add_track(&name, Instrument::Synth(SynthPreset::SoftKeys.settings()));
            let length = p.meter.bar_start(PIANO_BARS);
            let pattern = p.add_pattern_for(track, &text::pattern_name(1), length);
            p.add_clip(track, Ticks::ZERO, length, pattern);
            track
        });
        self.focus_track(track);
        track
    }

    /// A synth track's settings (`None` for other tracks).
    pub fn synth_settings(&self, track: TrackId) -> Option<SynthSettings> {
        self.project()
            .track(track)
            .and_then(|t| t.instrument.synth().copied())
    }

    /// Changes a synth track's settings. Sounding notes carry on with the
    /// new sound. Inside a gesture, a whole drag is one undo step.
    pub fn set_synth(&mut self, track: TrackId, settings: SynthSettings) {
        let settings = settings.repaired();
        if self.synth_settings(track).is_none_or(|s| s == settings) {
            return;
        }
        self.doc.edit(text::EDIT_CHANGE_SYNTH, |p| {
            if let Some(t) = p.track_mut(track) {
                t.instrument = Instrument::Synth(settings);
            }
        });
    }

    /// Sets every control of a synth track from a preset (one undo step).
    pub fn set_synth_preset(&mut self, track: TrackId, preset: SynthPreset) {
        if self
            .synth_settings(track)
            .is_none_or(|s| s == preset.settings())
        {
            return;
        }
        self.doc.edit(text::EDIT_SYNTH_PRESET, |p| {
            if let Some(t) = p.track_mut(track) {
                t.instrument = Instrument::Synth(preset.settings());
            }
        });
    }

    // ----- Synth windows

    /// Opens a synth track's window, or closes it if open.
    pub fn toggle_synth_window(&mut self, track: TrackId) {
        if let Some(at) = self.synth_windows.iter().position(|&t| t == track) {
            self.synth_windows.remove(at);
        } else if self.synth_settings(track).is_some() {
            self.synth_windows.push(track);
        }
    }

    /// Opens a synth track's window (or leaves it open).
    pub fn show_synth_window(&mut self, track: TrackId) {
        if !self.is_synth_window_open(track) {
            self.toggle_synth_window(track);
        }
    }

    /// Closes a synth track's window.
    pub fn close_synth_window(&mut self, track: TrackId) {
        self.synth_windows.retain(|&t| t != track);
    }

    /// Whether a synth track's window is open.
    pub fn is_synth_window_open(&self, track: TrackId) -> bool {
        self.synth_windows.contains(&track)
    }

    /// The synth windows to show, oldest first (tracks that are gone or no
    /// longer play the synth are left out).
    pub fn synth_windows(&mut self) -> Vec<TrackId> {
        let project = self.doc.project();
        self.synth_windows.retain(|&t| {
            project
                .track(t)
                .is_some_and(|track| track.instrument.synth().is_some())
        });
        self.synth_windows.clone()
    }
}

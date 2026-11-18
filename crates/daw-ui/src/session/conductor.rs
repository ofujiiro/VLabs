// SPDX-License-Identifier: GPL-3.0-or-later

//! The song's tempo, time signatures and key over time (decision 0018):
//! what the transport shows and changes at the play position, and the
//! changes the song view's signature row adds, moves and removes. Every
//! change is one undo step (or part of a gesture while dragging).

use daw_model::{MeterMap, Scale, TEMPO_RANGE, TempoChange, Ticks, TimeSignature};

use super::Session;
use crate::text;

impl Session {
    /// The bar the play position is in.
    fn bar_at_position(&self) -> u32 {
        self.project()
            .meter
            .position(self.song_position().max(Ticks::ZERO))
            .bar
    }

    // ----- At the play position (the transport)

    /// Where the tempo in effect at the play position was set (the part
    /// of the song the transport's tempo changes).
    pub fn tempo_change_at_position(&self) -> Ticks {
        self.project()
            .tempo
            .change_at(self.song_position().max(Ticks::ZERO))
            .at
    }

    /// The tempo in effect at the play position, in beats per minute (on a
    /// glide, the tempo it starts from).
    pub fn tempo(&self) -> f64 {
        self.project()
            .tempo
            .change_at(self.song_position().max(Ticks::ZERO))
            .bpm
    }

    /// Sets the tempo of the part of the song the play position is in (the
    /// whole song when the tempo never changes). Audio clips keep their
    /// length in seconds (decision 0017).
    pub fn set_tempo(&mut self, bpm: f64) {
        let at = self.tempo_change_at_position();
        self.set_tempo_at(at, bpm);
    }

    /// Sets the tempo of the change at `at` (adding one there if needed).
    pub fn set_tempo_at(&mut self, at: Ticks, bpm: f64) {
        let bpm = bpm.clamp(*TEMPO_RANGE.start(), *TEMPO_RANGE.end());
        self.doc
            .edit(text::EDIT_TEMPO, |p| p.set_tempo(at.max(Ticks::ZERO), bpm));
    }

    /// The time signature in effect at the play position.
    pub fn time_signature(&self) -> TimeSignature {
        self.project()
            .meter
            .signature_at_bar(self.bar_at_position())
    }

    /// Sets the time signature of the part of the song the play position
    /// is in (the whole song when it never changes), keeping the song in
    /// its bars (decision 0018).
    pub fn set_time_signature(&mut self, signature: TimeSignature) {
        if signature == self.time_signature() {
            return;
        }
        let bar = self.bar_at_position();
        let meter = &self.project().meter;
        let start = meter
            .changes()
            .iter()
            .rev()
            .find(|c| c.bar <= bar)
            .map_or(0, |c| c.bar);
        self.set_meter_change(start, Some(signature));
    }

    /// The key in effect at the play position.
    pub fn key(&self) -> Option<Scale> {
        self.project().key.at_bar(self.bar_at_position())
    }

    /// Sets the key of the part of the song the play position is in (the
    /// whole song when it never changes); `None` is no key.
    pub fn set_key(&mut self, scale: Option<Scale>) {
        if scale == self.key() {
            return;
        }
        let bar = self.bar_at_position();
        let start = self
            .project()
            .key
            .changes()
            .iter()
            .rev()
            .find(|c| c.bar <= bar)
            .map_or(0, |c| c.bar);
        self.set_key_change(start, Some(scale));
    }

    // ----- Changes along the song (the signature row)

    /// The tempo changes, in order (the first is at the start).
    pub fn tempo_changes(&self) -> &[TempoChange] {
        self.project().tempo.changes()
    }

    /// Adds a tempo change at `at`, at the tempo the song has there (so
    /// nothing changes until it is moved); returns where it is.
    pub fn add_tempo_change(&mut self, at: Ticks) -> Ticks {
        let at = at.max(Ticks::ZERO);
        let bpm = (self.project().tempo.bpm_at(at) * 10.0).round() / 10.0;
        self.doc.edit(text::EDIT_ADD_TEMPO_CHANGE, |p| {
            p.change_tempo(|tempo| tempo.set(at, bpm));
        });
        at
    }

    /// Removes the tempo change at `at` (the one at the start stays).
    pub fn remove_tempo_change(&mut self, at: Ticks) {
        if at <= Ticks::ZERO || !self.tempo_changes().iter().any(|c| c.at == at) {
            return;
        }
        self.doc.edit(text::EDIT_REMOVE_TEMPO_CHANGE, |p| {
            p.change_tempo(|tempo| tempo.remove(at));
        });
    }

    /// Moves the tempo change at `from` to `to` (not the one at the start,
    /// and not onto it); returns where it is now.
    pub fn move_tempo_change(&mut self, from: Ticks, to: Ticks) -> Ticks {
        if from <= Ticks::ZERO || to <= Ticks::ZERO || from == to {
            return from;
        }
        self.doc.edit(text::EDIT_MOVE_TEMPO_CHANGE, |p| {
            p.change_tempo(|tempo| tempo.move_change(from, to));
        });
        to
    }

    /// Makes the tempo change at `at` glide to the next one, or not.
    pub fn set_tempo_glide(&mut self, at: Ticks, glide: bool) {
        let present = self
            .tempo_changes()
            .iter()
            .any(|c| c.at == at && c.glide != glide);
        if present {
            self.doc.edit(text::EDIT_GLIDE, |p| {
                p.change_tempo(|tempo| tempo.set_glide(at, glide));
            });
        }
    }

    /// The bars where the time signature or the key changes, in order
    /// (bar zero always).
    pub fn signature_bars(&self) -> Vec<u32> {
        let project = self.project();
        let mut bars: Vec<u32> = project
            .meter
            .changes()
            .iter()
            .map(|c| c.bar)
            .chain(project.key.changes().iter().map(|c| c.bar))
            .collect();
        bars.sort_unstable();
        bars.dedup();
        bars
    }

    /// Whether the time signature changes at `bar` (always at bar zero).
    pub fn meter_change_at(&self, bar: u32) -> Option<TimeSignature> {
        self.project()
            .meter
            .changes()
            .iter()
            .find(|c| c.bar == bar)
            .map(|c| c.signature)
    }

    /// Whether the key changes at `bar` (always at bar zero): `Some(None)`
    /// is a change to no key.
    pub fn key_change_at(&self, bar: u32) -> Option<Option<Scale>> {
        self.project()
            .key
            .changes()
            .iter()
            .find(|c| c.bar == bar)
            .map(|c| c.scale)
    }

    /// Sets the time signature from `bar` on; `None` removes the change
    /// there (the song's first signature stays). The song keeps its bars.
    pub fn set_meter_change(&mut self, bar: u32, signature: Option<TimeSignature>) {
        let mut meter: MeterMap = self.project().meter.clone();
        match signature {
            Some(signature) => meter.set(bar, signature),
            None => meter.remove(bar),
        }
        if meter != self.project().meter {
            self.doc
                .edit(text::EDIT_TIME_SIGNATURE, |p| p.change_meter(meter));
        }
    }

    /// Sets the key from `bar` on: `Some(scale)` (a key, or `Some(None)`
    /// for no key), or `None` to remove the change there.
    pub fn set_key_change(&mut self, bar: u32, scale: Option<Option<Scale>>) {
        let mut key = self.project().key.clone();
        match scale {
            Some(scale) => key.set(bar, scale),
            None => key.remove(bar),
        }
        if key != self.project().key {
            self.doc.edit(text::EDIT_KEY, |p| p.key = key);
        }
    }

    /// Removes the time-signature and key changes at `bar` (not those at
    /// the start), as one undo step.
    pub fn remove_signature_changes(&mut self, bar: u32) {
        if bar == 0 {
            return;
        }
        let mut meter = self.project().meter.clone();
        let mut key = self.project().key.clone();
        meter.remove(bar);
        key.remove(bar);
        if meter != self.project().meter || key != self.project().key {
            self.doc.edit(text::EDIT_REMOVE_SIGNATURE, |p| {
                p.key = key;
                p.change_meter(meter);
            });
        }
    }

    /// Moves the time-signature and key changes at bar `from` to bar `to`
    /// (not those at the start, and not onto it); returns where they are.
    pub fn move_signature_changes(&mut self, from: u32, to: u32) -> u32 {
        if from == 0 || to == 0 || from == to {
            return from;
        }
        let mut meter = self.project().meter.clone();
        let mut key = self.project().key.clone();
        meter.move_change(from, to);
        key.move_change(from, to);
        self.doc.edit(text::EDIT_MOVE_SIGNATURE, |p| {
            p.key = key;
            p.change_meter(meter);
        });
        to
    }
}

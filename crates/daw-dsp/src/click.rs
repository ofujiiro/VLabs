// SPDX-License-Identifier: GPL-3.0-or-later

//! The metronome's click: a short, bright tick, higher on the first beat of
//! a bar.

/// A synthesised metronome click.
pub struct Click {
    sample_rate: f32,
    phase: f32,
    step: f32,
    level: f32,
    decay: f32,
    remaining: u32,
    /// Overall loudness (linear gain).
    pub volume: f32,
}

impl Click {
    /// A click generator for the given sample rate.
    pub fn new(sample_rate: u32) -> Self {
        #[allow(
            clippy::cast_precision_loss,
            reason = "sample rates are small integers"
        )]
        let sample_rate = sample_rate as f32;
        Self {
            sample_rate,
            phase: 0.0,
            step: 0.0,
            level: 0.0,
            decay: 0.0,
            remaining: 0,
            volume: 0.5,
        }
    }

    /// Starts a click; `accent` for the first beat of a bar.
    pub fn trigger(&mut self, accent: bool) {
        let frequency = if accent { 1760.0 } else { 1320.0 };
        self.step = frequency / self.sample_rate;
        self.phase = 0.0;
        self.level = if accent { 1.0 } else { 0.7 };
        // Falls by 60 dB in 40 ms.
        self.decay = (-6.9 / (0.040 * self.sample_rate)).exp();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small positive count"
        )]
        let remaining = (0.050 * self.sample_rate) as u32;
        self.remaining = remaining;
    }

    /// Adds the click's sound into both channels.
    pub fn add_to(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.remaining == 0 {
                return;
            }
            let value = (self.phase * std::f32::consts::TAU).sin() * self.level * self.volume;
            *l += value;
            *r += value;
            self.phase = (self.phase + self.step).fract();
            self.level *= self.decay;
            self.remaining -= 1;
        }
    }

    /// Whether a click is sounding.
    pub fn is_sounding(&self) -> bool {
        self.remaining > 0
    }
}

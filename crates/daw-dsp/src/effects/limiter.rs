// SPDX-License-Identifier: GPL-3.0-or-later

//! The limiter: makes sure nothing ever goes above a ceiling, so the song
//! can be loud without distorting. It looks 1.5 ms ahead, so it starts
//! turning down before a peak arrives, and a final check on every sample
//! guarantees the ceiling is never crossed.

/// The limiter's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimiterParams {
    /// The highest level that comes out, in dBFS.
    pub ceiling_db: f32,
    /// Gain added before limiting, in dB (louder, with peaks held down).
    pub gain_db: f32,
    /// How fast it lets go after a peak, in milliseconds.
    pub release_ms: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            ceiling_db: -0.3,
            gain_db: 0.0,
            release_ms: 80.0,
        }
    }
}

/// How far ahead the limiter looks, in seconds.
const LOOKAHEAD: f32 = 0.0015;

/// A stereo-linked look-ahead peak limiter.
pub struct Limiter {
    params: LimiterParams,
    sample_rate: f32,
    lookahead: usize,
    /// The delayed sound, as stereo frames.
    delay: Vec<[f32; 2]>,
    position: usize,
    /// A running maximum over the look-ahead window: (frame number, peak).
    window: Vec<(u64, f32)>,
    window_start: usize,
    window_len: usize,
    frame: u64,
    gain: f32,
    attack: f32,
    release: f32,
    ceiling: f32,
    input_gain: f32,
}

impl Limiter {
    /// A limiter with these settings. All memory is reserved here.
    pub fn new(params: &LimiterParams, sample_rate: u32) -> Self {
        #[allow(clippy::cast_precision_loss, reason = "sample rates are small")]
        let rate = sample_rate as f32;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a short, positive length"
        )]
        let lookahead = ((LOOKAHEAD * rate).round() as usize).max(1);
        #[allow(clippy::cast_precision_loss, reason = "a short length")]
        let attack = (-1.0 / (lookahead as f32 / 4.0)).exp();
        let mut limiter = Self {
            params: *params,
            sample_rate: rate,
            lookahead,
            delay: vec![[0.0; 2]; lookahead + 1],
            position: 0,
            window: vec![(0, 0.0); lookahead + 2],
            window_start: 0,
            window_len: 0,
            frame: 0,
            gain: 1.0,
            attack,
            release: 0.0,
            ceiling: 1.0,
            input_gain: 1.0,
        };
        limiter.set_params(params);
        limiter
    }

    /// Changes the settings.
    pub fn set_params(&mut self, params: &LimiterParams) {
        self.params = *params;
        self.ceiling = crate::db_to_gain(params.ceiling_db.min(0.0));
        self.input_gain = crate::db_to_gain(params.gain_db.clamp(-24.0, 24.0));
        self.release = (-1.0 / (params.release_ms.max(1.0) * 0.001 * self.sample_rate)).exp();
    }

    /// How late the sound comes out, in frames.
    pub fn latency(&self) -> usize {
        self.lookahead
    }

    /// How much the limiter is turning down right now, in dB (positive).
    pub fn reduction_db(&self) -> f32 {
        -20.0 * self.gain.max(1e-6).log10()
    }

    /// Adds a peak to the running maximum and drops peaks that left the
    /// window.
    fn push_peak(&mut self, peak: f32) -> f32 {
        let capacity = self.window.len();
        // Drop smaller peaks from the back: they can never be the maximum.
        while self.window_len > 0 {
            let back = (self.window_start + self.window_len - 1) % capacity;
            if self.window[back].1 <= peak {
                self.window_len -= 1;
            } else {
                break;
            }
        }
        let slot = (self.window_start + self.window_len) % capacity;
        self.window[slot] = (self.frame, peak);
        self.window_len += 1;
        // Drop peaks older than the window from the front.
        let oldest = self.frame.saturating_sub(self.lookahead as u64);
        while self.window[self.window_start].0 < oldest {
            self.window_start = (self.window_start + 1) % capacity;
            self.window_len -= 1;
        }
        self.window[self.window_start].1
    }

    /// Processes stereo sound in place (delayed by [`Limiter::latency`]).
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let length = self.delay.len();
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let input = [*l * self.input_gain, *r * self.input_gain];
            let peak = input[0].abs().max(input[1].abs());
            let window_peak = self.push_peak(peak);
            self.frame += 1;
            let target = if window_peak > self.ceiling {
                self.ceiling / window_peak
            } else {
                1.0
            };
            let coefficient = if target < self.gain {
                self.attack
            } else {
                self.release
            };
            self.gain = target + coefficient * (self.gain - target);

            self.delay[self.position] = input;
            let read = (self.position + 1) % length;
            let out = self.delay[read];
            self.position = read;
            // The final guarantee: never above the ceiling.
            let out_peak = out[0].abs().max(out[1].abs());
            let gain = if out_peak * self.gain > self.ceiling {
                self.ceiling / out_peak
            } else {
                self.gain
            };
            *l = out[0] * gain;
            *r = out[1] * gain;
        }
    }

    /// Forgets what it was holding.
    pub fn reset(&mut self) {
        self.delay.fill([0.0; 2]);
        self.window_len = 0;
        self.gain = 1.0;
    }
}

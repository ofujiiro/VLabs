// SPDX-License-Identifier: GPL-3.0-or-later

//! The compressor: turns down whatever goes above a threshold, by a ratio,
//! with a soft knee, so loud and quiet parts sit closer together. Both
//! channels are turned down together, so the stereo image stays put.

/// The compressor's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressorParams {
    /// The level above which sound is turned down, in dBFS.
    pub threshold_db: f32,
    /// How much: 4 means 4 dB over the threshold come out as 1 dB.
    pub ratio: f32,
    /// How fast it reacts to a loud sound, in milliseconds.
    pub attack_ms: f32,
    /// How fast it lets go afterwards, in milliseconds.
    pub release_ms: f32,
    /// Width of the soft knee around the threshold, in dB.
    pub knee_db: f32,
    /// Gain added afterwards, in dB, to make up for the turning down.
    pub makeup_db: f32,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 3.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            knee_db: 6.0,
            makeup_db: 0.0,
        }
    }
}

/// A stereo-linked feed-forward compressor, with the "smooth decoupled
/// peak" detector (Giannoulis, Massberg and Reiss, 2012): the turning-down
/// is held between the peaks of a sound by the release, and smoothed by the
/// attack, so a steady tone gets a steady gain.
pub struct Compressor {
    sample_rate: f32,
    params: CompressorParams,
    attack: f32,
    release: f32,
    /// Peak-held reduction, in dB (zero or positive).
    held_db: f32,
    /// The reduction being applied, in dB (zero or positive).
    envelope_db: f32,
}

impl Compressor {
    /// A compressor with these settings.
    pub fn new(params: &CompressorParams, sample_rate: u32) -> Self {
        #[allow(clippy::cast_precision_loss, reason = "sample rates are small")]
        let mut compressor = Self {
            sample_rate: sample_rate as f32,
            params: *params,
            attack: 0.0,
            release: 0.0,
            held_db: 0.0,
            envelope_db: 0.0,
        };
        compressor.configure();
        compressor
    }

    /// Changes the settings.
    pub fn set_params(&mut self, params: &CompressorParams) {
        if *params != self.params {
            self.params = *params;
            self.configure();
        }
    }

    fn configure(&mut self) {
        let coefficient = |ms: f32| (-1.0 / (ms.max(0.05) * 0.001 * self.sample_rate)).exp();
        self.attack = coefficient(self.params.attack_ms);
        self.release = coefficient(self.params.release_ms);
    }

    /// The gain change for an input level, in dB (the static curve).
    pub fn gain_change_db(&self, level_db: f32) -> f32 {
        let threshold = self.params.threshold_db;
        let ratio = self.params.ratio.max(1.0);
        let knee = self.params.knee_db.max(0.0);
        let over = level_db - threshold;
        let output = if 2.0 * over < -knee {
            level_db
        } else if 2.0 * over.abs() <= knee && knee > 0.0 {
            level_db + (1.0 / ratio - 1.0) * (over + knee / 2.0).powi(2) / (2.0 * knee)
        } else {
            threshold + over / ratio
        };
        output - level_db
    }

    /// How much the compressor is turning down right now, in dB (positive),
    /// for the meter.
    pub fn reduction_db(&self) -> f32 {
        self.envelope_db
    }

    /// Processes stereo sound in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let makeup = crate::db_to_gain(self.params.makeup_db);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let peak = l.abs().max(r.abs()).max(1e-9);
            let level_db = 20.0 * peak.log10();
            let reduction = -self.gain_change_db(level_db);
            self.held_db =
                reduction.max(self.release * self.held_db + (1.0 - self.release) * reduction);
            self.envelope_db = self.attack * self.envelope_db + (1.0 - self.attack) * self.held_db;
            let gain = crate::db_to_gain(-self.envelope_db) * makeup;
            *l *= gain;
            *r *= gain;
        }
    }

    /// Lets go at once.
    pub fn reset(&mut self) {
        self.held_db = 0.0;
        self.envelope_db = 0.0;
    }
}

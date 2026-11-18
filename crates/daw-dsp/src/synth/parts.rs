// SPDX-License-Identifier: GPL-3.0-or-later

//! The building blocks of the synth: band-limited waves, envelopes and the
//! filter. Everything is plain arithmetic on fixed-size state, safe for the
//! audio thread.

use super::{EnvParams, FilterMode, Wave};

/// The polyBLEP correction for a step at phase 0 (`t` the phase from 0 to
/// 1, `dt` the phase advance per sample): subtracted from a falling step,
/// it smooths the jump over two samples so high notes do not alias.
pub(super) fn poly_blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// The polyBLAMP correction for a corner at phase 0: the integral of
/// [`poly_blep`], which rounds a sudden change of slope.
pub(super) fn poly_blamp(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt - 1.0;
        -x * x * x / 3.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt + 1.0;
        x * x * x / 3.0
    } else {
        0.0
    }
}

/// One sample of a wave at phase `t` (0 to 1), advancing `dt` per sample,
/// from -1 to 1 without a DC offset. `width` is the square wave's pulse
/// width.
pub(super) fn wave(wave: Wave, t: f64, dt: f64, width: f64) -> f64 {
    match wave {
        Wave::Sine => (t * std::f64::consts::TAU).sin(),
        Wave::Saw => 2.0 * t - 1.0 - poly_blep(t, dt),
        Wave::Square => {
            let naive = if t < width { 1.0 } else { -1.0 };
            naive + poly_blep(t, dt) - poly_blep((t - width).rem_euclid(1.0), dt)
                // A pulse narrower or wider than half sits off centre.
                - (2.0 * width - 1.0)
        }
        Wave::Triangle => {
            // Corners at 0 (the lowest point) and at a half (the highest),
            // where the slope changes by eight per cycle.
            1.0 - 4.0 * (t - 0.5).abs()
                + 4.0 * dt * (poly_blamp(t, dt) - poly_blamp((t + 0.5).fract(), dt))
        }
    }
}

/// What an envelope is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// The quietest level an envelope keeps (-80 dB); below it, it ends.
const FLOOR: f32 = 1e-4;

/// How fast an envelope moves, worked out from its settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EnvRates {
    /// Added each sample while rising (a straight line to the top).
    attack: f32,
    /// Each sample, the distance to the sustain level is multiplied by this.
    decay: f32,
    /// The level held while the key is down.
    sustain: f32,
    /// Each sample, the level is multiplied by this after the key is up.
    release: f32,
}

impl EnvRates {
    /// The rates for envelope settings at a sample rate. A decay or release
    /// time is the time to cover 99 % of the way (five time constants).
    pub(super) fn new(params: &EnvParams, sample_rate: f32) -> Self {
        let coefficient = |seconds: f32| (-5.0 / (seconds * sample_rate)).exp();
        Self {
            attack: 1.0 / (params.attack.max(0.001) * sample_rate),
            decay: coefficient(params.decay.max(0.001)),
            sustain: params.sustain.clamp(0.0, 1.0),
            release: coefficient(params.release.max(0.005)),
        }
    }
}

impl Default for EnvRates {
    fn default() -> Self {
        Self::new(&EnvParams::default(), 48_000.0)
    }
}

/// An attack, decay, sustain and release envelope.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Envelope {
    stage: Stage,
    level: f32,
}

impl Envelope {
    /// Starts (again): rises from wherever it is, so a retriggered note
    /// does not click.
    pub(super) fn start(&mut self) {
        self.stage = Stage::Attack;
    }

    /// The key is up: falls to silence.
    pub(super) fn release(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// Silences at once.
    pub(super) fn stop(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    /// Whether it has ended.
    pub(super) fn is_idle(self) -> bool {
        self.stage == Stage::Idle
    }

    /// The present level, from 0 to 1.
    pub(super) fn level(self) -> f32 {
        self.level
    }

    /// Advances one sample and gives the new level.
    pub(super) fn next(&mut self, rates: &EnvRates) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.level += rates.attack;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level = rates.sustain + (self.level - rates.sustain) * rates.decay;
                if (self.level - rates.sustain).abs() < FLOOR {
                    self.stage = Stage::Sustain;
                }
                self.end_if_silent(rates);
            }
            Stage::Sustain => {
                // Follows a changed sustain level smoothly.
                self.level += (rates.sustain - self.level) * 0.002;
                self.end_if_silent(rates);
            }
            Stage::Release => {
                self.level *= rates.release;
                if self.level < FLOOR {
                    self.stop();
                }
            }
        }
        self.level
    }

    /// A sound that sustains at silence (a pluck) has ended even while its
    /// key is held.
    fn end_if_silent(&mut self, rates: &EnvRates) {
        if rates.sustain < FLOOR && self.level < FLOOR {
            self.stop();
        }
    }
}

/// The coefficients of a state-variable filter at one cutoff.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SvfCoeffs {
    a1: f32,
    a2: f32,
    a3: f32,
    /// Damping: 2 is no resonance at all, near 0 rings.
    k: f32,
}

impl SvfCoeffs {
    /// Coefficients for a cutoff (kept well below half the sample rate) and
    /// a damping.
    pub(super) fn new(cutoff_hz: f32, sample_rate: f32, k: f32) -> Self {
        let cutoff = cutoff_hz.clamp(10.0, 0.45 * sample_rate);
        let g = (std::f32::consts::PI * cutoff / sample_rate).tan();
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            a1,
            a2,
            a3: g * a2,
            k,
        }
    }
}

/// One zero-delay-feedback state-variable filter (trapezoidal
/// integration): stable however fast its cutoff moves.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Svf {
    ic1: f32,
    ic2: f32,
}

/// The three outputs of one filter step.
pub(super) struct SvfOut {
    pub(super) low: f32,
    pub(super) band: f32,
    pub(super) high: f32,
}

impl Svf {
    /// Filters one sample.
    pub(super) fn process(&mut self, c: &SvfCoeffs, input: f32) -> SvfOut {
        let v3 = input - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOut {
            low: v2,
            band: v1,
            high: input - c.k * v1 - v2,
        }
    }

    /// Forgets the sound so far (a new note).
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The damping of the resonant stage for a resonance from 0 to 1.
fn damping(base: f32, resonance: f32) -> f32 {
    let resonance = resonance.clamp(0.0, 1.0);
    base + (0.05 - base) * resonance
}

/// A voice's filter: one or two state-variable stages.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct VoiceFilter {
    stages: [Svf; 2],
    first: SvfCoeffs,
    second: SvfCoeffs,
    mode: Option<FilterMode>,
    /// Lowers the level going in as the resonance rises, so a ringing
    /// filter is not much louder than an open one.
    input_gain: f32,
}

impl VoiceFilter {
    /// Sets the cutoff and resonance (at the control rate).
    pub(super) fn tune(
        &mut self,
        mode: FilterMode,
        cutoff_hz: f32,
        resonance: f32,
        sample_rate: f32,
    ) {
        self.mode = Some(mode);
        self.input_gain = 1.0 - 0.5 * resonance.clamp(0.0, 1.0);
        match mode {
            FilterMode::LowPass24 => {
                // A fourth-order Butterworth split into two stages; the
                // resonance goes into the second.
                self.first = SvfCoeffs::new(cutoff_hz, sample_rate, 1.848);
                self.second = SvfCoeffs::new(cutoff_hz, sample_rate, damping(0.765, resonance));
            }
            FilterMode::LowPass12 | FilterMode::HighPass | FilterMode::BandPass => {
                self.first = SvfCoeffs::new(cutoff_hz, sample_rate, damping(1.414, resonance));
            }
        }
    }

    /// Filters one sample.
    pub(super) fn process(&mut self, input: f32) -> f32 {
        let input = input * self.input_gain;
        match self.mode {
            None => input,
            Some(FilterMode::LowPass24) => {
                let low = self.stages[0].process(&self.first, input).low;
                self.stages[1].process(&self.second, low).low
            }
            Some(FilterMode::LowPass12) => self.stages[0].process(&self.first, input).low,
            Some(FilterMode::HighPass) => self.stages[0].process(&self.first, input).high,
            Some(FilterMode::BandPass) => {
                // Scaled so the centre frequency passes at its own level.
                self.stages[0].process(&self.first, input).band * self.first.k
            }
        }
    }

    /// Forgets the sound so far (a new note).
    pub(super) fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }
}

/// A small, fast pseudo-random generator (noise, random LFO steps, start
/// phases): the same sequence every time, so renders are repeatable.
#[derive(Clone, Copy, Debug)]
pub(super) struct Random(u32);

impl Random {
    pub(super) fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    /// The next value, from -1 to 1.
    pub(super) fn next_bipolar(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        #[allow(clippy::cast_precision_loss, reason = "noise values")]
        let value = self.0 as f32 / u32::MAX as f32;
        value * 2.0 - 1.0
    }

    /// The next value, from 0 to 1.
    pub(super) fn next_unit(&mut self) -> f64 {
        f64::from(self.next_bipolar()).mul_add(0.5, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_rise_fall_and_end() {
        let rate = 1000.0;
        let rates = EnvRates::new(
            &EnvParams {
                attack: 0.1,
                decay: 0.2,
                sustain: 0.5,
                release: 0.3,
            },
            rate,
        );
        let mut env = Envelope::default();
        assert!(env.is_idle());
        env.start();
        let rise: Vec<f32> = (0..100).map(|_| env.next(&rates)).collect();
        assert!(
            (rise[49] - 0.5).abs() < 0.02,
            "half way up after half the attack"
        );
        assert!((rise[99] - 1.0).abs() < 1e-3, "at the top after the attack");
        for _ in 0..200 {
            env.next(&rates);
        }
        assert!(
            (env.level() - 0.5).abs() < 0.01,
            "at the sustain level after the decay"
        );
        env.release();
        for _ in 0..300 {
            env.next(&rates);
        }
        assert!(env.level() < 0.5 * 0.01, "99 % down after the release time");
        for _ in 0..1000 {
            env.next(&rates);
        }
        assert!(env.is_idle(), "and then it ends");
    }

    #[test]
    fn a_retriggered_envelope_rises_from_where_it_was() {
        let rates = EnvRates::new(
            &EnvParams {
                attack: 0.01,
                decay: 0.1,
                sustain: 0.6,
                release: 0.5,
            },
            48_000.0,
        );
        let mut env = Envelope::default();
        env.start();
        for _ in 0..4800 {
            env.next(&rates);
        }
        env.release();
        for _ in 0..2400 {
            env.next(&rates);
        }
        let before = env.level();
        env.start();
        let after = env.next(&rates);
        assert!(after > before && after - before < 0.01, "no jump");
    }

    #[test]
    #[allow(clippy::cast_precision_loss, reason = "test arithmetic")]
    fn waves_stay_in_range_and_centred() {
        for kind in [Wave::Sine, Wave::Triangle, Wave::Saw, Wave::Square] {
            for width in [0.5, 0.2] {
                let dt = 1000.0 / 48_000.0;
                let samples: Vec<f64> = (0..48_000)
                    .map(|i| wave(kind, (f64::from(i) * dt).fract(), dt, width))
                    .collect();
                let mean = samples.iter().sum::<f64>() / samples.len() as f64;
                let peak = samples.iter().fold(0.0_f64, |m, s| m.max(s.abs()));
                assert!(mean.abs() < 0.01, "{kind:?} is centred ({mean})");
                assert!(peak <= 1.9, "{kind:?} stays in range ({peak})");
            }
        }
    }
}

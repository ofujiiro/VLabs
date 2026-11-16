// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in effects: EQ, compressor, reverb, delay and limiter.
//!
//! Each effect is created off the audio thread (that is when its memory is
//! reserved); after that, processing sound and changing settings never
//! allocate, so both are safe on the audio thread. Changing a setting keeps
//! the effect's state, so a reverb tail or an echo carries on.

mod biquad;
mod compressor;
mod delay;
mod eq;
mod limiter;
mod reverb;

pub use biquad::{Biquad, FilterKind};
pub use compressor::{Compressor, CompressorParams};
pub use delay::{Delay, DelayParams, MAX_DELAY_SECONDS};
pub use eq::{Eq, EqParams};
pub use limiter::{Limiter, LimiterParams};
pub use reverb::{Reverb, ReverbParams};

/// The settings of any built-in effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectParams {
    /// EQ.
    Eq(EqParams),
    /// Compressor.
    Compressor(CompressorParams),
    /// Reverb.
    Reverb(ReverbParams),
    /// Delay.
    Delay(DelayParams),
    /// Limiter.
    Limiter(LimiterParams),
}

/// Any built-in effect.
pub enum Effect {
    /// EQ.
    Eq(Box<Eq>),
    /// Compressor.
    Compressor(Box<Compressor>),
    /// Reverb.
    Reverb(Box<Reverb>),
    /// Delay.
    Delay(Box<Delay>),
    /// Limiter.
    Limiter(Box<Limiter>),
}

impl Effect {
    /// Creates an effect (allocates; call off the audio thread).
    pub fn new(params: &EffectParams, sample_rate: u32) -> Self {
        match params {
            EffectParams::Eq(p) => Self::Eq(Box::new(Eq::new(p, sample_rate))),
            EffectParams::Compressor(p) => {
                Self::Compressor(Box::new(Compressor::new(p, sample_rate)))
            }
            EffectParams::Reverb(p) => Self::Reverb(Box::new(Reverb::new(p, sample_rate))),
            EffectParams::Delay(p) => Self::Delay(Box::new(Delay::new(p, sample_rate))),
            EffectParams::Limiter(p) => Self::Limiter(Box::new(Limiter::new(p, sample_rate))),
        }
    }

    /// Changes the settings in place. Returns `false` (and changes nothing)
    /// if the settings are for a different kind of effect.
    pub fn set_params(&mut self, params: &EffectParams) -> bool {
        match (self, params) {
            (Self::Eq(e), EffectParams::Eq(p)) => e.set_params(p),
            (Self::Compressor(e), EffectParams::Compressor(p)) => e.set_params(p),
            (Self::Reverb(e), EffectParams::Reverb(p)) => e.set_params(p),
            (Self::Delay(e), EffectParams::Delay(p)) => e.set_params(p),
            (Self::Limiter(e), EffectParams::Limiter(p)) => e.set_params(p),
            _ => return false,
        }
        true
    }

    /// Whether these settings are for this kind of effect.
    pub fn accepts(&self, params: &EffectParams) -> bool {
        matches!(
            (self, params),
            (Self::Eq(_), EffectParams::Eq(_))
                | (Self::Compressor(_), EffectParams::Compressor(_))
                | (Self::Reverb(_), EffectParams::Reverb(_))
                | (Self::Delay(_), EffectParams::Delay(_))
                | (Self::Limiter(_), EffectParams::Limiter(_))
        )
    }

    /// Processes stereo sound in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        match self {
            Self::Eq(e) => e.process(left, right),
            Self::Compressor(e) => e.process(left, right),
            Self::Reverb(e) => e.process(left, right),
            Self::Delay(e) => e.process(left, right),
            Self::Limiter(e) => e.process(left, right),
        }
    }

    /// Silences tails and forgets state.
    pub fn reset(&mut self) {
        match self {
            Self::Eq(e) => e.reset(),
            Self::Compressor(e) => e.reset(),
            Self::Reverb(e) => e.reset(),
            Self::Delay(e) => e.reset(),
            Self::Limiter(e) => e.reset(),
        }
    }

    /// How much the effect is turning the sound down right now, in dB, for
    /// effects that do (compressor, limiter).
    pub fn reduction_db(&self) -> Option<f32> {
        match self {
            Self::Compressor(e) => Some(e.reduction_db()),
            Self::Limiter(e) => Some(e.reduction_db()),
            Self::Eq(_) | Self::Reverb(_) | Self::Delay(_) => None,
        }
    }
}

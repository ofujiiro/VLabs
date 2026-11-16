// SPDX-License-Identifier: GPL-3.0-or-later

//! Mixing: the effects in each track's chain and on the master bus, and
//! their settings, as stored in the project.
//!
//! Every setting has a default, so files that leave one out (older files,
//! files from other programs) still load.

use serde::{Deserialize, Serialize};

use crate::song::EffectId;
use crate::time::Ticks;

/// An effect in a chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    /// Identity.
    pub id: EffectId,
    /// Whether it is heard (a bypassed effect passes the sound unchanged).
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Which effect, with its settings.
    pub kind: EffectKind,
}

fn yes() -> bool {
    true
}

/// The built-in effects, with their settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EffectKind {
    /// EQ.
    Eq(EqSettings),
    /// Compressor.
    Compressor(CompressorSettings),
    /// Reverb.
    Reverb(ReverbSettings),
    /// Delay.
    Delay(DelaySettings),
    /// Limiter.
    Limiter(LimiterSettings),
}

impl EffectKind {
    /// The kinds a person can add, with default settings.
    pub fn all() -> [Self; 5] {
        [
            Self::Eq(EqSettings::default()),
            Self::Compressor(CompressorSettings::default()),
            Self::Reverb(ReverbSettings::default()),
            Self::Delay(DelaySettings::default()),
            Self::Limiter(LimiterSettings::default()),
        ]
    }

    /// Whether two settings are for the same kind of effect.
    pub fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// EQ settings: low cut, low shelf, bell, high shelf, high cut.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqSettings {
    /// Removes rumble below this frequency, if set.
    pub low_cut_hz: Option<f32>,
    /// Low shelf gain, in dB.
    pub low_gain_db: f32,
    /// Low shelf corner frequency.
    pub low_hz: f32,
    /// Bell gain, in dB.
    pub mid_gain_db: f32,
    /// Bell centre frequency.
    pub mid_hz: f32,
    /// Bell width (Q).
    pub mid_q: f32,
    /// High shelf gain, in dB.
    pub high_gain_db: f32,
    /// High shelf corner frequency.
    pub high_hz: f32,
    /// Removes hiss above this frequency, if set.
    pub high_cut_hz: Option<f32>,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            low_cut_hz: None,
            low_gain_db: 0.0,
            low_hz: 120.0,
            mid_gain_db: 0.0,
            mid_hz: 1000.0,
            mid_q: 0.9,
            high_gain_db: 0.0,
            high_hz: 6000.0,
            high_cut_hz: None,
        }
    }
}

/// Compressor settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompressorSettings {
    /// Level above which sound is turned down, in dBFS.
    pub threshold_db: f32,
    /// How much.
    pub ratio: f32,
    /// Reaction time, in milliseconds.
    pub attack_ms: f32,
    /// Letting-go time, in milliseconds.
    pub release_ms: f32,
    /// Soft knee width, in dB.
    pub knee_db: f32,
    /// Gain added afterwards, in dB.
    pub makeup_db: f32,
}

impl Default for CompressorSettings {
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

/// Reverb settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReverbSettings {
    /// Room size, 0 to 1.
    pub size: f32,
    /// How quickly highs fade, 0 to 1.
    pub damping: f32,
    /// Stereo width, 0 to 1.
    pub width: f32,
    /// Dry and room balance, 0 to 1.
    pub mix: f32,
    /// Silence before the room answers, in milliseconds.
    pub pre_delay_ms: f32,
}

impl Default for ReverbSettings {
    fn default() -> Self {
        Self {
            size: 0.6,
            damping: 0.5,
            width: 1.0,
            mix: 0.25,
            pre_delay_ms: 10.0,
        }
    }
}

/// How the delay's time is set.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelayTime {
    /// A note value, following the song's tempo.
    Synced(Ticks),
    /// A fixed time in seconds.
    Seconds(f32),
}

/// Delay settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DelaySettings {
    /// Time between echoes.
    pub time: DelayTime,
    /// How much of each echo comes back, 0 to 0.95.
    pub feedback: f32,
    /// Dry and echo balance, 0 to 1.
    pub mix: f32,
    /// Echoes lose the frequencies above this, in hertz.
    pub tone_hz: f32,
    /// Echoes alternate between left and right.
    pub ping_pong: bool,
}

impl Default for DelaySettings {
    fn default() -> Self {
        Self {
            // A dotted eighth: three sixteenths.
            time: DelayTime::Synced(Ticks::note_value(3, 16)),
            feedback: 0.35,
            mix: 0.25,
            tone_hz: 6000.0,
            ping_pong: false,
        }
    }
}

/// Limiter settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LimiterSettings {
    /// The highest level that comes out, in dBFS.
    pub ceiling_db: f32,
    /// Gain added before limiting, in dB.
    pub gain_db: f32,
    /// Letting-go time, in milliseconds.
    pub release_ms: f32,
}

impl Default for LimiterSettings {
    fn default() -> Self {
        Self {
            ceiling_db: -0.3,
            gain_db: 0.0,
            release_ms: 80.0,
        }
    }
}

/// The master bus: where every track meets before the speakers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MasterBus {
    /// Linear gain: 1 is unchanged.
    pub gain: f32,
    /// The master effect chain, in order.
    pub effects: Vec<Effect>,
}

impl Default for MasterBus {
    fn default() -> Self {
        Self {
            gain: 1.0,
            effects: Vec::new(),
        }
    }
}

/// Where an effect sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainPlace {
    /// A track's chain.
    Track(crate::song::TrackId),
    /// The master bus's chain.
    Master,
}

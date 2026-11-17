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
    /// A third-party plugin effect.
    Plugin(crate::plugin::PluginSetup),
}

/// A setting of a built-in effect that can change over the song: its range,
/// and whether it moves in equal ratios (frequencies, times).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectParam {
    /// The lowest value.
    pub min: f32,
    /// The highest value.
    pub max: f32,
    /// Whether the scale is logarithmic.
    pub log: bool,
}

const fn lin(min: f32, max: f32) -> EffectParam {
    EffectParam {
        min,
        max,
        log: false,
    }
}

const fn log(min: f32, max: f32) -> EffectParam {
    EffectParam {
        min,
        max,
        log: true,
    }
}

const EQ_PARAMS: [EffectParam; 7] = [
    lin(-15.0, 15.0),
    log(40.0, 500.0),
    lin(-15.0, 15.0),
    log(200.0, 8000.0),
    log(0.3, 6.0),
    lin(-15.0, 15.0),
    log(1500.0, 16_000.0),
];
const COMPRESSOR_PARAMS: [EffectParam; 6] = [
    lin(-60.0, 0.0),
    log(1.0, 20.0),
    log(0.1, 100.0),
    log(10.0, 1000.0),
    lin(0.0, 24.0),
    lin(0.0, 24.0),
];
const REVERB_PARAMS: [EffectParam; 5] = [
    lin(0.0, 1.0),
    lin(0.0, 1.0),
    lin(0.0, 1.0),
    lin(0.0, 1.0),
    lin(0.0, 100.0),
];
const DELAY_PARAMS: [EffectParam; 3] = [lin(0.0, 0.95), lin(0.0, 1.0), log(1000.0, 20_000.0)];
const LIMITER_PARAMS: [EffectParam; 3] = [lin(0.0, 18.0), lin(-12.0, 0.0), log(10.0, 1000.0)];

impl EffectParam {
    /// A value as a fraction of the range.
    pub fn fraction(&self, value: f32) -> f32 {
        let fraction = if self.log && self.min > 0.0 {
            (value / self.min).ln() / (self.max / self.min).ln()
        } else {
            (value - self.min) / (self.max - self.min)
        };
        fraction.clamp(0.0, 1.0)
    }

    /// A fraction of the range as a value.
    pub fn value(&self, fraction: f32) -> f32 {
        let fraction = fraction.clamp(0.0, 1.0);
        if self.log && self.min > 0.0 {
            self.min * (self.max / self.min).powf(fraction)
        } else {
            self.min + fraction * (self.max - self.min)
        }
    }
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

    /// Whether two settings are for the same kind of effect (for plugins:
    /// the same plugin).
    pub fn same_kind(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Plugin(a), Self::Plugin(b)) => a.plugin == b.plugin,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }

    /// The settings of a built-in effect that can change over the song, in
    /// order (a plugin lists its own parameters instead).
    pub fn params(&self) -> &'static [EffectParam] {
        match self {
            Self::Eq(_) => &EQ_PARAMS,
            Self::Compressor(_) => &COMPRESSOR_PARAMS,
            Self::Reverb(_) => &REVERB_PARAMS,
            Self::Delay(_) => &DELAY_PARAMS,
            Self::Limiter(_) => &LIMITER_PARAMS,
            Self::Plugin(_) => &[],
        }
    }

    /// The setting a built-in effect's parameter number refers to.
    fn field(&mut self, index: usize) -> Option<&mut f32> {
        Some(match (self, index) {
            (Self::Eq(e), 0) => &mut e.low_gain_db,
            (Self::Eq(e), 1) => &mut e.low_hz,
            (Self::Eq(e), 2) => &mut e.mid_gain_db,
            (Self::Eq(e), 3) => &mut e.mid_hz,
            (Self::Eq(e), 4) => &mut e.mid_q,
            (Self::Eq(e), 5) => &mut e.high_gain_db,
            (Self::Eq(e), 6) => &mut e.high_hz,
            (Self::Compressor(c), 0) => &mut c.threshold_db,
            (Self::Compressor(c), 1) => &mut c.ratio,
            (Self::Compressor(c), 2) => &mut c.attack_ms,
            (Self::Compressor(c), 3) => &mut c.release_ms,
            (Self::Compressor(c), 4) => &mut c.knee_db,
            (Self::Compressor(c), 5) => &mut c.makeup_db,
            (Self::Reverb(r), 0) => &mut r.mix,
            (Self::Reverb(r), 1) => &mut r.size,
            (Self::Reverb(r), 2) => &mut r.damping,
            (Self::Reverb(r), 3) => &mut r.width,
            (Self::Reverb(r), 4) => &mut r.pre_delay_ms,
            (Self::Delay(d), 0) => &mut d.feedback,
            (Self::Delay(d), 1) => &mut d.mix,
            (Self::Delay(d), 2) => &mut d.tone_hz,
            (Self::Limiter(l), 0) => &mut l.gain_db,
            (Self::Limiter(l), 1) => &mut l.ceiling_db,
            (Self::Limiter(l), 2) => &mut l.release_ms,
            _ => return None,
        })
    }

    /// A built-in effect's setting, as a fraction of its range.
    pub fn param(&self, index: usize) -> Option<f32> {
        let spec = *self.params().get(index)?;
        let mut copy = self.clone();
        copy.field(index).map(|v| spec.fraction(*v))
    }

    /// Changes a built-in effect's setting from a fraction of its range.
    pub fn set_param(&mut self, index: usize, fraction: f32) {
        let Some(&spec) = self.params().get(index) else {
            return;
        };
        if let Some(field) = self.field(index) {
            *field = spec.value(fraction);
        }
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

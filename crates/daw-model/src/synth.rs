// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in synth's settings (decision 0019), as the song keeps them:
//! two oscillators and noise, a filter, two envelopes, an LFO, how it
//! plays, and its presets. Names for people live in the interface's text;
//! this module deals in numbers and choices.
//!
//! Every setting has a default, so a file that leaves one out still loads.
//! The continuous settings are numbered ([`SynthSettings::params`]) so
//! automation lanes can change them; the numbers never change meaning.

use serde::{Deserialize, Serialize};

use crate::mixer::EffectParam;

/// An oscillator's wave.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthWave {
    /// A pure tone.
    Sine,
    /// Soft and hollow.
    Triangle,
    /// Bright and buzzy.
    #[default]
    Saw,
    /// Hollow and reedy; the pulse width changes its colour.
    Square,
}

impl SynthWave {
    /// Every wave, in the order the interface offers them.
    pub const ALL: [Self; 4] = [Self::Sine, Self::Triangle, Self::Saw, Self::Square];
}

/// The filter's kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthFilter {
    /// Takes away the highs, steeply (24 dB per octave).
    #[default]
    LowPass24,
    /// Takes away the highs, gently (12 dB per octave).
    LowPass12,
    /// Takes away the lows.
    HighPass,
    /// Keeps a band around the cutoff.
    BandPass,
}

impl SynthFilter {
    /// Every kind, in the order the interface offers them.
    pub const ALL: [Self; 4] = [
        Self::LowPass24,
        Self::LowPass12,
        Self::HighPass,
        Self::BandPass,
    ];
}

/// The LFO's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LfoShape {
    /// Smooth.
    #[default]
    Sine,
    /// Straight up and down.
    Triangle,
    /// Jumps between two values.
    Square,
    /// Falls, then jumps back up.
    Saw,
    /// A new random value each cycle.
    Random,
}

impl LfoShape {
    /// Every shape, in the order the interface offers them.
    pub const ALL: [Self; 5] = [
        Self::Sine,
        Self::Triangle,
        Self::Square,
        Self::Saw,
        Self::Random,
    ];
}

/// One oscillator.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthOscillator {
    /// Its wave.
    pub wave: SynthWave,
    /// Octaves above or below the key played, from -3 to 3.
    pub octave: i8,
    /// Semitones above or below, from -12 to 12.
    pub semitones: i8,
    /// A fine tune, in cents, from -100 to 100.
    pub fine: f32,
    /// Its level, from 0 to 1.
    pub level: f32,
}

impl Default for SynthOscillator {
    fn default() -> Self {
        Self {
            wave: SynthWave::Saw,
            octave: 0,
            semitones: 0,
            fine: 0.0,
            level: 1.0,
        }
    }
}

impl SynthOscillator {
    /// The oscillator's pitch above the key played, in semitones.
    pub fn pitch(&self) -> f32 {
        f32::from(self.octave.clamp(-3, 3)) * 12.0
            + f32::from(self.semitones.clamp(-12, 12))
            + self.fine.clamp(-100.0, 100.0) / 100.0
    }
}

/// An envelope: times in seconds, the sustain as a level.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthEnvelope {
    /// Time to rise from silence to the top.
    pub attack: f32,
    /// Time to fall from the top to the sustain level.
    pub decay: f32,
    /// The level held while the key is down, from 0 to 1.
    pub sustain: f32,
    /// Time to fall to silence once the key is up.
    pub release: f32,
}

impl Default for SynthEnvelope {
    fn default() -> Self {
        Self {
            attack: 0.002,
            decay: 0.3,
            sustain: 1.0,
            release: 0.05,
        }
    }
}

/// Everything the built-in synth's sound depends on. Plain data, cheap to
/// copy (the audio thread copies it to apply automation).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthSettings {
    /// The first oscillator.
    pub osc1: SynthOscillator,
    /// The second oscillator.
    pub osc2: SynthOscillator,
    /// The square waves' pulse width, from 0.05 to 0.95 (0.5 is square).
    pub pulse_width: f32,
    /// The noise level, from 0 to 1.
    pub noise: f32,
    /// The filter's kind.
    pub filter: SynthFilter,
    /// The filter's cutoff, in hertz (20 to 20 000).
    pub cutoff_hz: f32,
    /// The filter's resonance, from 0 to 1.
    pub resonance: f32,
    /// How far the filter envelope moves the cutoff, in octaves (-6 to 6).
    pub envelope_amount: f32,
    /// How much the cutoff follows the key played, from 0 to 1.
    pub key_tracking: f32,
    /// The volume envelope.
    pub amp_envelope: SynthEnvelope,
    /// The filter envelope.
    pub filter_envelope: SynthEnvelope,
    /// The LFO's shape.
    pub lfo_shape: LfoShape,
    /// The LFO's speed, in hertz (0.05 to 20).
    pub lfo_rate_hz: f32,
    /// How far the LFO moves the pitch, in cents (0 to 100).
    pub vibrato_cents: f32,
    /// How far the LFO moves the cutoff, in octaves (0 to 4).
    pub wobble_octaves: f32,
    /// How deep the LFO dips the volume, from 0 to 1.
    pub tremolo: f32,
    /// One voice only, gliding between notes.
    pub mono: bool,
    /// How long a glide takes, in seconds (0 to 2; mono only).
    pub glide_seconds: f32,
    /// How much velocity changes loudness and brightness, from 0 to 1.
    pub velocity: f32,
    /// The output level, in dB (-24 to 6).
    pub volume_db: f32,
}

impl Default for SynthSettings {
    /// The "Plain" preset: one saw through an open filter.
    fn default() -> Self {
        Self {
            osc1: SynthOscillator::default(),
            osc2: SynthOscillator {
                level: 0.0,
                ..SynthOscillator::default()
            },
            pulse_width: 0.5,
            noise: 0.0,
            filter: SynthFilter::LowPass24,
            cutoff_hz: 20_000.0,
            resonance: 0.0,
            envelope_amount: 0.0,
            key_tracking: 0.0,
            amp_envelope: SynthEnvelope::default(),
            filter_envelope: SynthEnvelope::default(),
            lfo_shape: LfoShape::Sine,
            lfo_rate_hz: 5.0,
            vibrato_cents: 0.0,
            wobble_octaves: 0.0,
            tremolo: 0.0,
            mono: false,
            glide_seconds: 0.0,
            velocity: 0.5,
            volume_db: -6.0,
        }
    }
}

/// The automatable settings, in their fixed order: their ranges.
const PARAMS: [EffectParam; 25] = [
    log(20.0, 20_000.0), // 0 cutoff
    lin(0.0, 1.0),       // 1 resonance
    lin(-6.0, 6.0),      // 2 envelope amount
    lin(0.0, 1.0),       // 3 key tracking
    lin(0.0, 1.0),       // 4 oscillator 1 level
    lin(-100.0, 100.0),  // 5 oscillator 1 fine tune
    lin(0.0, 1.0),       // 6 oscillator 2 level
    lin(-100.0, 100.0),  // 7 oscillator 2 fine tune
    lin(0.0, 1.0),       // 8 noise
    lin(0.05, 0.95),     // 9 pulse width
    log(0.001, 10.0),    // 10 attack
    log(0.001, 10.0),    // 11 decay
    lin(0.0, 1.0),       // 12 sustain
    log(0.001, 10.0),    // 13 release
    log(0.001, 10.0),    // 14 filter attack
    log(0.001, 10.0),    // 15 filter decay
    lin(0.0, 1.0),       // 16 filter sustain
    log(0.001, 10.0),    // 17 filter release
    log(0.05, 20.0),     // 18 LFO rate
    lin(0.0, 100.0),     // 19 vibrato
    lin(0.0, 4.0),       // 20 wobble
    lin(0.0, 1.0),       // 21 tremolo
    lin(0.0, 2.0),       // 22 glide
    lin(0.0, 1.0),       // 23 velocity
    lin(-24.0, 6.0),     // 24 volume
];

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

impl SynthSettings {
    /// The settings automation can change, by number: their ranges.
    pub fn params() -> &'static [EffectParam] {
        &PARAMS
    }

    /// A setting's value by number.
    fn slot(&mut self, index: usize) -> Option<&mut f32> {
        Some(match index {
            0 => &mut self.cutoff_hz,
            1 => &mut self.resonance,
            2 => &mut self.envelope_amount,
            3 => &mut self.key_tracking,
            4 => &mut self.osc1.level,
            5 => &mut self.osc1.fine,
            6 => &mut self.osc2.level,
            7 => &mut self.osc2.fine,
            8 => &mut self.noise,
            9 => &mut self.pulse_width,
            10 => &mut self.amp_envelope.attack,
            11 => &mut self.amp_envelope.decay,
            12 => &mut self.amp_envelope.sustain,
            13 => &mut self.amp_envelope.release,
            14 => &mut self.filter_envelope.attack,
            15 => &mut self.filter_envelope.decay,
            16 => &mut self.filter_envelope.sustain,
            17 => &mut self.filter_envelope.release,
            18 => &mut self.lfo_rate_hz,
            19 => &mut self.vibrato_cents,
            20 => &mut self.wobble_octaves,
            21 => &mut self.tremolo,
            22 => &mut self.glide_seconds,
            23 => &mut self.velocity,
            24 => &mut self.volume_db,
            _ => return None,
        })
    }

    /// An automatable setting as a fraction of its range, by number.
    pub fn param(&self, index: usize) -> Option<f32> {
        let spec = PARAMS.get(index)?;
        let mut copy = *self;
        copy.slot(index).map(|value| spec.fraction(*value))
    }

    /// Sets an automatable setting from a fraction of its range.
    pub fn set_param(&mut self, index: usize, fraction: f32) {
        if let (Some(spec), Some(value)) = (PARAMS.get(index), self.slot(index)) {
            *value = spec.value(fraction);
        }
    }

    /// The same settings with every number in its range (a file written by
    /// hand or by another program may say anything).
    #[must_use]
    pub fn repaired(mut self) -> Self {
        for (index, spec) in PARAMS.iter().enumerate() {
            if let Some(value) = self.slot(index) {
                *value = if value.is_finite() {
                    value.clamp(spec.min, spec.max)
                } else {
                    spec.min
                };
            }
        }
        for osc in [&mut self.osc1, &mut self.osc2] {
            osc.octave = osc.octave.clamp(-3, 3);
            osc.semitones = osc.semitones.clamp(-12, 12);
        }
        self
    }
}

/// The synth's presets (decision 0019). Their names are in the interface's
/// text; a new synth track starts with [`SynthPreset::SoftKeys`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SynthPreset {
    /// Mellow keys for chords and melodies.
    SoftKeys,
    /// Slow, wide and warm.
    WarmPad,
    /// A punchy bass line.
    Bass,
    /// A deep, round bass.
    SubBass,
    /// A singing solo line, with vibrato and glide.
    Lead,
    /// Short and bright, like a plucked string.
    Pluck,
    /// Brassy swells.
    Brass,
    /// A bare saw to start from.
    Plain,
}

impl SynthPreset {
    /// Every preset, in the order the interface offers them.
    pub const ALL: [Self; 8] = [
        Self::SoftKeys,
        Self::WarmPad,
        Self::Bass,
        Self::SubBass,
        Self::Lead,
        Self::Pluck,
        Self::Brass,
        Self::Plain,
    ];

    /// The preset's settings.
    #[allow(clippy::too_many_lines, reason = "one table of sounds")]
    pub fn settings(self) -> SynthSettings {
        let plain = SynthSettings::default();
        let osc = |wave, octave, fine, level| SynthOscillator {
            wave,
            octave,
            semitones: 0,
            fine,
            level,
        };
        let env = |attack, decay, sustain, release| SynthEnvelope {
            attack,
            decay,
            sustain,
            release,
        };
        match self {
            Self::Plain => plain,
            Self::SoftKeys => SynthSettings {
                osc1: osc(SynthWave::Triangle, 0, 0.0, 0.8),
                osc2: osc(SynthWave::Saw, 0, 6.0, 0.25),
                cutoff_hz: 1800.0,
                resonance: 0.1,
                envelope_amount: 1.5,
                key_tracking: 0.5,
                amp_envelope: env(0.003, 1.4, 0.35, 0.35),
                filter_envelope: env(0.002, 0.6, 0.2, 0.3),
                velocity: 0.7,
                volume_db: -3.0,
                ..plain
            },
            Self::WarmPad => SynthSettings {
                osc1: osc(SynthWave::Saw, 0, -7.0, 0.8),
                osc2: osc(SynthWave::Saw, 0, 7.0, 0.8),
                cutoff_hz: 900.0,
                resonance: 0.15,
                envelope_amount: 1.0,
                key_tracking: 0.3,
                amp_envelope: env(0.6, 1.0, 0.85, 1.4),
                filter_envelope: env(0.8, 1.5, 0.6, 1.2),
                lfo_rate_hz: 0.3,
                vibrato_cents: 4.0,
                wobble_octaves: 0.3,
                velocity: 0.3,
                volume_db: -8.0,
                ..plain
            },
            Self::Bass => SynthSettings {
                osc1: osc(SynthWave::Saw, -1, 0.0, 1.0),
                osc2: osc(SynthWave::Square, -2, 0.0, 0.6),
                cutoff_hz: 180.0,
                resonance: 0.25,
                envelope_amount: 3.5,
                amp_envelope: env(0.002, 0.3, 0.7, 0.08),
                filter_envelope: env(0.001, 0.22, 0.1, 0.1),
                mono: true,
                velocity: 0.5,
                volume_db: -3.0,
                ..plain
            },
            Self::SubBass => SynthSettings {
                osc1: osc(SynthWave::Sine, -1, 0.0, 1.0),
                osc2: osc(SynthWave::Triangle, -1, 0.0, 0.3),
                filter: SynthFilter::LowPass12,
                cutoff_hz: 400.0,
                amp_envelope: env(0.003, 0.5, 0.9, 0.1),
                mono: true,
                glide_seconds: 0.03,
                velocity: 0.3,
                volume_db: -2.0,
                ..plain
            },
            Self::Lead => SynthSettings {
                osc1: osc(SynthWave::Square, 0, 0.0, 0.8),
                osc2: osc(SynthWave::Saw, 0, 10.0, 0.7),
                filter: SynthFilter::LowPass12,
                cutoff_hz: 2400.0,
                resonance: 0.3,
                envelope_amount: 1.0,
                key_tracking: 0.5,
                amp_envelope: env(0.005, 0.2, 0.8, 0.2),
                filter_envelope: env(0.01, 0.3, 0.5, 0.3),
                lfo_rate_hz: 5.5,
                vibrato_cents: 12.0,
                mono: true,
                glide_seconds: 0.05,
                velocity: 0.4,
                volume_db: -8.0,
                ..plain
            },
            Self::Pluck => SynthSettings {
                osc1: osc(SynthWave::Saw, 0, 0.0, 1.0),
                osc2: osc(SynthWave::Square, 1, 0.0, 0.3),
                cutoff_hz: 300.0,
                resonance: 0.2,
                envelope_amount: 4.5,
                key_tracking: 0.5,
                amp_envelope: env(0.001, 0.45, 0.0, 0.3),
                filter_envelope: env(0.001, 0.18, 0.0, 0.2),
                velocity: 0.8,
                volume_db: -4.0,
                ..plain
            },
            Self::Brass => SynthSettings {
                osc1: osc(SynthWave::Saw, 0, -5.0, 0.9),
                osc2: osc(SynthWave::Saw, 0, 5.0, 0.9),
                cutoff_hz: 500.0,
                resonance: 0.1,
                envelope_amount: 2.5,
                key_tracking: 0.4,
                amp_envelope: env(0.06, 0.3, 0.8, 0.25),
                filter_envelope: env(0.09, 0.5, 0.45, 0.3),
                lfo_rate_hz: 5.0,
                vibrato_cents: 6.0,
                velocity: 0.6,
                volume_db: -8.0,
                ..plain
            },
        }
    }

    /// The preset these settings are, if they are one exactly.
    pub fn matching(settings: &SynthSettings) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.settings() == *settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_round_trip_through_fractions() {
        let mut settings = SynthPreset::Lead.settings();
        for index in 0..SynthSettings::params().len() {
            let fraction = settings.param(index).unwrap();
            assert!((0.0..=1.0).contains(&fraction), "{index}");
            let before = settings;
            settings.set_param(index, fraction);
            let spec = SynthSettings::params()[index];
            let mut a = before;
            let mut b = settings;
            let (a, b) = (*a.slot(index).unwrap(), *b.slot(index).unwrap());
            assert!(
                (a - b).abs() <= (spec.max - spec.min).abs() * 1e-4,
                "setting {index}: {a} became {b}"
            );
        }
        assert_eq!(settings.param(SynthSettings::params().len()), None);
        settings.set_param(99, 0.5); // ignored
    }

    #[test]
    fn presets_are_distinct_and_recognised() {
        for preset in SynthPreset::ALL {
            assert_eq!(SynthPreset::matching(&preset.settings()), Some(preset));
            assert_eq!(
                preset.settings().repaired(),
                preset.settings(),
                "{preset:?}"
            );
        }
        let mut changed = SynthPreset::Bass.settings();
        changed.cutoff_hz = 1234.0;
        assert_eq!(SynthPreset::matching(&changed), None);
    }

    #[test]
    fn repairing_keeps_numbers_in_range() {
        let wild = SynthSettings {
            cutoff_hz: f32::NAN,
            resonance: 7.0,
            osc1: SynthOscillator {
                octave: 100,
                ..SynthOscillator::default()
            },
            volume_db: -1000.0,
            ..SynthSettings::default()
        };
        let fixed = wild.repaired();
        assert_eq!(fixed.cutoff_hz, 20.0);
        assert_eq!(fixed.resonance, 1.0);
        assert_eq!(fixed.osc1.octave, 3);
        assert_eq!(fixed.volume_db, -24.0);
    }

    #[test]
    fn missing_settings_take_their_defaults() {
        let settings: SynthSettings =
            serde_json::from_str(r#"{ "cutoff_hz": 500.0, "osc1": { "wave": "square" } }"#)
                .unwrap();
        assert_eq!(settings.cutoff_hz, 500.0);
        assert_eq!(settings.osc1.wave, SynthWave::Square);
        assert_eq!(settings.osc1.level, 1.0);
        assert_eq!(settings.amp_envelope, SynthEnvelope::default());
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! A minimal sine-wave instrument: the sound used when no other instrument
//! is available, and a precise reference in tests.

use crate::{Instrument, key_to_hz};

/// The most notes the sine synth plays at once.
const VOICES: usize = 32;

#[derive(Clone, Copy, Default)]
struct Voice {
    key: u8,
    active: bool,
    releasing: bool,
    phase: f32,
    step: f32,
    level: f32,
    target: f32,
}

/// A polyphonic sine synth with short attack and release ramps (no clicks).
pub struct SineSynth {
    voices: [Voice; VOICES],
    sample_rate: f32,
    ramp: f32,
}

impl SineSynth {
    /// A sine synth for the given sample rate.
    pub fn new(sample_rate: u32) -> Self {
        #[allow(
            clippy::cast_precision_loss,
            reason = "sample rates are small integers"
        )]
        let sample_rate = sample_rate as f32;
        Self {
            voices: [Voice::default(); VOICES],
            sample_rate,
            ramp: 1.0 / (0.005 * sample_rate),
        }
    }
}

impl Instrument for SineSynth {
    fn note_on(&mut self, key: u8, velocity: f32) {
        let index = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                (0..VOICES)
                    .min_by(|&a, &b| self.voices[a].level.total_cmp(&self.voices[b].level))
                    .unwrap_or(0)
            });
        self.voices[index] = Voice {
            key,
            active: true,
            releasing: false,
            phase: 0.0,
            step: key_to_hz(f32::from(key)) / self.sample_rate,
            level: 0.0,
            target: 0.25 * velocity.clamp(0.0, 1.0),
        };
    }

    fn note_off(&mut self, key: u8) {
        for voice in self.voices.iter_mut().filter(|v| v.active && v.key == key) {
            voice.releasing = true;
            voice.target = 0.0;
        }
    }

    fn all_notes_off(&mut self, immediate: bool) {
        for voice in &mut self.voices {
            if immediate {
                voice.active = false;
            } else {
                voice.releasing = true;
                voice.target = 0.0;
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        for voice in self.voices.iter_mut().filter(|v| v.active) {
            for sample in left.iter_mut() {
                let delta = (voice.target - voice.level).clamp(-self.ramp, self.ramp);
                voice.level += delta;
                *sample += (voice.phase * std::f32::consts::TAU).sin() * voice.level;
                voice.phase = (voice.phase + voice.step).fract();
            }
            if voice.releasing && voice.level <= 0.0 {
                voice.active = false;
            }
        }
        right.copy_from_slice(left);
    }

    fn is_sounding(&self) -> bool {
        self.voices.iter().any(|v| v.active)
    }
}

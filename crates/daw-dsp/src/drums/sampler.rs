// SPDX-License-Identifier: GPL-3.0-or-later

//! The drum sampler: plays a kit's sounds as one-shots, picking the
//! velocity layer for each hit, with per-pad volume, position, pitch and
//! length, and hi-hats that cut each other off.

use super::{DrumKit, PadParams, PadSound, PadTable};
use crate::instrument::InstrumentSettings;
use crate::{Instrument, db_to_gain};

/// The most sounds the sampler plays at once.
pub const VOICES: usize = 48;

/// How long a sound cut off by another (hi-hats) takes to fade, in seconds.
const CHOKE_FADE: f32 = 0.012;
/// How long the end of a sound fades when it plays in full, in seconds.
const END_FADE: f32 = 0.004;

#[derive(Clone, Copy, Default)]
struct Voice {
    active: bool,
    pad: usize,
    layer: usize,
    choke: Option<u8>,
    position: f64,
    step: f64,
    end: f64,
    fade_start: f64,
    zone_gain: f32,
    /// Fades from 1 to 0 when the voice is cut off.
    release: f32,
    release_step: f32,
    /// When it started, to find the oldest voice.
    started: u64,
}

/// Plays a [`DrumKit`].
pub struct DrumSampler {
    kit: DrumKit,
    /// The pad index for each key.
    pads_by_key: [Option<usize>; 128],
    params: Box<PadTable>,
    voices: [Voice; VOICES],
    sample_rate: f32,
    hits: u64,
}

impl DrumSampler {
    /// A sampler for a kit at the engine's sample rate.
    pub fn new(kit: DrumKit, sample_rate: u32) -> Self {
        let mut pads_by_key = [None; 128];
        for (key, slot) in pads_by_key.iter_mut().enumerate() {
            let key = u8::try_from(key).unwrap_or(0);
            if let Some(sound) = PadSound::from_key(key) {
                *slot = kit.pads.iter().position(|p| p.sound == sound);
            }
        }
        #[allow(clippy::cast_precision_loss, reason = "sample rates are small")]
        let sample_rate = sample_rate as f32;
        Self {
            kit,
            pads_by_key,
            params: Box::new([PadParams::default(); 128]),
            voices: [Voice::default(); VOICES],
            sample_rate,
            hits: 0,
        }
    }

    /// The settings of the pad a key plays (shared by all its keys).
    fn params_for(&self, pad: usize) -> PadParams {
        self.params[usize::from(self.kit.pads[pad].sound.key())]
    }

    /// Changes every pad's settings (indexed by the pad's main key).
    pub fn set_pads(&mut self, table: &PadTable) {
        self.params.copy_from_slice(table);
    }

    fn free_voice(&self) -> usize {
        if let Some(index) = self.voices.iter().position(|v| !v.active) {
            return index;
        }
        // Steal the oldest sound.
        (0..VOICES)
            .min_by_key(|&i| self.voices[i].started)
            .unwrap_or(0)
    }

    fn fade_out(voice: &mut Voice, seconds: f32, sample_rate: f32) {
        voice.release_step = voice
            .release_step
            .max(1.0 / (seconds * sample_rate).max(1.0));
    }
}

impl Instrument for DrumSampler {
    fn note_on(&mut self, key: u8, velocity: f32) {
        let Some(pad_index) = self.pads_by_key[usize::from(key & 0x7F)] else {
            return;
        };
        let params = self.params_for(pad_index);
        if params.muted {
            return;
        }
        let pad = &self.kit.pads[pad_index];
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 1..=127"
        )]
        let velocity = (velocity.clamp(0.0, 1.0) * 127.0).round().clamp(1.0, 127.0) as u8;
        let Some(layer_index) = pad
            .layers
            .iter()
            .position(|l| velocity <= l.top)
            .or(pad.layers.len().checked_sub(1))
        else {
            return;
        };
        let layer = &pad.layers[layer_index];
        let bottom = layer_index
            .checked_sub(1)
            .map_or(1, |i| pad.layers[i].top.saturating_add(1));
        let span = f32::from(layer.top.saturating_sub(bottom)).max(1.0);
        let within = (f32::from(velocity.saturating_sub(bottom)) / span).clamp(0.0, 1.0);
        let zone_gain = 0.5 + 0.5 * within;

        // Hi-hats cut each other off.
        let choke = pad.sound.choke_group();
        if let Some(group) = choke {
            for voice in self
                .voices
                .iter_mut()
                .filter(|v| v.active && v.choke == Some(group))
            {
                Self::fade_out(voice, CHOKE_FADE, self.sample_rate);
            }
        }

        let sample = &layer.sample;
        #[allow(clippy::cast_precision_loss, reason = "sample lengths fit in f64")]
        let full = sample.len() as f64;
        let length = f64::from(params.length.clamp(0.05, 1.0));
        let end = (full * length).max(2.0) - 1.0;
        let fade = if length < 1.0 {
            end * 0.3
        } else {
            f64::from(END_FADE * self.sample_rate)
        };
        let step = f64::from(sample.sample_rate) / f64::from(self.sample_rate)
            * f64::from(2.0_f32.powf(params.tune.clamp(-24.0, 24.0) / 12.0));
        self.hits += 1;
        let index = self.free_voice();
        self.voices[index] = Voice {
            active: true,
            pad: pad_index,
            layer: layer_index,
            choke,
            position: 0.0,
            step,
            end,
            fade_start: (end - fade).max(0.0),
            zone_gain,
            release: 1.0,
            release_step: 0.0,
            started: self.hits,
        };
    }

    fn note_off(&mut self, _key: u8) {
        // Drums are one-shots: a hit plays out whatever the key does.
    }

    fn all_notes_off(&mut self, immediate: bool) {
        if immediate {
            for voice in self.voices.iter_mut().filter(|v| v.active) {
                Self::fade_out(voice, 0.003, self.sample_rate);
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        for index in 0..VOICES {
            if !self.voices[index].active {
                continue;
            }
            let pad = self.voices[index].pad;
            let params = self.params_for(pad);
            let voice = &mut self.voices[index];
            let sample = &self.kit.pads[pad].layers[voice.layer].sample;
            let (source_left, source_right) = (&sample.left[..], sample.right_or_left());
            let gain = if params.muted {
                0.0
            } else {
                db_to_gain(params.gain_db.clamp(-60.0, 24.0)) * voice.zone_gain
            };
            let pan = params.pan.clamp(-1.0, 1.0);
            let gain_left = gain * (1.0 - pan).min(1.0);
            let gain_right = gain * (1.0 + pan).min(1.0);
            for (out_left, out_right) in left.iter_mut().zip(right.iter_mut()) {
                if voice.position >= voice.end || voice.release <= 0.0 {
                    voice.active = false;
                    break;
                }
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "inside the sample"
                )]
                let whole = voice.position as usize;
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_precision_loss,
                    reason = "a fraction of a sample position"
                )]
                let fraction = (voice.position - whole as f64) as f32;
                let next = (whole + 1).min(source_left.len() - 1);
                let l = source_left[whole] + (source_left[next] - source_left[whole]) * fraction;
                let r = source_right[whole] + (source_right[next] - source_right[whole]) * fraction;
                let mut envelope = voice.release;
                if voice.position > voice.fade_start {
                    #[allow(clippy::cast_possible_truncation, reason = "a ratio")]
                    let tail = ((voice.end - voice.position)
                        / (voice.end - voice.fade_start).max(1.0))
                        as f32;
                    envelope *= tail.clamp(0.0, 1.0);
                }
                *out_left += l * gain_left * envelope;
                *out_right += r * gain_right * envelope;
                voice.position += voice.step;
                if voice.release_step > 0.0 {
                    voice.release -= voice.release_step;
                }
            }
        }
    }

    fn is_sounding(&self) -> bool {
        self.voices.iter().any(|v| v.active)
    }

    fn apply_settings(&mut self, settings: &InstrumentSettings) {
        if let InstrumentSettings::Drums(table) = settings {
            self.set_pads(table);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::drums::{Layer, Pad, Sample};

    fn kit() -> DrumKit {
        let tone = |frequency: f32, level: f32| {
            Arc::new(Sample {
                sample_rate: 48_000,
                left: (0..4800)
                    .map(|i| {
                        #[allow(clippy::cast_precision_loss, reason = "test values")]
                        let t = i as f32 / 48_000.0;
                        level * (std::f32::consts::TAU * frequency * t).sin()
                    })
                    .collect(),
                right: Vec::new(),
            })
        };
        DrumKit {
            pads: vec![
                Pad {
                    sound: PadSound::Kick,
                    layers: vec![
                        Layer {
                            top: 64,
                            sample: tone(60.0, 0.3),
                        },
                        Layer {
                            top: 127,
                            sample: tone(60.0, 0.9),
                        },
                    ],
                },
                Pad {
                    sound: PadSound::ClosedHat,
                    layers: vec![Layer {
                        top: 127,
                        sample: tone(8000.0, 0.5),
                    }],
                },
                Pad {
                    sound: PadSound::OpenHat,
                    layers: vec![Layer {
                        top: 127,
                        sample: tone(6000.0, 0.5),
                    }],
                },
            ],
        }
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    #[test]
    fn hits_pick_their_layer_and_play_out() {
        let mut sampler = DrumSampler::new(kit(), 48_000);
        let (mut l, mut r) = (vec![0.0; 1000], vec![0.0; 1000]);
        sampler.note_on(36, 0.3);
        sampler.render(&mut l, &mut r);
        let soft = peak(&l);
        assert!(soft > 0.1 && soft <= 0.3, "the soft layer plays: {soft}");
        let mut loud = DrumSampler::new(kit(), 48_000);
        loud.note_on(36, 1.0);
        loud.render(&mut l, &mut r);
        assert!(peak(&l) > 0.8, "the loud layer plays");
        assert_eq!(l, r, "a mono sound sits in the middle");

        // A one-shot ignores the key coming up, and stops at its end.
        loud.note_off(36);
        for _ in 0..10 {
            loud.render(&mut l, &mut r);
        }
        assert!(!loud.is_sounding(), "finished after its 4800 frames");
        assert!(peak(&l) < 1e-6);
    }

    #[test]
    fn the_closed_hat_cuts_the_open_one() {
        let mut sampler = DrumSampler::new(kit(), 48_000);
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        sampler.note_on(46, 1.0);
        sampler.render(&mut l, &mut r);
        sampler.note_on(42, 1.0);
        for _ in 0..4 {
            sampler.render(&mut l, &mut r);
        }
        let open_voices = sampler
            .voices
            .iter()
            .filter(|v| v.active && sampler.kit.pads[v.pad].sound == PadSound::OpenHat)
            .count();
        assert_eq!(open_voices, 0, "the open hat was cut off within 12 ms");
    }

    #[test]
    fn pad_settings_change_volume_position_and_pitch() {
        let mut table = [PadParams::default(); 128];
        table[36] = PadParams {
            gain_db: -6.0,
            pan: 1.0,
            tune: 12.0,
            ..PadParams::default()
        };
        let mut sampler = DrumSampler::new(kit(), 48_000);
        sampler.apply_settings(&InstrumentSettings::Drums(Box::new(table)));
        let (mut l, mut r) = (vec![0.0; 2400], vec![0.0; 2400]);
        sampler.note_on(36, 1.0);
        sampler.render(&mut l, &mut r);
        assert!(peak(&l) < 1e-6, "panned fully right");
        assert!(
            (peak(&r) - 0.9 * 0.501).abs() < 0.02,
            "6 dB down: {}",
            peak(&r)
        );
        sampler.render(&mut l, &mut r);
        assert!(!sampler.is_sounding(), "an octave up plays twice as fast");

        table[36].muted = true;
        sampler.set_pads(&table);
        sampler.note_on(36, 1.0);
        assert!(!sampler.is_sounding(), "a muted pad stays silent");
    }

    #[test]
    fn other_keys_for_the_same_sound_play_it() {
        let mut sampler = DrumSampler::new(kit(), 48_000);
        sampler.note_on(35, 1.0);
        assert!(sampler.is_sounding(), "35 is the other kick key");
        let mut silent = DrumSampler::new(kit(), 48_000);
        silent.note_on(38, 1.0);
        assert!(!silent.is_sounding(), "this kit has no snare");
    }
}

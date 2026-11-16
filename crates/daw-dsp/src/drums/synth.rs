// SPDX-License-Identifier: GPL-3.0-or-later

//! The electronic kit, synthesised by our own code in the spirit of classic
//! analogue drum machines: pitch-swept sines for kick and toms, filtered
//! noise for snares, claps and cymbals, clusters of square waves for
//! metallic hats and the cowbell. Generated once when the kit is loaded,
//! deterministically (the same every time), and dedicated to the public
//! domain like the rest of the bundled sounds.

use std::f32::consts::TAU;
use std::sync::Arc;

use super::{DrumKit, Layer, Pad, PadSound, Sample};
use crate::effects::{Biquad, FilterKind};

/// The six detuned square waves of a metallic cymbal sound (in hertz),
/// as used by many analogue drum machines.
const METAL: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

/// A small, fast, deterministic noise source (xorshift).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        #[allow(clippy::cast_precision_loss, reason = "noise values")]
        let value = self.0 as f32 / u32::MAX as f32;
        value * 2.0 - 1.0
    }
}

struct Synth {
    rate: f32,
}

impl Synth {
    fn frames(&self, seconds: f32) -> usize {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "short durations"
        )]
        let frames = (seconds * self.rate) as usize;
        frames
    }

    fn time(&self, frame: usize) -> f32 {
        #[allow(clippy::cast_precision_loss, reason = "short durations")]
        let t = frame as f32 / self.rate;
        t
    }

    /// A band-pass made of a high-pass and a low-pass in series.
    fn band(&self, low: f32, high: f32) -> (Biquad, Biquad) {
        let mut hp = Biquad::default();
        let mut lp = Biquad::default();
        hp.set(
            FilterKind::HighPass,
            f64::from(self.rate),
            f64::from(low),
            0.707,
            0.0,
        );
        lp.set(
            FilterKind::LowPass,
            f64::from(self.rate),
            f64::from(high),
            0.707,
            0.0,
        );
        (hp, lp)
    }

    /// Sum of the metallic square waves, scaled in pitch.
    fn metal(&self, frame: usize, scale: f32) -> f32 {
        let t = self.time(frame);
        METAL
            .iter()
            .map(|f| {
                if (t * f * scale).fract() < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            })
            .sum::<f32>()
            / 6.0
    }

    fn sample(&self, left: Vec<f32>, right: Vec<f32>, peak: f32) -> Arc<Sample> {
        let mut sample = Sample {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a sample rate"
            )]
            sample_rate: self.rate as u32,
            left,
            right,
        };
        // A short fade at the end, then the wanted loudness.
        let fade = self.frames(0.005).min(sample.len());
        let len = sample.len();
        for i in 0..fade {
            #[allow(clippy::cast_precision_loss, reason = "short fades")]
            let gain = i as f32 / fade as f32;
            sample.left[len - 1 - i] *= gain;
            if let Some(r) = sample.right.get_mut(len - 1 - i) {
                *r *= gain;
            }
        }
        let current = sample.peak().max(1e-9);
        let scale = peak / current;
        sample.left.iter_mut().for_each(|s| *s *= scale);
        sample.right.iter_mut().for_each(|s| *s *= scale);
        Arc::new(sample)
    }

    fn kick(&self) -> Arc<Sample> {
        let mut noise = Noise(0x1234_5678);
        let mut phase = 0.0;
        let out = (0..self.frames(0.7))
            .map(|i| {
                let t = self.time(i);
                let frequency = 46.0 + 110.0 * (-t / 0.035).exp();
                phase += frequency / self.rate;
                let body = (phase * TAU).sin() * (-t / 0.22).exp();
                let click = noise.next() * (-t / 0.0015).exp() * 0.25;
                ((body + click) * 1.6).tanh()
            })
            .collect();
        self.sample(out, Vec::new(), 0.95)
    }

    fn snare(&self) -> Arc<Sample> {
        let mut noise = Noise(0x0bad_cafe);
        let (mut hp, mut lp) = self.band(1200.0, 9000.0);
        let out = (0..self.frames(0.35))
            .map(|i| {
                let t = self.time(i);
                let tone = 0.55 * (TAU * 185.0 * t).sin() * (-t / 0.05).exp()
                    + 0.3 * (TAU * 330.0 * t).sin() * (-t / 0.035).exp();
                let rattle = lp.process(0, hp.process(0, noise.next())) * (-t / 0.085).exp();
                tone + rattle * 1.4
            })
            .collect();
        self.sample(out, Vec::new(), 0.8)
    }

    fn clap(&self) -> Arc<Sample> {
        let mut noise = Noise(0x00c1_a900);
        let (mut hp, mut lp) = self.band(900.0, 3800.0);
        let bursts = [0.0, 0.009, 0.018, 0.028];
        let out = (0..self.frames(0.45))
            .map(|i| {
                let t = self.time(i);
                let mut envelope: f32 = bursts
                    .iter()
                    .filter(|&&start| t >= start)
                    .map(|start| (-(t - start) / 0.004).exp())
                    .sum();
                if t > 0.028 {
                    envelope += 0.6 * (-(t - 0.028) / 0.13).exp();
                }
                lp.process(0, hp.process(0, noise.next())) * envelope
            })
            .collect();
        self.sample(out, Vec::new(), 0.75)
    }

    fn hat(&self, seconds: f32, decay: f32, seed: u32, peak: f32) -> Arc<Sample> {
        let mut noise = Noise(seed);
        let mut hp1 = Biquad::default();
        let mut hp2 = Biquad::default();
        hp1.set(
            FilterKind::HighPass,
            f64::from(self.rate),
            7000.0,
            0.707,
            0.0,
        );
        hp2.set(
            FilterKind::HighPass,
            f64::from(self.rate),
            7000.0,
            0.707,
            0.0,
        );
        let out = (0..self.frames(seconds))
            .map(|i| {
                let t = self.time(i);
                let source = self.metal(i, 2.0) * 0.7 + noise.next() * 0.3;
                hp2.process(0, hp1.process(0, source)) * (-t / decay).exp()
            })
            .collect();
        self.sample(out, Vec::new(), peak)
    }

    fn rimshot(&self) -> Arc<Sample> {
        let mut noise = Noise(0x0001_2345);
        let (mut hp, mut lp) = self.band(2000.0, 6000.0);
        let out = (0..self.frames(0.1))
            .map(|i| {
                let t = self.time(i);
                (TAU * 1700.0 * t).sin() * (-t / 0.006).exp() * 0.6
                    + (TAU * 480.0 * t).sin() * (-t / 0.02).exp() * 0.5
                    + lp.process(0, hp.process(0, noise.next())) * (-t / 0.01).exp()
            })
            .collect();
        self.sample(out, Vec::new(), 0.7)
    }

    fn side_stick(&self) -> Arc<Sample> {
        let mut noise = Noise(0x5eed_0001);
        let (mut hp, mut lp) = self.band(1500.0, 5000.0);
        let out = (0..self.frames(0.08))
            .map(|i| {
                let t = self.time(i);
                (TAU * 1150.0 * t).sin() * (-t / 0.012).exp()
                    + lp.process(0, hp.process(0, noise.next())) * (-t / 0.005).exp() * 0.6
            })
            .collect();
        self.sample(out, Vec::new(), 0.6)
    }

    fn tom(&self, low: f32, sweep: f32, seed: u32) -> Arc<Sample> {
        let mut noise = Noise(seed);
        let mut phase = 0.0;
        let out = (0..self.frames(0.6))
            .map(|i| {
                let t = self.time(i);
                let frequency = low + sweep * (-t / 0.06).exp();
                phase += frequency / self.rate;
                (phase * TAU).sin() * (-t / 0.17).exp() + noise.next() * (-t / 0.004).exp() * 0.15
            })
            .collect();
        self.sample(out, Vec::new(), 0.8)
    }

    fn cymbal(&self, seconds: f32, decay: f32, low_cut: f32, metal: f32, peak: f32) -> Arc<Sample> {
        let channel = |seed: u32| {
            let mut noise = Noise(seed);
            let mut hp1 = Biquad::default();
            let mut hp2 = Biquad::default();
            hp1.set(
                FilterKind::HighPass,
                f64::from(self.rate),
                f64::from(low_cut),
                0.707,
                0.0,
            );
            hp2.set(
                FilterKind::HighPass,
                f64::from(self.rate),
                f64::from(low_cut),
                0.707,
                0.0,
            );
            (0..self.frames(seconds))
                .map(|i| {
                    let t = self.time(i);
                    let attack = (t / 0.001).min(1.0);
                    let source = noise.next() * (1.0 - metal) + self.metal(i, 3.1) * metal;
                    hp2.process(0, hp1.process(0, source)) * (-t / decay).exp() * attack
                })
                .collect::<Vec<f32>>()
        };
        self.sample(channel(0x0c0f_fee1), channel(0x0c0f_fee2), peak)
    }

    fn bell(&self) -> Arc<Sample> {
        let partials = [(1180.0, 1.0), (1790.0, 0.6), (2650.0, 0.45), (3520.0, 0.3)];
        let out = (0..self.frames(1.4))
            .map(|i| {
                let t = self.time(i);
                let tone: f32 = partials
                    .iter()
                    .map(|(f, level)| (TAU * f * t).sin() * level)
                    .sum();
                tone * (-t / 0.45).exp() + self.metal(i, 4.0) * 0.15 * (-t / 0.2).exp()
            })
            .collect();
        self.sample(out, Vec::new(), 0.5)
    }

    fn cowbell(&self) -> Arc<Sample> {
        let (mut hp, mut lp) = self.band(500.0, 2600.0);
        let out = (0..self.frames(0.4))
            .map(|i| {
                let t = self.time(i);
                let square = |f: f32| if (t * f).fract() < 0.5 { 1.0 } else { -1.0 };
                let envelope = 0.65 * (-t / 0.03).exp() + 0.35 * (-t / 0.14).exp();
                lp.process(
                    0,
                    hp.process(0, f32::midpoint(square(540.0), square(800.0))),
                ) * envelope
            })
            .collect();
        self.sample(out, Vec::new(), 0.55)
    }
}

/// Builds the electronic kit.
pub fn electronic_kit(sample_rate: u32) -> DrumKit {
    #[allow(clippy::cast_precision_loss, reason = "sample rates are small")]
    let synth = Synth {
        rate: sample_rate as f32,
    };
    let pads = [
        (PadSound::Kick, synth.kick()),
        (PadSound::Snare, synth.snare()),
        (PadSound::Clap, synth.clap()),
        (
            PadSound::ClosedHat,
            synth.hat(0.12, 0.022, 0x4a7_0001, 0.45),
        ),
        (PadSound::PedalHat, synth.hat(0.08, 0.014, 0x4a7_0002, 0.35)),
        (PadSound::OpenHat, synth.hat(0.8, 0.25, 0x4a7_0003, 0.45)),
        (PadSound::Rimshot, synth.rimshot()),
        (PadSound::SideStick, synth.side_stick()),
        (PadSound::LowTom, synth.tom(82.0, 45.0, 0x70_0001)),
        (PadSound::MidTom, synth.tom(118.0, 60.0, 0x70_0002)),
        (PadSound::HighTom, synth.tom(165.0, 75.0, 0x70_0003)),
        (PadSound::Crash, synth.cymbal(2.4, 0.6, 3500.0, 0.25, 0.55)),
        (PadSound::Ride, synth.cymbal(2.0, 0.65, 2500.0, 0.55, 0.4)),
        (PadSound::RideBell, synth.bell()),
        (PadSound::Cowbell, synth.cowbell()),
    ];
    DrumKit {
        pads: pads
            .into_iter()
            .map(|(sound, sample)| Pad {
                sound,
                layers: vec![Layer { top: 127, sample }],
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How much of a sound's energy is above `frequency`, 0 to 1 (a
    /// one-pole split: rough, but enough to tell a kick from a hat).
    fn brightness(sample: &Sample, rate: f32) -> f32 {
        let mut low = Biquad::default();
        low.set(FilterKind::LowPass, f64::from(rate), 1500.0, 0.707, 0.0);
        let mut total = 0.0;
        let mut below = 0.0;
        for s in &sample.left {
            let l = low.process(0, *s);
            total += s * s;
            below += l * l;
        }
        1.0 - (below / total.max(1e-12)).min(1.0)
    }

    #[test]
    fn every_pad_has_a_sound_that_starts_at_once_and_fades() {
        let kit = electronic_kit(48_000);
        assert_eq!(kit.pads.len(), PadSound::ALL.len(), "a sound for every pad");
        for pad in &kit.pads {
            let sample = &pad.layers[0].sample;
            let name = format!("{:?}", pad.sound);
            assert!(
                sample.peak() > 0.3 && sample.peak() <= 0.951,
                "{name} peak {}",
                sample.peak()
            );
            let early = sample.left[..240]
                .iter()
                .fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(early > 0.05, "{name} is heard within 5 ms");
            let end = &sample.left[sample.len() - 48..];
            assert!(end.iter().all(|s| s.abs() < 0.02), "{name} ends quietly");
        }
    }

    #[test]
    fn low_drums_are_low_and_cymbals_are_bright() {
        let kit = electronic_kit(48_000);
        let bright = |sound| brightness(&kit.pad(sound).unwrap().layers[0].sample, 48_000.0);
        assert!(
            bright(PadSound::Kick) < 0.1,
            "kick {}",
            bright(PadSound::Kick)
        );
        assert!(bright(PadSound::LowTom) < 0.1);
        assert!(
            bright(PadSound::ClosedHat) > 0.9,
            "hat {}",
            bright(PadSound::ClosedHat)
        );
        assert!(bright(PadSound::Crash) > 0.9);
        let crash = &kit.pad(PadSound::Crash).unwrap().layers[0].sample;
        assert_eq!(crash.right.len(), crash.left.len(), "cymbals are stereo");
    }

    #[test]
    fn the_kit_is_the_same_every_time() {
        let a = electronic_kit(48_000);
        let b = electronic_kit(48_000);
        for (x, y) in a.pads.iter().zip(&b.pads) {
            assert_eq!(x.layers[0].sample, y.layers[0].sample);
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Conversion between sample rates, for imported files and recordings whose
//! rate differs from the engine's (decision 0017).
//!
//! Each output sample is the input filtered through a windowed sinc (a
//! Kaiser window), evaluated at the output sample's exact position in the
//! input. The filter's cutoff sits just under the lower of the two Nyquist
//! frequencies, so converting down cannot fold high frequencies back as
//! noise, and converting up adds no mirror images. Offline only: it
//! allocates and takes about a second per minute of sound.

use std::f64::consts::PI;
use std::sync::OnceLock;

/// Lobes of the sinc on each side of its centre.
const ZERO_CROSSINGS: usize = 48;
/// Table entries per lobe (between them, values are interpolated).
const STEPS: usize = 256;
/// The Kaiser window's shape: sidelobes about 90 dB down.
const BETA: f64 = 9.0;
/// The cutoff, as a share of the lower Nyquist frequency: the filter is
/// fully closed by the Nyquist frequency itself, and flat to about 91 % of
/// it (20 kHz at 44.1 kHz).
const CUTOFF: f64 = 0.94;

/// Converts one channel of sound from rate `from` to rate `to`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "frame positions are far below 2^52; table positions are non-negative"
)]
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 || input.is_empty() {
        return input.to_vec();
    }
    let (from_rate, to_rate) = (u64::from(from), u64::from(to));
    let frames = (input.len() as u64 * to_rate).div_ceil(from_rate);
    // The cutoff relative to the input's Nyquist frequency.
    let cutoff = CUTOFF * (f64::from(to) / f64::from(from)).min(1.0);
    // How far either side of an output sample the filter reaches, in input
    // samples.
    let reach = ZERO_CROSSINGS as f64 / cutoff;
    let scale = STEPS as f64 * cutoff;
    let table = table();
    let last_input = input.len() as i64 - 1;
    #[allow(clippy::cast_possible_truncation, reason = "a gain near 1")]
    let gain = cutoff as f32;

    let mut output = Vec::with_capacity(frames as usize);
    for index in 0..frames {
        // The output sample's position in the input: `whole + fraction`,
        // computed exactly from integers so long files never drift.
        let numerator = index * from_rate;
        let whole = (numerator / to_rate) as i64;
        let fraction = (numerator % to_rate) as f64 / to_rate as f64;
        let first = (whole + (fraction - reach).ceil() as i64).max(0);
        let last = (whole + (fraction + reach).floor() as i64).min(last_input);
        let mut sum = 0.0_f32;
        for n in first..=last {
            let distance = ((whole - n) as f64 + fraction).abs() * scale;
            let step = distance as usize;
            let between = (distance - step as f64) as f32;
            let weight = table[step] + (table[step + 1] - table[step]) * between;
            sum += input[n as usize] * weight;
        }
        output.push(sum * gain);
    }
    output
}

/// The windowed sinc from its centre outwards, `STEPS` values per lobe.
fn table() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let end = ZERO_CROSSINGS * STEPS;
        (0..=end + 1)
            .map(|i| {
                #[allow(clippy::cast_precision_loss, reason = "small table indices")]
                let u = i as f64 / STEPS as f64;
                #[allow(clippy::cast_precision_loss, reason = "a small constant")]
                let lobes = ZERO_CROSSINGS as f64;
                if u >= lobes {
                    return 0.0;
                }
                #[allow(clippy::cast_possible_truncation, reason = "a weight below 1")]
                let value = (sinc(u) * kaiser(u / lobes)) as f32;
                value
            })
            .collect()
    })
}

fn sinc(u: f64) -> f64 {
    if u.abs() < 1e-12 {
        1.0
    } else {
        (PI * u).sin() / (PI * u)
    }
}

/// The Kaiser window at `r` from its centre (0) to its edge (1).
fn kaiser(r: f64) -> f64 {
    bessel_i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / bessel_i0(BETA)
}

/// The modified Bessel function of the first kind, order zero.
fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..100 {
        term *= half / f64::from(k);
        let squared = term * term;
        sum += squared;
        if squared < sum * 1e-17 {
            break;
        }
    }
    sum
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test signals"
)]
mod tests {
    use super::*;

    fn sine(hz: f64, rate: u32, seconds: f64) -> Vec<f32> {
        let frames = (seconds * f64::from(rate)) as usize;
        (0..frames)
            .map(|i| (2.0 * PI * hz * i as f64 / f64::from(rate)).sin() as f32)
            .collect()
    }

    /// The level of the middle half (away from the edges, where the filter
    /// sees silence beyond the sound).
    fn middle_rms(samples: &[f32]) -> f32 {
        let middle = &samples[samples.len() / 4..samples.len() * 3 / 4];
        (middle.iter().map(|s| s * s).sum::<f32>() / middle.len() as f32).sqrt()
    }

    /// The strength of one frequency (Goertzel), as an amplitude.
    fn strength(samples: &[f32], hz: f64, rate: u32) -> f64 {
        let w = 2.0 * PI * hz / f64::from(rate);
        let (mut a, mut b) = (0.0, 0.0);
        for &s in samples {
            let next = f64::from(s) + 2.0 * w.cos() * a - b;
            b = a;
            a = next;
        }
        let power = a * a + b * b - 2.0 * w.cos() * a * b;
        2.0 * power.sqrt() / samples.len() as f64
    }

    #[test]
    fn converting_keeps_length_pitch_and_level() {
        let input = sine(1000.0, 44_100, 1.0);
        let output = resample(&input, 44_100, 48_000);
        assert_eq!(output.len(), 48_000, "one second stays one second");
        let rms = middle_rms(&output);
        assert!(
            (rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.005,
            "level kept: {rms}"
        );
        let middle = &output[12_000..36_000];
        let at = strength(middle, 1000.0, 48_000);
        let beside = strength(middle, 1100.0, 48_000);
        assert!(at > 0.99 && beside < 0.01, "pitch kept: {at} vs {beside}");
    }

    #[test]
    fn steady_sound_keeps_its_level() {
        let output = resample(&vec![0.5; 4410], 44_100, 48_000);
        let middle = &output[1000..3800];
        assert!(middle.iter().all(|s| (s - 0.5).abs() < 0.001));
    }

    #[test]
    fn converting_down_leaves_out_what_the_new_rate_cannot_hold() {
        // At 24 kHz nothing above 12 kHz can exist: a 15 kHz tone must go,
        // not fold back as a 9 kHz one; a 5 kHz tone stays.
        let high = resample(&sine(15_000.0, 48_000, 0.5), 48_000, 24_000);
        assert!(middle_rms(&high) < 0.001, "gone: {}", middle_rms(&high));
        let low = resample(&sine(5_000.0, 48_000, 0.5), 48_000, 24_000);
        let rms = middle_rms(&low);
        assert!(
            (rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
            "kept: {rms}"
        );
    }

    #[test]
    fn a_click_stays_at_its_moment() {
        let mut input = vec![0.0; 44_100];
        input[4410] = 1.0; // 0.1 s.
        let output = resample(&input, 44_100, 48_000);
        let loudest = output
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i);
        assert_eq!(loudest, Some(4800));
    }

    #[test]
    fn the_same_rate_changes_nothing() {
        let input = sine(440.0, 48_000, 0.01);
        assert_eq!(resample(&input, 48_000, 48_000), input);
        assert_eq!(resample(&[], 44_100, 48_000), Vec::<f32>::new());
    }
}

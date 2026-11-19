// SPDX-License-Identifier: GPL-3.0-or-later

//! The voice, measured without ears: a synthetic voice (a breathy burst,
//! then a vowel made of harmonics under fixed formants) is analysed,
//! shifted, stretched and joined, and the results' pitch, length, level and
//! smoothness are measured.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test signals"
)]

use super::*;

const RATE: u32 = 22_050;

/// A deterministic noise.
fn noise(i: usize) -> f32 {
    let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    (x as f32 / u64::MAX as f32) * 2.0 - 1.0
}

/// A syllable sung at `hz`: 50 ms of noise (a consonant), then `seconds` of
/// a vowel whose harmonics follow two formants (700 and 1200 Hz, like
/// "a"), so shifting the pitch keeps the vowel's colour.
fn syllable(hz: f64, seconds: f64) -> Vec<f32> {
    let consonant = (0.05 * f64::from(RATE)) as usize;
    let vowel = (seconds * f64::from(RATE)) as usize;
    let mut out: Vec<f32> = (0..consonant).map(|i| 0.2 * noise(i)).collect();
    let formant = |f: f64| {
        let peak = |centre: f64, width: f64| (-((f - centre) / width).powi(2)).exp();
        peak(700.0, 250.0) + 0.6 * peak(1200.0, 300.0) + 0.05
    };
    let harmonics: Vec<(f64, f64)> = (1..=((5000.0 / hz) as usize))
        .map(|h| (hz * h as f64, formant(hz * h as f64)))
        .collect();
    let norm: f64 = harmonics.iter().map(|(_, a)| a).sum();
    for i in 0..vowel {
        let t = i as f64 / f64::from(RATE);
        let value: f64 = harmonics
            .iter()
            .map(|(f, a)| a * (std::f64::consts::TAU * f * t).sin())
            .sum();
        out.push((0.5 * value / norm) as f32);
    }
    out
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

/// The pitch of a stretch of rendered sound.
fn measured(samples: &[f32], from: f64, to: f64) -> f64 {
    VoiceSample::new(RATE, samples.to_vec())
        .pitch_between(from, to)
        .expect("no pitch found")
}

fn region(sample: &VoiceSample) -> Stretch {
    Stretch {
        start: 0.0,
        fixed: 0.05,
        end: sample.seconds(),
    }
}

#[test]
fn a_recording_s_pitch_is_found() {
    let sample = VoiceSample::new(RATE, syllable(220.0, 0.6));
    let pitch = sample.pitch_between(0.1, 0.6).unwrap();
    assert!((pitch / 220.0 - 1.0).abs() < 0.01, "{pitch} Hz");
    assert_eq!(
        sample.pitch_between(0.0, 0.03),
        None,
        "the consonant is not voiced"
    );
}

#[test]
fn a_syllable_is_shifted_up_and_down_and_stretched() {
    let sample = VoiceSample::new(RATE, syllable(220.0, 0.6));
    let level = rms(&sample.samples()[(0.1 * f64::from(RATE)) as usize..]);
    for target in [440.0, 330.0, 196.0, 110.0] {
        // Longer than the recording: the vowel is stretched.
        let sung = resample(&sample, region(&sample), 1.2, RATE, |_| target);
        assert_eq!(sung.len(), (1.2 * f64::from(RATE)).ceil() as usize);
        let pitch = measured(&sung, 0.2, 1.1);
        assert!(
            (pitch / target - 1.0).abs() < 0.015,
            "{target} Hz sang {pitch}"
        );
        let sung_level =
            rms(&sung[(0.2 * f64::from(RATE)) as usize..(1.1 * f64::from(RATE)) as usize]);
        assert!(
            sung_level > level * 0.5 && sung_level < level * 2.0,
            "{target} Hz: level {sung_level} against {level}"
        );
    }
    // Shorter than the recording: it is cut, not sped up.
    let short = resample(&sample, region(&sample), 0.3, RATE, |_| 220.0);
    assert_eq!(short.len(), (0.3 * f64::from(RATE)).ceil() as usize);
    let pitch = measured(&short, 0.1, 0.3);
    assert!((pitch / 220.0 - 1.0).abs() < 0.015, "{pitch}");
}

#[test]
fn a_phrase_lands_each_note_on_its_beat_at_its_pitch_without_clicks() {
    let sample = Arc::new(VoiceSample::new(RATE, syllable(220.0, 0.6)));
    let entry = OtoEntry {
        alias: "a".to_owned(),
        file: "a.wav".to_owned(),
        offset: 0.0,
        consonant: 50.0,
        cutoff: 0.0,
        preutterance: 60.0,
        overlap: 30.0,
    };
    let keys = [60.0, 64.0, 67.0];
    let notes: Vec<SungNote> = keys
        .iter()
        .enumerate()
        .map(|(i, &key)| SungNote::new(Arc::clone(&sample), &entry, 1.0 + 0.5 * i as f64, 0.5, key))
        .collect();
    let phrase = sing_phrase(&notes, RATE);
    assert!(
        (phrase.start - 0.94).abs() < 1e-9,
        "starts its preutterance early"
    );
    let at = |seconds: f64| seconds - phrase.start;
    for (i, &key) in keys.iter().enumerate() {
        let note = 1.0 + 0.5 * i as f64;
        let pitch = measured(&phrase.samples, at(note + 0.15), at(note + 0.4));
        assert!(
            (pitch / key_hz(key) - 1.0).abs() < 0.02,
            "note {i}: {pitch} Hz for {}",
            key_hz(key)
        );
    }
    // It ends with the last note.
    let length = phrase.samples.len() as f64 / f64::from(RATE);
    assert!((phrase.start + length - 2.5).abs() < 0.01, "{length}");
    // No clicks: no jump between samples bigger than the recording's own
    // (its noisy consonant has the biggest), joins and grains included.
    let biggest_jump = |x: &[f32]| {
        x.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f32, f32::max)
    };
    let own = biggest_jump(sample.samples());
    let jump = biggest_jump(&phrase.samples);
    assert!(
        jump < own * 1.25,
        "a jump of {jump}, the recording's biggest {own}"
    );
    let peak = phrase.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak < 1.0, "no clipping: {peak}");
}

#[test]
fn a_short_note_shortens_the_next_preutterance() {
    let sample = Arc::new(VoiceSample::new(RATE, syllable(220.0, 0.4)));
    let entry = OtoEntry {
        alias: "a".to_owned(),
        file: "a.wav".to_owned(),
        offset: 0.0,
        consonant: 50.0,
        cutoff: 0.0,
        preutterance: 200.0,
        overlap: 80.0,
    };
    // A note of 0.1 s, then one whose syllable wants to start 0.2 s early.
    let notes = [
        SungNote::new(Arc::clone(&sample), &entry, 1.0, 0.1, 60.0),
        SungNote::new(Arc::clone(&sample), &entry, 1.1, 0.4, 62.0),
    ];
    let phrase = sing_phrase(&notes, RATE);
    // The first note still sounds before the second takes over.
    assert!((phrase.start - 0.8).abs() < 1e-9);
    assert_ne!(phrase.samples.len(), 0);
}

#[test]
fn oto_times_place_the_syllable_in_its_recording() {
    let mut entry = OtoEntry {
        alias: "か".to_owned(),
        file: "ka.wav".to_owned(),
        offset: 100.0,
        consonant: 80.0,
        cutoff: -400.0,
        preutterance: 60.0,
        overlap: 20.0,
    };
    // A negative cutoff is a length after the offset.
    assert_eq!(
        entry.stretch(2.0),
        Stretch {
            start: 0.1,
            fixed: 0.08,
            end: 0.5
        }
    );
    // A positive one leaves out the end of the recording.
    entry.cutoff = 300.0;
    assert_eq!(entry.stretch(2.0).end, 1.7);
    // Never past the recording, never empty.
    entry.cutoff = -5000.0;
    assert_eq!(entry.stretch(2.0).end, 2.0);
}

#[test]
fn aliases_are_found_with_the_prefix_map_first() {
    let entry = |alias: &str| OtoEntry {
        alias: alias.to_owned(),
        file: format!("{alias}.wav"),
        offset: 0.0,
        consonant: 0.0,
        cutoff: 0.0,
        preutterance: 0.0,
        overlap: 0.0,
    };
    let bank = VoiceBank::new(
        vec![entry("a か"), entry("か"), entry("か↑"), entry("か")],
        vec![(72, String::new(), "↑".to_owned())],
        Box::new(|file| Err(format!("no {file}"))),
    );
    assert_eq!(bank.len(), 3, "the first line of an alias wins");
    let candidates = ["a か".to_owned(), "か".to_owned()];
    assert_eq!(bank.find(&candidates, 60).unwrap().alias, "a か");
    assert_eq!(
        bank.find(&candidates, 72).unwrap().alias,
        "か↑",
        "the high recording"
    );
    assert!(bank.find(&["き".to_owned()], 60).is_none());
    assert_eq!(bank.sample("x.wav").unwrap_err(), "no x.wav");
}

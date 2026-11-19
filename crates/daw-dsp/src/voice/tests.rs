// SPDX-License-Identifier: GPL-3.0-or-later

//! The voice, measured without ears: a synthetic voice (a breathy burst,
//! then a vowel made of harmonics under fixed formants) is analysed,
//! shifted, stretched and joined, and the results' pitch, length, level and
//! smoothness are measured; then its expression (decision 0028): a pitch
//! that bends and swings, colour moving the formants, breath adding noise,
//! loudness.

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

/// The glide 7a sang between notes (80 ms around each start), as the
/// model now works it out: a pitch for `sing_phrase`.
fn glide(notes: &[(f64, f64)]) -> impl Fn(f64) -> f64 + '_ {
    move |at| {
        let index = notes
            .partition_point(|(start, _)| start - 0.04 <= at)
            .saturating_sub(1);
        let (start, key) = notes[index];
        let Some(&(_, before)) = index.checked_sub(1).map(|i| &notes[i]) else {
            return key;
        };
        let into = ((at - (start - 0.04)) / 0.08).clamp(0.0, 1.0);
        before + (key - before) * (0.5 - 0.5 * (std::f64::consts::PI * into).cos())
    }
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
        let sung = resample(
            &sample,
            region(&sample),
            1.2,
            RATE,
            |_| target,
            Colour::default(),
        );
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
    let short = resample(
        &sample,
        region(&sample),
        0.3,
        RATE,
        |_| 220.0,
        Colour::default(),
    );
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
    let notes: Vec<SungNote> = (0..keys.len())
        .map(|i| SungNote::new(Arc::clone(&sample), &entry, 1.0 + 0.5 * i as f64, 0.5))
        .collect();
    let starts: Vec<(f64, f64)> = (0..keys.len())
        .map(|i| (1.0 + 0.5 * i as f64, keys[i]))
        .collect();
    let phrase = sing_phrase(&notes, glide(&starts), RATE);
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
        SungNote::new(Arc::clone(&sample), &entry, 1.0, 0.1),
        SungNote::new(Arc::clone(&sample), &entry, 1.1, 0.4),
    ];
    let phrase = sing_phrase(&notes, glide(&[(1.0, 60.0), (1.1, 62.0)]), RATE);
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

// ----- Expression (decision 0028)

/// One long note of the syllable at 220 Hz, sung with `colour` at the pitch
/// `pitch` gives.
fn one_note(colour: Colour, pitch: impl Fn(f64) -> f64) -> Vec<f32> {
    let sample = VoiceSample::new(RATE, syllable(220.0, 0.6));
    resample(
        &sample,
        region(&sample),
        1.5,
        RATE,
        |t| key_hz(pitch(t)),
        colour,
    )
}

/// The middle of a rendering, from 0.3 s to 1.3 s.
fn middle(x: &[f32]) -> &[f32] {
    &x[(0.3 * f64::from(RATE)) as usize..(1.3 * f64::from(RATE)) as usize]
}

/// The amplitude of the component of `x` at `hz`.
fn amplitude(x: &[f32], hz: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (n, &value) in x.iter().enumerate() {
        let phase = std::f64::consts::TAU * hz * n as f64 / f64::from(RATE);
        re += f64::from(value) * phase.cos();
        im += f64::from(value) * phase.sin();
    }
    2.0 * re.hypot(im) / x.len() as f64
}

/// Where the energy of a voice at `hz` sits: the mean of its harmonics'
/// frequencies, weighted by their power (up to 5 kHz).
fn centroid(x: &[f32], hz: f64) -> f64 {
    let harmonics: Vec<(f64, f64)> = (1..=(5000.0 / hz) as usize)
        .map(|k| (k as f64 * hz, amplitude(x, k as f64 * hz).powi(2)))
        .collect();
    let power: f64 = harmonics.iter().map(|(_, p)| p).sum();
    harmonics.iter().map(|(f, p)| f * p).sum::<f64>() / power
}

/// Only the part of `x` below `hz` (two one-pole low-pass filters).
fn low_passed(x: &[f32], hz: f64) -> Vec<f32> {
    let k = (-std::f64::consts::TAU * hz / f64::from(RATE)).exp() as f32;
    let mut out = x.to_vec();
    for _ in 0..2 {
        let mut last = 0.0_f32;
        for value in &mut out {
            last = *value + k * (last - *value);
            *value = last;
        }
    }
    out
}

/// How alike the sound is to itself one period of `hz` later (1: exactly
/// periodic; noise lowers it).
fn periodicity(x: &[f32], hz: f64) -> f64 {
    let period = f64::from(RATE) / hz;
    let best = |lag: usize| {
        let (a, b) = (&x[..x.len() - lag], &x[lag..]);
        let dot: f64 = a
            .iter()
            .zip(b)
            .map(|(&p, &q)| f64::from(p) * f64::from(q))
            .sum();
        let norm = rms(a) * rms(b) * a.len() as f64;
        dot / norm
    };
    ((period.floor() as usize - 1)..=(period.ceil() as usize + 1))
        .map(best)
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn a_bent_and_swinging_pitch_is_sung_as_it_says() {
    // A 5.5 Hz vibrato of 50 cents either side around A3 (57: 220 Hz),
    // after a scoop up from a whole tone below over the first 0.2 s.
    let pitch = |t: f64| {
        if t < 0.2 {
            55.0 + 2.0 * (t / 0.2)
        } else {
            57.0 + 0.5 * (std::f64::consts::TAU * 5.5 * (t - 0.2)).sin()
        }
    };
    let sung = one_note(Colour::default(), pitch);
    let analysed = VoiceSample::new(RATE, sung);
    // The pitch every 10 ms, as cents from 220 Hz.
    let trace: Vec<(f64, f64)> = (30..140)
        .filter_map(|i| {
            let t = f64::from(i) / 100.0;
            analysed
                .pitch_between(t - 0.01, t + 0.01)
                .map(|hz| (t, 1200.0 * (hz / 220.0).log2()))
        })
        .collect();
    assert!(trace.len() > 100, "the pitch is found throughout");
    let deepest = trace.iter().map(|(_, c)| c.abs()).fold(0.0, f64::max);
    assert!(
        (35.0..=65.0).contains(&deepest),
        "swings {deepest} cents either side"
    );
    // Where the swing crosses the middle (interpolated), half a swing apart.
    let crossings: Vec<f64> = trace
        .windows(2)
        .filter(|w| (w[0].1 < 0.0) != (w[1].1 < 0.0))
        .map(|w| w[0].0 + (w[1].0 - w[0].0) * w[0].1 / (w[0].1 - w[1].1))
        .collect();
    let halves = (crossings.len() - 1) as f64;
    let rate = halves / 2.0 / (crossings[crossings.len() - 1] - crossings[0]);
    assert!((rate - 5.5).abs() < 0.25, "swings {rate} times a second");
    // Each moment sings what the pitch said then (allowing for the
    // analysis' own smoothing).
    let followed = trace
        .iter()
        .filter(|(t, cents)| (cents - 100.0 * (pitch(*t) - 57.0)).abs() < 25.0)
        .count();
    assert!(
        followed * 10 >= trace.len() * 8,
        "{followed} of {}",
        trace.len()
    );
}

#[test]
fn colour_moves_the_formants_and_keeps_the_pitch() {
    let at = |formant: f64| {
        one_note(
            Colour {
                formant,
                ..Colour::default()
            },
            |_| 57.0,
        )
    };
    let (plain, lighter, darker) = (at(1.0), at(1.26), at(0.794));
    let centre = centroid(middle(&plain), 220.0);
    let up = centroid(middle(&lighter), 220.0);
    let down = centroid(middle(&darker), 220.0);
    assert!(up > centre * 1.12, "lighter: {up} Hz against {centre}");
    assert!(down < centre * 0.9, "darker: {down} Hz against {centre}");
    for sung in [&lighter, &darker] {
        let pitch = measured(sung, 0.3, 1.3);
        assert!(
            (pitch / 220.0 - 1.0).abs() < 0.015,
            "the pitch stays: {pitch}"
        );
        let level = rms(middle(sung)) / rms(middle(&plain));
        assert!((0.6..1.6).contains(&level), "about as loud: {level}");
    }
}

#[test]
fn breath_mixes_in_a_whisper_and_keeps_the_pitch() {
    let at = |breath: f32| {
        one_note(
            Colour {
                breath,
                ..Colour::default()
            },
            |_| 57.0,
        )
    };
    let (clear, half, full) = (at(0.0), at(0.5), at(1.0));
    let periodic = |x: &[f32]| periodicity(middle(x), 220.0);
    // Breath is noise in the highs: the part above 1 kHz stops repeating.
    let highs_periodic = |x: &[f32]| periodicity(&high_passed(middle(x).to_vec(), RATE), 220.0);
    assert!(periodic(&clear) > 0.95, "clear: {}", periodic(&clear));
    assert!(
        periodic(&half) < periodic(&clear) - 0.03 && periodic(&full) < periodic(&half),
        "less periodic with breath: {} {} {}",
        periodic(&clear),
        periodic(&half),
        periodic(&full)
    );
    assert!(
        highs_periodic(&clear) > 0.9,
        "clear highs: {}",
        highs_periodic(&clear)
    );
    assert!(
        highs_periodic(&half) < 0.88 && highs_periodic(&full) < 0.6,
        "breathy highs: {} {} {}",
        highs_periodic(&clear),
        highs_periodic(&half),
        highs_periodic(&full)
    );
    for sung in [&half, &full] {
        // The pitch is in the low harmonics, which breath leaves.
        let pitch = measured(&low_passed(sung, 500.0), 0.3, 1.3);
        assert!(
            (pitch / 220.0 - 1.0).abs() < 0.02,
            "still sung at {pitch} Hz"
        );
        let level = rms(middle(sung)) / rms(middle(&clear));
        assert!((0.6..1.6).contains(&level), "about as loud: {level}");
        let peak = sung.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak < 1.0, "no clipping: {peak}");
    }
    assert_eq!(at(0.5), half, "the same whisper every time");
}

#[test]
fn a_note_is_sung_at_its_gain() {
    let sample = Arc::new(VoiceSample::new(RATE, syllable(220.0, 0.6)));
    let entry = OtoEntry {
        alias: "a".to_owned(),
        file: "a.wav".to_owned(),
        offset: 0.0,
        consonant: 50.0,
        cutoff: 0.0,
        preutterance: 0.0,
        overlap: 0.0,
    };
    let sing = |gain: f32| {
        let mut note = SungNote::new(Arc::clone(&sample), &entry, 0.0, 1.0);
        note.gain = gain;
        sing_phrase(&[note], |_| 57.0, RATE).samples
    };
    let ratio = rms(&sing(0.5)) / rms(&sing(1.0));
    assert!((ratio - 0.5).abs() < 1e-6, "{ratio}");
}

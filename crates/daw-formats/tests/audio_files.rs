// SPDX-License-Identifier: GPL-3.0-or-later

//! Compressed audio files made by the usual encoders read back with the
//! right rate, length, level and pitch. The two files in `tests/data` are
//! half a second of a 440 Hz tone (left at half level, right at a quarter),
//! generated for this test and encoded with LAME (128 kbit/s) and oggenc
//! (quality 4).

#![allow(clippy::unwrap_used, clippy::cast_precision_loss, reason = "test code")]

use std::path::PathBuf;

use daw_formats::audio::{AudioFormat, decode, read_file};

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(name)
}

/// The strength of one frequency (Goertzel), as an amplitude.
fn strength(samples: &[f32], hz: f64, rate: u32) -> f64 {
    let w = 2.0 * std::f64::consts::PI * hz / f64::from(rate);
    let (mut a, mut b) = (0.0, 0.0);
    for &s in samples {
        let next = f64::from(s) + 2.0 * w.cos() * a - b;
        b = a;
        a = next;
    }
    (a * a + b * b - 2.0 * w.cos() * a * b).sqrt() * 2.0 / samples.len() as f64
}

fn check(name: &str, format: AudioFormat) {
    let decoded = decode(&read_file(&data(name)).unwrap()).unwrap();
    assert_eq!(decoded.format, format, "{name}");
    assert_eq!(decoded.sample_rate, 44_100, "{name}");
    assert_eq!(decoded.channels.len(), 2, "{name}");
    let seconds = decoded.frames() as f64 / 44_100.0;
    assert!((0.45..0.56).contains(&seconds), "{name}: {seconds} s");
    let middle = |channel: &[f32]| channel[4410..17_640].to_vec();
    let (left, right) = (middle(&decoded.channels[0]), middle(&decoded.channels[1]));
    let at = strength(&left, 440.0, 44_100);
    assert!((at - 0.5).abs() < 0.05, "{name}: left level {at}");
    assert!(
        strength(&left, 600.0, 44_100) < 0.02,
        "{name}: only the tone"
    );
    let right_at = strength(&right, 440.0, 44_100);
    assert!(
        (right_at - 0.25).abs() < 0.04,
        "{name}: right level {right_at}"
    );
}

#[test]
fn mp3_files_read_back() {
    check("sine-440.mp3", AudioFormat::Mp3);
}

#[test]
fn ogg_vorbis_files_read_back() {
    check("sine-440.ogg", AudioFormat::Ogg);
}

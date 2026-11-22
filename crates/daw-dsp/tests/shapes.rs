// SPDX-License-Identifier: GPL-3.0-or-later

//! Note shapes (decision 0031), heard: the bundled piano's notes cut off at
//! their end or rising slowly, a drum hit stopping with its note, and the
//! sampler's sounds following the same shape; nothing allocates on the
//! audio thread.

#![allow(
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test code"
)]

use std::sync::Arc;

use daw_dsp::drums::{DrumKit, DrumSampler, PadSound};
use daw_dsp::sampler::{LoopMode, Region, Sample, Sampler, SamplerBank};
use daw_dsp::{Instrument, NoteShape, SoundFontBank, SoundFontPlayer};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

static PIANO: &[u8] = include_bytes!("../../../assets/sounds/piano.sf2");

const RATE: u32 = 48_000;

/// Plays middle C for `held` seconds, then renders `after` more seconds;
/// returns the sound (left channel).
fn play(instrument: &mut dyn Instrument, key: u8, held: f32, after: f32) -> Vec<f32> {
    let frames = |seconds: f32| (seconds * RATE as f32) as usize;
    let mut out = Vec::new();
    let mut render = |instrument: &mut dyn Instrument, count: usize| {
        let (mut left, mut right) = (vec![0.0; count], vec![0.0; count]);
        for (l, r) in left.chunks_mut(256).zip(right.chunks_mut(256)) {
            let _guard = daw_rt::RtGuard::enter();
            instrument.render(l, r);
        }
        out.extend(left);
    };
    {
        let _guard = daw_rt::RtGuard::enter();
        instrument.note_on(key, 0.8);
    }
    render(instrument, frames(held));
    {
        let _guard = daw_rt::RtGuard::enter();
        instrument.note_off(key);
    }
    render(instrument, frames(after));
    out
}

/// The loudest sample from `from` to `to` seconds.
fn peak(sound: &[f32], from: f32, to: f32) -> f32 {
    let at = |seconds: f32| ((seconds * RATE as f32) as usize).min(sound.len());
    sound[at(from)..at(to)]
        .iter()
        .fold(0.0, |m, s| m.max(s.abs()))
}

fn piano(shape: NoteShape) -> SoundFontPlayer {
    let bank = SoundFontBank::from_bytes_shaped(PIANO, shape).unwrap();
    let mut player = SoundFontPlayer::new(&bank, RATE, 0, 0).unwrap();
    // The reverb's tail would hide the note's own end.
    player.set_reverb(0.0);
    player
}

#[test]
fn a_cut_release_stops_the_piano_at_the_end_of_the_note() {
    let natural = play(&mut piano(NoteShape::default()), 60, 1.0, 1.0);
    let cut = play(
        &mut piano(NoteShape {
            attack: None,
            release: Some(0.01),
        }),
        60,
        1.0,
        1.0,
    );
    // While the key is down, both sound the same.
    let held = peak(&natural, 0.9, 1.0);
    assert!(held > 0.005, "the note is heard ({held})");
    for (from, to) in [(0.0, 0.2), (0.9, 1.0)] {
        let (a, b) = (peak(&natural, from, to), peak(&cut, from, to));
        assert!((a - b).abs() <= a * 0.05, "{from}-{to} s: {b} against {a}");
    }
    // Just after the key comes up the natural piano still rings; the cut
    // one is silent (60 dB below the note).
    let ringing = peak(&natural, 1.05, 1.15);
    let gone = peak(&cut, 1.05, 1.15);
    assert!(
        ringing > held * 0.05,
        "the natural release rings ({ringing} against {held})"
    );
    assert!(
        gone < held * 0.001,
        "the cut note is gone ({gone} against {held})"
    );
    // It stops without a click: no jump at the moment the key comes up.
    let at = RATE as usize;
    let steepest = |from: usize, to: usize| {
        cut[from..to]
            .windows(2)
            .fold(0.0_f32, |m, w| m.max((w[1] - w[0]).abs()))
    };
    let (jump, normal) = (steepest(at - 64, at + 960), steepest(at - 4096, at - 64));
    assert!(jump <= normal * 1.5, "no click: {jump} against {normal}");
}

#[test]
fn a_long_release_lets_the_piano_ring_on_and_a_slow_attack_swells() {
    let long = play(
        &mut piano(NoteShape {
            attack: Some(0.4),
            release: Some(3.0),
        }),
        60,
        1.0,
        1.5,
    );
    let natural = play(&mut piano(NoteShape::default()), 60, 1.0, 1.5);
    // The attack: far quieter than the natural piano at first, rising
    // over the first part of a second.
    let (early, natural_early) = (peak(&long, 0.0, 0.05), peak(&natural, 0.0, 0.05));
    assert!(
        early < natural_early * 0.3,
        "it swells: {early} against {natural_early}"
    );
    assert!(peak(&long, 0.2, 0.3) > early * 1.5, "rising");
    // The release: a second after the key comes up, the long one still
    // rings; the natural one is long gone.
    let long_tail = peak(&long, 2.0, 2.2);
    let natural_tail = peak(&natural, 2.0, 2.2);
    assert!(long_tail > 0.0005, "rings on ({long_tail})");
    assert!(
        natural_tail < long_tail * 0.1,
        "{natural_tail} against {long_tail}"
    );
}

#[test]
fn shaping_the_piano_is_quick_enough_to_follow_a_slider() {
    let start = std::time::Instant::now();
    let bank = SoundFontBank::from_bytes_shaped(
        PIANO,
        NoteShape {
            attack: Some(0.0),
            release: Some(0.2),
        },
    )
    .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(
        bank.presets().len(),
        SoundFontBank::from_bytes(PIANO).unwrap().presets().len()
    );
    // Generous: a debug build on a slow machine.
    assert!(elapsed.as_secs_f32() < 2.0, "{elapsed:?}");
    eprintln!("shaping the piano took {elapsed:?}");
}

#[test]
fn a_drum_hit_with_a_release_stops_with_its_note() {
    let kit = || DrumKit::electronic(RATE);
    let key = PadSound::Clap.key();
    let natural = play(&mut DrumSampler::new(kit(), RATE), key, 0.02, 0.5);
    let mut shaped = DrumSampler::new(kit(), RATE);
    shaped.set_shape(NoteShape {
        attack: None,
        release: Some(0.01),
    });
    let cut = play(&mut shaped, key, 0.02, 0.5);
    let ringing = peak(&natural, 0.05, 0.1);
    assert!(ringing > 0.01, "the clap rings on ({ringing})");
    assert!(peak(&cut, 0.04, 0.5) < 1e-4, "cut off with its note");
    assert!(peak(&cut, 0.0, 0.02) > 0.01, "heard while the note lasts");
}

/// A bank of one sound: a second of 220 Hz on every key, as a one-shot.
fn one_shot_bank() -> Arc<SamplerBank> {
    let left: Vec<f32> = (0..RATE)
        .map(|i| (2.0 * std::f32::consts::PI * 220.0 * i as f32 / RATE as f32).sin() * 0.5)
        .collect();
    let mut bank = SamplerBank::default();
    bank.samples.push(Sample {
        sample_rate: RATE,
        left,
        right: Vec::new(),
        file_loop: None,
    });
    bank.regions.push(Region {
        sample: Some(0),
        lokey: 0,
        hikey: 127,
        pitch_keycenter: 57,
        loop_mode: Some(LoopMode::OneShot),
        ..Region::default()
    });
    Arc::new(bank)
}

#[test]
fn the_sampler_follows_the_shape_too() {
    let natural = play(&mut Sampler::new(one_shot_bank(), RATE), 57, 0.2, 0.6);
    let mut shaped = Sampler::new(one_shot_bank(), RATE);
    shaped.set_shape(NoteShape {
        attack: Some(0.1),
        release: Some(0.02),
    });
    let cut = play(&mut shaped, 57, 0.2, 0.6);
    // A one-shot plays on; with a release it stops with its note.
    let level = peak(&natural, 0.1, 0.2);
    assert!(level > 0.1, "heard ({level})");
    assert!(
        peak(&natural, 0.4, 0.6) > level * 0.9,
        "a one-shot plays to its end"
    );
    assert!(peak(&cut, 0.3, 0.8) < level * 1e-3, "cut off with its note");
    // And it rises over its attack.
    assert!(peak(&cut, 0.0, 0.01) < 0.1 * peak(&cut, 0.12, 0.2));
}

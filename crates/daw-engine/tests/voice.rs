// SPDX-License-Identifier: GPL-3.0-or-later

//! A voice track in the engine (decision 0027), with a voicebank the test
//! makes in memory: two recordings of a synthetic vowel at 220 Hz, named
//! "- あ", "あ", "a か" and "か". The song is rendered offline (the
//! renderer works at once there) and each sung note is measured: present,
//! on its beat, at its pitch. A song naming a voicebank that is not
//! installed stays silent and says so. Expression (decision 0028): a
//! vibrato swings, a soft note is quieter, and an edit renders again.

#![allow(
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "test code"
)]

use std::sync::Arc;

use daw_dsp::voice::{OtoEntry, VoiceBank, VoiceSample, key_hz};
use daw_engine::{EngineConfig, SoundLibrary, render_offline};
use daw_model::expression::{NoteExpression, Vibrato};
use daw_model::{DEFAULT_VELOCITY, Instrument, Note, NoteId, Project, Ticks};

const RATE: u32 = 48_000;
const VOICE_RATE: u32 = 22_050;

/// A vowel at 220 Hz for a second, after 50 ms of quiet breath.
fn recording() -> VoiceSample {
    let lead = (0.05 * f64::from(VOICE_RATE)) as usize;
    let mut samples: Vec<f32> = (0..lead)
        .map(|i| 0.01 * ((i * 7919 % 97) as f32 / 48.5 - 1.0))
        .collect();
    samples.extend((0..VOICE_RATE as usize).map(|i| {
        let t = f64::from(i as u32) / f64::from(VOICE_RATE);
        (0.3 * (std::f64::consts::TAU * 220.0 * t).sin()
            + 0.12 * (std::f64::consts::TAU * 440.0 * t).sin()) as f32
    }));
    VoiceSample::new(VOICE_RATE, samples)
}

fn voicebank() -> VoiceBank {
    let entry = |alias: &str| OtoEntry {
        alias: alias.to_owned(),
        file: "vowel.wav".to_owned(),
        offset: 20.0,
        consonant: 40.0,
        cutoff: -950.0,
        preutterance: 50.0,
        overlap: 20.0,
    };
    VoiceBank::new(
        vec![entry("- あ"), entry("あ"), entry("a か"), entry("か")],
        Vec::new(),
        Box::new(|file| {
            if file == "vowel.wav" {
                Ok(recording())
            } else {
                Err(format!("no {file}"))
            }
        }),
    )
}

fn library() -> SoundLibrary {
    let mut library = SoundLibrary::new();
    library.set_voice_reader(Arc::new(|name| {
        if name == "Test voice" {
            Ok(voicebank())
        } else {
            Err("it is not installed".to_owned())
        }
    }));
    library
}

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        max_block: 512,
    }
}

/// A song at 120 BPM with a voice track singing "a ka a" on C4, E4 and G4,
/// a quarter note each from beat 2.
fn song(voicebank: &str) -> Project {
    let mut project = Project::empty();
    project.loop_region = None;
    let voice = project.add_track(
        "Voice",
        Instrument::Voice {
            voicebank: voicebank.to_owned(),
        },
    );
    let pattern = project.add_pattern_for(voice, "Verse", Ticks::WHOLE * 2);
    let mut lyrics = Vec::new();
    for (i, (key, lyric)) in [(60, "a"), (64, "ka"), (67, "a")].iter().enumerate() {
        let id = project
            .add_note(
                pattern,
                Note {
                    id: NoteId(0),
                    start: Ticks::QUARTER * (1 + i as i64),
                    length: Ticks::QUARTER,
                    key: *key,
                    velocity: DEFAULT_VELOCITY,
                    pitch_offset: 0.0,
                },
            )
            .unwrap();
        lyrics.push((id, (*lyric).to_owned()));
    }
    project.set_lyrics(pattern, &lyrics);
    project.add_clip(voice, Ticks::ZERO, Ticks::WHOLE * 2, pattern);
    project
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn frames(seconds: f64) -> usize {
    (seconds * f64::from(RATE)) as usize
}

#[test]
fn a_voice_track_sings_its_notes_on_their_beats_at_their_pitches() {
    let (left, right) = render_offline(&song("Test voice"), library(), config(), 2.5);
    // Nothing before the first syllable starts (50 ms before beat 2).
    assert!(rms(&left[..frames(0.4)]) < 1e-4, "silent before the voice");
    for (i, key) in [60.0, 64.0, 67.0].into_iter().enumerate() {
        let beat = 0.5 * (1 + i) as f64;
        let middle = &left[frames(beat + 0.12)..frames(beat + 0.42)];
        assert!(rms(middle) > 0.02, "note {i} is sung");
        let pitch = VoiceSample::new(RATE, middle.to_vec())
            .pitch_between(0.0, 0.3)
            .unwrap();
        assert!(
            (pitch / key_hz(key) - 1.0).abs() < 0.02,
            "note {i}: {pitch} Hz for {}",
            key_hz(key)
        );
    }
    // The voice is in the middle, and done after its last note.
    assert_eq!(left, right);
    assert!(rms(&left[frames(2.1)..]) < 1e-4, "silent after the voice");
}

#[test]
fn a_voicebank_that_is_not_installed_leaves_its_track_silent() {
    let (left, _) = render_offline(&song("Somebody else's"), library(), config(), 2.0);
    assert!(rms(&left) < 1e-6);
}

/// The pitch every 10 ms between two times of a rendering, in cents from
/// `key`.
fn cents_from(samples: &[f32], key: f64, from: f64, to: f64) -> Vec<f64> {
    let analysed = VoiceSample::new(RATE, samples.to_vec());
    let mut trace = Vec::new();
    let mut t = from;
    while t < to {
        if let Some(hz) = analysed.pitch_between(t - 0.01, t + 0.01) {
            trace.push(1200.0 * (hz / key_hz(key)).log2());
        }
        t += 0.01;
    }
    trace
}

#[test]
fn a_voice_track_sings_as_each_note_s_expression_says() {
    // One library for both renders: its phrases are kept between them.
    let library = library();
    let plain = song("Test voice");
    let (before, _) = render_offline(&plain, library.clone(), config(), 2.5);
    // The second note (E4, from 1 s) swings; the third (G4) is sung soft.
    let mut shaped = plain.clone();
    let voice = shaped
        .tracks
        .iter()
        .find(|t| t.instrument.voicebank().is_some())
        .unwrap();
    let pattern = voice.clips[0].pattern().unwrap();
    let ids: Vec<NoteId> = shaped
        .pattern(pattern)
        .unwrap()
        .notes
        .iter()
        .map(|n| n.id)
        .collect();
    shaped.set_expressions(
        pattern,
        &[(
            ids[1],
            NoteExpression {
                vibrato: Some(Vibrato {
                    depth: 60.0,
                    rate: 6.0,
                    start: 0.05,
                    fade_in: 0.0,
                }),
                ..NoteExpression::default()
            },
        )],
    );
    shaped.update_notes(pattern, &[ids[2]], |n| n.velocity = DEFAULT_VELOCITY / 2.0);
    let (after, _) = render_offline(&shaped, library.clone(), config(), 2.5);
    // The first note is sung as before.
    assert_eq!(
        before[frames(0.62)..frames(0.92)],
        after[frames(0.62)..frames(0.92)]
    );
    // The second swings: before, it held its pitch.
    let still = cents_from(&before, 64.0, 1.12, 1.36);
    let swinging = cents_from(&after, 64.0, 1.12, 1.36);
    let widest = |trace: &[f64]| trace.iter().map(|c| c.abs()).fold(0.0, f64::max);
    assert!(widest(&still) < 15.0, "held: {still:?}");
    assert!(widest(&swinging) > 35.0, "swings: {swinging:?}");
    assert!(
        swinging.iter().any(|&c| c > 25.0) && swinging.iter().any(|&c| c < -25.0),
        "both ways: {swinging:?}"
    );
    // The third, at half the velocity, sings at a quarter of the level.
    let level = rms(&after[frames(1.62)..frames(1.92)]) / rms(&before[frames(1.62)..frames(1.92)]);
    assert!((level - 0.25).abs() < 0.03, "{level}");
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine, tested by listening with numbers: projects are rendered
//! offline and the sound is checked for presence, pitch, timing and
//! clipping. The instrument is a SoundFont built on the fly that plays a pure
//! sine, so pitch and onsets can be measured precisely.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use daw_dsp::sf2_writer::sine_test_font;
use daw_engine::{EngineConfig, SoundLibrary, render_offline};
use daw_model::{BUILTIN_PIANO, DEFAULT_VELOCITY, Note, NoteId, Project, Region, Ticks};

const RATE: u32 = 48_000;

fn library() -> SoundLibrary {
    let bytes: &'static [u8] = Box::leak(sine_test_font(RATE).to_bytes().into_boxed_slice());
    let mut library = SoundLibrary::new();
    library.register_builtin(BUILTIN_PIANO, bytes);
    library.set_soundfont_reverb(0.0);
    library
}

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        max_block: 512,
    }
}

fn note(start: Ticks, length: Ticks, key: u8) -> Note {
    Note {
        id: NoteId(0),
        start,
        length,
        key,
        velocity: DEFAULT_VELOCITY,
        pitch_offset: 0.0,
    }
}

/// A project at 120 BPM (a quarter note is half a second), no loop, and no
/// master limiter (its look-ahead would delay everything by 1.5 ms; it has
/// tests of its own).
fn project_with(notes: &[Note]) -> Project {
    let mut project = Project::starter("P", "T");
    project.loop_region = None;
    project.master.effects.clear();
    let (_, pattern) = project.first_pattern().unwrap();
    project.add_notes(pattern, notes);
    project
}

fn seconds(s: f64) -> usize {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test arithmetic"
    )]
    let frames = (s * f64::from(RATE)) as usize;
    frames
}

fn rms(samples: &[f32]) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "test arithmetic")]
    let n = samples.len().max(1) as f32;
    (samples.iter().map(|s| s * s).sum::<f32>() / n).sqrt()
}

/// Frequency by counting upward zero crossings.
fn frequency(samples: &[f32]) -> f32 {
    let crossings = samples
        .windows(2)
        .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
        .count();
    #[allow(clippy::cast_precision_loss, reason = "test arithmetic")]
    let hz = crossings as f32 / (samples.len() as f32 / RATE as f32);
    hz
}

/// The first frame after `from` where the signal rises above `threshold`.
fn onset(samples: &[f32], from: usize, threshold: f32) -> Option<usize> {
    samples[from..]
        .iter()
        .position(|s| s.abs() > threshold)
        .map(|i| i + from)
}

#[test]
fn notes_sound_at_the_right_pitch() {
    let project = project_with(&[
        note(Ticks::ZERO, Ticks::QUARTER * 2, 60),
        note(Ticks::QUARTER * 2, Ticks::QUARTER * 2, 69),
    ]);
    let (left, right) = render_offline(&project, library(), config(), 2.5);
    assert_eq!(left.len(), right.len());
    let c = &left[seconds(0.1)..seconds(0.9)];
    let a = &left[seconds(1.1)..seconds(1.9)];
    assert!(rms(c) > 0.02, "middle C is silent (rms {})", rms(c));
    assert!(
        (frequency(c) - 261.6).abs() < 2.0,
        "middle C at {} Hz",
        frequency(c)
    );
    assert!(
        (frequency(a) - 440.0).abs() < 2.0,
        "A4 at {} Hz",
        frequency(a)
    );
    let peak = left
        .iter()
        .chain(&right)
        .fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak < 1.0, "the output clips (peak {peak})");
}

#[test]
fn notes_start_and_stop_on_time() {
    // At 120 BPM: a note at beat 1 (0.5 s) lasting one beat.
    let project = project_with(&[note(Ticks::QUARTER, Ticks::QUARTER, 69)]);
    let (left, _) = render_offline(&project, library(), config(), 2.0);
    let start = onset(&left, 0, 0.01).expect("the note never starts");
    // The SoundFont player places notes on 16-frame blocks; allow a little
    // more for the attack ramp.
    let expected = seconds(0.5);
    assert!(
        start >= expected && start < expected + 64,
        "starts at frame {start}, expected {expected}"
    );
    assert!(
        rms(&left[seconds(0.4)..seconds(0.49)]) < 1e-4,
        "sound before the note"
    );
    // After the release (50 ms in the test font) the note is gone.
    assert!(
        rms(&left[seconds(1.2)..seconds(1.4)]) < 0.002,
        "the note does not stop"
    );
}

#[test]
fn the_loop_repeats() {
    let mut project = project_with(&[note(Ticks::ZERO, Ticks::QUARTER, 69)]);
    // Loop the first two beats: the note plays every second.
    project.loop_region = Region::new(Ticks::ZERO, Ticks::QUARTER * 2);
    let (left, _) = render_offline(&project, library(), config(), 3.2);
    for cycle in 0..3 {
        let from = seconds(f64::from(cycle) * 1.0);
        let start = onset(&left, from, 0.01).unwrap_or(usize::MAX);
        assert!(
            start < from + 64,
            "cycle {cycle} starts at {start}, expected about {from}"
        );
        assert!(
            rms(&left[from + seconds(0.6)..from + seconds(0.9)]) < 0.002,
            "cycle {cycle}: silence between notes"
        );
    }
}

#[test]
fn tempo_changes_move_notes_in_time() {
    let mut project = project_with(&[note(Ticks::QUARTER * 4, Ticks::QUARTER, 69)]);
    // 120 BPM for four beats (2 s), then 60 BPM.
    project.tempo.set(Ticks::QUARTER * 2, 60.0);
    // Beat 4 at: 2 beats at 120 (1 s) + 2 beats at 60 (2 s) = 3 s.
    let (left, _) = render_offline(&project, library(), config(), 4.0);
    let start = onset(&left, 0, 0.01).expect("the note never starts");
    let expected = seconds(3.0);
    assert!(
        start >= expected && start < expected + 64,
        "starts at {start}, expected {expected}"
    );
}

#[test]
fn a_tempo_glide_speeds_up_smoothly() {
    // From 60 BPM gliding to 120 BPM over four beats: the note on beat 4
    // sounds after 4 * ln 2 seconds (about 2.77 s), between the 4 s it would
    // take at 60 and the 2 s at 120.
    let mut project = project_with(&[note(Ticks::QUARTER * 4, Ticks::QUARTER, 69)]);
    project.tempo = daw_model::TempoMap::constant(60.0);
    project.tempo.set(Ticks::QUARTER * 4, 120.0);
    project.tempo.set_glide(Ticks::ZERO, true);
    let (left, _) = render_offline(&project, library(), config(), 3.5);
    let start = onset(&left, 0, 0.01).expect("the note never starts");
    let expected = seconds(4.0 * std::f64::consts::LN_2);
    // This moment is not on one of the SoundFont player's 16-frame blocks:
    // allow for the block as well as the attack ramp.
    assert!(
        start >= expected.saturating_sub(1) && start < expected + 16 + 64,
        "starts at {start}, expected {expected}"
    );
}

#[test]
fn swing_delays_the_off_beats_by_the_amount_chosen() {
    // Straight eighths at 120 BPM fall every 0.25 s; with 100 % swing on
    // eighths, the off-beats move to a dotted eighth: 0.375 s after the beat.
    // Very short notes (a 1/128, about 16 ms), so each one's release has
    // died away before the next starts.
    let eighth = Ticks::note_value(1, 8);
    let notes: Vec<Note> = (0..4)
        .map(|i| note(eighth * i, eighth / 16, 69 + 2 * i as u8))
        .collect();
    let mut project = project_with(&notes);
    let (_, pattern) = project.first_pattern().unwrap();
    project.pattern_mut(pattern).unwrap().swing = daw_model::Swing {
        percent: 100,
        unit: eighth,
    };
    let (left, _) = render_offline(&project, library(), config(), 1.3);
    let mut from = 0;
    for (i, at) in [0.0, 0.375, 0.5, 0.875].into_iter().enumerate() {
        let expected = seconds(at);
        let start = onset(&left, from, 0.01).expect("a swung note is missing");
        assert!(
            start >= expected.saturating_sub(1) && start < expected + 64,
            "note {i} at {start}, expected {expected}"
        );
        from = expected + seconds(0.1);
    }
}

#[test]
fn tuplets_land_exactly() {
    // Five quintuplet sixteenths in one beat, on different keys so one
    // note's release never overlaps the next one's start.
    let quintuplet = Ticks::note_value(1, 20);
    let notes: Vec<Note> = (0..5)
        .map(|i| note(quintuplet * i, quintuplet / 2, 69 + 2 * i as u8))
        .collect();
    let project = project_with(&notes);
    let (left, _) = render_offline(&project, library(), config(), 1.0);
    let mut from = 0;
    for i in 0..5 {
        let expected = seconds(0.1 * f64::from(i));
        let start = onset(&left, from, 0.01).expect("a quintuplet note is missing");
        assert!(
            start >= expected.saturating_sub(1) && start < expected + 64,
            "note {i} at {start}, expected {expected}"
        );
        from = expected + seconds(0.07);
    }
}

#[test]
fn muted_tracks_are_silent() {
    let mut project = project_with(&[note(Ticks::ZERO, Ticks::QUARTER, 69)]);
    let (track, _) = project.first_pattern().unwrap();
    project.track_mut(track).unwrap().strip.mute = true;
    let (left, _) = render_offline(&project, library(), config(), 1.0);
    assert!(rms(&left) < 1e-6);
}

#[test]
fn a_missing_sound_falls_back_to_a_sine() {
    // No SoundFont registered: the engine must still make a sound.
    let project = project_with(&[note(Ticks::ZERO, Ticks::QUARTER, 69)]);
    let (left, _) = render_offline(&project, SoundLibrary::new(), config(), 0.5);
    assert!(rms(&left[seconds(0.1)..seconds(0.4)]) > 0.02);
}

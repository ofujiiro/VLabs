// SPDX-License-Identifier: GPL-3.0-or-later

//! A pattern's controller curves (decision 0033), heard: an offline render
//! follows a drawn bend, the wheel rests after the clip that bent it and
//! when playback stops, and a take records what the wheel did while the
//! old curve it overrides stays quiet.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use daw_dsp::sf2_writer::sine_test_font;
use daw_engine::{
    Control, Engine, EngineConfig, EngineEvent, EngineProcessor, PlaybackScope, SoundLibrary,
    render_offline,
};
use daw_model::{
    BUILTIN_PIANO, ControlKind, ControlPoint, DEFAULT_VELOCITY, Note, NoteId, Project, Ticks,
};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

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

fn point(at: Ticks, value: f32) -> ControlPoint {
    ControlPoint { at, value }
}

/// The frequency of a clean tone, from its rising zero crossings.
fn frequency(samples: &[f32]) -> f32 {
    let mut crossings = Vec::new();
    for i in 1..samples.len() {
        let (a, b) = (samples[i - 1], samples[i]);
        if a < 0.0 && b >= 0.0 {
            crossings.push(i as f32 - 1.0 + a / (a - b));
        }
    }
    assert!(crossings.len() > 2, "no tone to measure");
    let span = crossings.last().unwrap() - crossings.first().unwrap();
    (crossings.len() - 1) as f32 * RATE as f32 / span
}

fn semitones(hz: f32) -> f32 {
    12.0 * (hz / 440.0).log2()
}

fn at(seconds: f64) -> usize {
    (seconds * f64::from(RATE)) as usize
}

/// A one-bar pattern (two seconds at 120 BPM) holding A4 for the whole
/// bar, with the wheel rising to the top over its first beat and coming
/// back over its last; placed in bar 1 only. A second, plain pattern on the
/// same track plays A4 in bar 2.
fn bent_song() -> Project {
    let mut project = Project::starter("P", "T");
    project.loop_region = None;
    project.master.effects.clear();
    let (track, pattern) = project.first_pattern().unwrap();
    let bar = Ticks::WHOLE;
    project.pattern_mut(pattern).unwrap().length = bar;
    project.add_note(pattern, note(Ticks::ZERO, bar, 69));
    project.pattern_mut(pattern).unwrap().set_control(
        ControlKind::PitchBend,
        vec![
            point(Ticks::ZERO, 0.0),
            point(Ticks::QUARTER, 1.0),
            point(Ticks::QUARTER * 3, 1.0),
            point(bar, 0.0),
        ],
    );
    // The starter's clip is four bars long: one bar only.
    let clip = project.track(track).unwrap().clips[0].id;
    project.set_clip_length(clip, bar);
    let plain = project.add_pattern_for(track, "Plain", bar);
    project.add_note(plain, note(Ticks::ZERO, bar, 69));
    project.add_clip(track, bar, bar, plain);
    project
}

#[test]
fn a_drawn_bend_is_heard_and_rests_after_its_clip() {
    let (left, _) = render_offline(&bent_song(), library(), config(), 4.0);
    // Bar 1: from A4 up two semitones over the first beat, held, back down
    // over the last beat.
    // The curve rises four semitones a second over the first beat: on
    // average 0.2 between 20 and 80 ms, 1 between 220 and 280 ms.
    let start = semitones(frequency(&left[at(0.02)..at(0.08)]));
    let rising = semitones(frequency(&left[at(0.22)..at(0.28)]));
    let top = semitones(frequency(&left[at(0.6)..at(1.4)]));
    let falling = semitones(frequency(&left[at(1.72)..at(1.78)]));
    assert!((start - 0.2).abs() < 0.03, "rising from the start: {start}");
    assert!((rising - 1.0).abs() < 0.03, "half way up: {rising}");
    assert!((top - 2.0).abs() < 0.02, "held at the top: {top}");
    assert!((falling - 1.0).abs() < 0.03, "half way down: {falling}");
    // Bar 2: the plain pattern's note is not bent.
    let after = semitones(frequency(&left[at(2.2)..at(3.8)]));
    assert!(after.abs() < 0.02, "the next clip is not bent: {after}");
}

#[test]
fn a_bend_held_when_playback_stops_goes_back_to_rest() {
    let mut project = bent_song();
    // The wheel never comes back down in this one.
    let (track, pattern) = project.first_pattern().unwrap();
    project
        .pattern_mut(pattern)
        .unwrap()
        .set_control(ControlKind::PitchBend, vec![point(Ticks::ZERO, 1.0)]);
    let (mut engine, mut processor) = Engine::new(config(), library());
    engine.set_metronome(false, false, 0.0);
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.set_live_target(Some(track));
    let mut events = Vec::new();
    engine.play();
    let bent = run(&mut processor, &mut engine, 40, &mut events);
    assert!(
        (semitones(frequency(&bent[BLOCK * 10..])) - 2.0).abs() < 0.03,
        "the pattern bends"
    );
    engine.stop();
    run(&mut processor, &mut engine, 30, &mut events);
    // A key played after the stop sounds at its own pitch.
    let midi = engine.midi_sender();
    midi.note_on(69, 0.8);
    run(&mut processor, &mut engine, 8, &mut events);
    let played = run(&mut processor, &mut engine, 40, &mut events);
    midi.note_off(69);
    let pitch = semitones(frequency(&played));
    assert!(pitch.abs() < 0.03, "the wheel is back at rest: {pitch}");
}

/// Runs the processor for `blocks` blocks and returns the left channel.
fn run(
    processor: &mut EngineProcessor,
    engine: &mut Engine,
    blocks: usize,
    events: &mut Vec<EngineEvent>,
) -> Vec<f32> {
    let mut buffer = vec![0.0_f32; BLOCK * 2];
    let mut left = Vec::with_capacity(blocks * BLOCK);
    for _ in 0..blocks {
        processor.process(&mut buffer, 2);
        left.extend(buffer.as_chunks::<2>().0.iter().map(|f| f[0]));
        events.extend(engine.poll());
    }
    left
}

#[test]
fn a_take_records_the_wheel_and_silences_the_curve_it_overrides() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let before = daw_rt::violation_count();
    let mut project = bent_song();
    let (track, _) = project.first_pattern().unwrap();
    project.loop_region = None;
    let (mut engine, mut processor) = Engine::new(config(), library());
    engine.set_metronome(false, false, 0.0);
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.set_live_target(Some(track));
    let midi = engine.midi_sender();
    let mut events = Vec::new();
    // The wheel is half way down before recording starts: the take starts
    // with it there.
    midi.control(Control::PitchBend(-0.5));
    run(&mut processor, &mut engine, 2, &mut events);
    engine.record(track, 0);
    run(&mut processor, &mut engine, 4, &mut events);
    // The hand pushes the wheel up in steps; the pattern's own curve (which
    // rises to the top) is not heard meanwhile.
    for step in 0..=10 {
        midi.control(Control::PitchBend(step as f32 / 20.0));
        run(&mut processor, &mut engine, 2, &mut events);
    }
    let held = run(&mut processor, &mut engine, 40, &mut events);
    let pitch = semitones(frequency(&held));
    assert!(
        (pitch - 1.0).abs() < 0.03,
        "the hand's half-way bend, not the curve's full one: {pitch}"
    );
    engine.stop();
    run(&mut processor, &mut engine, 2, &mut events);

    let recorded: Vec<(ControlKind, f32, Ticks)> = events
        .iter()
        .filter_map(|e| match *e {
            EngineEvent::RecordedControl { control, value, at } => Some((control, value, at)),
            _ => None,
        })
        .collect();
    assert_eq!(
        recorded.first().map(|r| (r.0, r.1)),
        Some((ControlKind::PitchBend, -0.5)),
        "the take starts with the wheel where it was"
    );
    assert_eq!(
        recorded.last().map(|r| r.1),
        Some(0.5),
        "and ends at the top"
    );
    assert!(recorded.len() >= 11, "{recorded:?}");
    assert!(
        recorded.windows(2).all(|w| w[0].2 <= w[1].2),
        "in time order"
    );
    assert_eq!(
        daw_rt::violation_count(),
        before,
        "the audio thread allocated"
    );
}

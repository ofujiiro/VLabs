// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine as the audio thread sees it: no allocation while processing,
//! even while the program is replaced and notes are played live; recording
//! with a count-in; the metronome; the transport's stop behaviour.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use daw_dsp::sf2_writer::sine_test_font;
use daw_engine::{Engine, EngineConfig, EngineEvent, RecordState, SoundLibrary};
use daw_model::{BUILTIN_PIANO, DEFAULT_VELOCITY, Document, Note, NoteId, Project, Region, Ticks};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

fn engine() -> (Engine, daw_engine::EngineProcessor) {
    let bytes: &'static [u8] = Box::leak(sine_test_font(RATE).to_bytes().into_boxed_slice());
    let mut library = SoundLibrary::new();
    library.register_builtin(BUILTIN_PIANO, bytes);
    library.set_soundfont_reverb(0.0);
    Engine::new(
        EngineConfig {
            sample_rate: RATE,
            max_block: 512,
        },
        library,
    )
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

/// Runs the processor for `blocks` blocks and returns the left channel.
fn run(
    processor: &mut daw_engine::EngineProcessor,
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
fn processing_never_allocates() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let (mut engine, mut processor) = engine();
    let mut document = Document::new(Project::starter("P", "T"));
    let (track, pattern) = document.project().first_pattern().unwrap();
    for i in 0..64 {
        document.edit("Add", |p| {
            p.add_note(
                pattern,
                note(
                    Ticks::note_value(i, 16),
                    Ticks::note_value(1, 16),
                    48 + (i % 24) as u8,
                ),
            )
        });
    }
    engine.sync(document.project(), document.revision());
    engine.set_metronome(true, true, 0.5);
    engine.play();
    let mut events = Vec::new();
    let before = daw_rt::violation_count();
    for round in 0..40 {
        run(&mut processor, &mut engine, 8, &mut events);
        // Edit while playing: a new program is installed each round.
        document.edit("Move", |p| {
            let ids: Vec<NoteId> = p
                .pattern(pattern)
                .unwrap()
                .notes
                .iter()
                .take(4)
                .map(|n| n.id)
                .collect();
            p.update_notes(pattern, &ids, |n| n.key = 60 + (round % 12) as u8);
        });
        engine.sync(document.project(), document.revision());
        // Live notes too.
        engine.note_on(track, 72, 0.9);
        run(&mut processor, &mut engine, 2, &mut events);
        engine.note_off(track, 72);
    }
    engine.stop();
    run(&mut processor, &mut engine, 4, &mut events);
    assert_eq!(
        daw_rt::violation_count() - before,
        0,
        "the audio thread allocated or freed memory"
    );
}

#[test]
fn recording_with_a_count_in_places_notes_where_they_were_played() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let (mut engine, mut processor) = engine();
    let mut project = Project::starter("P", "T");
    project.loop_region = None;
    let (track, _) = project.first_pattern().unwrap();
    engine.sync(&project, 1);
    let mut events = Vec::new();
    run(&mut processor, &mut engine, 2, &mut events);

    // One bar of count-in at 120 BPM in 4/4: two seconds.
    engine.record(track, 1);
    run(&mut processor, &mut engine, 4, &mut events);
    assert!(
        matches!(engine.record_state(), RecordState::CountIn(_)),
        "should be counting in"
    );
    // The four blocks above were already part of the count-in.
    let count_in_blocks = (2 * RATE as usize).div_ceil(BLOCK) - 4;
    run(&mut processor, &mut engine, count_in_blocks, &mut events);
    assert_eq!(engine.record_state(), RecordState::Recording);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::RecordingStarted { at } if *at == Ticks::ZERO))
    );

    // Wait about one beat, play a note for about one beat, then stop.
    let beat_blocks = (RATE as usize / 2).div_ceil(BLOCK);
    run(&mut processor, &mut engine, beat_blocks, &mut events);
    engine.note_on(track, 64, 0.75);
    run(&mut processor, &mut engine, beat_blocks, &mut events);
    engine.note_off(track, 64);
    run(&mut processor, &mut engine, 4, &mut events);
    engine.stop();
    run(&mut processor, &mut engine, 2, &mut events);

    let recorded: Vec<(u8, Ticks, Ticks)> = events
        .iter()
        .filter_map(|e| match *e {
            EngineEvent::RecordedNote {
                key, start, end, ..
            } => Some((key, start, end)),
            _ => None,
        })
        .collect();
    assert_eq!(recorded.len(), 1, "one note was played: {events:?}");
    let (key, start, end) = recorded[0];
    assert_eq!(key, 64);
    // Within one block (256 frames = about 0.01 beat at 120 BPM) of beat 1.
    let tolerance = Ticks::QUARTER / 40;
    assert!(
        (start - Ticks::QUARTER).0.abs() <= tolerance.0,
        "started at {:.3} beats",
        start.as_quarters()
    );
    assert!(
        (end - start - Ticks::QUARTER).0.abs() <= tolerance.0 * 2,
        "lasted {:.3} beats",
        (end - start).as_quarters()
    );
    assert!(events.contains(&EngineEvent::RecordingStopped));
    assert_eq!(engine.record_state(), RecordState::Idle);
}

#[test]
fn a_note_held_across_the_loop_end_is_split() {
    let (mut engine, mut processor) = engine();
    let mut project = Project::starter("P", "T");
    project.loop_region = Region::new(Ticks::ZERO, Ticks::QUARTER * 2); // one second
    let (track, _) = project.first_pattern().unwrap();
    engine.sync(&project, 1);
    let mut events = Vec::new();
    engine.record(track, 0);
    run(&mut processor, &mut engine, 2, &mut events);
    let half_second = (RATE as usize / 2).div_ceil(BLOCK);
    run(&mut processor, &mut engine, half_second, &mut events);
    engine.note_on(track, 60, 0.8);
    // Hold through the loop end.
    run(&mut processor, &mut engine, half_second * 2, &mut events);
    engine.note_off(track, 60);
    run(&mut processor, &mut engine, 2, &mut events);
    engine.stop();
    run(&mut processor, &mut engine, 2, &mut events);
    let pieces: Vec<(Ticks, Ticks)> = events
        .iter()
        .filter_map(|e| match *e {
            EngineEvent::RecordedNote { start, end, .. } => Some((start, end)),
            _ => None,
        })
        .collect();
    assert_eq!(pieces.len(), 2, "{events:?}");
    assert_eq!(
        pieces[0].1,
        Ticks::QUARTER * 2,
        "the first piece ends at the loop end"
    );
    assert_eq!(
        pieces[1].0,
        Ticks::ZERO,
        "the second piece starts at the loop start"
    );
}

#[test]
fn the_metronome_clicks_on_every_beat() {
    let (mut engine, mut processor) = engine();
    let mut project = Project::starter("P", "T");
    project.loop_region = None;
    engine.sync(&project, 1);
    engine.set_metronome(true, true, 0.5);
    engine.play();
    let mut events = Vec::new();
    let left = run(
        &mut processor,
        &mut engine,
        (2 * RATE as usize).div_ceil(BLOCK),
        &mut events,
    );
    // Beats every half second: clicks start at 0, 0.5, 1.0, 1.5 s.
    for beat in 0..4 {
        let expected = beat * RATE as usize / 2;
        let start = left[expected.saturating_sub(64)..]
            .iter()
            .position(|s| s.abs() > 0.01)
            .map(|i| i + expected.saturating_sub(64));
        assert!(
            start.is_some_and(|s| s.abs_diff(expected) < 32),
            "beat {beat}: click at {start:?}, expected {expected}"
        );
    }
    // Silence between clicks.
    let between = &left[RATE as usize / 4..RATE as usize / 4 + 1000];
    assert!(between.iter().all(|s| s.abs() < 1e-3));
}

#[test]
fn stop_returns_to_where_playback_started_then_to_the_start() {
    let (mut engine, mut processor) = engine();
    let mut project = Project::starter("P", "T");
    project.loop_region = None;
    engine.sync(&project, 1);
    let mut events = Vec::new();
    engine.locate(Ticks::QUARTER * 4);
    engine.play();
    run(&mut processor, &mut engine, 40, &mut events);
    assert!(engine.position() > Ticks::QUARTER * 4);
    engine.stop();
    run(&mut processor, &mut engine, 1, &mut events);
    assert_eq!(engine.position(), Ticks::QUARTER * 4);
    assert!(!engine.is_playing());
    engine.stop();
    run(&mut processor, &mut engine, 1, &mut events);
    assert_eq!(engine.position(), Ticks::ZERO);
}

#[test]
fn midi_keyboard_notes_play_the_live_track_only_when_one_is_chosen() {
    let (mut engine, mut processor) = engine();
    let project = Project::starter("P", "T");
    let (track, _) = project.first_pattern().unwrap();
    engine.sync(&project, 1);
    let midi = engine.midi_sender();
    let mut events = Vec::new();
    run(&mut processor, &mut engine, 2, &mut events);

    // No live track chosen: the note goes nowhere.
    midi.note_on(69, 0.8);
    let silent = run(&mut processor, &mut engine, 20, &mut events);
    midi.note_off(69);
    assert!(
        silent.iter().all(|s| s.abs() < 1e-6),
        "a note played with no live track chosen"
    );

    engine.set_live_target(Some(track));
    midi.note_on(69, 0.8);
    let sounding = run(&mut processor, &mut engine, 20, &mut events);
    midi.note_off(69);
    let peak = sounding.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "the MIDI note is silent (peak {peak})");
}

#[test]
fn the_sustain_pedal_holds_notes_and_lengthens_recorded_ones() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let (mut engine, mut processor) = engine();
    let mut project = Project::starter("P", "T");
    project.loop_region = None;
    let (track, _) = project.first_pattern().unwrap();
    engine.sync(&project, 1);
    engine.set_live_target(Some(track));
    engine.set_metronome(false, false, 0.0); // Only the note is measured.
    let midi = engine.midi_sender();
    let mut events = Vec::new();
    run(&mut processor, &mut engine, 2, &mut events);
    engine.record(track, 0);
    run(&mut processor, &mut engine, 2, &mut events);

    // Pedal down, a short note: it keeps sounding after its key comes up.
    let beat_blocks = (RATE as usize / 2).div_ceil(BLOCK);
    midi.sustain(true);
    midi.note_on(64, 0.8);
    run(&mut processor, &mut engine, 4, &mut events);
    midi.note_off(64);
    let held = run(&mut processor, &mut engine, beat_blocks, &mut events);
    let peak = |samples: &[f32]| samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    let tail = &held[held.len() - BLOCK * 4..];
    assert!(
        peak(tail) > 0.05,
        "the pedal holds the note (peak {})",
        peak(tail)
    );

    // Pedal up: the note is released, and fades.
    midi.sustain(false);
    let released = run(&mut processor, &mut engine, beat_blocks * 2, &mut events);
    let end = &released[released.len() - BLOCK * 4..];
    assert!(
        peak(end) < 0.001,
        "released when the pedal came up (peak {})",
        peak(end)
    );
    engine.stop();
    run(&mut processor, &mut engine, 2, &mut events);

    let recorded: Vec<(Ticks, Ticks)> = events
        .iter()
        .filter_map(|e| match *e {
            EngineEvent::RecordedNote { start, end, .. } => Some((start, end)),
            _ => None,
        })
        .collect();
    assert_eq!(recorded.len(), 1, "{events:?}");
    let (start, end) = recorded[0];
    assert!(
        end - start > Ticks::QUARTER,
        "the recorded note lasts until the pedal came up: {:.2} beats",
        (end - start).as_quarters()
    );
    assert_eq!(
        daw_rt::violation_count(),
        0,
        "allocated on the audio thread"
    );
}

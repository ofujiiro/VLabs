// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 5's theory and rhythm tools (decision 0018), driven the way the
//! interface drives them: the song's key, keeping notes in it, chord
//! stamps, inversions and intervals, swing, patterns of any length, and
//! tempo, time-signature and key changes along the song.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use daw_engine::{Engine, EngineConfig, EngineProcessor, SoundLibrary};
use daw_model::{ChordKind, ChordShape, Scale, ScaleKind, Swing, Ticks, TimeSignature, TrackId};
use daw_ui::session::Draw;
use daw_ui::{Session, text};
use egui::Key;

const BLOCK: usize = 256;

/// A new song as a new user sees it: the whole song.
fn new_song() -> (Session, EngineProcessor) {
    let (engine, processor) = Engine::new(EngineConfig::default(), SoundLibrary::new());
    let mut session = Session::starter(engine);
    session.update();
    (session, processor)
}

fn piano(session: &Session) -> TrackId {
    session
        .project()
        .tracks
        .iter()
        .find(|t| !t.instrument.is_drums())
        .unwrap()
        .id
}

/// A new song, open on its (empty, four-bar) piano pattern.
fn piano_song() -> (Session, EngineProcessor) {
    let (mut session, processor) = new_song();
    let piano = piano(&session);
    session.open_track(piano);
    session.update();
    (session, processor)
}

fn run(session: &mut Session, processor: &mut EngineProcessor, seconds: f64) {
    let blocks = (seconds * 48_000.0 / BLOCK as f64).ceil() as usize;
    let mut buffer = vec![0.0_f32; BLOCK * 2];
    for _ in 0..blocks.max(1) {
        processor.process(&mut buffer, 2);
        session.update();
    }
}

/// The open pattern's keys, low to high.
fn keys(session: &Session) -> Vec<u8> {
    let mut keys: Vec<u8> = session.pattern().notes.iter().map(|n| n.key).collect();
    keys.sort_unstable();
    keys
}

/// The keys of the notes that start at `at`, low to high.
fn keys_at(session: &Session, at: Ticks) -> Vec<u8> {
    let mut keys: Vec<u8> = session
        .pattern()
        .notes
        .iter()
        .filter(|n| n.start == at)
        .map(|n| n.key)
        .collect();
    keys.sort_unstable();
    keys
}

fn c_major() -> Scale {
    Scale::new(0, ScaleKind::Major)
}

#[test]
fn notes_are_kept_in_the_key_when_asked() {
    let (mut s, _processor) = piano_song();
    assert_eq!(s.key(), None, "a new song has no key");
    let a_minor = Scale::new(9, ScaleKind::Minor);
    s.set_key(Some(a_minor));
    assert_eq!(s.key(), Some(a_minor));
    assert_eq!(s.pattern_key(), Some(a_minor), "the piano roll sees it");
    assert!(
        !s.keeps_in_key(),
        "keeping notes in the key is off at first"
    );

    // C sharp is not in A minor; off, a note goes where it is put.
    s.add_chord(Ticks::ZERO, 61, 0.8);
    assert_eq!(keys(&s), [61]);
    s.keep_in_key = true;
    assert!(s.keeps_in_key());
    s.add_chord(Ticks::QUARTER, 61, 0.8);
    assert_eq!(keys_at(&s, Ticks::QUARTER), [62], "halfway: up to D");

    // The arrows move the selection (the new note) by steps of the key.
    s.step_selection(true);
    assert_eq!(keys_at(&s, Ticks::QUARTER), [64], "D up a step: E");
    s.step_selection(false);
    s.step_selection(false);
    assert_eq!(keys_at(&s, Ticks::QUARTER), [60], "two steps down: C");
    s.undo();
    assert_eq!(
        keys_at(&s, Ticks::QUARTER),
        [62],
        "the arrow steps are one undo step"
    );

    // Dragging a chord by steps of the key keeps it a chord of the key.
    let moved = s.moved_keys(&[57, 60, 64], 57, 59).unwrap();
    assert_eq!(moved, [59, 62, 65], "A minor up a step: B diminished");
    // With no key, notes move by semitones and "keep in key" does nothing.
    s.set_key(None);
    assert!(!s.keeps_in_key());
    assert_eq!(s.moved_keys(&[57, 60, 64], 57, 59).unwrap(), [59, 62, 66]);
}

#[test]
fn chord_stamps_place_whole_chords_in_one_step() {
    let (mut s, _processor) = piano_song();
    s.set_key(Some(c_major()));
    s.draw = Draw::Chord(ChordShape::InKey(3));
    let ids = s.add_chord(Ticks::ZERO, 62, 0.8);
    assert_eq!(ids.len(), 3);
    assert_eq!(keys_at(&s, Ticks::ZERO), [62, 65, 69], "D minor in C major");
    assert_eq!(s.status(), Some(text::placed("D minor").as_str()));
    assert_eq!(s.selection().len(), 3, "the chord is selected");
    assert_eq!(s.chord_name(67).as_deref(), Some("G major"));

    s.draw = Draw::Chord(ChordShape::InKey(4));
    s.add_chord(Ticks::WHOLE, 67, 0.8);
    assert_eq!(keys_at(&s, Ticks::WHOLE), [67, 71, 74, 77], "G7");

    s.draw = Draw::Chord(ChordShape::Kind(ChordKind::MajorSeventh));
    s.chord_inversion = 1;
    s.add_chord(Ticks::WHOLE * 2, 60, 0.8);
    assert_eq!(
        keys_at(&s, Ticks::WHOLE * 2),
        [64, 67, 71, 72],
        "Cmaj7, first inversion"
    );
    s.undo();
    assert!(
        keys_at(&s, Ticks::WHOLE * 2).is_empty(),
        "one undo takes the whole chord away"
    );
    assert_eq!(s.pattern().notes.len(), 7);
}

#[test]
fn chords_invert_and_move_by_intervals() {
    let (mut s, _processor) = piano_song();
    s.draw = Draw::Chord(ChordShape::Kind(ChordKind::Major));
    s.add_chord(Ticks::ZERO, 60, 0.8);
    s.invert_selection(true);
    assert_eq!(keys(&s), [64, 67, 72], "first inversion");
    s.invert_selection(true);
    assert_eq!(keys(&s), [67, 72, 76], "second inversion");
    s.invert_selection(false);
    assert_eq!(keys(&s), [64, 67, 72]);
    s.undo();
    assert_eq!(keys(&s), [67, 72, 76], "each inversion is one undo step");

    s.transpose_by(7);
    assert_eq!(keys(&s), [74, 79, 83], "up a perfect fifth");
    s.transpose_by(-12);
    assert_eq!(keys(&s), [62, 67, 71], "down an octave");
    s.set_key(Some(c_major()));
    s.transpose_in_key(2);
    assert_eq!(keys(&s), [65, 71, 74], "up a third in C major");
    s.transpose_by(60);
    assert_eq!(keys(&s), [65, 71, 74], "nothing leaves the keyboard");
    assert_eq!(s.status(), Some(text::WOULD_LEAVE_KEYBOARD));

    let one = s.pattern().notes[0].id;
    s.select_only([one]);
    s.invert_selection(true);
    assert_eq!(s.status(), Some(text::NOTHING_TO_INVERT));
}

#[test]
fn swing_is_set_on_the_pattern_and_undone() {
    let (mut s, _processor) = new_song();
    assert!(s.swing().is_straight());
    let feel = Swing {
        percent: 67,
        unit: Ticks::note_value(1, 16),
    };
    s.set_swing(feel);
    assert_eq!(s.swing(), feel);
    assert_eq!(
        s.document().undo_label(),
        Some(text::EDIT_SWING),
        "one undo step"
    );
    s.undo();
    assert!(s.swing().is_straight());
}

#[test]
fn a_hit_recorded_late_in_a_swung_beat_is_written_on_its_step() {
    let (mut s, mut processor) = new_song();
    s.set_count_in_bars(0);
    let swing = Swing {
        percent: 100,
        unit: Ticks::note_value(1, 16),
    };
    s.set_swing(swing);
    // Play the snare where the second sixteenth sounds with this swing:
    // 0.1875 s after the bar line at 120 BPM, instead of 0.125 s.
    let played = swing.apply(Ticks::note_value(1, 16));
    let lead = Ticks::QUARTER / 5;
    s.locate_song(played - lead);
    s.toggle_record();
    run(&mut s, &mut processor, 0.1);
    s.key_down(Key::X);
    run(&mut s, &mut processor, 0.03);
    s.key_up(Key::X);
    run(&mut s, &mut processor, 0.02);
    s.toggle_record();
    run(&mut s, &mut processor, 0.02);
    let notes = &s.pattern().notes;
    assert_eq!(notes.len(), 1, "{notes:?}");
    // Within a few audio blocks (each a little over 5 ms) of the step.
    let error = (notes[0].start - Ticks::note_value(1, 16)).0.abs();
    let block = Ticks::QUARTER.0 / 94;
    assert!(
        error < 3 * block,
        "written {:?}, {error} ticks from the step",
        notes[0].start
    );
}

#[test]
fn patterns_of_different_lengths_turn_against_each_other() {
    let (mut s, _processor) = new_song();
    let drums = s.track_id();
    let three_beats = Ticks::QUARTER * 3;
    s.set_pattern_length(three_beats);
    assert_eq!(s.pattern().length, three_beats);
    let clip = s.project().track(drums).unwrap().clips[0];
    assert_eq!(
        clip.length,
        Ticks::WHOLE * 4,
        "the clip keeps its four bars, repeating the three beats"
    );
    assert_eq!(s.document().undo_label(), Some(text::EDIT_PATTERN_LENGTH));
    s.set_pattern_bars(1);
    assert_eq!(s.pattern().length, Ticks::WHOLE, "back to a whole bar");
}

#[test]
fn tempo_changes_are_added_moved_glided_and_removed() {
    let (mut s, mut processor) = new_song();
    s.show_song();
    let bar5 = Ticks::WHOLE * 4;
    let at = s.add_tempo_change(bar5);
    assert_eq!(s.tempo_changes().len(), 2);
    assert_eq!(
        s.tempo_changes()[1].bpm,
        120.0,
        "a new change starts at the tempo the song has there"
    );
    s.set_tempo_at(at, 90.0);

    // The transport shows and changes the tempo where the play position is
    // (once the audio thread has moved it there).
    assert_eq!(s.tempo(), 120.0);
    s.locate_song(bar5 + Ticks::QUARTER);
    run(&mut s, &mut processor, 0.01);
    assert_eq!(s.tempo(), 90.0);
    s.set_tempo(100.0);
    assert_eq!(s.tempo_changes()[1].bpm, 100.0);
    assert_eq!(s.tempo_changes()[0].bpm, 120.0, "the start keeps its tempo");

    s.set_tempo_glide(Ticks::ZERO, true);
    assert!(s.tempo_changes()[0].glide);
    let moved = s.move_tempo_change(bar5, Ticks::WHOLE * 2);
    assert_eq!(s.tempo_changes()[1].at, Ticks::WHOLE * 2);
    s.remove_tempo_change(moved);
    assert_eq!(s.tempo_changes().len(), 1);
    s.remove_tempo_change(Ticks::ZERO);
    assert_eq!(s.tempo_changes().len(), 1, "the start stays");
    for _ in 0..6 {
        s.undo();
    }
    assert_eq!(s.tempo_changes().len(), 1);
    assert_eq!(s.tempo_changes()[0].bpm, 120.0, "all undone");
}

#[test]
fn a_time_signature_change_keeps_the_song_in_its_bars() {
    let (mut s, mut processor) = new_song();
    s.show_song();
    let piano = piano(&s);
    let later = s.place_pattern(piano, Ticks::WHOLE * 5).unwrap();
    let seven_eight = TimeSignature::new(7, 8).unwrap();
    s.set_meter_change(2, Some(seven_eight));
    let bar = |s: &Session, n: u32| s.project().meter.bar_start(n);
    assert_eq!(s.meter_change_at(2), Some(seven_eight));
    assert_eq!(s.signature_bars(), [0, 2]);
    let clip = *s.project().clip(later).unwrap().1;
    assert_eq!(clip.start, bar(&s, 5), "the clip is still at bar 6");
    assert_eq!(bar(&s, 5), Ticks::WHOLE * 2 + Ticks::note_value(21, 8));

    // The transport shows the signature where the play position is.
    s.locate_song(bar(&s, 3));
    run(&mut s, &mut processor, 0.01);
    assert_eq!(s.time_signature(), seven_eight);
    assert_eq!(s.document().undo_label(), Some(text::EDIT_TIME_SIGNATURE));

    // Moving the change moves the bars after it, clips with them.
    assert_eq!(s.move_signature_changes(2, 3), 3);
    assert_eq!(s.meter_change_at(3), Some(seven_eight));
    let clip = *s.project().clip(later).unwrap().1;
    assert_eq!(clip.start, bar(&s, 5));

    // Key changes sit at bars in the same row.
    let a_minor = Scale::new(9, ScaleKind::Minor);
    s.set_key_change(4, Some(Some(a_minor)));
    assert_eq!(s.signature_bars(), [0, 3, 4]);
    assert_eq!(s.key_change_at(4), Some(Some(a_minor)));
    s.remove_signature_changes(3);
    assert_eq!(s.signature_bars(), [0, 4]);
    let clip = *s.project().clip(later).unwrap().1;
    assert_eq!(clip.start, Ticks::WHOLE * 5, "4/4 again: back at bar 6");
    s.undo();
    s.undo();
    s.undo();
    assert_eq!(s.meter_change_at(2), Some(seven_eight));
}

#[test]
fn a_pattern_counts_its_bars_from_where_it_starts_in_the_song() {
    let (mut s, _processor) = new_song();
    let piano = piano(&s);
    s.open_track(piano);
    let pattern = s.new_pattern();
    s.show_song();
    // After the starter's four-bar piano clip: bar 6.
    s.place_pattern(piano, Ticks::WHOLE * 5).unwrap();
    s.set_meter_change(5, Some(TimeSignature::new(3, 4).unwrap()));
    s.open_pattern(piano, pattern);
    let meter = s.pattern_meter();
    assert_eq!(
        meter.initial(),
        TimeSignature::new(3, 4).unwrap(),
        "the pattern starts in the 3/4 part"
    );
    assert_eq!(
        s.pattern_bars(),
        4,
        "the empty four-bar pattern kept its bars"
    );
    assert_eq!(s.pattern().length, Ticks::quarters(12));
}

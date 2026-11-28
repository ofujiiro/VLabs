// SPDX-License-Identifier: GPL-3.0-or-later

//! The session's editing, playing, recording and file handling, driven the
//! way the interface drives it, with a real engine whose audio thread is
//! simulated by calling the processor directly.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::assertions_on_result_states,
    clippy::assert_is_empty,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::path::PathBuf;

use daw_engine::{Engine, EngineConfig, EngineProcessor, PlaybackScope, SoundLibrary};
use daw_model::{Region, Ticks, TimeSignature, TrackId};
use daw_ui::session::Grid;
use daw_ui::{Session, View, text};
use egui::Key;

/// Frames per simulated audio block.
const BLOCK: usize = 256;

/// A new song, open on its piano pattern (the phase 1 tests work there).
fn session() -> (Session, EngineProcessor) {
    let (mut session, processor) = new_song();
    let piano = piano_track(&session);
    session.open_track(piano);
    session.update();
    (session, processor)
}

/// A new song as a new user sees it: the whole song.
fn new_song() -> (Session, EngineProcessor) {
    // No piano is registered, so the piano plays a sine synth: quick to
    // build and still audible. Drums fall back to the electronic kit.
    let (engine, processor) = Engine::new(EngineConfig::default(), SoundLibrary::new());
    let mut session = Session::starter(engine);
    session.update();
    (session, processor)
}

fn piano_track(session: &Session) -> TrackId {
    session
        .project()
        .tracks
        .iter()
        .find(|t| !t.instrument.is_drums())
        .unwrap()
        .id
}

/// Lets `seconds` of audio pass, updating the session between blocks the
/// way the interface does between frames.
fn run(session: &mut Session, processor: &mut EngineProcessor, seconds: f64) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "short test durations"
    )]
    let blocks = (seconds * 48_000.0 / BLOCK as f64).ceil() as usize;
    let mut buffer = vec![0.0_f32; BLOCK * 2];
    for _ in 0..blocks.max(1) {
        processor.process(&mut buffer, 2);
        session.update();
    }
}

fn keys(session: &Session) -> Vec<u8> {
    session.pattern().notes.iter().map(|n| n.key).collect()
}

fn starts(session: &Session) -> Vec<Ticks> {
    session.pattern().notes.iter().map(|n| n.start).collect()
}

fn temp_path(name: &str) -> PathBuf {
    let unique = format!(
        "{}-{}-{}",
        std::process::id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    std::env::temp_dir().join(unique)
}

#[test]
fn placing_editing_and_undoing_notes() {
    let (mut s, _processor) = session();
    let first = s.add_note(Ticks::ZERO, 60, 0.8).expect("a note is placed");
    assert_eq!(keys(&s), [60]);
    assert!(s.is_selected(first), "a new note is selected");
    assert!(s.is_dirty());

    s.duplicate();
    assert_eq!(
        starts(&s),
        [Ticks::ZERO, Ticks::QUARTER],
        "the copy goes right after"
    );
    s.transpose(1);
    assert_eq!(keys(&s), [60, 61], "only the selected copy moves up");
    s.nudge(1);
    assert_eq!(
        s.pattern().notes[1].start,
        Ticks::QUARTER + Ticks::note_value(1, 16)
    );
    s.delete_selected();
    assert_eq!(keys(&s), [60]);

    assert_eq!(s.document().undo_label(), Some(text::EDIT_DELETE_NOTES));
    s.undo();
    s.undo();
    s.undo();
    assert_eq!(keys(&s), [60, 60], "delete, nudge and transpose undone");
    s.undo();
    s.undo();
    assert!(keys(&s).is_empty(), "everything undone");
    s.redo();
    assert_eq!(keys(&s), [60]);
}

#[test]
fn pasting_lines_copies_up_after_the_original() {
    let (mut s, _processor) = session();
    s.note_length = Ticks::QUARTER;
    let a = s.add_note(Ticks::ZERO, 60, 0.8).unwrap();
    let b = s.add_note(Ticks::QUARTER * 2, 64, 0.8).unwrap();
    s.select_only([a, b]);
    assert!(s.copy());
    s.paste();
    s.paste();
    let bar = Ticks::WHOLE;
    assert_eq!(
        starts(&s),
        [
            Ticks::ZERO,
            Ticks::QUARTER * 2,
            bar,
            bar + Ticks::QUARTER * 2,
            bar * 2,
            bar * 2 + Ticks::QUARTER * 2
        ],
        "a phrase longer than half a bar repeats bar by bar"
    );
    assert_eq!(s.selection().len(), 2, "the pasted notes are selected");

    s.locate(Ticks::QUARTER);
    s.paste();
    assert!(
        starts(&s).contains(&(Ticks::QUARTER * 3)),
        "after clicking the ruler, notes paste at the play position"
    );
}

#[test]
fn the_pattern_grows_for_notes_past_its_end() {
    let (mut s, _processor) = session();
    assert_eq!(s.pattern_bars(), 4);
    s.add_note(Ticks::WHOLE * 5 + Ticks::QUARTER, 60, 0.8);
    assert_eq!(s.pattern_bars(), 6);
    let placement = s.placement();
    assert_eq!(placement.length, Ticks::WHOLE * 6);
    assert!(s.is_looping());
    assert_eq!(
        s.playback_scope(),
        PlaybackScope::Looping(Region::new(Ticks::ZERO, Ticks::WHOLE * 6).unwrap()),
        "the loop follows the pattern"
    );
    s.undo();
    assert_eq!(s.pattern_bars(), 4, "growing is part of the same undo step");
}

#[test]
fn shortening_never_cuts_notes_off() {
    let (mut s, _processor) = session();
    s.add_note(Ticks::WHOLE * 2, 60, 0.8);
    s.set_pattern_bars(1);
    assert_eq!(s.pattern_bars(), 3, "the pattern keeps room for its notes");
    s.set_pattern_bars(8);
    assert_eq!(s.pattern_bars(), 8);
}

#[test]
fn computer_keys_play_and_release() {
    let (mut s, mut processor) = session();
    let base = s.keyboard_base();
    assert!(s.key_down(Key::Z));
    assert!(s.key_down(Key::Z), "auto-repeat is ignored");
    assert!(!s.key_down(Key::Space), "Space does not play notes");
    assert!(s.is_key_lit(base));
    run(&mut s, &mut processor, 0.05);
    assert!(
        s.engine().peak() > 0.01,
        "a held key is heard: peak {}",
        s.engine().peak()
    );

    s.key_up(Key::Z);
    assert!(!s.is_key_lit(base));
    run(&mut s, &mut processor, 1.0);
    assert!(
        s.engine().peak() < 0.01,
        "a released key fades out: peak {}",
        s.engine().peak()
    );

    s.shift_keyboard_octave(1);
    assert!(s.key_down(Key::Q));
    assert!(
        s.is_key_lit(base + 24),
        "Q plays an octave above Z, and the octave moved"
    );
    s.release_keys();
    assert!(!s.is_key_lit(base + 24), "losing focus releases every key");
}

#[test]
fn recording_a_take_is_one_undo_step() {
    let (mut s, mut processor) = session();
    s.set_count_in_bars(0);
    s.toggle_record();
    run(&mut s, &mut processor, 0.1);
    assert!(s.is_recording());

    s.key_down(Key::Z);
    run(&mut s, &mut processor, 0.25);
    assert_eq!(
        s.notes_being_recorded().len(),
        1,
        "a held note shows while recording"
    );
    s.key_up(Key::Z);
    run(&mut s, &mut processor, 0.1);
    s.key_down(Key::X);
    run(&mut s, &mut processor, 0.25);
    s.key_up(Key::X);
    run(&mut s, &mut processor, 0.1);
    s.toggle_record();
    run(&mut s, &mut processor, 0.05);

    assert!(!s.is_recording());
    let base = s.keyboard_base();
    assert_eq!(keys(&s), [base, base + 2]);
    let first = s.pattern().notes[0];
    // At 120 BPM a quarter note lasts half a second: 0.25 s is an eighth.
    let eighth = Ticks::note_value(1, 8).0;
    assert!(
        (first.length.0 - eighth).abs() < eighth / 5,
        "the note lasts as long as the key was held: {:?}",
        first.length
    );
    assert_eq!(s.status(), Some(text::recorded(2).as_str()));
    assert!(
        s.take_finished(),
        "the editor is told once that a take ended"
    );
    assert!(!s.take_finished());
    assert_eq!(
        s.document().undo_label(),
        Some(text::edit_record(2).as_str())
    );
    s.undo();
    assert!(keys(&s).is_empty(), "one undo removes the whole take");
}

#[test]
fn recorded_notes_can_snap_to_the_grid() {
    let (mut s, mut processor) = session();
    s.set_count_in_bars(0);
    s.set_snap_recording(true);
    s.grid = Grid::Note(4);
    s.toggle_record();
    run(&mut s, &mut processor, 0.6); // A little after the second beat.
    s.key_down(Key::C);
    run(&mut s, &mut processor, 0.2);
    s.key_up(Key::C);
    run(&mut s, &mut processor, 0.05);
    s.stop();
    run(&mut s, &mut processor, 0.05);
    assert_eq!(starts(&s), [Ticks::QUARTER], "the note moved onto the beat");
}

#[test]
fn snapping_clips_is_remembered_between_sessions() {
    let (mut s, _processor) = session();
    assert!(s.snap_clips(), "clips snap by default");
    s.set_snap_clips(false);
    let mut settings = daw_ui::Settings::default();
    assert!(settings.snap_clips);
    s.store_settings(&mut settings);
    assert!(!settings.snap_clips);
    let (mut other, _processor) = session();
    other.apply_settings(&settings);
    assert!(!other.snap_clips());
    // Settings written before the option existed keep snapping.
    let old: daw_ui::Settings = serde_json::from_str("{}").unwrap();
    assert!(old.snap_clips);
}

#[test]
fn the_count_in_comes_before_recording() {
    let (mut s, mut processor) = session();
    s.set_count_in_bars(1);
    s.toggle_record();
    run(&mut s, &mut processor, 0.5);
    assert!(
        matches!(s.record_state(), daw_engine::RecordState::CountIn(_)),
        "counting in: {:?}",
        s.record_state()
    );
    run(&mut s, &mut processor, 1.7); // One 4/4 bar at 120 BPM is 2 seconds.
    assert_eq!(s.record_state(), daw_engine::RecordState::Recording);
}

#[test]
fn playback_stops_at_the_end_when_not_looping() {
    let (mut s, mut processor) = session();
    s.set_pattern_bars(1);
    s.toggle_loop();
    assert!(!s.is_looping());
    s.play();
    run(&mut s, &mut processor, 1.0);
    assert!(s.is_playing());
    run(&mut s, &mut processor, 1.5);
    assert!(
        !s.is_playing(),
        "stopped after the pattern's single bar (2 s)"
    );

    s.toggle_loop();
    s.play();
    run(&mut s, &mut processor, 5.0);
    assert!(s.is_playing(), "a loop plays on");
}

#[test]
fn tempo_and_time_signature_are_undoable() {
    let (mut s, _processor) = session();
    s.set_tempo(90.0);
    assert!((s.tempo() - 90.0).abs() < 1e-9);
    s.set_tempo(5000.0);
    assert!(s.tempo() <= 999.0, "the tempo stays in range");
    s.undo();
    s.undo();
    assert!((s.tempo() - 120.0).abs() < 1e-9);

    s.set_time_signature(TimeSignature::new(3, 4).unwrap());
    assert_eq!(s.pattern_bars(), 4, "the pattern keeps its bars");
    assert_eq!(s.pattern().length, Ticks::QUARTER * 12);
    s.undo();
    assert_eq!(s.pattern().length, Ticks::QUARTER * 16);
}

#[test]
fn saving_and_opening_round_trip() {
    let (mut s, _processor) = session();
    s.add_note(Ticks::QUARTER, 67, 0.5);
    let path = temp_path("song");
    s.save_as(&path).expect("saved");
    assert!(!s.is_dirty());
    let saved = s.path().expect("the song has a file now").to_path_buf();
    assert_eq!(
        saved.extension().and_then(|e| e.to_str()),
        Some(daw_ident::PROJECT_FILE_EXTENSION),
        "the file ending is added"
    );
    assert_eq!(
        s.display_name(),
        saved.file_stem().unwrap().to_string_lossy()
    );

    let (mut other, _processor) = session();
    other.open(&saved).expect("opened");
    assert_eq!(other.view(), View::Song, "an opened song shows the song");
    let piano = piano_track(&other);
    other.open_track(piano);
    assert_eq!(keys(&other), [67]);
    assert!(!other.is_dirty());
    assert!(
        other.open(&temp_path("missing")).is_err(),
        "a missing file is reported"
    );
    assert_eq!(keys(&other), [67], "a failed open leaves the song alone");
    std::fs::remove_file(saved).ok();
}

/// The file formats' reader, slowed down as on a slow disk.
fn slow_sfz_reader(path: &std::path::Path) -> Result<daw_dsp::sampler::SamplerBank, String> {
    std::thread::sleep(std::time::Duration::from_millis(400));
    daw_formats::sfz::load(path).map_err(|e| e.to_string())
}

#[test]
fn an_sfz_track_says_it_is_being_read_then_plays() {
    let folder = std::env::temp_dir().join(format!("session-sfz-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let mut writer = daw_formats::wav::WavWriter::create_with_channels(
        &folder.join("tone.wav"),
        daw_formats::wav::WavFormat::Pcm16,
        48_000,
        1,
    )
    .unwrap();
    let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.06).sin() * 0.5).collect();
    writer.write_interleaved(&tone).unwrap();
    writer.finish().unwrap();
    let file = folder.join("Slow.sfz");
    std::fs::write(&file, "<region> sample=tone.wav loop_mode=loop_continuous").unwrap();

    let mut library = SoundLibrary::new();
    library.set_sfz_reader(slow_sfz_reader);
    library.set_background_loading(true);
    let (engine, mut processor) = Engine::new(EngineConfig::default(), library);
    // A song of a synth alone: nothing else for the status line to say.
    let mut project = daw_model::Project::empty();
    let synth = project.add_track(
        "Synth",
        daw_model::Instrument::Synth(daw_model::SynthPreset::Plain.settings()),
    );
    let pattern = project.add_pattern_for(synth, "P", Ticks::WHOLE);
    project.add_clip(synth, Ticks::ZERO, Ticks::WHOLE, pattern);
    let mut s = Session::new(engine, project, None, true);
    s.update();
    assert_eq!(s.status(), None);
    let track = s.add_sfz_track(&file);
    s.update();
    assert!(s.is_track_reading(track), "read in the background");
    let path = file.to_string_lossy().into_owned();
    assert_eq!(
        s.status(),
        Some(text::reading_instruments(&[path]).as_str()),
        "the status line says so"
    );
    let start = std::time::Instant::now();
    while s.is_reading_instruments() {
        assert!(start.elapsed().as_secs() < 20, "never read");
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.update();
    }
    assert_eq!(s.status(), None, "the notice goes once it is ready");
    assert!(!s.is_track_reading(track));
    // It plays from the computer keyboard.
    assert!(s.key_down(Key::Q));
    let mut buffer = vec![0.0_f32; BLOCK * 2];
    let mut loudest = 0.0_f32;
    for _ in 0..40 {
        processor.process(&mut buffer, 2);
        s.update();
        loudest = buffer.iter().fold(loudest, |m, v| m.max(v.abs()));
    }
    assert!(loudest > 0.05, "heard once ready: {loudest}");
    s.key_up(Key::Q);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn recording_the_pitch_wheel_keeps_its_curve_with_the_take() {
    let (mut s, mut processor) = session();
    s.set_count_in_bars(0);
    s.toggle_record();
    run(&mut s, &mut processor, 0.1);
    assert!(s.is_recording());
    let midi = s.engine().midi_sender();

    // A note, with the wheel pushed up in a straight line, held, then let
    // back down: dozens of moves.
    midi.note_on(60, 0.8);
    run(&mut s, &mut processor, 0.05);
    for step in 0..=20 {
        midi.control(daw_engine::Control::PitchBend(step as f32 / 20.0));
        run(&mut s, &mut processor, 0.01);
    }
    run(&mut s, &mut processor, 0.2);
    for step in (0..=20).rev() {
        midi.control(daw_engine::Control::PitchBend(step as f32 / 20.0));
        run(&mut s, &mut processor, 0.01);
    }
    midi.note_off(60);
    run(&mut s, &mut processor, 0.05);
    s.toggle_record();
    run(&mut s, &mut processor, 0.05);

    assert_eq!(keys(&s), [60]);
    let curve = s.control_points(daw_model::ControlKind::PitchBend);
    assert!(
        (3..=8).contains(&curve.len()),
        "the gesture is kept as a handful of points: {curve:?}"
    );
    let top = curve.iter().map(|p| p.value).fold(f32::MIN, f32::max);
    assert!((top - 1.0).abs() < 1e-6, "it reaches the top: {curve:?}");
    assert!(
        curve.first().is_some_and(|p| p.value.abs() < 0.06)
            && curve.last().is_some_and(|p| p.value.abs() < 1e-6),
        "it starts near rest and ends at rest: {curve:?}"
    );
    assert!(
        curve.windows(2).all(|w| w[0].at < w[1].at),
        "in time order: {curve:?}"
    );
    let note = s.pattern().notes[0];
    assert!(
        curve.first().is_some_and(|p| p.at >= note.start)
            && curve
                .last()
                .is_some_and(|p| p.at <= note.end() + Ticks::note_value(1, 16)),
        "the bend lies over the note it bent"
    );
    s.undo();
    assert!(keys(&s).is_empty(), "one undo removes the take's notes");
    assert!(
        s.control_points(daw_model::ControlKind::PitchBend)
            .is_empty(),
        "and its curve"
    );
}

#[test]
fn a_take_on_drums_keeps_no_curves() {
    let (mut s, mut processor) = new_song();
    let drums = s.project().tracks[0].id;
    s.open_track(drums);
    s.update();
    s.set_count_in_bars(0);
    s.toggle_record();
    run(&mut s, &mut processor, 0.1);
    let midi = s.engine().midi_sender();
    midi.note_on(36, 0.8);
    midi.control(daw_engine::Control::PitchBend(1.0));
    run(&mut s, &mut processor, 0.2);
    midi.note_off(36);
    run(&mut s, &mut processor, 0.05);
    s.toggle_record();
    run(&mut s, &mut processor, 0.05);
    assert_eq!(keys(&s), [36], "the hit is recorded");
    assert!(s.pattern().controls.is_empty(), "drums are not bent");
}

#[test]
fn a_midi_file_dropped_on_a_track_goes_into_it() {
    let (mut s, _processor) = new_song();
    let piano = piano_track(&s);
    let path = temp_path("tune").with_extension("mid");
    let mut project = daw_model::Project::starter("Tune", "Piano");
    let (_, pattern) = project.first_pattern().unwrap();
    project.add_note(
        pattern,
        daw_model::Note {
            id: daw_model::NoteId(0),
            start: Ticks::ZERO,
            length: Ticks::QUARTER,
            key: 72,
            velocity: 0.8,
            pitch_offset: 0.0,
        },
    );
    let bytes = daw_formats::midi::write(&daw_formats::midi::song_to_midi(&project));
    std::fs::write(&path, bytes).unwrap();
    let tracks = s.project().tracks.len();

    s.import_files(vec![path.clone()], Some(piano), Ticks::ZERO);
    assert_eq!(s.project().tracks.len(), tracks, "no new piano track");
    let into: Vec<u8> = s
        .project()
        .track_patterns(piano)
        .flat_map(|p| p.notes.iter().map(|n| n.key))
        .collect();
    assert!(
        into.contains(&72),
        "the notes are on the piano track: {into:?}"
    );
    s.undo();
    assert!(
        !s.project()
            .track_patterns(piano)
            .any(|p| p.notes.iter().any(|n| n.key == 72)),
        "one undo takes it back"
    );

    // Dropped away from the tracks, it still makes tracks of its own.
    s.import_files(vec![path.clone()], None, Ticks::ZERO);
    assert_eq!(s.project().tracks.len(), tracks + 1);
    std::fs::remove_file(path).ok();
}

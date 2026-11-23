// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 2 in the session, driven the way the interface drives it: the
//! song and pattern views and what Space plays in each, drum hits, tracks,
//! patterns and clips, the mixer and effects, and exporting a WAV file.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::assertions_on_result_states,
    clippy::assert_is_empty,
    clippy::chunks_exact_to_as_chunks,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::path::PathBuf;

use daw_engine::{Engine, EngineConfig, EngineProcessor, PlaybackScope, SoundLibrary};
use daw_model::{
    ChainPlace, EffectKind, Instrument, PadSetting, Region, ReverbSettings, Ticks, TrackId,
};
use daw_ui::{ExportFormat, Session, View, text};
use egui::Key;

const BLOCK: usize = 256;

/// A new song, showing the whole song. No piano is registered: the piano plays
/// a sine synth, the drums the electronic kit.
fn new_song() -> (Session, EngineProcessor) {
    let (engine, processor) = Engine::new(EngineConfig::default(), SoundLibrary::new());
    let mut session = Session::starter(engine);
    session.update();
    (session, processor)
}

/// Lets `seconds` of audio pass; returns the loudest sample.
fn run(session: &mut Session, processor: &mut EngineProcessor, seconds: f64) -> f32 {
    let blocks = (seconds * 48_000.0 / BLOCK as f64).ceil() as usize;
    let mut buffer = vec![0.0_f32; BLOCK * 2];
    let mut peak = 0.0_f32;
    for _ in 0..blocks.max(1) {
        processor.process(&mut buffer, 2);
        peak = buffer.iter().fold(peak, |m, s| m.max(s.abs()));
        session.update();
    }
    peak
}

fn track(session: &Session, drums: bool) -> TrackId {
    session
        .project()
        .tracks
        .iter()
        .find(|t| t.instrument.is_drums() == drums)
        .unwrap()
        .id
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ))
}

#[test]
fn a_new_song_opens_on_the_song_and_its_empty_beat_plays_at_once() {
    let (mut s, mut processor) = new_song();
    assert_eq!(s.view(), View::Song, "a new song shows the whole song");
    assert!(!s.is_dirty(), "nothing to save yet");
    assert!(s.is_drum_track(), "F5 opens the beat");
    let drums = s
        .project()
        .tracks
        .iter()
        .find(|t| t.instrument.is_drums())
        .unwrap()
        .id;
    s.open_track(drums);
    assert_eq!(s.view(), View::Pattern);
    assert!(s.is_drum_track());
    assert_eq!(s.pattern().length, Ticks::WHOLE, "a one-bar beat");
    assert_eq!(s.keyboard_base(), 36, "Z plays the kick");
    assert!(!s.is_dirty(), "nothing to save yet");
    // The beat repeats over the song's four bars: they loop.
    assert_eq!(
        s.playback_scope(),
        PlaybackScope::Looping(Region::new(Ticks::ZERO, Ticks::WHOLE * 4).unwrap())
    );

    // Four clicks on the step grid, then Space.
    for step in [0, 4, 8, 12] {
        s.add_hit(36, s.step() * step, 0.8);
    }
    assert_eq!(s.pattern().notes.len(), 4);
    s.play();
    let peak = run(&mut s, &mut processor, 0.2);
    assert!(peak > 0.1, "the kick is heard: {peak}");

    // In the second bar the play position folds back onto the one-bar beat.
    s.stop();
    s.locate_song(Ticks::WHOLE + Ticks::QUARTER);
    run(&mut s, &mut processor, 0.02);
    assert_eq!(s.song_position(), Ticks::WHOLE + Ticks::QUARTER);
    assert_eq!(s.position(), Ticks::QUARTER);
}

#[test]
fn space_plays_what_is_on_screen() {
    let (mut s, _processor) = new_song();
    let piano = track(&s, false);
    s.open_track(piano);
    assert_eq!(
        s.playback_scope(),
        PlaybackScope::Looping(Region::new(Ticks::ZERO, Ticks::WHOLE * 4).unwrap()),
        "a placed pattern loops the part of the song where it is"
    );
    s.toggle_loop();
    assert_eq!(s.playback_scope(), PlaybackScope::Unlooped);
    s.toggle_loop();

    let fresh = s.new_pattern();
    assert_eq!(s.pattern_id(), fresh);
    assert!(!s.is_placed());
    assert_eq!(
        s.playback_scope(),
        PlaybackScope::Pattern {
            track: piano,
            pattern: fresh,
            looping: true,
        },
        "a pattern not placed anywhere plays alone"
    );

    s.show_song();
    assert_eq!(s.view(), View::Song);
    assert_eq!(s.playback_scope(), PlaybackScope::Song);
    assert!(!s.is_looping(), "the song plays through once");
    s.toggle_loop();
    assert_eq!(
        s.project().loop_region,
        Region::new(Ticks::ZERO, Ticks::WHOLE * 4),
        "looping the song loops all of it"
    );
}

#[test]
fn recording_in_the_song_view_records_into_the_pattern() {
    let (mut s, mut processor) = new_song();
    s.show_song();
    s.set_count_in_bars(0);
    s.toggle_record();
    assert_eq!(s.view(), View::Pattern, "recording opens the pattern");
    run(&mut s, &mut processor, 0.1);
    assert!(s.is_recording());
    s.key_down(Key::Z);
    run(&mut s, &mut processor, 0.1);
    s.key_up(Key::Z);
    run(&mut s, &mut processor, 0.05);
    s.toggle_record();
    run(&mut s, &mut processor, 0.05);
    let keys: Vec<u8> = s.pattern().notes.iter().map(|n| n.key).collect();
    assert_eq!(keys, [36], "a kick in the beat");
}

#[test]
fn recording_into_a_repeated_beat_folds_back_onto_it() {
    let (mut s, mut processor) = new_song();
    s.set_count_in_bars(0);
    // Start in the third bar of the four-bar clip that repeats the beat.
    s.locate_song(Ticks::WHOLE * 2);
    s.toggle_record();
    run(&mut s, &mut processor, 0.3);
    s.key_down(Key::X);
    run(&mut s, &mut processor, 0.1);
    s.key_up(Key::X);
    run(&mut s, &mut processor, 0.05);
    s.toggle_record();
    run(&mut s, &mut processor, 0.05);
    let notes = &s.pattern().notes;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].key, 38, "X is the snare");
    assert!(
        notes[0].start < Ticks::WHOLE,
        "the hit lands in the one-bar beat: {:?}",
        notes[0].start
    );
    assert_eq!(s.pattern().length, Ticks::WHOLE, "the beat did not grow");
}

#[test]
fn drum_hits_are_added_removed_and_undone() {
    let (mut s, _processor) = new_song();
    let hit = s.add_hit(42, s.step() * 2, 0.6).unwrap();
    assert_eq!(s.pattern().notes[0].length, s.step(), "one step long");
    s.set_velocities(&[hit], 0.3);
    assert!((s.pattern().notes[0].velocity - 0.3).abs() < 1e-6);
    s.remove_notes(&[hit]);
    assert!(s.pattern().notes.is_empty());
    s.undo();
    s.undo();
    assert!((s.pattern().notes[0].velocity - 0.6).abs() < 1e-6);

    s.set_step(Ticks::note_value(1, 8));
    assert_eq!(s.step(), Ticks::note_value(1, 8));
    s.undo();
    assert_eq!(s.step(), Ticks::note_value(1, 16));
}

#[test]
fn tracks_are_added_renamed_moved_and_removed() {
    let (mut s, _processor) = new_song();
    let piano = s.add_piano_track();
    assert_eq!(s.project().track(piano).unwrap().name, "Piano 2");
    assert_eq!(s.track_id(), piano, "a new track comes into focus");
    let drums = s.add_drum_track();
    assert_eq!(s.project().track(drums).unwrap().name, "Drums 2");
    let clip = s.project().track(drums).unwrap().clips[0];
    assert_eq!(
        clip.length,
        Ticks::WHOLE * 4,
        "the new beat covers the song"
    );

    s.rename_track(drums, "  Percussion ");
    assert_eq!(s.project().track(drums).unwrap().name, "Percussion");
    s.rename_track(drums, "   ");
    assert_eq!(s.project().track(drums).unwrap().name, "Percussion");
    s.move_track(drums, -1);
    assert_eq!(s.project().tracks[2].id, drums);

    s.set_kit(drums, daw_model::ELECTRONIC_KIT);
    assert_eq!(s.kit(drums), Some(daw_model::ELECTRONIC_KIT));

    s.open_track(drums);
    s.remove_track(drums);
    assert!(s.project().track(drums).is_none());
    assert_ne!(
        s.track_id(),
        drums,
        "the focus moves to a track that exists"
    );
    s.undo();
    assert!(s.project().track(drums).is_some(), "undo brings it back");

    for _ in 0..4 {
        let first = s.project().tracks[0].id;
        s.remove_track(first);
    }
    assert_eq!(s.project().tracks.len(), 1, "the last track stays");
}

#[test]
fn patterns_are_made_copied_renamed_and_removed() {
    let (mut s, _processor) = new_song();
    let beat = s.pattern_id();
    s.add_hit(36, Ticks::ZERO, 0.8);
    let second = s.new_pattern();
    assert_eq!(s.pattern().name, "Beat 2");
    assert!(s.pattern().notes.is_empty());
    let copy = s.duplicate_pattern().unwrap();
    assert_eq!(s.pattern().name, "Beat 3");
    s.open_pattern(s.track_id(), beat);
    let copy_of_beat = s.duplicate_pattern().unwrap();
    assert_eq!(s.pattern().notes.len(), 1, "the copy has the notes");
    assert_ne!(
        s.pattern().notes[0].id,
        s.project().pattern(beat).unwrap().notes[0].id
    );

    s.rename_pattern(copy_of_beat, "Fill");
    assert_eq!(s.pattern().name, "Fill");

    let drums = s.track_id();
    for pattern in [second, copy, copy_of_beat] {
        s.remove_pattern(pattern);
    }
    assert_eq!(
        s.pattern_id(),
        beat,
        "the open pattern was removed: another opens"
    );
    s.remove_pattern(beat);
    assert_eq!(
        s.project().track_patterns(drums).count(),
        1,
        "a track keeps one (empty) pattern"
    );
    assert!(s.project().track(drums).unwrap().clips.is_empty());
    s.undo();
    assert!(s.project().pattern(beat).is_some());
    assert_eq!(s.project().track(drums).unwrap().clips.len(), 1);
}

#[test]
fn clips_are_placed_moved_repeated_copied_and_removed() {
    let (mut s, _processor) = new_song();
    let piano = track(&s, false);
    s.show_song();
    assert_eq!(
        s.place_pattern(piano, Ticks::WHOLE),
        None,
        "not on top of a clip"
    );
    let clip = s.place_pattern(piano, Ticks::WHOLE * 6).unwrap();
    assert_eq!(s.selected_clip(), Some(clip));
    assert_eq!(s.song_length(), Ticks::WHOLE * 10);

    s.begin_gesture(text::EDIT_MOVE_CLIP);
    s.move_clip(clip, Ticks::WHOLE * 5);
    s.move_clip(clip, Ticks::WHOLE * 4);
    s.end_gesture();
    assert_eq!(s.project().clip(clip).unwrap().1.start, Ticks::WHOLE * 4);
    s.resize_clip(clip, Ticks::WHOLE * 8);
    let copy = s.duplicate_clip(clip).unwrap();
    assert_eq!(s.project().clip(copy).unwrap().1.start, Ticks::WHOLE * 12);
    s.nudge_clip(1);
    assert_eq!(s.project().clip(copy).unwrap().1.start, Ticks::WHOLE * 13);
    s.remove_clip(copy);
    assert!(s.project().clip(copy).is_none());
    assert_eq!(s.selected_clip(), None);

    s.undo(); // remove
    s.undo(); // nudge
    s.undo(); // duplicate
    s.undo(); // resize
    s.undo(); // the whole move gesture at once
    assert_eq!(s.project().clip(clip).unwrap().1.start, Ticks::WHOLE * 6);
}

#[test]
fn the_mixer_and_effects_are_undoable() {
    let (mut s, _processor) = new_song();
    let drums = track(&s, true);
    s.set_track_gain(drums, 0.5);
    s.set_track_pan(drums, -0.25);
    s.toggle_mute(drums);
    s.toggle_solo(drums);
    s.set_master_gain(3.0);
    let strip = s.project().track(drums).unwrap().strip;
    assert!((strip.gain - 0.5).abs() < 1e-6 && (strip.pan + 0.25).abs() < 1e-6);
    assert!(strip.mute && strip.solo);
    assert!(
        (s.project().master.gain - 2.0).abs() < 1e-6,
        "+6 dB at most"
    );

    let place = ChainPlace::Track(drums);
    let reverb = s
        .add_effect(place, EffectKind::Reverb(ReverbSettings::default()))
        .unwrap();
    let eq = s
        .add_effect(place, EffectKind::Eq(daw_model::EqSettings::default()))
        .unwrap();
    s.move_effect(eq, 0);
    assert_eq!(s.project().chain(place)[0].id, eq);
    s.set_effect(
        reverb,
        EffectKind::Reverb(ReverbSettings {
            mix: 0.6,
            ..ReverbSettings::default()
        }),
    );
    s.set_effect(
        reverb,
        EffectKind::Limiter(daw_model::LimiterSettings::default()),
    );
    assert!(
        matches!(s.project().effect(reverb).unwrap().kind, EffectKind::Reverb(r) if (r.mix - 0.6).abs() < 1e-6),
        "an effect keeps its kind"
    );
    s.set_effect_enabled(reverb, false);
    assert!(!s.project().effect(reverb).unwrap().enabled);
    s.remove_effect(eq);
    assert_eq!(s.project().chain(place).len(), 1);

    for _ in 0..11 {
        s.undo();
    }
    let strip = s.project().track(drums).unwrap().strip;
    assert!((strip.gain - 1.0).abs() < 1e-6 && !strip.mute && !strip.solo);
    assert!(s.project().chain(place).is_empty());
}

#[test]
fn pad_sounds_are_set_per_track() {
    let (mut s, _processor) = new_song();
    let drums = track(&s, true);
    let mut kick = s.pad(drums, 36);
    assert!(!kick.muted);
    kick.muted = true;
    kick.tune = 2.0;
    s.set_pad(drums, kick);
    assert_eq!(s.pad(drums, 36), kick);
    let Instrument::Drums { pads, .. } = &s.project().track(drums).unwrap().instrument else {
        panic!("a drum track");
    };
    assert_eq!(pads.len(), 1, "only changed pads are stored");
    s.undo();
    assert_eq!(s.pad(drums, 36), PadSetting::new(36));
}

#[test]
fn the_song_exports_to_a_wav_file() {
    let (mut s, _processor) = new_song();
    s.add_hit(36, Ticks::ZERO, 1.0);
    let path = temp_path("export");
    s.start_export(&path, ExportFormat::Cd);
    assert!(s.export_progress().is_some());
    s.finish_export();
    assert!(s.export_progress().is_none());
    let wanted = format!("{}.wav", path.display());
    assert_eq!(s.status(), Some(text::exported(&wanted).as_str()));
    let bytes = std::fs::read(&wanted).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
    assert_eq!((rate, bits), (44_100, 16), "CD quality");
    let data = u32::from_le_bytes(bytes[54..58].try_into().unwrap()) as usize;
    let seconds = data as f64 / 4.0 / 44_100.0;
    // Four bars at 120 BPM last eight seconds; the kick's tail is short.
    assert!(
        (8.0..10.0).contains(&seconds),
        "the song and its tail: {seconds} s"
    );
    let loudest = bytes[58..58 + data]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs())
        .max()
        .unwrap();
    assert!(loudest > 3000, "the kick is in the file: {loudest}");
    std::fs::remove_file(&wanted).ok();
}

/// A 16-bit stereo WAV file's left channel, as numbers from -1 to 1.
fn wav_left(bytes: &[u8]) -> Vec<f32> {
    let data = u32::from_le_bytes(bytes[54..58].try_into().unwrap()) as usize;
    bytes[58..58 + data]
        .chunks_exact(4)
        .map(|frame| f32::from(i16::from_le_bytes([frame[0], frame[1]])) / 32768.0)
        .collect()
}

#[test]
fn an_export_plays_the_song_once_and_ends_with_its_sound() {
    let (mut s, _processor) = new_song();
    // The one-bar beat, repeated over four bars, with a kick on its first
    // and last beats; and a loop on the first two bars, as someone working
    // on the start has.
    s.add_hit(36, Ticks::ZERO, 1.0);
    s.add_hit(36, Ticks::QUARTER * 3, 1.0);
    s.set_loop_region(Region::new(Ticks::ZERO, Ticks::WHOLE * 2));
    let path = temp_path("once");
    s.start_export(&path, ExportFormat::Cd);
    s.finish_export();
    let wanted = format!("{}.wav", path.display());
    let left = wav_left(&std::fs::read(&wanted).unwrap());
    let rate = 44_100.0;
    let at = |seconds: f64| (seconds * rate) as usize;
    let peak = |from: f64, to: f64| {
        left[at(from).min(left.len())..at(to).min(left.len())]
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()))
    };
    // The song plays straight through to its last beat's kick. It ends
    // with that kick's sound: the four bars (eight seconds), then only as
    // long as the kick still rings, with no silence after it. (Sent back by
    // the loop, it would never fall quiet and run on for fifteen seconds
    // more.)
    assert!(peak(0.0, 0.1) > 0.1, "the first kick");
    assert!(peak(7.5, 7.6) > 0.1, "the last beat's kick");
    let seconds = left.len() as f64 / rate;
    assert!((8.0..8.6).contains(&seconds), "{seconds} s");
    let end = &left[left.len().saturating_sub(at(0.05))..];
    assert!(
        seconds < 8.001 || end.iter().any(|s| s.abs() > 1e-4),
        "the file ends where the sound does"
    );
    std::fs::remove_file(&wanted).ok();
}

#[test]
fn a_cancelled_export_writes_nothing() {
    let (mut s, _processor) = new_song();
    let path = temp_path("cancelled.wav");
    s.start_export(&path, ExportFormat::Studio);
    s.cancel_export();
    s.finish_export();
    assert!(!path.exists());
    assert_eq!(s.status(), Some(text::EXPORT_CANCELLED));
}

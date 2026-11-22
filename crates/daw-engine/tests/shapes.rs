// SPDX-License-Identifier: GPL-3.0-or-later

//! Note shapes in songs (decision 0031): a track's shape reaches its
//! instrument when the song is played or exported, and a change of shape
//! is heard at once, without rebuilding a kit.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code"
)]

use std::sync::Arc;

use daw_dsp::audio::AudioBuffer;
use daw_engine::{
    Engine, EngineConfig, EngineProcessor, PlaybackScope, SoundLibrary, render_offline,
};
use daw_model::{
    AudioFile, AudioId, BUILTIN_PIANO, DEFAULT_VELOCITY, Instrument, Note, NoteId, NoteShape,
    Project, Ticks, TrackId,
};

static PIANO: &[u8] = include_bytes!("../../../assets/sounds/piano.sf2");

const RATE: u32 = 48_000;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        max_block: 512,
    }
}

fn frames(seconds: f64) -> usize {
    (seconds * f64::from(RATE)) as usize
}

fn peak(sound: &[f32], from: f64, to: f64) -> f32 {
    sound[frames(from).min(sound.len())..frames(to).min(sound.len())]
        .iter()
        .fold(0.0, |m, s| m.max(s.abs()))
}

/// A song at 120 BPM whose piano plays middle C for a beat (half a
/// second), with no loop, no limiter and no reverb on the piano.
fn piano_song(shape: NoteShape) -> Project {
    let mut project = Project::starter("P", "T");
    project.loop_region = None;
    project.master.effects.clear();
    let (track, pattern) = project.first_pattern().unwrap();
    project.add_notes(
        pattern,
        &[Note {
            id: NoteId(0),
            start: Ticks::ZERO,
            length: Ticks::QUARTER,
            key: 60,
            velocity: DEFAULT_VELOCITY,
            pitch_offset: 0.0,
        }],
    );
    project.track_mut(track).unwrap().shape = shape;
    project
}

fn piano_library() -> SoundLibrary {
    let mut library = SoundLibrary::new();
    library.register_builtin(BUILTIN_PIANO, PIANO);
    library.set_soundfont_reverb(0.0);
    library
}

#[test]
fn a_cut_piano_stops_at_the_end_of_its_note_in_the_song() {
    let render = |shape| render_offline(&piano_song(shape), piano_library(), config(), 1.5).0;
    let natural = render(NoteShape::NATURAL);
    let cut = render(NoteShape {
        attack: None,
        release: Some(NoteShape::CUT_RELEASE),
    });
    let long = render(NoteShape {
        attack: None,
        release: Some(2.0),
    });
    let held = peak(&natural, 0.4, 0.5);
    assert!(held > 0.005, "heard ({held})");
    assert!(
        (peak(&cut, 0.4, 0.5) - held).abs() < held * 0.05,
        "the same while held"
    );
    assert!(
        peak(&natural, 0.55, 0.65) > held * 0.05,
        "the natural piano rings on"
    );
    assert!(
        peak(&cut, 0.55, 0.65) < held * 0.001,
        "the cut one is silent"
    );
    assert!(
        peak(&long, 1.0, 1.2) > peak(&natural, 1.0, 1.2) * 5.0,
        "the long one rings longest"
    );
}

#[test]
fn a_cut_piano_leaves_its_own_reverb_out() {
    // The program's piano has its SoundFont's gentle room on; a cut must
    // still sound cut.
    let mut library = SoundLibrary::new();
    library.register_builtin(BUILTIN_PIANO, PIANO);
    let natural = render_offline(
        &piano_song(NoteShape::NATURAL),
        library.clone(),
        config(),
        1.5,
    )
    .0;
    let cut = render_offline(
        &piano_song(NoteShape {
            attack: None,
            release: Some(NoteShape::CUT_RELEASE),
        }),
        library,
        config(),
        1.5,
    )
    .0;
    let held = peak(&natural, 0.4, 0.5);
    assert!(peak(&natural, 0.6, 0.7) > held * 0.05, "the room rings on");
    assert!(
        peak(&cut, 0.6, 0.7) < held * 0.003,
        "the cut piano is silent"
    );
}

/// Half a second of a sine at 440 Hz.
fn tone() -> Arc<AudioBuffer> {
    let samples = (0..RATE / 2)
        .map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / RATE as f32).sin())
        .collect();
    Arc::new(AudioBuffer::new(RATE, vec![samples]))
}

/// A custom kit whose one pad (a half-second tone) is hit for a
/// sixteenth note (an eighth of a second) on beat 1.
fn kit_song() -> (Project, TrackId) {
    let mut project = Project::empty();
    let kit = project.add_track("Kit", Instrument::Kit { pads: Vec::new() });
    let tone = project.add_audio_file(AudioFile {
        id: AudioId(0),
        name: "Tone".to_owned(),
        file: "Tone.wav".to_owned(),
        frames: u64::from(RATE / 2),
        sample_rate: RATE,
        channels: 1,
    });
    project.add_kit_pad(kit, "Tone", tone);
    let pattern = project.add_pattern_for(kit, "Beat", Ticks::WHOLE);
    project.add_clip(kit, Ticks::ZERO, Ticks::WHOLE, pattern);
    project.add_notes(
        pattern,
        &[Note {
            id: NoteId(0),
            start: Ticks::ZERO,
            length: Ticks::note_value(1, 16),
            key: 36,
            velocity: 1.0,
            pitch_offset: 0.0,
        }],
    );
    (project, kit)
}

fn run(engine: &mut Engine, processor: &mut EngineProcessor, count: usize) -> Vec<f32> {
    let mut left = Vec::with_capacity(count);
    let mut buffer = vec![0.0_f32; 512];
    while left.len() < count {
        processor.process(&mut buffer, 2);
        left.extend(buffer.as_chunks::<2>().0.iter().map(|f| f[0]));
        engine.poll();
    }
    left
}

#[test]
fn a_kit_cut_to_its_notes_is_heard_at_once() {
    let (mut project, kit) = kit_song();
    let mut library = SoundLibrary::new();
    library.register_audio("Tone.wav", tone());
    let (mut engine, mut processor) = Engine::new(config(), library);
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.play();
    // The hit plays its whole half second.
    let natural = run(&mut engine, &mut processor, frames(1.0));
    assert!(peak(&natural, 0.3, 0.4) > 0.2, "the whole sound plays");
    engine.stop();
    // Cut to its note: it stops after an eighth of a second.
    project.track_mut(kit).unwrap().shape = NoteShape {
        attack: None,
        release: Some(NoteShape::CUT_RELEASE),
    };
    engine.sync(&project, 2, PlaybackScope::Song);
    engine.locate(Ticks::ZERO);
    engine.play();
    let cut = run(&mut engine, &mut processor, frames(1.0));
    let start = cut.iter().position(|s| s.abs() > 0.01).unwrap_or(0) as f64 / f64::from(RATE);
    assert!(
        peak(&cut, start + 0.02, start + 0.1) > 0.2,
        "heard while the note lasts"
    );
    assert!(
        peak(&cut, start + 0.16, start + 0.4) < 1e-3,
        "and cut off with it"
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Custom kits in the engine (decision 0025): each pad plays its own sound
//! once, on its key, with its settings; a sound that becomes ready after
//! the kit was built is played from then on.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::sync::Arc;

use daw_dsp::audio::AudioBuffer;
use daw_engine::{
    Engine, EngineConfig, EngineProcessor, PlaybackScope, SoundLibrary, render_offline,
};
use daw_model::{AudioFile, AudioId, Instrument, Note, NoteId, Project, Ticks, TrackId};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        max_block: 512,
    }
}

/// Half a second of a sine at `hz`.
fn tone(hz: f32) -> Arc<AudioBuffer> {
    let samples = (0..RATE / 2)
        .map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin())
        .collect();
    Arc::new(AudioBuffer::new(RATE, vec![samples]))
}

fn file(name: &str) -> AudioFile {
    AudioFile {
        id: AudioId(0),
        name: name.to_owned(),
        file: format!("{name}.wav"),
        frames: u64::from(RATE / 2),
        sample_rate: RATE,
        channels: 1,
    }
}

/// A song at 120 BPM with a custom kit of two pads (Low on 36, High on
/// 37), each hit once: Low on beat 1, High on beat 3.
fn song() -> (Project, TrackId) {
    let mut project = Project::empty();
    let kit = project.add_track("Kit", Instrument::Kit { pads: Vec::new() });
    let low = project.add_audio_file(file("Low"));
    let high = project.add_audio_file(file("High"));
    assert_eq!(project.add_kit_pad(kit, "Low", low), Some(36));
    assert_eq!(project.add_kit_pad(kit, "High", high), Some(37));
    let length = Ticks::WHOLE;
    let pattern = project.add_pattern_for(kit, "Beat", length);
    project.add_clip(kit, Ticks::ZERO, length, pattern);
    let hit = |start, key| Note {
        id: NoteId(0),
        start,
        length: Ticks::note_value(1, 16),
        key,
        velocity: 1.0,
        pitch_offset: 0.0,
    };
    project.add_notes(
        pattern,
        &[hit(Ticks::ZERO, 36), hit(Ticks::QUARTER * 2, 37)],
    );
    (project, kit)
}

fn library() -> SoundLibrary {
    let mut library = SoundLibrary::new();
    library.register_audio("Low.wav", tone(440.0));
    library.register_audio("High.wav", tone(880.0));
    library
}

fn frequency(samples: &[f32]) -> f32 {
    let mut crossings = Vec::new();
    for i in 1..samples.len() {
        let (a, b) = (samples[i - 1], samples[i]);
        if a < 0.0 && b >= 0.0 {
            crossings.push(i as f32 - 1.0 + a / (a - b));
        }
    }
    let span = crossings.last().unwrap() - crossings.first().unwrap();
    (crossings.len() - 1) as f32 * RATE as f32 / span
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// Frames at a beat (120 BPM: half a second each).
fn beat(n: f32) -> usize {
    (n * 0.5 * RATE as f32) as usize
}

#[test]
fn each_pad_plays_its_own_sound_on_its_key() {
    let (project, _) = song();
    let (left, right) = render_offline(&project, library(), config(), 2.0);
    assert_eq!(left, right, "mono sounds in the middle");
    let low = frequency(&left[beat(0.05)..beat(0.9)]);
    assert!((low - 440.0).abs() < 1.0, "the first pad: {low} Hz");
    assert!(
        rms(&left[beat(1.05)..beat(1.95)]) < 1e-4,
        "silence in between"
    );
    let high = frequency(&left[beat(2.05)..beat(2.9)]);
    assert!((high - 880.0).abs() < 2.0, "the second pad: {high} Hz");
}

#[test]
fn pad_settings_change_how_each_sound_plays() {
    let (mut project, kit) = song();
    let instrument = &mut project.track_mut(kit).unwrap().instrument;
    let mut low = instrument.pad(36);
    low.length = 0.2;
    low.tune = 12.0;
    instrument.set_pad(low);
    let mut high = instrument.pad(37);
    high.muted = true;
    instrument.set_pad(high);
    let (left, _) = render_offline(&project, library(), config(), 2.0);
    let low_hz = frequency(&left[beat(0.02)..beat(0.15)]);
    assert!((low_hz - 880.0).abs() < 3.0, "an octave up: {low_hz} Hz");
    assert!(
        rms(&left[beat(0.25)..beat(0.9)]) < 1e-4,
        "a fifth of its length, then silent"
    );
    assert!(rms(&left[beat(2.0)..beat(3.0)]) < 1e-6, "the muted pad");
}

fn run(engine: &mut Engine, processor: &mut EngineProcessor, frames: usize) -> Vec<f32> {
    let mut buffer = vec![0.0_f32; 1024];
    let mut left = Vec::new();
    while left.len() < frames {
        processor.process(&mut buffer, 2);
        left.extend(buffer.as_chunks::<2>().0.iter().map(|f| f[0]));
        engine.poll();
    }
    left
}

#[test]
fn a_sound_ready_later_is_played_from_then_on() {
    let (project, _) = song();
    // The sounds are still being read when the song starts.
    let (mut engine, mut processor) = Engine::new(config(), SoundLibrary::new());
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.play();
    let before = run(&mut engine, &mut processor, beat(1.0));
    assert!(rms(&before) < 1e-6, "nothing to play yet");
    engine.register_audio("Low.wav", tone(440.0));
    engine.register_audio("High.wav", tone(880.0));
    engine.sync(&project, 1, PlaybackScope::Song);
    let after = run(&mut engine, &mut processor, beat(2.0));
    let high = frequency(&after[beat(1.05)..beat(1.9)]);
    assert!(
        (high - 880.0).abs() < 2.0,
        "the kit plays once ready: {high} Hz"
    );
}

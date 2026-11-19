// SPDX-License-Identifier: GPL-3.0-or-later

//! SFZ instruments in the engine (decision 0024): read by the reader the
//! program hands over; in the background for the live engine (the track is
//! silent until its instrument is ready, then plays); waited for by
//! exports. A missing or broken file plays a plain sound and says why, and
//! what an instrument plays without is told once.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use daw_dsp::sampler::{LeftOut, Region, Sample, SamplerBank};
use daw_engine::{
    Engine, EngineConfig, EngineProcessor, PlaybackScope, Problem, SoundLibrary, render_offline,
};
use daw_model::{Instrument, Note, NoteId, Project, Ticks};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        max_block: 512,
    }
}

/// A stand-in for the file formats' reader: a looped A4 sine for every
/// key, slow to read when its file says so, with parts left out.
fn reader(path: &Path) -> Result<SamplerBank, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if text.contains("slow") {
        std::thread::sleep(Duration::from_millis(300));
    }
    if text.contains("broken") {
        return Err("it is damaged".to_owned());
    }
    assert!(!text.contains("panic"), "a reader that fails badly");
    let left = (0..48_000)
        .map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin())
        .collect();
    Ok(SamplerBank {
        regions: vec![Region {
            sample: Some(0),
            pitch_keycenter: 69,
            amp_veltrack: 0.0,
            ..Region::default()
        }],
        samples: vec![Sample {
            sample_rate: 48_000,
            left,
            right: Vec::new(),
            file_loop: Some((0, 47_999)),
        }],
        left_out: vec![LeftOut::Lfos],
        missing: vec!["Gone.wav".to_owned()],
        ..SamplerBank::default()
    })
}

/// An instrument file of this test's own holding `text`.
fn instrument_file(name: &str, text: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("engine-sfz-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// A song at 120 BPM with one SFZ track playing A4 for a beat, then A5.
fn song(file: &Path) -> Project {
    let mut project = Project::empty();
    let track = project.add_track(
        "Strings",
        Instrument::Sfz {
            file: file.to_string_lossy().into_owned(),
        },
    );
    let length = Ticks::WHOLE * 4;
    let pattern = project.add_pattern_for(track, "P", length);
    project.add_clip(track, Ticks::ZERO, length, pattern);
    let note = |start, key| Note {
        id: NoteId(0),
        start,
        length: Ticks::QUARTER,
        key,
        velocity: 1.0,
        pitch_offset: 0.0,
    };
    project.add_notes(
        pattern,
        &[note(Ticks::ZERO, 69), note(Ticks::QUARTER * 2, 81)],
    );
    project
}

fn library() -> SoundLibrary {
    let mut library = SoundLibrary::new();
    library.set_sfz_reader(reader);
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
fn an_export_waits_for_the_instrument_and_plays_its_notes() {
    let file = instrument_file("Export.sfz", "slow");
    let mut library = library();
    // The live engine reads in the background; an export waits all the same.
    library.set_background_loading(true);
    let (left, _) = render_offline(&song(&file), library, config(), 2.0);
    let first = frequency(&left[2400..21_600]);
    assert!((first - 440.0).abs() < 1.0, "A4: {first} Hz");
    let second = frequency(&left[50_400..69_600]);
    assert!((second - 880.0).abs() < 2.0, "A5: {second} Hz");
}

#[test]
fn the_live_engine_reads_in_the_background_and_plays_once_ready() {
    let file = instrument_file("Live.sfz", "slow");
    let project = song(&file);
    let mut library = library();
    library.set_background_loading(true);
    let (mut engine, mut processor) = Engine::new(config(), library);
    assert_eq!(engine.loading_sounds(), Vec::<String>::new());
    let started = Instant::now();
    engine.sync(&project, 1, PlaybackScope::Song);
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "the interface did not wait for the instrument"
    );
    assert_eq!(engine.loading_sounds(), [file.to_string_lossy()]);
    engine.play();
    let early = run(&mut engine, &mut processor, 4800);
    assert!(rms(&early) < 1e-6, "silent while it is read");

    while !engine.loading_sounds().is_empty() {
        assert!(started.elapsed() < Duration::from_secs(10), "never read");
        std::thread::sleep(Duration::from_millis(10));
        engine.sync(&project, 1, PlaybackScope::Song);
    }
    let problems = engine.take_problems();
    assert_eq!(
        problems,
        [Problem::PartlyRead {
            sound: file.to_string_lossy().into_owned(),
            left_out: vec![LeftOut::Lfos],
            missing: vec!["Gone.wav".to_owned()],
        }]
    );
    engine.stop();
    engine.locate(Ticks::ZERO);
    engine.sync(&project, 2, PlaybackScope::Song);
    engine.play();
    let ready = run(&mut engine, &mut processor, 21_600);
    let hz = frequency(&ready[2400..]);
    assert!((hz - 440.0).abs() < 1.0, "it plays once ready: {hz} Hz");
    assert_eq!(
        engine.take_problems(),
        [],
        "what it leaves out is told once"
    );
}

#[test]
fn a_missing_or_broken_instrument_plays_a_plain_sound_and_says_why() {
    let gone = std::env::temp_dir()
        .join(format!("engine-sfz-gone-{}", std::process::id()))
        .join("Gone.sfz");
    let (mut engine, mut processor) = Engine::new(config(), library());
    engine.sync(&song(&gone), 1, PlaybackScope::Song);
    assert_eq!(
        engine.take_problems(),
        [Problem::MissingFile(gone.to_string_lossy().into_owned())]
    );
    engine.play();
    let plain = run(&mut engine, &mut processor, 9600);
    assert!(rms(&plain[2400..]) > 0.01, "a plain sound plays instead");

    // A reader that panics does not leave the track waiting forever.
    let panicking = instrument_file("Panicking.sfz", "panic");
    let mut reading = library();
    reading.set_background_loading(true);
    let (mut engine, _processor) = Engine::new(config(), reading);
    let project = song(&panicking);
    engine.sync(&project, 1, PlaybackScope::Song);
    let started = Instant::now();
    while !engine.loading_sounds().is_empty() {
        assert!(started.elapsed() < Duration::from_secs(10), "waits forever");
        std::thread::sleep(Duration::from_millis(10));
        engine.sync(&project, 1, PlaybackScope::Song);
    }
    assert!(matches!(
        engine.take_problems().as_slice(),
        [Problem::UnreadableSound { .. }]
    ));

    let broken = instrument_file("Broken.sfz", "broken");
    let (mut engine, _processor) = Engine::new(config(), library());
    engine.sync(&song(&broken), 1, PlaybackScope::Song);
    assert_eq!(
        engine.take_problems(),
        [Problem::UnreadableSound {
            sound: broken.to_string_lossy().into_owned(),
            reason: "it is damaged".to_owned(),
        }]
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 4 in the engine, measured (decision 0017): audio clips play at
//! their place and offset, with their volume and fades, cuts do not click
//! and splits do not show, sound at another rate keeps its speed, audio
//! tracks go through the mixer; recording passes the input on, take by
//! take; nothing allocates on the audio thread.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::sync::Arc;

use daw_dsp::audio::AudioBuffer;
use daw_engine::{
    Engine, EngineConfig, EngineEvent, EngineProcessor, PlaybackScope, SoundLibrary, render_offline,
};
use daw_model::{AudioFile, AudioId, ClipContent, ClipId, Project, Ticks, TrackId};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;
/// At 120 beats per minute a bar lasts two seconds.
const BAR_FRAMES: usize = 2 * RATE as usize;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        max_block: 512,
    }
}

/// A song with one audio track playing `sound` from `offset` (frames of
/// the sound) at `start`, for `seconds`; the library knows the sound.
fn song(
    sound: AudioBuffer,
    start: Ticks,
    seconds: f64,
    offset: u64,
) -> (Project, TrackId, ClipId, SoundLibrary) {
    let mut project = Project::empty();
    let track = project.add_audio_track("Audio");
    let audio = project.add_audio_file(AudioFile {
        id: AudioId(0),
        name: "Sound".to_owned(),
        file: "sound.wav".to_owned(),
        frames: sound.frames() as u64,
        sample_rate: sound.sample_rate(),
        channels: sound.channel_count() as u16,
    });
    let length = project.ticks_for(start, seconds);
    let clip = project
        .add_audio_clip(track, start, length, audio, offset)
        .unwrap();
    let mut library = SoundLibrary::new();
    library.register_audio("sound.wav", Arc::new(sound));
    (project, track, clip, library)
}

fn steady(left: f32, right: f32, seconds: f64, rate: u32) -> AudioBuffer {
    let frames = (seconds * f64::from(rate)) as usize;
    AudioBuffer::new(rate, vec![vec![left; frames], vec![right; frames]])
}

fn sine(hz: f64, seconds: f64, rate: u32) -> AudioBuffer {
    let frames = (seconds * f64::from(rate)) as usize;
    let samples = (0..frames)
        .map(|i| (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(rate)).sin() as f32 * 0.5)
        .collect();
    AudioBuffer::new(rate, vec![samples])
}

fn set_content(
    project: &mut Project,
    clip: ClipId,
    change: impl FnOnce(&mut f32, &mut f32, &mut f32),
) {
    project.update_clip(clip, |c| {
        if let ClipContent::Audio {
            gain,
            fade_in,
            fade_out,
            ..
        } = &mut c.content
        {
            change(gain, fade_in, fade_out);
        }
    });
}

#[test]
fn an_audio_clip_plays_at_its_place_from_its_offset() {
    // A ramp from 0 to 1 over a second, played from a quarter of the way
    // in, for half a second, from bar 2.
    let ramp: Vec<f32> = (0..48_000).map(|i| i as f32 / 48_000.0).collect();
    let sound = AudioBuffer::new(RATE, vec![ramp.clone(), ramp.iter().map(|s| -s).collect()]);
    let (project, _, _, library) = song(sound, Ticks::WHOLE, 0.5, 12_000);
    let (left, right) = render_offline(&project, library, config(), 3.0);
    let start = BAR_FRAMES;
    let end = start + 24_000;
    assert!(left[..start].iter().all(|&s| s == 0.0), "silent before");
    assert!(left[end..].iter().all(|&s| s == 0.0), "silent after");
    // Past the short fade at the edge, the sound is the ramp from frame
    // 12,000 on.
    for k in [200, 1000, 10_000, 23_000] {
        let expected = (12_000 + k) as f32 / 48_000.0;
        assert!((left[start + k] - expected).abs() < 1e-5, "frame {k}");
        assert!(
            (right[start + k] + expected).abs() < 1e-5,
            "frame {k}, right"
        );
    }
    // The edges fade in and out within a few milliseconds.
    assert!(left[start] > 0.0 && left[start] < 0.01, "{}", left[start]);
    assert!(left[end - 1] < 0.02, "{}", left[end - 1]);
}

#[test]
fn volume_and_fades_shape_the_clip() {
    let (mut project, _, clip, library) = song(steady(0.8, 0.8, 2.0, RATE), Ticks::WHOLE, 2.0, 0);
    set_content(&mut project, clip, |gain, fade_in, fade_out| {
        *gain = 0.5;
        *fade_in = 0.5;
        *fade_out = 0.25;
    });
    let (left, _) = render_offline(&project, library, config(), 4.5);
    let at = |seconds: f64| left[BAR_FRAMES + (seconds * f64::from(RATE)) as usize];
    let quarter_in = 0.8 * 0.5 * std::f32::consts::FRAC_PI_4.sin();
    assert!((at(0.25) - quarter_in).abs() < 1e-3, "{}", at(0.25));
    assert!((at(1.0) - 0.4).abs() < 1e-5, "full volume after the fade");
    assert!(at(1.95) < at(1.8) && at(1.8) < 0.4, "fading out");
    assert!(at(1.9999) < 0.01);
}

#[test]
fn a_split_clip_sounds_exactly_like_the_whole() {
    let (whole, _, _, library) = song(sine(440.0, 2.0, RATE), Ticks::WHOLE, 2.0, 0);
    let mut split = whole.clone();
    let clip = split.tracks[0].clips[0].id;
    split
        .split_clip(clip, Ticks::WHOLE + Ticks::note_value(3, 7))
        .unwrap();
    let (a, _) = render_offline(&whole, library.clone(), config(), 4.5);
    let (b, _) = render_offline(&split, library, config(), 4.5);
    let worst = a
        .iter()
        .zip(&b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max);
    assert!(worst < 1e-6, "the split shows: {worst}");
}

#[test]
fn sound_at_another_rate_keeps_its_speed() {
    let (project, _, _, library) = song(sine(1000.0, 1.0, 24_000), Ticks::ZERO, 1.0, 0);
    let (left, _) = render_offline(&project, library, config(), 1.0);
    let middle = &left[12_000..36_000];
    let crossings = middle
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    assert!(
        (499..=501).contains(&crossings),
        "{crossings} cycles in half a second"
    );
    assert!(middle.iter().fold(0.0_f32, |m, s| m.max(s.abs())) > 0.45);
}

#[test]
fn audio_tracks_go_through_the_mixer() {
    let (mut project, track, _, library) = song(steady(0.5, 0.5, 1.0, RATE), Ticks::ZERO, 1.0, 0);
    project.track_mut(track).unwrap().strip.gain = 0.5;
    project.track_mut(track).unwrap().strip.pan = 1.0;
    let (left, right) = render_offline(&project, library.clone(), config(), 1.0);
    assert!(left[24_000].abs() < 1e-6, "panned right");
    assert!((right[24_000] - 0.25).abs() < 1e-5, "half volume");
    project.track_mut(track).unwrap().strip.mute = true;
    let (left, right) = render_offline(&project, library, config(), 1.0);
    assert!(left.iter().chain(&right).all(|&s| s == 0.0), "muted");
}

/// Runs the processor for `frames` and collects what the engine reports.
fn run(engine: &mut Engine, processor: &mut EngineProcessor, frames: usize) -> Vec<EngineEvent> {
    let mut buffer = vec![0.0_f32; 512];
    let mut events = Vec::new();
    for _ in 0..frames.div_ceil(256) {
        processor.process(&mut buffer, 2);
        events.extend(engine.poll());
    }
    events
}

#[test]
fn recording_passes_the_input_on_take_by_take_without_allocating() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let mut project = Project::empty();
    project.add_audio_track("Audio");
    let (mut engine, mut processor) = Engine::new(config(), SoundLibrary::new());
    engine.sync(&project, 1, PlaybackScope::Song);
    let mut port = engine.take_input_port().unwrap();
    let mut recorded = engine.take_recorded_audio().unwrap();
    assert!(engine.take_input_port().is_none(), "taken once");
    engine.set_record_latency(0.012);

    // Sound before recording is let go.
    port.push(&[0.9; 500], 1, 44_100);
    run(&mut engine, &mut processor, 256);
    assert_eq!(engine.input_frames(), 500);
    assert!((engine.take_input_peak() - 0.9).abs() < 1e-6);

    engine.record_audio(0);
    let violations = daw_rt::violation_count();
    let mut sent = Vec::new();
    let mut events = Vec::new();
    for block in 0..40 {
        let samples: Vec<f32> = (0..300)
            .map(|i| (block * 300 + i) as f32 / 100_000.0)
            .collect();
        port.push(&samples, 1, 44_100);
        sent.extend_from_slice(&samples);
        events.extend(run(&mut engine, &mut processor, 256));
    }
    engine.stop();
    events.extend(run(&mut engine, &mut processor, 256));
    assert_eq!(
        daw_rt::violation_count(),
        violations,
        "allocated on the audio thread"
    );

    assert!(events.contains(&EngineEvent::AudioRecordingStarted {
        take: 1,
        at: Ticks::ZERO,
        sample_rate: 44_100,
        channels: 1,
        latency: 0.012,
    }));
    assert!(events.contains(&EngineEvent::AudioRecordingStopped {
        take: 1,
        frames: sent.len() as u64,
        dropped: 0,
    }));
    let mut frames = Vec::new();
    while let Some(chunk) = recorded.pop() {
        assert_eq!(chunk.take, 1);
        assert_eq!((chunk.sample_rate, chunk.channels), (44_100, 1));
        assert_eq!(chunk.first, frames.len() as u64 / 2, "no gaps");
        frames.extend_from_slice(chunk.samples());
    }
    let left: Vec<f32> = frames.iter().step_by(2).copied().collect();
    let right: Vec<f32> = frames.iter().skip(1).step_by(2).copied().collect();
    assert_eq!(left, sent, "every sample, in order");
    assert_eq!(right, sent, "a mono input on both sides");

    // The next take has the next number.
    engine.record_audio(0);
    port.push(&[0.1, 0.2, 0.3, 0.4], 2, 48_000);
    run(&mut engine, &mut processor, 256);
    engine.stop();
    let events = run(&mut engine, &mut processor, 256);
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::AudioRecordingStopped {
            take: 2,
            frames: 2,
            ..
        }
    )));
    let chunk = recorded.pop().unwrap();
    assert_eq!(
        (chunk.take, chunk.samples()),
        (2, &[0.1, 0.2, 0.3, 0.4][..])
    );
}

#[test]
fn playing_audio_clips_allocates_nothing() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let (project, _, _, library) = song(sine(440.0, 2.0, 44_100), Ticks::ZERO, 2.0, 0);
    let (mut engine, mut processor) = Engine::new(config(), library);
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.play();
    run(&mut engine, &mut processor, 1024);
    let violations = daw_rt::violation_count();
    run(&mut engine, &mut processor, 48_000);
    assert_eq!(daw_rt::violation_count(), violations);
}

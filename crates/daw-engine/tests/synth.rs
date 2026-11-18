// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in synth in the engine (decision 0019): a synth track plays
//! its notes at their pitch, takes new settings while it plays without
//! allocating, and its settings follow automation lanes.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use daw_engine::{
    Engine, EngineConfig, EngineProcessor, PlaybackScope, SoundLibrary, render_offline,
};
use daw_model::{
    AutomationPoint, AutomationTarget, Instrument, Note, NoteId, Project, SynthPreset,
    SynthSettings, SynthWave, Ticks, TrackId,
};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;

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
        velocity: 1.0,
        pitch_offset: 0.0,
    }
}

/// A song at 120 BPM with one synth track playing `notes` in a four-bar
/// pattern, no loop and no master limiter.
fn synth_song(settings: SynthSettings, notes: &[Note]) -> (Project, TrackId) {
    let mut project = Project::empty();
    let track = project.add_track("Synth", Instrument::Synth(settings));
    let length = Ticks::WHOLE * 4;
    let pattern = project.add_pattern_for(track, "P", length);
    project.add_clip(track, Ticks::ZERO, length, pattern);
    project.add_notes(pattern, notes);
    (project, track)
}

fn sine_settings() -> SynthSettings {
    let mut settings = SynthPreset::Plain.settings();
    settings.osc1.wave = SynthWave::Sine;
    settings
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

/// The level of what is left after taking away everything below about
/// 2 kHz (a crude high-pass: the difference of neighbouring samples).
fn brightness(samples: &[f32]) -> f32 {
    let highs: Vec<f32> = samples.windows(2).map(|w| w[1] - w[0]).collect();
    rms(&highs) / rms(samples).max(1e-9)
}

#[test]
fn a_synth_track_plays_its_notes_at_their_pitch() {
    let (project, _) = synth_song(
        sine_settings(),
        &[
            note(Ticks::ZERO, Ticks::QUARTER, 69),
            note(Ticks::QUARTER * 2, Ticks::QUARTER, 81),
        ],
    );
    let (left, right) = render_offline(&project, SoundLibrary::new(), config(), 2.0);
    assert_eq!(left, right, "the synth sits in the middle");
    let first = frequency(&left[2400..21_600]);
    assert!((first - 440.0).abs() < 1.0, "A4: {first} Hz");
    let second = frequency(&left[50_400..69_600]);
    assert!((second - 880.0).abs() < 2.0, "A5 a beat later: {second} Hz");
    assert!(rms(&left[30_000..46_000]) < 1e-4, "silence in between");
    let peak = left.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05 && peak < 1.0, "heard, not clipping: {peak}");
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
fn new_settings_reach_the_playing_synth_without_allocating() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let (mut project, track) =
        synth_song(sine_settings(), &[note(Ticks::ZERO, Ticks::WHOLE * 4, 57)]);
    let (mut engine, mut processor) = Engine::new(config(), SoundLibrary::new());
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.play();
    let before = run(&mut engine, &mut processor, 9600);
    assert!((frequency(&before[2400..]) - 220.0).abs() < 1.0);
    let quiet = brightness(&before[2400..]);

    // A saw instead of a sine: the held note carries on, brighter.
    let mut saw = sine_settings();
    saw.osc1.wave = SynthWave::Saw;
    if let Some(t) = project.track_mut(track) {
        t.instrument = Instrument::Synth(saw);
    }
    engine.sync(&project, 2, PlaybackScope::Song);
    let violations = daw_rt::violation_count();
    let after = run(&mut engine, &mut processor, 9600);
    assert_eq!(
        daw_rt::violation_count(),
        violations,
        "allocated on the audio thread"
    );
    assert!(rms(&after[4800..]) > 0.05, "the held note goes on");
    assert!(
        brightness(&after[4800..]) > 3.0 * quiet,
        "with the new sound: {} against {quiet}",
        brightness(&after[4800..])
    );
}

#[test]
fn an_automation_lane_opens_the_synths_filter_over_the_song() {
    let mut settings = SynthPreset::Plain.settings();
    settings.cutoff_hz = 100.0;
    let (mut project, track) = synth_song(settings, &[note(Ticks::ZERO, Ticks::WHOLE * 4, 45)]);
    let cutoff = AutomationTarget::Instrument { param: 0 };
    project.add_lane(track, cutoff);
    project.update_lane(track, cutoff, |lane| {
        lane.insert(AutomationPoint {
            at: Ticks::ZERO,
            value: 0.1,
        });
        lane.insert(AutomationPoint {
            at: Ticks::WHOLE * 2,
            value: 0.9,
        });
    });
    // Four seconds: two bars of opening filter, then two held open.
    let (left, _) = render_offline(&project, SoundLibrary::new(), config(), 6.0);
    let early = brightness(&left[4800..14_400]);
    let middle = brightness(&left[90_000..99_600]);
    let late = brightness(&left[200_000..209_600]);
    // Below the note's fundamental at first (only the fundamental's own
    // slope shows), a few harmonics half way, wide open at the end.
    assert!(
        middle > 1.5 * early,
        "the filter opens: {early} then {middle}"
    );
    assert!(
        late > 2.0 * middle,
        "and goes on opening: {middle} then {late}"
    );
}

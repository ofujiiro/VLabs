// SPDX-License-Identifier: GPL-3.0-or-later

//! A MIDI controller's wheels and pedals on the included instruments
//! (decision 0033), measured: the pitch wheel bends two semitones each way
//! (an SFZ region its own range), the modulation wheel and pressure add
//! vibrato, the expression pedal shapes loudness, drum kits stay put, and
//! none of it allocates (it all runs on the audio thread).

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::sync::Arc;

use daw_dsp::sampler::{LoopMode, Region, Sample, Sampler, SamplerBank};
use daw_dsp::sf2_writer::sine_test_font;
use daw_dsp::synth::{Synth, SynthParams, Wave};
use daw_dsp::{
    BlockEvent, BlockEventKind, Control, EXPRESSION, Instrument, MODULATION, SineSynth,
    SoundFontBank, SoundFontPlayer,
};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;
const A4: f32 = 440.0;

/// Renders `seconds` in uneven blocks (as the engine splits them around
/// notes) and returns the left channel.
fn render(instrument: &mut dyn Instrument, seconds: f32) -> Vec<f32> {
    let frames = (seconds * RATE as f32) as usize;
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    let mut done = 0;
    for size in [64, 333, 1024, 17].iter().cycle() {
        if done >= frames {
            break;
        }
        let end = (done + size).min(frames);
        instrument.render(&mut left[done..end], &mut right[done..end]);
        done = end;
    }
    left
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
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
    let span = crossings.last().unwrap() - crossings.first().unwrap();
    (crossings.len() - 1) as f32 * RATE as f32 / span
}

/// The interval between two frequencies, in semitones.
fn semitones(from: f32, to: f32) -> f32 {
    12.0 * (to / from).log2()
}

/// The lowest and highest pitch over sixteenths of a second, in cents
/// around their middle.
fn vibrato_cents(sound: &[f32]) -> f32 {
    let piece = RATE as usize / 16;
    let pitches: Vec<f32> = sound.chunks_exact(piece).map(frequency).collect::<Vec<_>>();
    let (low, high) = pitches
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), &p| (l.min(p), h.max(p)));
    1200.0 * (high / low).log2() / 2.0
}

fn sine_synth() -> Synth {
    let mut params = SynthParams::default();
    params.osc[0].wave = Wave::Sine;
    params.lfo_rate_hz = 4.0;
    Synth::new(RATE, params)
}

fn modulation(value: f32) -> Control {
    Control::Controller {
        number: MODULATION,
        value,
    }
}

fn expression(value: f32) -> Control {
    Control::Controller {
        number: EXPRESSION,
        value,
    }
}

/// Checks the wheel on any instrument playing A4 as a pure tone: up two
/// semitones, down two, back at rest, and a note started while the wheel is
/// held starts bent.
fn bends_two_semitones(make: &dyn Fn() -> Box<dyn Instrument>, tolerance: f32) {
    let mut instrument = make();
    instrument.note_on(69, 0.8);
    render(instrument.as_mut(), 0.1);
    let still = frequency(&render(instrument.as_mut(), 0.25));
    assert!((still - A4).abs() < 2.0, "A4 at rest: {still} Hz");

    instrument.control(Control::PitchBend(1.0));
    render(instrument.as_mut(), 0.05);
    let up = semitones(A4, frequency(&render(instrument.as_mut(), 0.25)));
    assert!(
        (up - 2.0).abs() < tolerance,
        "all the way up: {up} semitones"
    );

    instrument.control(Control::PitchBend(-1.0));
    render(instrument.as_mut(), 0.05);
    let down = semitones(A4, frequency(&render(instrument.as_mut(), 0.25)));
    assert!(
        (down + 2.0).abs() < tolerance,
        "all the way down: {down} semitones"
    );

    instrument.control(Control::PitchBend(0.5));
    render(instrument.as_mut(), 0.05);
    let half = semitones(A4, frequency(&render(instrument.as_mut(), 0.25)));
    assert!(
        (half - 1.0).abs() < tolerance,
        "half way up: {half} semitones"
    );

    instrument.control(Control::PitchBend(0.0));
    render(instrument.as_mut(), 0.05);
    let back = frequency(&render(instrument.as_mut(), 0.25));
    assert!((back - A4).abs() < 2.0, "back at rest: {back} Hz");

    // A note started with the wheel held starts bent.
    let mut instrument = make();
    instrument.control(Control::PitchBend(1.0));
    instrument.note_on(69, 0.8);
    render(instrument.as_mut(), 0.1);
    let bent = semitones(A4, frequency(&render(instrument.as_mut(), 0.25)));
    assert!(
        (bent - 2.0).abs() < tolerance,
        "a note started while bent: {bent} semitones"
    );
}

#[test]
fn the_synth_bends_two_semitones_each_way() {
    bends_two_semitones(&|| Box::new(sine_synth()), 0.02);
}

#[test]
fn the_modulation_wheel_and_pressure_add_vibrato_to_the_synth() {
    let still = {
        let mut synth = sine_synth();
        synth.note_on(69, 0.8);
        render(&mut synth, 0.1);
        vibrato_cents(&render(&mut synth, 1.0))
    };
    assert!(still < 2.0, "no vibrato at rest: ±{still} cents");

    for control in [modulation(1.0), Control::Pressure(1.0)] {
        let mut synth = sine_synth();
        synth.note_on(69, 0.8);
        synth.control(control);
        render(&mut synth, 0.1);
        let depth = vibrato_cents(&render(&mut synth, 1.0));
        assert!(
            (35.0..=55.0).contains(&depth),
            "{control:?} gives about ±50 cents: ±{depth}"
        );
    }

    // Half way, half the depth.
    let mut synth = sine_synth();
    synth.note_on(69, 0.8);
    synth.control(modulation(0.5));
    render(&mut synth, 0.1);
    let depth = vibrato_cents(&render(&mut synth, 1.0));
    assert!(
        (15.0..=30.0).contains(&depth),
        "half the wheel, about ±25 cents: ±{depth}"
    );
}

#[test]
fn the_expression_pedal_shapes_the_synths_loudness() {
    let level = |pedal: f32| {
        let mut synth = sine_synth();
        synth.note_on(69, 0.8);
        synth.control(expression(pedal));
        render(&mut synth, 0.1);
        rms(&render(&mut synth, 0.2))
    };
    let full = level(1.0);
    assert!(full > 0.05);
    let half = level(0.5) / full;
    assert!(
        (half - 0.25).abs() < 0.01,
        "half way is about -12 dB: {half}"
    );
    assert!(level(0.0) < 1e-4, "heel down is silent");
}

#[test]
fn soundfonts_bend_and_follow_the_expression_pedal() {
    let bank = SoundFontBank::from_bytes(&sine_test_font(RATE).to_bytes()).unwrap();
    let make = || -> Box<dyn Instrument> {
        let mut player = SoundFontPlayer::new(&bank, RATE, 0, 0).unwrap();
        player.set_reverb(0.0);
        Box::new(player)
    };
    bends_two_semitones(&make, 0.03);

    let level = |pedal: f32| {
        let mut player = make();
        player.note_on(69, 0.8);
        player.control(expression(pedal));
        render(player.as_mut(), 0.2);
        rms(&render(player.as_mut(), 0.2))
    };
    let half = level(0.5) / level(1.0);
    assert!(half < 0.5, "the pedal half way turns it down: {half}");
}

/// A sampler region playing a looped 440 Hz sine on every key.
fn sine_region() -> (Region, Sample) {
    let frames = RATE as usize;
    let left = (0..frames)
        .map(|i| 0.5 * (std::f32::consts::TAU * A4 * i as f32 / RATE as f32).sin())
        .collect();
    let sample = Sample {
        sample_rate: RATE,
        left,
        right: Vec::new(),
        file_loop: Some((0, frames as u64 - 1)),
    };
    let region = Region {
        sample: Some(0),
        pitch_keycenter: 69,
        loop_mode: Some(LoopMode::Continuous),
        ..Region::default()
    };
    (region, sample)
}

fn sampler_with(region: Region, sample: Sample) -> Sampler {
    Sampler::new(
        Arc::new(SamplerBank {
            regions: vec![region],
            samples: vec![sample],
            ..SamplerBank::default()
        }),
        RATE,
    )
}

#[test]
fn sfz_instruments_bend_by_their_own_range() {
    // SFZ's default range is two semitones each way.
    bends_two_semitones(
        &|| {
            let (region, sample) = sine_region();
            Box::new(sampler_with(region, sample))
        },
        0.03,
    );

    // An instrument that bends an octave up and a fifth down.
    let (mut region, sample) = sine_region();
    region.bend_up = 1200.0;
    region.bend_down = -700.0;
    let mut sampler = sampler_with(region, sample);
    sampler.note_on(69, 0.8);
    sampler.control(Control::PitchBend(1.0));
    render(&mut sampler, 0.05);
    let up = semitones(A4, frequency(&render(&mut sampler, 0.25)));
    assert!((up - 12.0).abs() < 0.05, "an octave up: {up}");
    sampler.control(Control::PitchBend(-1.0));
    render(&mut sampler, 0.05);
    let down = semitones(A4, frequency(&render(&mut sampler, 0.25)));
    assert!((down + 7.0).abs() < 0.05, "a fifth down: {down}");

    // The expression pedal shapes its loudness too.
    let level = |pedal: f32| {
        let (region, sample) = sine_region();
        let mut sampler = sampler_with(region, sample);
        sampler.note_on(69, 1.0);
        sampler.control(expression(pedal));
        render(&mut sampler, 0.1);
        rms(&render(&mut sampler, 0.2))
    };
    let half = level(0.5) / level(1.0);
    assert!((half - 0.25).abs() < 0.01, "half way, -12 dB: {half}");
}

#[test]
fn a_kit_made_of_sounds_is_left_alone_by_the_wheel_and_pedal() {
    let (region, sample) = sine_region();
    let mut kit = sampler_with(region, sample);
    kit.set_drum_kit(true);
    kit.note_on(69, 0.8);
    render(&mut kit, 0.05);
    let level = rms(&render(&mut kit, 0.1));
    kit.control(Control::PitchBend(1.0));
    kit.control(expression(0.0));
    render(&mut kit, 0.05);
    let after = render(&mut kit, 0.25);
    assert!((frequency(&after) - A4).abs() < 2.0, "drums are not bent");
    assert!((rms(&after) / level - 1.0).abs() < 0.02, "nor turned down");
}

#[test]
fn the_plain_sound_bends_too() {
    bends_two_semitones(&|| Box::new(SineSynth::new(RATE)), 0.02);
}

#[test]
fn controllers_at_their_frame_in_a_block() {
    // The wheel moves half way through a block: the first half plays A4,
    // the second B4.
    let mut synth = sine_synth();
    synth.note_on(69, 0.8);
    render(&mut synth, 0.1);
    let frames = RATE as usize / 2;
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    let events = [BlockEvent {
        offset: frames / 2,
        kind: BlockEventKind::Control(Control::PitchBend(1.0)),
    }];
    synth.render_block(&events, &mut left, &mut right);
    let before = frequency(&left[..frames / 2 - 64]);
    let after = semitones(A4, frequency(&left[frames / 2 + 64..]));
    assert!((before - A4).abs() < 1.0, "before the move: {before} Hz");
    assert!((after - 2.0).abs() < 0.02, "after it: {after} semitones");
}

#[test]
fn moving_controllers_never_allocates() {
    let bank = SoundFontBank::from_bytes(&sine_test_font(RATE).to_bytes()).unwrap();
    let (region, sample) = sine_region();
    let mut instruments: Vec<Box<dyn Instrument>> = vec![
        Box::new(sine_synth()),
        Box::new(SoundFontPlayer::new(&bank, RATE, 0, 0).unwrap()),
        Box::new(sampler_with(region, sample)),
        Box::new(SineSynth::new(RATE)),
    ];
    let mut left = vec![0.0; 256];
    let mut right = vec![0.0; 256];
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let before = daw_rt::violation_count();
    {
        let _guard = daw_rt::RtGuard::enter();
        for instrument in &mut instruments {
            instrument.note_on(60, 0.8);
            for step in 0..64 {
                let value = step as f32 / 63.0;
                instrument.control(Control::PitchBend(value * 2.0 - 1.0));
                instrument.control(modulation(value));
                instrument.control(Control::Pressure(value));
                instrument.control(expression(1.0 - value));
                instrument.control(Control::Controller { number: 74, value });
                instrument.render(&mut left, &mut right);
            }
            for control in Control::REST {
                instrument.control(control);
            }
            instrument.render(&mut left, &mut right);
        }
    }
    assert_eq!(
        daw_rt::violation_count(),
        before,
        "moving a controller allocated on the audio thread"
    );
}

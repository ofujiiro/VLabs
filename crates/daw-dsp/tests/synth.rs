// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in synth, measured: pitch, the filter, the envelopes, voices,
//! glide, extreme settings, and that playing it and changing its settings
//! never allocate (it runs on the audio thread).

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "test code: helpers unwrap and convert freely"
)]

use daw_dsp::synth::{EnvParams, FilterMode, LfoShape, OscParams, Synth, SynthParams, Wave};
use daw_dsp::{Instrument, InstrumentSettings};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;

fn sine_params() -> SynthParams {
    let mut p = SynthParams::default();
    p.osc[0].wave = Wave::Sine;
    p
}

fn render(synth: &mut Synth, seconds: f32) -> Vec<f32> {
    let frames = (seconds * RATE as f32) as usize;
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    // In odd-sized blocks, as the engine splits them around notes.
    let mut done = 0;
    for size in [64, 333, 1024, 17].iter().cycle() {
        if done >= frames {
            break;
        }
        let end = (done + size).min(frames);
        synth.render(&mut left[done..end], &mut right[done..end]);
        done = end;
    }
    assert_eq!(left, right, "the synth is centred");
    left
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

fn db(ratio: f32) -> f32 {
    20.0 * ratio.max(1e-9).log10()
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

/// How strong one frequency is in a signal (Goertzel), as an amplitude.
fn amplitude_at(samples: &[f32], hz: f32) -> f32 {
    let w = std::f32::consts::TAU * hz / RATE as f32;
    let (mut s1, mut s2) = (0.0_f64, 0.0_f64);
    let coefficient = 2.0 * f64::from(w.cos());
    for &x in samples {
        let s = f64::from(x) + coefficient * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    let power = s1 * s1 + s2 * s2 - coefficient * s1 * s2;
    (2.0 * power.sqrt() / samples.len() as f64) as f32
}

#[test]
fn a_note_plays_at_its_pitch_and_stops_after_its_release() {
    let mut synth = Synth::new(RATE, sine_params());
    synth.note_on(69, 1.0);
    let held = render(&mut synth, 0.5);
    let measured = frequency(&held[4800..]);
    assert!(
        (measured - 440.0).abs() < 0.5,
        "A4 is 440 Hz, got {measured}"
    );
    assert!(peak(&held) > 0.1, "it is clearly heard");
    synth.note_off(69);
    let tail = render(&mut synth, 0.3);
    assert!(
        rms(&tail[tail.len() - 480..]) < 1e-5,
        "silent after the release"
    );
    assert!(!synth.is_sounding(), "and its voice is free again");
}

#[test]
fn oscillator_pitch_settings_move_by_octaves_semitones_and_cents() {
    let mut params = sine_params();
    params.osc[0].pitch = -12.0 + 7.0 + 0.1; // an octave down, a fifth up, 10 cents
    let mut synth = Synth::new(RATE, params);
    synth.note_on(69, 1.0);
    let held = render(&mut synth, 0.5);
    let expected = 440.0 * 2.0_f32.powf((-12.0 + 7.0 + 0.1) / 12.0);
    let measured = frequency(&held[4800..]);
    assert!(
        (measured - expected).abs() < 0.3,
        "{measured} Hz, expected {expected}"
    );
}

#[test]
fn every_wave_has_its_fundamental_and_high_notes_do_not_alias_much() {
    for wave in [Wave::Sine, Wave::Triangle, Wave::Saw, Wave::Square] {
        let mut params = SynthParams::default();
        params.osc[0].wave = wave;
        let mut synth = Synth::new(RATE, params);
        // E7: 2637 Hz, with harmonics far above half the sample rate.
        synth.note_on(100, 1.0);
        let sound = render(&mut synth, 1.0);
        let steady = &sound[9600..];
        let fundamental = amplitude_at(steady, 2637.02);
        assert!(fundamental > 0.05, "{wave:?} has its fundamental");
        // The strongest alias of a naive saw at this pitch lands near
        // 48000 - 19 × 2637 = -2103 Hz, folded to 2103 Hz.
        let worst = [2103.6_f32, 4740.6, 3200.0]
            .iter()
            .map(|&hz| amplitude_at(steady, hz))
            .fold(0.0, f32::max);
        assert!(
            db(worst / fundamental) < -30.0,
            "{wave:?}: aliases at {:.1} dB",
            db(worst / fundamental)
        );
    }
}

#[test]
fn the_filter_takes_away_what_its_kind_says() {
    // A 4 kHz tone (a sine an octave and a fifth above E6 would be
    // fiddly: use the note whose pitch is closest, B7 = 3951 Hz).
    let level_through = |mode: FilterMode, cutoff_hz: f32| {
        let mut params = sine_params();
        params.filter_mode = mode;
        params.cutoff_hz = cutoff_hz;
        let mut synth = Synth::new(RATE, params);
        synth.note_on(107, 1.0);
        let sound = render(&mut synth, 0.5);
        rms(&sound[9600..])
    };
    let open = level_through(FilterMode::LowPass24, 20_000.0);
    let steep = db(level_through(FilterMode::LowPass24, 500.0) / open);
    let gentle = db(level_through(FilterMode::LowPass12, 500.0) / open);
    let high = db(level_through(FilterMode::HighPass, 500.0) / open);
    let high_cut = db(level_through(FilterMode::HighPass, 16_000.0) / open);
    let band = db(level_through(FilterMode::BandPass, 3951.0) / open);
    assert!(
        steep < -60.0,
        "24 dB/octave, three octaves above: {steep:.1} dB"
    );
    assert!(
        (-45.0..-28.0).contains(&gentle),
        "12 dB/octave, three octaves above: {gentle:.1} dB"
    );
    assert!(
        high.abs() < 1.0,
        "a high-pass lets highs through: {high:.1} dB"
    );
    assert!(
        high_cut < -24.0,
        "and takes lows away (12 dB/octave, two octaves below): {high_cut:.1} dB"
    );
    assert!(
        band.abs() < 1.0,
        "a band-pass keeps its centre: {band:.1} dB"
    );
}

#[test]
fn the_filter_envelope_opens_and_closes_the_filter() {
    let mut params = SynthParams::default();
    params.osc[0].wave = Wave::Saw;
    params.cutoff_hz = 200.0;
    params.filter_env_octaves = 5.0;
    params.filter_env = EnvParams {
        attack: 0.001,
        decay: 0.2,
        sustain: 0.0,
        release: 0.1,
    };
    let mut synth = Synth::new(RATE, params);
    synth.note_on(45, 1.0);
    let sound = render(&mut synth, 0.6);
    // The upper harmonics are strong at first, then the filter closes.
    let early = amplitude_at(&sound[480..2880], 3520.0);
    let late = amplitude_at(&sound[24_000..28_800], 3520.0);
    assert!(
        db(late / early) < -30.0,
        "the 32nd harmonic fades as the filter closes: {:.1} dB",
        db(late / early)
    );
}

#[test]
fn attack_and_release_take_the_time_they_say() {
    let mut params = sine_params();
    params.amp_env = EnvParams {
        attack: 0.1,
        decay: 0.1,
        sustain: 1.0,
        release: 0.2,
    };
    let mut synth = Synth::new(RATE, params);
    synth.note_on(69, 1.0);
    let rise = render(&mut synth, 0.3);
    let level = |samples: &[f32], at: f32| {
        let i = (at * RATE as f32) as usize;
        peak(&samples[i - 120..i + 120])
    };
    let top = level(&rise, 0.25);
    assert!(
        (level(&rise, 0.05) / top - 0.5).abs() < 0.06,
        "half way at half the attack"
    );
    assert!(
        level(&rise, 0.1) / top > 0.95,
        "at the top after the attack"
    );
    synth.note_off(69);
    let fall = render(&mut synth, 0.3);
    let after = db(level(&fall, 0.2) / top);
    assert!(
        (-50.0..-37.0).contains(&after),
        "99 % down after the release time: {after:.1} dB"
    );
}

#[test]
fn sixteen_voices_at_most_and_a_repeated_key_reuses_its_voice() {
    let mut synth = Synth::new(RATE, SynthParams::default());
    for key in 40..70 {
        synth.note_on(key, 0.8);
    }
    render(&mut synth, 0.05);
    assert_eq!(synth.active_voices(), 16, "the oldest notes give way");
    synth.all_notes_off(true);
    assert_eq!(synth.active_voices(), 0);
    synth.note_on(60, 0.8);
    synth.note_on(60, 0.8);
    synth.note_on(60, 0.8);
    render(&mut synth, 0.05);
    assert_eq!(
        synth.active_voices(),
        1,
        "the same key starts again in place"
    );
}

#[test]
fn mono_glides_between_held_notes_and_goes_back_to_the_one_still_held() {
    let mut params = sine_params();
    params.mono = true;
    params.glide_seconds = 0.1;
    let mut synth = Synth::new(RATE, params);
    synth.note_on(57, 1.0); // A3, 220 Hz
    let first = render(&mut synth, 0.3);
    assert!((frequency(&first[4800..]) - 220.0).abs() < 0.5);
    synth.note_on(69, 1.0); // A4 while A3 is held: legato
    let gliding = render(&mut synth, 0.4);
    assert_eq!(synth.active_voices(), 1, "one voice");
    let early = frequency(&gliding[960..2400]);
    assert!(early > 225.0 && early < 430.0, "on its way: {early} Hz");
    assert!(
        (frequency(&gliding[14_400..]) - 440.0).abs() < 0.5,
        "arrived after the glide"
    );
    synth.note_off(69);
    let back = render(&mut synth, 0.4);
    assert!(
        (frequency(&back[14_400..]) - 220.0).abs() < 0.5,
        "back to the key still held"
    );
    synth.note_off(57);
    render(&mut synth, 0.3);
    assert!(!synth.is_sounding());
}

#[test]
fn the_lfo_wobbles_the_pitch_and_the_volume() {
    let mut params = sine_params();
    params.lfo_shape = LfoShape::Sine;
    params.lfo_rate_hz = 4.0;
    params.vibrato_cents = 50.0;
    let mut synth = Synth::new(RATE, params);
    synth.note_on(69, 1.0);
    let sound = render(&mut synth, 1.0);
    let quarter = RATE as usize / 16;
    let pitches: Vec<f32> = (0..16)
        .map(|i| frequency(&sound[i * quarter..(i + 1) * quarter]))
        .collect();
    let (low, high) = pitches
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), &p| (l.min(p), h.max(p)));
    assert!(high / low > 1.03, "vibrato moves the pitch: {low}..{high}");

    let mut params = sine_params();
    params.lfo_rate_hz = 4.0;
    params.tremolo = 1.0;
    let mut synth = Synth::new(RATE, params);
    synth.note_on(69, 1.0);
    let sound = render(&mut synth, 1.0);
    let levels: Vec<f32> = (0..16)
        .map(|i| rms(&sound[i * quarter..(i + 1) * quarter]))
        .collect();
    let (low, high) = levels
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), &p| (l.min(p), h.max(p)));
    assert!(low < 0.3 * high, "tremolo dips the volume: {low}..{high}");
}

#[test]
fn velocity_changes_loudness_as_much_as_asked() {
    let level_at = |velocity_amount: f32, velocity: f32| {
        let mut params = sine_params();
        params.velocity = velocity_amount;
        let mut synth = Synth::new(RATE, params);
        synth.note_on(69, velocity);
        rms(&render(&mut synth, 0.2)[4800..])
    };
    assert!((level_at(0.0, 0.2) / level_at(0.0, 1.0) - 1.0).abs() < 1e-3);
    assert!(level_at(1.0, 0.2) / level_at(1.0, 1.0) < 0.25);
}

#[test]
fn extreme_settings_stay_finite_and_bounded() {
    let mut params = SynthParams {
        osc: [
            OscParams {
                wave: Wave::Square,
                pitch: 36.0,
                level: 1.0,
            },
            OscParams {
                wave: Wave::Saw,
                pitch: -36.0,
                level: 1.0,
            },
        ],
        pulse_width: 0.05,
        noise: 1.0,
        resonance: 1.0,
        filter_env_octaves: 6.0,
        key_tracking: 1.0,
        lfo_shape: LfoShape::Random,
        lfo_rate_hz: 20.0,
        wobble_octaves: 4.0,
        gain: 2.0,
        ..SynthParams::default()
    };
    for mode in [
        FilterMode::LowPass24,
        FilterMode::LowPass12,
        FilterMode::HighPass,
        FilterMode::BandPass,
    ] {
        for cutoff in [20.0, 1000.0, 20_000.0] {
            params.filter_mode = mode;
            params.cutoff_hz = cutoff;
            let mut synth = Synth::new(RATE, params);
            for key in [0, 24, 60, 96, 127] {
                synth.note_on(key, 1.0);
            }
            let sound = render(&mut synth, 0.5);
            assert!(sound.iter().all(|s| s.is_finite()), "{mode:?} {cutoff}");
            assert!(peak(&sound) < 8.0, "{mode:?} {cutoff}: {}", peak(&sound));
        }
    }
}

#[test]
fn changing_settings_while_notes_sound_keeps_them_sounding() {
    let mut synth = Synth::new(RATE, sine_params());
    synth.note_on(60, 1.0);
    synth.note_on(64, 1.0);
    render(&mut synth, 0.1);
    let mut brighter = sine_params();
    brighter.osc[0].wave = Wave::Saw;
    brighter.cutoff_hz = 3000.0;
    synth.apply_settings(&InstrumentSettings::Synth(brighter));
    let after = render(&mut synth, 0.1);
    assert_eq!(synth.active_voices(), 2, "the notes carry on");
    assert!(rms(&after) > 0.05);
    assert_eq!(synth.params(), &brighter);
}

#[test]
fn playing_and_changing_settings_never_allocate() {
    let mut synth = Synth::new(RATE, SynthParams::default());
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    let mono = SynthParams {
        mono: true,
        glide_seconds: 0.05,
        ..SynthParams::default()
    };
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let before = daw_rt::violation_count();
    {
        let _guard = daw_rt::RtGuard::enter();
        for key in 30..80 {
            synth.note_on(key, 0.7);
            synth.render(&mut left, &mut right);
        }
        synth.apply_settings(&InstrumentSettings::Synth(mono));
        synth.note_on(50, 1.0);
        synth.note_on(55, 1.0);
        synth.render(&mut left, &mut right);
        synth.note_off(55);
        synth.render(&mut left, &mut right);
        synth.all_notes_off(false);
        synth.render(&mut left, &mut right);
    }
    assert_eq!(
        daw_rt::violation_count(),
        before,
        "the synth allocated on the audio thread"
    );
}

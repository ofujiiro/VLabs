// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in effects, measured: EQ responses at known frequencies,
//! compressor levels, echo timing, reverb tails, the limiter's ceiling, and
//! that processing and changing settings never allocate (they run on the
//! audio thread).

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "test code: helpers unwrap and convert freely"
)]

use daw_dsp::effects::{
    Compressor, CompressorParams, Delay, DelayParams, Effect, EffectParams, Eq, EqParams, Limiter,
    LimiterParams, Reverb, ReverbParams,
};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;

fn sine(frequency: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
    let frames = (seconds * RATE as f32) as usize;
    (0..frames)
        .map(|i| amplitude * (std::f32::consts::TAU * frequency * i as f32 / RATE as f32).sin())
        .collect()
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

/// Runs a mono signal through an EQ (both channels) and returns the level
/// change in dB, measured after the filters settle.
fn eq_gain_db(params: &EqParams, frequency: f32) -> f32 {
    let mut eq = Eq::new(params, RATE);
    let input = sine(frequency, 0.5, 0.25);
    let mut left = input.clone();
    let mut right = input.clone();
    eq.process(&mut left, &mut right);
    let settled = input.len() / 2;
    db(rms(&left[settled..]) / rms(&input[settled..]))
}

#[test]
fn the_eq_boosts_and_cuts_where_asked() {
    let bell = EqParams {
        mid_gain_db: 6.0,
        mid_hz: 1000.0,
        ..EqParams::default()
    };
    assert!(
        (eq_gain_db(&bell, 1000.0) - 6.0).abs() < 0.3,
        "{}",
        eq_gain_db(&bell, 1000.0)
    );
    assert!(eq_gain_db(&bell, 100.0).abs() < 0.5);
    assert!(eq_gain_db(&bell, 10_000.0).abs() < 0.5);

    let cuts = EqParams {
        low_cut_hz: Some(100.0),
        high_cut_hz: Some(5000.0),
        ..EqParams::default()
    };
    assert!(
        eq_gain_db(&cuts, 25.0) < -20.0,
        "two octaves below a 12 dB/oct cut"
    );
    assert!(
        eq_gain_db(&cuts, 1000.0).abs() < 0.5,
        "the middle is untouched"
    );
    assert!(eq_gain_db(&cuts, 20_000.0) < -18.0);

    let shelves = EqParams {
        low_gain_db: -6.0,
        high_gain_db: 4.0,
        ..EqParams::default()
    };
    assert!((eq_gain_db(&shelves, 30.0) + 6.0).abs() < 0.5);
    assert!((eq_gain_db(&shelves, 16_000.0) - 4.0).abs() < 0.5);

    let flat = EqParams::default();
    for frequency in [50.0, 500.0, 5000.0] {
        assert!(
            eq_gain_db(&flat, frequency).abs() < 0.01,
            "a flat EQ changes nothing"
        );
    }
    // The curve shown on screen matches what is heard.
    let eq = Eq::new(&bell, RATE);
    assert!((eq.response_db(1000.0) - 6.0).abs() < 0.1);
}

#[test]
fn the_compressor_follows_its_curve() {
    let params = CompressorParams {
        threshold_db: -20.0,
        ratio: 4.0,
        attack_ms: 5.0,
        release_ms: 50.0,
        knee_db: 0.0,
        makeup_db: 0.0,
    };
    let mut compressor = Compressor::new(&params, RATE);
    // A sine peaking at -8 dBFS is 12 dB over: it should come out 9 dB lower.
    let input = sine(200.0, 1.0, db_gain(-8.0));
    let mut left = input.clone();
    let mut right = input.clone();
    compressor.process(&mut left, &mut right);
    let settled = &left[left.len() / 2..];
    let out_db = db(peak(settled));
    assert!(
        (out_db - (-17.0)).abs() < 0.6,
        "came out at {out_db:.2} dBFS"
    );
    assert!((compressor.reduction_db() - 9.0).abs() < 0.8);

    // Below the threshold nothing changes.
    let mut quiet = Compressor::new(&params, RATE);
    let input = sine(200.0, 0.5, db_gain(-30.0));
    let mut left = input.clone();
    let mut right = input.clone();
    quiet.process(&mut left, &mut right);
    assert!((db(peak(&left[12_000..])) - (-30.0)).abs() < 0.1);

    // The soft knee bends the curve smoothly through the threshold.
    let soft = Compressor::new(
        &CompressorParams {
            knee_db: 6.0,
            ..params
        },
        RATE,
    );
    assert!(soft.gain_change_db(-30.0).abs() < 1e-6);
    assert!(soft.gain_change_db(-20.0) < 0.0 && soft.gain_change_db(-20.0) > -1.0);
    assert!((soft.gain_change_db(-8.0) - (-9.0)).abs() < 1e-4);
}

fn db_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[test]
fn echoes_arrive_on_time_and_fade_by_the_feedback() {
    let params = DelayParams {
        time_seconds: 0.1,
        feedback: 0.5,
        mix: 0.5,
        tone_hz: 20_000.0,
        ping_pong: false,
    };
    let mut delay = Delay::new(&params, RATE);
    let mut left = vec![0.0_f32; RATE as usize];
    left[0] = 1.0;
    let mut right = left.clone();
    delay.process(&mut left, &mut right);
    let echo_at = |from: usize| {
        let window = &left[from..from + 200];
        let (offset, value) = window
            .iter()
            .enumerate()
            .fold((0, 0.0_f32), |best, (i, v)| {
                if v.abs() > best.1.abs() {
                    (i, *v)
                } else {
                    best
                }
            });
        (from + offset, value)
    };
    let (first, first_level) = echo_at(4700);
    let (second, second_level) = echo_at(9500);
    assert!(
        (first as i64 - 4800).abs() <= 1,
        "first echo at frame {first}"
    );
    assert!(
        (second as i64 - 9600).abs() <= 2,
        "second echo at frame {second}"
    );
    assert!(
        (first_level - 0.5).abs() < 0.05,
        "first echo level {first_level}"
    );
    assert!(
        (second_level / first_level - 0.5).abs() < 0.05,
        "each echo is half the last: {second_level} after {first_level}"
    );
    assert!((left[0] - 0.5).abs() < 1e-6, "the dry sound is mixed in");

    // Ping-pong: echoes alternate sides.
    let mut ping = Delay::new(
        &DelayParams {
            ping_pong: true,
            ..params
        },
        RATE,
    );
    let mut left = vec![0.0_f32; RATE as usize];
    left[0] = 1.0;
    let mut right = left.clone();
    ping.process(&mut left, &mut right);
    let energy = |s: &[f32], at: usize| peak(&s[at - 50..at + 50]);
    assert!(
        energy(&left, 4800) > 0.2 && energy(&right, 4800) < 0.01,
        "first echo on the left"
    );
    assert!(
        energy(&right, 9600) > 0.1 && energy(&left, 9600) < 0.01,
        "second on the right"
    );
}

#[test]
fn the_reverb_answers_and_fades() {
    let params = ReverbParams {
        mix: 0.5,
        pre_delay_ms: 20.0,
        ..ReverbParams::default()
    };
    let mut reverb = Reverb::new(&params, RATE);
    let mut left = vec![0.0_f32; RATE as usize * 4];
    left[0] = 1.0;
    let mut right = left.clone();
    reverb.process(&mut left, &mut right);
    let pre = (0.020 * RATE as f32) as usize;
    assert!(
        peak(&left[1..pre - 10]) < 1e-6,
        "nothing before the pre-delay"
    );
    let early = rms(&left[pre..pre + 9600]);
    let late = rms(&left[RATE as usize * 3..]);
    assert!(early > 1e-3, "the room answers: {early}");
    assert!(late < early / 30.0, "the tail fades: {late} after {early}");
    assert!(peak(&left) <= 1.0 && peak(&right) <= 1.0);
    assert!((left[1..] != right[1..]), "the tail is stereo");

    // A bigger room rings longer.
    let tail = |size: f32| {
        let mut reverb = Reverb::new(&ReverbParams { size, ..params }, RATE);
        let mut l = vec![0.0_f32; RATE as usize * 3];
        l[0] = 1.0;
        let mut r = l.clone();
        reverb.process(&mut l, &mut r);
        rms(&l[RATE as usize * 2..])
    };
    assert!(tail(0.9) > tail(0.2) * 3.0);

    // Fully dry passes the sound through unchanged.
    let mut dry = Reverb::new(&ReverbParams { mix: 0.0, ..params }, RATE);
    let input = sine(440.0, 0.1, 0.5);
    let mut l = input.clone();
    let mut r = input.clone();
    dry.process(&mut l, &mut r);
    assert_eq!(l, input);
}

#[test]
fn the_limiter_never_crosses_its_ceiling() {
    let params = LimiterParams {
        ceiling_db: -1.0,
        gain_db: 6.0,
        release_ms: 50.0,
    };
    let mut limiter = Limiter::new(&params, RATE);
    let ceiling = db_gain(-1.0);
    // A loud sine with sudden bursts on top.
    let mut left = sine(150.0, 1.0, 0.9);
    for i in (0..left.len()).step_by(4000) {
        left[i] = 1.5;
    }
    let mut right: Vec<f32> = left.iter().map(|s| -s * 0.8).collect();
    limiter.process(&mut left, &mut right);
    let highest = peak(&left).max(peak(&right));
    assert!(
        highest <= ceiling + 1e-6,
        "peaked at {highest} over the ceiling {ceiling}"
    );
    assert!(highest > ceiling * 0.95, "and uses the room up to it");

    // A quiet sound passes untouched, only late by the look-ahead.
    let mut gentle = Limiter::new(&LimiterParams::default(), RATE);
    let input = sine(440.0, 0.2, 0.1);
    let mut l = input.clone();
    let mut r = input.clone();
    gentle.process(&mut l, &mut r);
    let latency = gentle.latency();
    for (out, inp) in l[latency..].iter().zip(&input) {
        assert!((out - inp).abs() < 1e-6);
    }
}

#[test]
fn processing_and_changing_settings_never_allocate() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let all = [
        EffectParams::Eq(EqParams {
            low_cut_hz: Some(80.0),
            mid_gain_db: 3.0,
            ..EqParams::default()
        }),
        EffectParams::Compressor(CompressorParams::default()),
        EffectParams::Reverb(ReverbParams::default()),
        EffectParams::Delay(DelayParams::default()),
        EffectParams::Limiter(LimiterParams::default()),
    ];
    let mut effects: Vec<Effect> = all.iter().map(|p| Effect::new(p, RATE)).collect();
    let mut left = sine(330.0, 0.05, 0.5);
    let mut right = left.clone();
    let before = daw_rt::violation_count();
    {
        let _guard = daw_rt::RtGuard::enter();
        for (effect, params) in effects.iter_mut().zip(&all) {
            effect.process(&mut left, &mut right);
            assert!(effect.set_params(params));
            effect.process(&mut left, &mut right);
            effect.reset();
        }
    }
    assert_eq!(
        daw_rt::violation_count(),
        before,
        "an effect allocated on the audio thread"
    );
    assert!(
        !effects[0].set_params(&all[1]),
        "settings for another kind are refused"
    );
}

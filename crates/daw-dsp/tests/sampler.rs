// SPDX-License-Identifier: GPL-3.0-or-later

//! The sampler (decision 0024), measured from outside: a looped, filtered,
//! layered instrument plays in blocks of any size, and playing it never
//! allocates (it runs on the audio thread).

#![allow(
    clippy::cast_precision_loss,
    reason = "test code: helpers convert freely"
)]

use std::sync::Arc;

use daw_dsp::Instrument;
use daw_dsp::sampler::{
    EnvelopeSpec, FilterSpec, KeySwitch, LoopMode, Region, Sample, Sampler, SamplerBank, Trigger,
};
use daw_dsp::synth::FilterMode;

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;

/// Half a second of a stereo sine, looped over its last part.
fn looped_tone(hz: f32) -> Sample {
    let wave = |phase: f32| -> Vec<f32> {
        (0..24_000)
            .map(|i| 0.4 * (std::f32::consts::TAU * hz * i as f32 / 48_000.0 + phase).sin())
            .collect()
    };
    Sample {
        sample_rate: RATE,
        left: wave(0.0),
        right: wave(0.3),
        file_loop: Some((12_000, 23_999)),
    }
}

/// An instrument using most of what the sampler does at once.
fn instrument() -> Arc<SamplerBank> {
    let mut fileg = EnvelopeSpec::filter();
    fileg.attack = 0.05;
    fileg.depth = 2400.0;
    let mut ampeg = EnvelopeSpec::amp();
    ampeg.attack = 0.002;
    ampeg.decay = 0.3;
    ampeg.sustain = 0.6;
    ampeg.release = 0.2;
    let sustained = Region {
        sample: Some(0),
        hikey: 95,
        ampeg,
        width: 0.5,
        pan: -0.25,
        xfout_lovel: 90,
        xfout_hivel: 110,
        filter: Some(FilterSpec {
            mode: FilterMode::LowPass12,
            cutoff: 800.0,
            resonance: 6.0,
            keytrack: 50.0,
            keycenter: 60,
            veltrack: 1200.0,
            random: 100.0,
            envelope: fileg,
        }),
        seq_length: 2,
        ..Region::default()
    };
    let other_turn = Region {
        seq_position: 2,
        tune: 7.0,
        ..sustained.clone()
    };
    let loud_layer = Region {
        sample: Some(0),
        hikey: 95,
        xfin_lovel: 90,
        xfin_hivel: 110,
        loop_mode: Some(LoopMode::Sustain),
        ..Region::default()
    };
    let release = Region {
        sample: Some(1),
        lokey: 36,
        hikey: 95,
        trigger: Trigger::Release,
        rt_decay: 3.0,
        ..Region::default()
    };
    let switched = Region {
        sample: Some(1),
        lokey: 96,
        sw_last: Some(24),
        group: 1,
        off_by: Some(1),
        loop_mode: Some(LoopMode::OneShot),
        note_polyphony: Some(1),
        ..Region::default()
    };
    Arc::new(SamplerBank {
        regions: vec![sustained, other_turn, loud_layer, release, switched],
        samples: vec![
            looped_tone(220.0),
            Sample {
                sample_rate: 44_100,
                left: vec![0.1; 4410],
                right: Vec::new(),
                file_loop: None,
            },
        ],
        keyswitch: Some(KeySwitch {
            lokey: 24,
            hikey: 25,
            default: None,
        }),
        ..SamplerBank::default()
    })
}

/// Renders `frames` in blocks of odd sizes, as the engine splits them
/// around notes.
fn render(sampler: &mut Sampler, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
    let mut done = 0;
    for size in [64, 333, 1024, 17].iter().cycle() {
        if done >= frames {
            break;
        }
        let end = (done + size).min(frames);
        sampler.render(&mut left[done..end], &mut right[done..end]);
        done = end;
    }
    (left, right)
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn a_layered_looped_filtered_instrument_plays_in_any_blocks() {
    let mut sampler = Sampler::new(instrument(), RATE);
    sampler.note_on(60, 0.5);
    sampler.note_on(67, 1.0);
    let (held_left, held_right) = render(&mut sampler, 48_000);
    assert!(peak(&held_left[40_000..]) > 0.05, "the loop sustains");
    assert!(held_left.iter().chain(&held_right).all(|s| s.is_finite()));
    assert!(
        peak(&held_left) < 1.5,
        "no runaway filter: {}",
        peak(&held_left)
    );
    assert_ne!(held_left, held_right, "a stereo sound stays stereo");

    sampler.note_off(60);
    sampler.note_off(67);
    let (tail, _) = render(&mut sampler, 48_000);
    assert!(
        peak(&tail[..2000]) > 0.01,
        "release sounds and fading notes"
    );
    assert!(peak(&tail[30_000..]) < 1e-3, "then silence");
    assert!(!sampler.is_sounding());

    // A keyswitch opens the switched region, a one-shot that cuts itself
    // off when repeated.
    sampler.note_on(96, 1.0);
    assert!(!sampler.is_sounding(), "no keyswitch pressed yet");
    sampler.note_on(24, 1.0);
    sampler.note_off(24);
    sampler.note_on(96, 1.0);
    sampler.note_off(96);
    assert!(sampler.is_sounding());
    let (shot, _) = render(&mut sampler, 9000);
    assert!(peak(&shot[..4000]) > 0.05);
    assert!(!sampler.is_sounding(), "a one-shot plays to its end");
}

#[test]
fn playing_never_allocates() {
    let mut sampler = Sampler::new(instrument(), RATE);
    let (mut left, mut right) = (vec![0.0; 512], vec![0.0; 512]);
    daw_rt::rt_region(|| {
        for round in 0..3_u8 {
            for key in (20..110).step_by(3) {
                sampler.note_on(key, f32::from(key % 127) / 127.0);
                sampler.render(&mut left, &mut right);
                if round > 0 {
                    sampler.note_off(key);
                }
            }
            sampler.note_on(24, 1.0);
            sampler.note_on(96, 1.0);
            sampler.render(&mut left, &mut right);
            sampler.all_notes_off(round == 2);
            sampler.render(&mut left, &mut right);
        }
    });
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The bundled kits, played: the recorded acoustic kit loads from its
//! bundle, every pad of both kits sounds through the sampler, and playing
//! never allocates on the audio thread.

#![allow(clippy::unwrap_used, reason = "test code")]

use daw_dsp::Instrument;
use daw_dsp::drums::{DrumKit, DrumSampler, PadSound};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

static ACOUSTIC: &[u8] = include_bytes!("../../../assets/drums/acoustic.kit");

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn every_pad_of_both_kits_sounds() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let electronic = DrumKit::electronic(48_000);
    let acoustic = DrumKit::from_bundle(ACOUSTIC)
        .unwrap()
        .completed_with(&electronic);
    assert_eq!(
        acoustic.pads.len(),
        PadSound::ALL.len(),
        "the acoustic kit borrows clap and cowbell"
    );
    for kit in [acoustic, electronic] {
        let mut sampler = DrumSampler::new(kit, 48_000);
        let (mut left, mut right) = (vec![0.0_f32; 4800], vec![0.0_f32; 4800]);
        for sound in PadSound::ALL {
            let before = daw_rt::violation_count();
            {
                let _guard = daw_rt::RtGuard::enter();
                sampler.note_on(sound.key(), 0.9);
                sampler.render(&mut left, &mut right);
            }
            assert_eq!(
                daw_rt::violation_count(),
                before,
                "{sound:?} allocated on the audio thread"
            );
            let level = peak(&left).max(peak(&right));
            assert!(level > 0.05, "{sound:?} is silent ({level})");
            assert!(level < 1.0, "{sound:?} clips ({level})");
            sampler.all_notes_off(true);
            sampler.render(&mut left, &mut right);
        }
    }
}

#[test]
fn soft_hits_are_quieter_than_hard_ones() {
    let kit = DrumKit::from_bundle(ACOUSTIC).unwrap();
    let level = |velocity: f32| {
        let mut sampler = DrumSampler::new(kit.clone(), 48_000);
        let (mut left, mut right) = (vec![0.0_f32; 9600], vec![0.0_f32; 9600]);
        sampler.note_on(PadSound::Snare.key(), velocity);
        sampler.render(&mut left, &mut right);
        peak(&left)
    };
    assert!(level(0.2) < level(0.6) && level(0.6) < level(1.0));
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound library's preview (decision 0023): a sound plays once over
//! everything else, stops when asked or when it ends, and the audio thread
//! never allocates or frees memory for it.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::sync::Arc;

use daw_dsp::audio::AudioBuffer;
use daw_engine::{Engine, EngineConfig, SoundLibrary};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;
const BLOCK: usize = 480;

/// A tone of `seconds` at 440 Hz, half as loud as it can be.
fn tone(seconds: f32) -> Arc<AudioBuffer> {
    let frames = (seconds * RATE as f32) as usize;
    let samples: Vec<f32> = (0..frames)
        .map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / RATE as f32).sin())
        .collect();
    Arc::new(AudioBuffer::new(RATE, vec![samples]))
}

/// The loudest sample of `blocks` blocks, with allocation checked.
fn loudest(processor: &mut daw_engine::EngineProcessor, blocks: usize) -> f32 {
    let mut out = vec![0.0_f32; BLOCK * 2];
    let mut loudest = 0.0_f32;
    for _ in 0..blocks {
        let before = daw_rt::violation_count();
        processor.process(&mut out, 2);
        assert_eq!(
            daw_rt::violation_count(),
            before,
            "the audio thread allocated or freed memory"
        );
        loudest = out.iter().fold(loudest, |m, s| m.max(s.abs()));
    }
    loudest
}

#[test]
fn a_preview_plays_once_over_everything_and_stops_when_asked() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let (mut engine, mut processor) = Engine::new(
        EngineConfig {
            sample_rate: RATE,
            max_block: 512,
        },
        SoundLibrary::new(),
    );
    assert!(!engine.is_previewing());
    assert!(loudest(&mut processor, 2) < 1e-6, "silence first");

    // A tenth of a second: heard, then over by itself.
    engine.preview(Some(tone(0.1)));
    assert!(engine.is_previewing(), "playing as soon as it is asked for");
    let heard = loudest(&mut processor, 5);
    assert!(heard > 0.3 && heard < 0.4, "played at about -3 dB: {heard}");
    assert!(engine.is_previewing());
    assert!(loudest(&mut processor, 10) < 0.4);
    assert!(
        loudest(&mut processor, 2) < 1e-6,
        "over after a tenth of a second"
    );
    assert!(!engine.is_previewing());
    engine.poll(); // Frees what the audio thread let go.

    // A long one, stopped; then another replaces it.
    engine.preview(Some(tone(5.0)));
    assert!(loudest(&mut processor, 2) > 0.3);
    engine.preview(None);
    assert!(!engine.is_previewing());
    assert!(loudest(&mut processor, 2) < 1e-6, "stopped at once");
    engine.preview(Some(tone(5.0)));
    loudest(&mut processor, 2);
    engine.preview(Some(tone(5.0)));
    assert!(loudest(&mut processor, 2) > 0.3);
    assert!(engine.is_previewing());
    engine.panic();
    assert!(
        loudest(&mut processor, 2) < 1e-6,
        "silenced with everything else"
    );
    engine.poll();
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The reference plugins, hosted as the program hosts them: Surge XT and
//! Dexed (instruments) and Surge XT Effects, each as CLAP and VST3. They are
//! downloaded test fixtures, never linked or bundled; these tests run when
//! `VLABS_TEST_PLUGINS` names the folder that holds them (`surge/` and
//! `dexed/` inside), and pass quietly otherwise.

#![allow(
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "test code: helpers unwrap, report and convert freely"
)]

use std::path::PathBuf;

use daw_plugin_host::{
    AudioConfig, Event, Format, Kind, LoadedPlugin, PluginInfo, Transport, load, scan,
};

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

fn fixtures() -> Option<PathBuf> {
    let folder = PathBuf::from(std::env::var_os("VLABS_TEST_PLUGINS")?);
    if folder.is_dir() {
        Some(folder)
    } else {
        eprintln!("VLABS_TEST_PLUGINS does not name a folder; skipped");
        None
    }
}

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: RATE,
        max_block: BLOCK,
    }
}

/// The plugin of `kind` in a fixture file.
fn find(file: &str, format: Format, kind: Kind) -> Option<PluginInfo> {
    let path = fixtures()?.join(file);
    let plugins = scan::describe(format, &path).unwrap();
    let found = plugins.into_iter().find(|p| p.kind == kind);
    assert!(found.is_some(), "no {kind:?} in {}", path.display());
    found
}

/// Renders `seconds` of audio, sending each event in the block it falls in.
fn render(
    plugin: &mut LoadedPlugin,
    events: &[(usize, Event)],
    seconds: f64,
    input: Option<f32>,
) -> Vec<f32> {
    let frames = (seconds * f64::from(RATE)) as usize;
    let mut out = Vec::with_capacity(frames);
    let mut left = vec![0.0_f32; BLOCK];
    let mut right = vec![0.0_f32; BLOCK];
    let mut block_events = Vec::new();
    let mut start = 0;
    while start < frames {
        let len = BLOCK.min(frames - start);
        block_events.clear();
        for &(at, event) in events {
            if at >= start && at < start + len {
                let frame = (at - start) as u32;
                block_events.push(match event {
                    Event::NoteOn { key, velocity, .. } => Event::NoteOn {
                        frame,
                        key,
                        velocity,
                    },
                    Event::NoteOff { key, .. } => Event::NoteOff { frame, key },
                    Event::Param { id, value, .. } => Event::Param { frame, id, value },
                    Event::PitchBend { value, .. } => Event::PitchBend { frame, value },
                    Event::Controller { number, value, .. } => Event::Controller {
                        frame,
                        number,
                        value,
                    },
                    Event::Pressure { value, .. } => Event::Pressure { frame, value },
                });
            }
        }
        for (i, (l, r)) in left[..len]
            .iter_mut()
            .zip(right[..len].iter_mut())
            .enumerate()
        {
            let sample = input.map_or(0.0, |level| {
                level * ((start + i) as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin()
            });
            *l = sample;
            *r = sample;
        }
        let transport = Transport {
            playing: true,
            frames: start as i64,
            ..Transport::default()
        };
        plugin.processor.process(
            &transport,
            &block_events,
            &mut left[..len],
            &mut right[..len],
        );
        out.extend_from_slice(&left[..len]);
        plugin.handle.idle();
        start += len;
    }
    out
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

fn note(at: f64, key: u8) -> (usize, Event) {
    let frame = (at * f64::from(RATE)) as usize;
    (
        frame,
        Event::NoteOn {
            frame: 0,
            key,
            velocity: 0.8,
        },
    )
}

fn release(at: f64, key: u8) -> (usize, Event) {
    let frame = (at * f64::from(RATE)) as usize;
    (frame, Event::NoteOff { frame: 0, key })
}

const INSTRUMENTS: [(&str, Format); 4] = [
    ("surge/Surge XT.clap", Format::Clap),
    ("surge/Surge XT.vst3", Format::Vst3),
    ("dexed/Dexed.clap", Format::Clap),
    ("dexed/Dexed.vst3", Format::Vst3),
];

#[test]
fn instruments_play_the_notes_they_are_sent() {
    for (file, format) in INSTRUMENTS {
        let Some(info) = find(file, format, Kind::Instrument) else {
            return;
        };
        let mut plugin = load(&info, config()).unwrap();
        let sound = render(&mut plugin, &[note(0.2, 60), release(0.6, 60)], 1.0, None);
        let before = rms(&sound[..(0.15 * f64::from(RATE)) as usize]);
        let during =
            rms(&sound[(0.25 * f64::from(RATE)) as usize..(0.55 * f64::from(RATE)) as usize]);
        let peak = sound.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        eprintln!(
            "{} ({format:?}): before {before:.5}, during {during:.4}, peak {peak:.3}",
            info.name
        );
        assert!(before < 1e-3, "{}: silent before the note", info.name);
        assert!(during > 0.005, "{}: the note sounds", info.name);
        assert!(peak.is_finite() && peak < 1.0, "{}: no clipping", info.name);
        plugin.processor.stop();
        drop(plugin.processor);
    }
}

/// How well a sound matches itself `lag` frames later (fractional lags
/// read between frames), from -1 to 1.
fn self_match(samples: &[f32], lag: f32, window: usize) -> f32 {
    let whole = lag.floor() as usize;
    let part = lag - lag.floor();
    let (mut together, mut first, mut later) = (0.0_f32, 0.0_f32, 0.0_f32);
    for i in 0..window {
        let a = samples[i];
        let b = samples[i + whole] * (1.0 - part) + samples[i + whole + 1] * part;
        together += a * b;
        first += a * a;
        later += b * b;
    }
    together / (first * later).sqrt().max(1e-12)
}

/// The period of a sound, in frames: the shortest lag (between 1500 and
/// 70 Hz) at which it matches itself nearly as well as at its best.
fn period(samples: &[f32]) -> f32 {
    let (shortest, longest) = ((RATE / 1500) as usize, (RATE / 70) as usize);
    let window = samples.len() - longest - 2;
    let scores: Vec<f32> = (shortest..=longest)
        .map(|lag| self_match(samples, lag as f32, window))
        .collect();
    let best = scores.iter().copied().fold(f32::MIN, f32::max);
    let mut at = scores.iter().position(|&s| s > 0.9 * best).unwrap();
    while at + 1 < scores.len() && scores[at + 1] > scores[at] {
        at += 1;
    }
    (at + shortest) as f32
}

/// How far `bent` is from `still` in pitch, in semitones (within seven
/// either way): the shift whose period `bent` matches itself best at.
fn shift(still: &[f32], bent: &[f32]) -> f32 {
    let period = period(still);
    let window = bent.len() - (period * 1.6) as usize - 2;
    (-700..=700)
        .map(|cents| {
            let lag = period * 2.0_f32.powf(-cents as f32 / 1200.0);
            (cents, self_match(bent, lag, window))
        })
        .fold(
            (0, f32::MIN),
            |best, next| if next.1 > best.1 { next } else { best },
        )
        .0 as f32
        / 100.0
}

#[test]
fn the_pitch_wheel_bends_instruments() {
    for (file, format) in INSTRUMENTS {
        let Some(info) = find(file, format, Kind::Instrument) else {
            return;
        };
        let mut plugin = load(&info, config()).unwrap();
        let at = |seconds: f64| (seconds * f64::from(RATE)) as usize;
        let wheel = |seconds: f64, value: f32| (at(seconds), Event::PitchBend { frame: 0, value });
        // A quarter of the way each way: half a semitone at the usual range,
        // three at the widest.
        let sound = render(
            &mut plugin,
            &[
                note(0.2, 57),
                wheel(0.8, 0.25),
                wheel(1.4, -0.25),
                wheel(2.0, 0.0),
                release(2.6, 57),
            ],
            2.7,
            None,
        );
        let still = &sound[at(0.4)..at(0.75)];
        let up = shift(still, &sound[at(1.0)..at(1.35)]);
        let down = shift(still, &sound[at(1.6)..at(1.95)]);
        let back = shift(still, &sound[at(2.2)..at(2.55)]);
        eprintln!(
            "{} ({format:?}): a quarter of the wheel up {up:+.2}, down {down:+.2}, back {back:+.2} semitones",
            info.name
        );
        assert!(up > 0.2, "{}: the wheel bends up", info.name);
        assert!(down < -0.2, "{}: the wheel bends down", info.name);
        assert!(back.abs() < 0.06, "{}: back at rest", info.name);
        plugin.processor.stop();
        drop(plugin.processor);
    }
}

/// The parameter of an instrument whose name contains `word`.
fn param(plugin: &mut LoadedPlugin, word: &str) -> u32 {
    plugin
        .handle
        .params()
        .into_iter()
        .find(|p| p.name.to_lowercase().contains(word))
        .unwrap_or_else(|| panic!("no parameter named like {word}"))
        .id
}

#[test]
fn a_parameter_changed_by_the_song_is_heard() {
    for (file, format) in &INSTRUMENTS[..2] {
        let Some(info) = find(file, *format, Kind::Instrument) else {
            return;
        };
        let mut plugin = load(&info, config()).unwrap();
        let volume = param(&mut plugin, "global volume");
        let loud = render(&mut plugin, &[note(0.1, 60), release(0.5, 60)], 0.8, None);
        let quiet_events = [
            (
                0,
                Event::Param {
                    frame: 0,
                    id: volume,
                    value: 0.1,
                },
            ),
            note(0.1, 60),
            release(0.5, 60),
        ];
        let quiet = render(&mut plugin, &quiet_events, 0.8, None);
        let window = (0.2 * f64::from(RATE)) as usize..(0.45 * f64::from(RATE)) as usize;
        let (loud, quiet) = (rms(&loud[window.clone()]), rms(&quiet[window]));
        eprintln!("{format:?}: volume at its default {loud:.4}, turned down {quiet:.5}");
        assert!(quiet < loud / 4.0, "turning the volume down is heard");
        plugin.processor.stop();
        drop(plugin.processor);
    }
}

#[test]
fn settings_come_back_after_saving() {
    for (file, format) in &INSTRUMENTS[..2] {
        let Some(info) = find(file, *format, Kind::Instrument) else {
            return;
        };
        let mut first = load(&info, config()).unwrap();
        let volume = param(&mut first, "global volume");
        // Turn the volume down, as the editor would, then save.
        render(
            &mut first,
            &[(
                0,
                Event::Param {
                    frame: 0,
                    id: volume,
                    value: 0.2,
                },
            )],
            0.1,
            None,
        );
        let saved_value = first.handle.param_value(volume).unwrap();
        let state = first.handle.save_state().unwrap();
        let reference = render(&mut first, &[note(0.05, 64), release(0.3, 64)], 0.5, None);
        first.processor.stop();
        drop(first.processor);
        drop(first.handle);

        let mut second = load(&info, config()).unwrap();
        second.handle.load_state(&state).unwrap();
        let restored_value = second.handle.param_value(volume).unwrap();
        let restored = render(&mut second, &[note(0.05, 64), release(0.3, 64)], 0.5, None);
        let (a, b) = (rms(&reference), rms(&restored));
        eprintln!(
            "{format:?}: state {} bytes; value {saved_value:.3} -> {restored_value:.3}; level {a:.4} -> {b:.4}",
            state.len()
        );
        assert!(
            (saved_value - restored_value).abs() < 1e-3,
            "the setting comes back"
        );
        assert!((a - b).abs() < a * 0.1 + 1e-4, "and sounds the same");
        second.processor.stop();
        drop(second.processor);
    }
}

#[test]
fn an_effect_changes_the_sound_passing_through_it() {
    for (file, format) in [
        ("surge/Surge XT Effects.clap", Format::Clap),
        ("surge/Surge XT Effects.vst3", Format::Vst3),
    ] {
        let Some(info) = find(file, format, Kind::Effect) else {
            return;
        };
        let mut plugin = load(&info, config()).unwrap();
        let out = render(&mut plugin, &[], 1.0, Some(0.25));
        let level = rms(&out[RATE as usize / 2..]);
        eprintln!(
            "{} ({format:?}): a 440 Hz tone at 0.177 rms comes out at {level:.4}",
            info.name
        );
        assert!(level > 0.01, "sound comes through the effect");
        assert!(out.iter().all(|s| s.is_finite() && s.abs() < 2.0));
        plugin.processor.stop();
        drop(plugin.processor);
    }
}

#[test]
fn editors_open_and_close() {
    if std::env::var_os("DISPLAY").is_none() && cfg!(target_os = "linux") {
        eprintln!("no display; skipped");
        return;
    }
    daw_plugin_host::prepare_display();
    for (file, format) in &INSTRUMENTS[..2] {
        let Some(info) = find(file, *format, Kind::Instrument) else {
            return;
        };
        let mut plugin = load(&info, config()).unwrap();
        assert!(plugin.handle.has_editor(), "{format:?}: it has an editor");
        plugin.handle.open_editor(&info.name).unwrap();
        assert!(plugin.handle.is_editor_open());
        let started = std::time::Instant::now();
        while started.elapsed().as_secs_f64() < 1.5 {
            render(&mut plugin, &[], 0.02, None);
        }
        assert!(plugin.handle.is_editor_open(), "it stays open");
        plugin.handle.close_editor();
        assert!(!plugin.handle.is_editor_open());
        plugin.processor.stop();
        drop(plugin.processor);
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 0 spike (b): host a CLAP and a VST3 instrument.
//!
//! For one plugin file, this checks the whole chain the brief asks about:
//! load it, send it notes and get audio back (checking the pitch), change a
//! parameter and save its state, restore that state into a fresh instance
//! and check it sounds the same, then open its editor window, keep it alive
//! for a few seconds and take a screenshot.
//!
//! Usage:
//! `spike-plugin-host <plugin.clap | plugin.vst3> [--editor SECONDS] [--out DIR]`
//!
//! Throwaway code: its findings are recorded in
//! docs/decisions/0009-phase-0-spike-results.md.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a test tool reports by printing"
)]
#![allow(
    missing_docs,
    reason = "throwaway spike; the items are described where they are used"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "audio sample arithmetic on small, known ranges"
)]

mod clap;
mod vst3;
mod window;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// The sample rate and block size every test runs at.
pub const SAMPLE_RATE: u32 = 48_000;
pub const BLOCK: usize = 256;

/// A note event at a frame offset from the start of a render.
#[derive(Clone, Copy, Debug)]
pub enum TimedEvent {
    NoteOn {
        frame: usize,
        key: u8,
        velocity: f32,
    },
    NoteOff {
        frame: usize,
        key: u8,
    },
}

impl TimedEvent {
    pub fn frame(self) -> usize {
        match self {
            Self::NoteOn { frame, .. } | Self::NoteOff { frame, .. } => frame,
        }
    }
}

/// A parameter as the plugin describes it.
pub struct ParamDescription {
    pub id: u32,
    pub name: String,
}

/// What the test needs from a hosted plugin, whatever its format.
///
/// This is deliberately close to what the real host layer will offer the
/// engine, so the spike also tests that one interface fits both formats.
pub trait HostedPlugin {
    /// Name, vendor, version and format.
    fn description(&self) -> String;
    /// Renders `frames` frames of stereo audio with the given note events.
    fn render(&mut self, events: &[TimedEvent], frames: usize) -> Result<[Vec<f32>; 2], String>;
    /// The plugin's parameters.
    fn params(&mut self) -> Vec<ParamDescription>;
    /// Sets a parameter, as a fraction (0 to 1) of its range, from the next render on.
    fn set_param(&mut self, id: u32, fraction: f64) -> Result<(), String>;
    /// The parameter's current value as a fraction of its range.
    fn param_fraction(&mut self, id: u32) -> Option<f64>;
    /// Saves everything needed to restore the plugin.
    fn save_state(&mut self) -> Result<Vec<u8>, String>;
    /// Restores a state saved by `save_state`.
    fn load_state(&mut self, data: &[u8]) -> Result<(), String>;
    /// Opens the editor inside `window`; returns the editor's size.
    fn open_editor(&mut self, window: &window::EditorWindow) -> Result<(u32, u32), String>;
    /// Housekeeping the plugin asked for on the main thread: callbacks,
    /// timers, file descriptors. Called often while an editor is open.
    fn idle(&mut self);
    /// Closes the editor.
    fn close_editor(&mut self);
}

struct Options {
    plugin: PathBuf,
    editor_seconds: Option<f64>,
    out: PathBuf,
}

fn parse_options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut plugin = None;
    let mut editor_seconds = None;
    let mut out = PathBuf::from("spike-output");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--editor" => {
                let value = args.next().ok_or("--editor needs a number of seconds")?;
                editor_seconds = Some(value.parse().map_err(|_| "--editor needs a number")?);
            }
            "--out" => out = PathBuf::from(args.next().ok_or("--out needs a folder")?),
            other if plugin.is_none() => plugin = Some(PathBuf::from(other)),
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    Ok(Options {
        plugin: plugin.ok_or("give the path of a .clap or .vst3 plugin")?,
        editor_seconds,
        out,
    })
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = std::fs::create_dir_all(&options.out) {
        eprintln!("error: cannot create {}: {error}", options.out.display());
        return ExitCode::FAILURE;
    }
    match run(&options) {
        Ok(0) => {
            println!("RESULT ok: every check passed");
            ExitCode::SUCCESS
        }
        Ok(failures) => {
            println!("RESULT failed: {failures} check(s) failed");
            ExitCode::FAILURE
        }
        Err(message) => {
            println!("RESULT failed: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Which format a path is, from its extension.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Clap,
    Vst3,
}

fn format_of(path: &Path) -> Result<Format, String> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("clap") => Ok(Format::Clap),
        Some(e) if e.eq_ignore_ascii_case("vst3") => Ok(Format::Vst3),
        _ => Err(format!(
            "{} is neither a .clap nor a .vst3 plugin",
            path.display()
        )),
    }
}

fn load(format: Format, path: &Path) -> Result<Box<dyn HostedPlugin>, String> {
    match format {
        Format::Clap => clap::ClapPlugin::load(path).map(|p| Box::new(p) as Box<dyn HostedPlugin>),
        Format::Vst3 => vst3::Vst3Plugin::load(path).map(|p| Box::new(p) as Box<dyn HostedPlugin>),
    }
}

/// Two notes: middle C, then the E above it.
fn phrase() -> (Vec<TimedEvent>, usize) {
    let s = |seconds: f64| (seconds * f64::from(SAMPLE_RATE)) as usize;
    let events = vec![
        TimedEvent::NoteOn {
            frame: s(0.10),
            key: 60,
            velocity: 0.8,
        },
        TimedEvent::NoteOff {
            frame: s(0.90),
            key: 60,
        },
        TimedEvent::NoteOn {
            frame: s(1.10),
            key: 64,
            velocity: 0.8,
        },
        TimedEvent::NoteOff {
            frame: s(1.90),
            key: 64,
        },
    ];
    (events, s(2.6))
}

struct Checks {
    failures: usize,
}

impl Checks {
    fn check(&mut self, name: &str, passed: bool, detail: &str) {
        let verdict = if passed { "PASS" } else { "FAIL" };
        println!("CHECK {verdict} {name}: {detail}");
        if !passed {
            self.failures += 1;
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "a linear test script reads best in one place"
)]
fn run(options: &Options) -> Result<usize, String> {
    let format = format_of(&options.plugin)?;
    let tag = match format {
        Format::Clap => "clap",
        Format::Vst3 => "vst3",
    };
    let mut checks = Checks { failures: 0 };

    // 1. Load.
    if options.editor_seconds.is_some() {
        window::prepare_display();
    }
    let started = Instant::now();
    let mut plugin = load(format, &options.plugin)?;
    println!(
        "== Loaded in {:.0} ms: {}",
        ms(started.elapsed()),
        plugin.description()
    );

    // 2. Notes in, audio out, at the right pitch.
    let (events, frames) = phrase();
    let first = plugin.render(&events, frames)?;
    write_wav(&options.out.join(format!("{tag}-1-default.wav")), &first)?;
    let silence_before = rms(&first[0][window(0.0, 0.09)]);
    let level_c = rms(&first[0][window(0.30, 0.80)]);
    let level_e = rms(&first[0][window(1.30, 1.80)]);
    checks.check(
        "notes produce sound",
        level_c > 0.001 && level_e > 0.001,
        &format!(
            "level during the notes {level_c:.4} and {level_e:.4}, before them {silence_before:.5}"
        ),
    );
    let pitch_c = estimate_pitch(&first[0][window(0.35, 0.75)]);
    let pitch_e = estimate_pitch(&first[0][window(1.35, 1.75)]);
    checks.check(
        "pitch of middle C",
        near(pitch_c, 261.63, 0.03),
        &format!("{pitch_c:.1} Hz (expected 261.6 Hz)"),
    );
    checks.check(
        "pitch of the E above",
        near(pitch_e, 329.63, 0.03),
        &format!("{pitch_e:.1} Hz (expected 329.6 Hz)"),
    );
    checks.check(
        "no clipping",
        peak(&first[0]) <= 1.0 && peak(&first[1]) <= 1.0,
        &format!("peak {:.3}", peak(&first[0]).max(peak(&first[1]))),
    );

    // 3. Change a parameter so the sound is clearly different.
    let params = plugin.params();
    println!("== {} parameters", params.len());
    let Some(target) = choose_volume_param(&params) else {
        return Err("found no volume parameter to change".to_owned());
    };
    let before = plugin.param_fraction(target.id);
    plugin.set_param(target.id, 0.25)?;
    let changed = plugin.render(&events, frames)?;
    write_wav(&options.out.join(format!("{tag}-2-changed.wav")), &changed)?;
    let after = plugin.param_fraction(target.id);
    let level_changed = rms(&changed[0][window(0.30, 0.80)]);
    checks.check(
        "a parameter change is heard",
        level_changed < level_c * 0.8,
        &format!(
            "`{}` from {:?} to {:?}: level {level_c:.4} -> {level_changed:.4}",
            target.name,
            before.map(|v| (v * 1000.0).round() / 1000.0),
            after.map(|v| (v * 1000.0).round() / 1000.0)
        ),
    );

    // 4. Save the state, restore it into a fresh instance, compare.
    let state = plugin.save_state()?;
    checks.check(
        "state saves",
        !state.is_empty(),
        &format!("{} bytes", state.len()),
    );
    drop(plugin);

    let mut restored = load(format, &options.plugin)?;
    restored.load_state(&state)?;
    let again = restored.render(&events, frames)?;
    write_wav(&options.out.join(format!("{tag}-3-restored.wav")), &again)?;
    let level_restored = rms(&again[0][window(0.30, 0.80)]);
    let ratio = level_restored / level_changed.max(1e-9);
    checks.check(
        "restored state sounds like the saved one",
        (0.85..=1.15).contains(&ratio),
        &format!(
            "level {level_restored:.4} vs {level_changed:.4} when saved (default was {level_c:.4})"
        ),
    );
    let restored_value = restored.param_fraction(target.id);
    checks.check(
        "restored parameter value",
        restored_value.is_some_and(|v| (v - after.unwrap_or(0.25)).abs() < 0.01),
        &format!("{restored_value:?}"),
    );

    // 5. The editor window.
    if let Some(seconds) = options.editor_seconds {
        editor_test(restored.as_mut(), seconds, &options.out, tag, &mut checks);
    }
    drop(restored);
    println!("== Unloaded cleanly");
    Ok(checks.failures)
}

fn editor_test(
    plugin: &mut dyn HostedPlugin,
    seconds: f64,
    out: &Path,
    tag: &str,
    checks: &mut Checks,
) {
    let window = match window::EditorWindow::new("Plugin editor (spike)", 800, 600) {
        Ok(window) => window,
        Err(message) => {
            checks.check(
                "editor window",
                false,
                &format!("could not create a window: {message}"),
            );
            return;
        }
    };
    let size = match plugin.open_editor(&window) {
        Ok(size) => size,
        Err(message) => {
            checks.check("editor opens", false, &message);
            return;
        }
    };
    window.resize(size.0, size.1);
    checks.check(
        "editor opens",
        true,
        &format!("{} x {} pixels", size.0, size.1),
    );

    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < deadline {
        plugin.idle();
        window.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    let shot = out.join(format!("{tag}-editor.bmp"));
    match window.screenshot(&shot) {
        Ok(drawn) => checks.check(
            "editor draws itself",
            drawn > 0.5,
            &format!(
                "{:.0}% of the window drawn by the plugin; saved {}",
                drawn * 100.0,
                shot.display()
            ),
        ),
        Err(message) => checks.check("editor draws itself", false, &message),
    }
    plugin.close_editor();
    // Let the plugin finish closing its window.
    let settle = Instant::now() + Duration::from_millis(300);
    while Instant::now() < settle {
        plugin.idle();
        window.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(window);
    checks.check("editor closes", true, "closed without errors");
}

/// Picks the parameter most likely to be the overall output volume.
fn choose_volume_param(params: &[ParamDescription]) -> Option<&ParamDescription> {
    let lower = |p: &ParamDescription| p.name.to_lowercase();
    let preferences: [&dyn Fn(&str) -> bool; 4] = [
        &|n| n == "global volume" || n == "master volume",
        &|n| n.contains("global") && n.contains("volume"),
        &|n| n == "volume",
        &|n| n.contains("volume") || n.contains("gain"),
    ];
    preferences
        .iter()
        .find_map(|matches| params.iter().find(|p| matches(&lower(p))))
}

fn window(from_seconds: f64, to_seconds: f64) -> std::ops::Range<usize> {
    let s = |seconds: f64| (seconds * f64::from(SAMPLE_RATE)) as usize;
    s(from_seconds)..s(to_seconds)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |max, s| max.max(s.abs()))
}

fn near(value: f32, target: f32, tolerance: f32) -> bool {
    (value - target).abs() <= target * tolerance
}

/// Estimates the fundamental frequency of a steady tone by autocorrelation:
/// the delay at which the signal best matches itself is one period.
fn estimate_pitch(samples: &[f32]) -> f32 {
    let rate = SAMPLE_RATE as f32;
    let min_lag = (rate / 1500.0) as usize;
    let max_lag = ((rate / 50.0) as usize).min(samples.len() / 2);
    let correlation = |lag: usize| -> f32 {
        let n = samples.len() - lag;
        let mut sum = 0.0;
        let mut energy = 0.0;
        for i in 0..n {
            sum += samples[i] * samples[i + lag];
            energy += samples[i] * samples[i] + samples[i + lag] * samples[i + lag];
        }
        if energy > 0.0 {
            2.0 * sum / energy
        } else {
            0.0
        }
    };
    let values: Vec<f32> = (0..=max_lag)
        .map(|lag| if lag < min_lag { 0.0 } else { correlation(lag) })
        .collect();
    // The first peak that comes close to the best one avoids picking a
    // multiple of the period.
    let best = values.iter().copied().fold(0.0, f32::max);
    let mut lag = min_lag;
    for candidate in min_lag + 1..max_lag {
        let v = values[candidate];
        if v >= best * 0.9 && v >= values[candidate - 1] && v >= values[candidate + 1] {
            lag = candidate;
            break;
        }
    }
    // Refine between samples with a parabola through the peak.
    let (a, b, c) = (values[lag - 1], values[lag], values[lag + 1]);
    let denominator = a - 2.0 * b + c;
    let offset = if denominator.abs() > 1e-12 {
        0.5 * (a - c) / denominator
    } else {
        0.0
    };
    rate / (lag as f32 + offset)
}

/// Writes 32-bit float stereo WAV, so the owner can listen to every render.
fn write_wav(path: &Path, channels: &[Vec<f32>; 2]) -> Result<(), String> {
    let frames = channels[0].len();
    let data_bytes = (frames * 2 * 4) as u32;
    let mut bytes = Vec::with_capacity(44 + data_bytes as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&3_u16.to_le_bytes()); // IEEE float
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 8).to_le_bytes());
    bytes.extend_from_slice(&8_u16.to_le_bytes());
    bytes.extend_from_slice(&32_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    for frame in 0..frames {
        bytes.extend_from_slice(&channels[0][frame].to_le_bytes());
        bytes.extend_from_slice(&channels[1][frame].to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// Splits a render into blocks and hands each block its events, with frame
/// offsets relative to the block. Shared by both formats.
pub fn for_each_block(
    events: &[TimedEvent],
    frames: usize,
    mut process: impl FnMut(usize, usize, &[TimedEvent]) -> Result<(), String>,
) -> Result<(), String> {
    let mut block_events = Vec::with_capacity(events.len());
    let mut start = 0;
    while start < frames {
        let len = BLOCK.min(frames - start);
        block_events.clear();
        for event in events {
            let frame = event.frame();
            if frame >= start && frame < start + len {
                block_events.push(match *event {
                    TimedEvent::NoteOn { key, velocity, .. } => TimedEvent::NoteOn {
                        frame: frame - start,
                        key,
                        velocity,
                    },
                    TimedEvent::NoteOff { key, .. } => TimedEvent::NoteOff {
                        frame: frame - start,
                        key,
                    },
                });
            }
        }
        process(start, len, &block_events)?;
        start += len;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SAMPLE_RATE, estimate_pitch};

    #[test]
    fn pitch_estimate_finds_a_sawtooth_fundamental() {
        for frequency in [110.0_f32, 261.63, 329.63, 880.0] {
            let samples: Vec<f32> = (0..9600)
                .map(|i| {
                    let phase = (i as f32 * frequency / SAMPLE_RATE as f32).fract();
                    2.0 * phase - 1.0
                })
                .collect();
            let estimate = estimate_pitch(&samples);
            assert!(
                (estimate - frequency).abs() < frequency * 0.01,
                "{frequency} Hz estimated as {estimate} Hz"
            );
        }
    }
}

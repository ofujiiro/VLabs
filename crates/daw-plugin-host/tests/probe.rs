// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugins someone reports, probed as the program hosts them: every CLAP
//! and VST3 instrument in the folder `VLABS_PROBE_PLUGINS` names is loaded
//! and played across the keyboard at once, after a few seconds, and with
//! its window open, and what each note gives is reported. Some plugins stay
//! silent until something in their window is clicked (a start screen):
//! `VLABS_PROBE_HOLD` keeps the window open that many seconds first. Without
//! `VLABS_PROBE_PLUGINS` this test passes quietly (the plugins are never
//! part of the program).

#![allow(
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "test code: it reports what it found"
)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use daw_plugin_host::{AudioConfig, Event, Format, Kind, LoadedPlugin, Transport, load, scan};

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

fn plugins(folder: &Path, found: &mut Vec<(PathBuf, Format)>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(format) = Format::of_path(&path) {
            found.push((path, format));
        } else if path.is_dir() {
            plugins(&path, found);
        }
    }
}

/// Lets `seconds` pass in real time, the audio side processing silence and
/// the main side looked after as the program does between frames (a plugin
/// may load its sounds in the background, or on its timers).
fn wait(plugin: &mut LoadedPlugin, seconds: f64, clock: &mut i64) {
    let mut left = vec![0.0_f32; BLOCK];
    let mut right = vec![0.0_f32; BLOCK];
    let started = Instant::now();
    while started.elapsed().as_secs_f64() < seconds {
        for _ in 0..3 {
            left.fill(0.0);
            right.fill(0.0);
            let transport = Transport {
                playing: true,
                frames: *clock,
                ..Transport::default()
            };
            plugin
                .processor
                .process(&transport, &[], &mut left, &mut right);
            *clock += BLOCK as i64;
        }
        plugin.handle.idle();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}

/// Plays `key` for half a second (after `warm_up` seconds of silence) and
/// gives the loudest sample, both channels.
fn play(plugin: &mut LoadedPlugin, key: u8, warm_up: f64, clock: &mut i64) -> f32 {
    let mut left = vec![0.0_f32; BLOCK];
    let mut right = vec![0.0_f32; BLOCK];
    let mut loudest = 0.0_f32;
    let warm = (warm_up * f64::from(RATE)) as usize / BLOCK;
    let blocks = warm + (0.7 * f64::from(RATE)) as usize / BLOCK;
    for block in 0..blocks {
        let events: Vec<Event> = if block == warm {
            vec![Event::NoteOn {
                frame: 0,
                key,
                velocity: 0.9,
            }]
        } else if block == warm + (0.5 * f64::from(RATE)) as usize / BLOCK {
            vec![Event::NoteOff { frame: 0, key }]
        } else {
            Vec::new()
        };
        left.fill(0.0);
        right.fill(0.0);
        let transport = Transport {
            playing: true,
            frames: *clock,
            ..Transport::default()
        };
        plugin
            .processor
            .process(&transport, &events, &mut left, &mut right);
        *clock += BLOCK as i64;
        if block >= warm {
            loudest = left
                .iter()
                .chain(&right)
                .fold(loudest, |m, s| m.max(s.abs()));
        }
        plugin.handle.idle();
    }
    loudest
}

#[test]
fn reported_plugins_are_loaded_and_played() {
    let Some(folder) = std::env::var_os("VLABS_PROBE_PLUGINS") else {
        return;
    };
    // As the program does before any plugin window opens.
    daw_plugin_host::prepare_display();
    let mut found = Vec::new();
    plugins(Path::new(&folder), &mut found);
    found.sort();
    for (path, format) in found {
        let described = match scan::describe(format, &path) {
            Ok(described) => described,
            Err(error) => {
                eprintln!("{}: not described: {error:?}", path.display());
                continue;
            }
        };
        for info in described.into_iter().filter(|p| p.kind == Kind::Instrument) {
            let started = Instant::now();
            let mut plugin = match load(
                &info,
                AudioConfig {
                    sample_rate: RATE,
                    max_block: BLOCK,
                },
            ) {
                Ok(plugin) => plugin,
                Err(error) => {
                    eprintln!("{} ({format:?}): not loaded: {error}", info.name);
                    continue;
                }
            };
            eprintln!(
                "{} ({format:?}) by {}: loaded in {:.2} s",
                info.name,
                info.vendor,
                started.elapsed().as_secs_f64()
            );
            let mut clock = 0;
            // As a person plays: a few seconds after loading, then with the
            // plugin's window open.
            let listen = |plugin: &mut LoadedPlugin, clock: &mut i64, when: &str| {
                let mut heard = Vec::new();
                for key in [36_u8, 38, 42, 48, 60, 72] {
                    let loudest = play(plugin, key, 0.1, clock);
                    heard.push(format!("{key}: {loudest:.4}"));
                }
                eprintln!("  {when}: {}", heard.join(", "));
            };
            listen(&mut plugin, &mut clock, "at once");
            wait(&mut plugin, 4.0, &mut clock);
            listen(&mut plugin, &mut clock, "after 4 s");
            // Asking a VST3 plugin whether it has an editor makes one: how
            // long that takes shows how costly it would be to ask often.
            let asked = Instant::now();
            let has_editor = plugin.handle.has_editor();
            eprintln!(
                "  has an editor: {has_editor} (asked in {:.0} ms)",
                asked.elapsed().as_secs_f64() * 1000.0
            );
            if has_editor {
                let opening = Instant::now();
                match plugin.handle.open_editor(&info.name) {
                    Ok(()) => {
                        eprintln!(
                            "  window opened in {:.0} ms",
                            opening.elapsed().as_secs_f64() * 1000.0
                        );
                        // Time to click through what the window asks first
                        // (VLABS_PROBE_HOLD seconds), as a person does.
                        let hold = std::env::var("VLABS_PROBE_HOLD")
                            .ok()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(4.0);
                        wait(&mut plugin, hold, &mut clock);
                        listen(&mut plugin, &mut clock, "with its window open");
                        plugin.handle.close_editor();
                    }
                    Err(error) => eprintln!("  the window did not open: {error}"),
                }
            }
            plugin.processor.stop();
            drop(plugin.processor);
        }
    }
}

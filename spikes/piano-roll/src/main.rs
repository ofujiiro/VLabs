// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 0 spike (c): a piano roll with 10,000 notes that scrolls and zooms
//! smoothly, drawn with egui.
//!
//! - `spike-piano-roll` opens a window to try it: scroll with the wheel,
//!   Shift+wheel or drag with the right button to move, Ctrl+wheel (or
//!   pinch) to zoom in time, Ctrl+Alt+wheel to zoom the keys.
//! - `spike-piano-roll --bench` measures, without a window, how long each
//!   frame takes to build and tessellate while scrolling and zooming through
//!   egui's real input path, including the worst case where all 10,000 notes
//!   are on screen at once.
//! - `--autopilot SECONDS` animates scrolling and zooming in the window and
//!   reports the real frame rate; `--screenshot FILE` saves what is on screen.
//!
//! Throwaway code: findings are in docs/decisions/0009-phase-0-spike-results.md.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a measuring tool reports by printing"
)]
#![allow(missing_docs, reason = "throwaway spike")]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "screen coordinates and statistics"
)]

mod roll;

use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{
    Event, FontData, FontDefinitions, FontFamily, Modifiers, MouseWheelSource, MouseWheelUnit,
    RawInput, TouchPhase, pos2, vec2,
};

use roll::{PianoRoll, generate_notes};

const NOTE_COUNT: usize = 10_000;

/// A frame must be built in well under the 16.7 ms a 60 Hz screen allows,
/// leaving room for the rest of the interface and for drawing.
const BUDGET_P95_MS: f64 = 8.0;

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    let regular = include_bytes!("../../../assets/fonts/Go-Regular.ttf");
    let mono = include_bytes!("../../../assets/fonts/Go-Mono.ttf");
    fonts
        .font_data
        .insert("Go".to_owned(), Arc::new(FontData::from_static(regular)));
    fonts
        .font_data
        .insert("Go Mono".to_owned(), Arc::new(FontData::from_static(mono)));
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["Go".to_owned()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["Go Mono".to_owned()]);
    fonts
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value_after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--bench") {
        return bench();
    }
    let autopilot = value_after("--autopilot").and_then(|v| v.parse::<f64>().ok());
    let screenshot = value_after("--screenshot");
    match run_window(autopilot, screenshot) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("RESULT failed: {message}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// Headless benchmark

struct Scenario {
    name: &'static str,
    frames: usize,
    /// Input for frame `i`.
    input: fn(usize) -> Vec<Event>,
    /// Called once before the scenario starts.
    setup: fn(&mut PianoRoll, f32),
}

fn wheel(delta: egui::Vec2, modifiers: Modifiers) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta,
        phase: TouchPhase::Move,
        source: MouseWheelSource::Unknown,
        modifiers,
    }
}

#[allow(clippy::too_many_lines, reason = "a linear measuring script")]
fn bench() -> ExitCode {
    let (width, height) = (1600.0, 900.0);
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts());
    let mut roll = PianoRoll::new(generate_notes(NOTE_COUNT));
    println!(
        "== {} notes over {} bars, window {width} x {height} points",
        roll.note_count(),
        roll.song_length() / (4 * roll::PPQ)
    );

    let scenarios = [
        Scenario {
            name: "scroll through the song (normal zoom)",
            frames: 400,
            input: |_| vec![wheel(vec2(-25.0, 0.0), Modifiers::NONE)],
            setup: |_, _| {},
        },
        Scenario {
            name: "scroll up and down the keys",
            frames: 200,
            input: |i| {
                vec![wheel(
                    vec2(0.0, if (i / 50) % 2 == 0 { 20.0 } else { -20.0 }),
                    Modifiers::NONE,
                )]
            },
            setup: |_, _| {},
        },
        Scenario {
            name: "zoom out to the whole song and back in",
            frames: 400,
            input: |i| {
                let delta = if i < 200 { -12.0 } else { 12.0 };
                vec![wheel(vec2(0.0, delta), Modifiers::CTRL)]
            },
            setup: |_, _| {},
        },
        Scenario {
            name: "whole song on screen, scrolling keys (worst case)",
            frames: 300,
            input: |i| {
                vec![wheel(
                    vec2(0.0, if (i / 75) % 2 == 0 { 8.0 } else { -8.0 }),
                    Modifiers::NONE,
                )]
            },
            setup: |roll, width| roll.show_everything(width),
        },
    ];

    let mut worst_p95 = 0.0_f64;
    let start = Instant::now();
    let mut frame_index = 0_u64;
    for scenario in &scenarios {
        (scenario.setup)(&mut roll, width);
        let mut times = Vec::with_capacity(scenario.frames);
        let mut visible = Vec::with_capacity(scenario.frames);
        let mut vertices = 0;
        for i in 0..scenario.frames {
            let mut events = vec![Event::PointerMoved(pos2(width * 0.6, height * 0.5))];
            events.extend((scenario.input)(i));
            let input = RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    pos2(0.0, 0.0),
                    vec2(width, height),
                )),
                time: Some(start.elapsed().as_secs_f64()),
                predicted_dt: 1.0 / 60.0,
                events,
                ..RawInput::default()
            };
            let began = Instant::now();
            let mut output = ctx.run_ui(input, |ui| roll.ui(ui));
            let shapes = std::mem::take(&mut output.shapes);
            let primitives = ctx.tessellate(shapes, output.pixels_per_point);
            // No renderer here: font texture updates are dropped on purpose.
            output.textures_delta.clear();
            let elapsed = began.elapsed();
            vertices = primitives
                .iter()
                .map(|p| match &p.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                    egui::epaint::Primitive::Callback(_) => 0,
                })
                .sum::<usize>();
            frame_index += 1;
            // The first frames build font atlases; they are not steady state.
            if frame_index > 5 {
                times.push(elapsed.as_secs_f64() * 1000.0);
            }
            visible.push(roll.stats.visible_notes);
        }
        times.sort_by(f64::total_cmp);
        let p = |fraction: f64| times[((times.len() - 1) as f64 * fraction).round() as usize];
        let max_visible = visible.iter().copied().max().unwrap_or(0);
        println!(
            "SCENARIO {}: frame build+tessellate p50 {:.2} ms, p95 {:.2} ms, max {:.2} ms; up to {max_visible} notes visible; {vertices} vertices in the last frame",
            scenario.name,
            p(0.5),
            p(0.95),
            p(1.0)
        );
        worst_p95 = worst_p95.max(p(0.95));
    }
    if worst_p95 <= BUDGET_P95_MS {
        println!("RESULT ok: worst p95 {worst_p95:.2} ms (budget {BUDGET_P95_MS} ms)");
        ExitCode::SUCCESS
    } else {
        println!(
            "RESULT failed: worst p95 {worst_p95:.2} ms is over the {BUDGET_P95_MS} ms budget"
        );
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------------------------
// Window

struct App {
    roll: PianoRoll,
    autopilot: Option<f64>,
    screenshot: Option<String>,
    started: Instant,
    frames: Vec<f64>,
    last_frame: Instant,
    screenshot_requested: bool,
    done: Arc<Mutex<bool>>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        self.frames
            .push(now.duration_since(self.last_frame).as_secs_f64() * 1000.0);
        self.last_frame = now;
        let elapsed = self.started.elapsed().as_secs_f64();

        if let Some(seconds) = self.autopilot {
            // Scroll and zoom continuously, like an impatient user.
            self.roll.autopilot(elapsed);
            if elapsed > seconds && !self.screenshot_requested {
                report_frames(&self.frames);
                self.finish(&ctx);
            }
        } else if self.screenshot.is_some() && elapsed > 2.0 && !self.screenshot_requested {
            self.finish(&ctx);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.roll.ui(ui));

        let fps_text = format!(
            "{} notes, {} visible  |  {:.1} ms/frame",
            self.roll.note_count(),
            self.roll.stats.visible_notes,
            self.frames.iter().rev().take(30).sum::<f64>() / self.frames.len().clamp(1, 30) as f64
        );
        egui::Area::new(egui::Id::unique("stats"))
            .anchor(egui::Align2::RIGHT_TOP, vec2(-12.0, 4.0))
            .show(&ctx, |ui| ui.label(fps_text));
        ctx.request_repaint();
        if *self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl App {
    fn finish(&mut self, ctx: &egui::Context) {
        self.screenshot_requested = true;
        let Some(path) = self.screenshot.clone() else {
            *self
                .done
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
            return;
        };
        let done = Arc::clone(&self.done);
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
            egui::ScreenshotCallback::new(move |image| {
                match write_bmp(&path, &image) {
                    Ok(()) => println!("== Screenshot saved to {path}"),
                    Err(e) => eprintln!("could not save the screenshot: {e}"),
                }
                *done
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
            }),
        ));
    }
}

fn report_frames(frames: &[f64]) {
    let mut steady: Vec<f64> = frames.iter().copied().skip(10).collect();
    if steady.is_empty() {
        return;
    }
    steady.sort_by(f64::total_cmp);
    let p = |fraction: f64| steady[((steady.len() - 1) as f64 * fraction).round() as usize];
    println!(
        "WINDOW frames {}: interval p50 {:.2} ms ({:.0} fps), p95 {:.2} ms, max {:.2} ms",
        steady.len(),
        p(0.5),
        1000.0 / p(0.5),
        p(0.95),
        p(1.0)
    );
}

fn write_bmp(path: &str, image: &egui::ColorImage) -> Result<(), String> {
    let [width, height] = image.size;
    let row_bytes = (width * 3).div_ceil(4) * 4;
    let image_bytes = row_bytes * height;
    let mut bytes = Vec::with_capacity(54 + image_bytes);
    let u32le = |v: usize| (v as u32).to_le_bytes();
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&u32le(54 + image_bytes));
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&u32le(54));
    bytes.extend_from_slice(&u32le(40));
    bytes.extend_from_slice(&u32le(width));
    bytes.extend_from_slice(&u32le(height));
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&24_u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&u32le(image_bytes));
    bytes.extend_from_slice(&[0; 16]);
    for y in (0..height).rev() {
        for x in 0..width {
            let pixel = image.pixels[y * width + x];
            bytes.extend_from_slice(&[pixel.b(), pixel.g(), pixel.r()]);
        }
        bytes.resize(bytes.len() + row_bytes - width * 3, 0);
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

fn run_window(autopilot: Option<f64>, screenshot: Option<String>) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 860.0])
            .with_title("Piano roll spike: 10,000 notes"),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "piano-roll-spike",
        options,
        Box::new(move |creation| {
            creation.egui_ctx.set_fonts(fonts());
            Ok(Box::new(App {
                roll: PianoRoll::new(generate_notes(NOTE_COUNT)),
                autopilot,
                screenshot,
                started: Instant::now(),
                frames: Vec::new(),
                last_frame: Instant::now(),
                screenshot_requested: false,
                done: Arc::new(Mutex::new(false)),
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    // Give a pending screenshot callback time to run.
    std::thread::sleep(Duration::from_millis(100));
    Ok(())
}

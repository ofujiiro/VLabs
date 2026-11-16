// SPDX-License-Identifier: GPL-3.0-or-later

//! A custom-drawn piano roll: keys down the side, time across, notes as
//! blocks. Only what is visible is drawn, and all notes go into a single
//! triangle mesh, so the cost per frame depends on what is on screen, not on
//! how many notes the song has.

use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

/// Ticks per quarter note (decision 0007).
pub const PPQ: i64 = 2_882_880;

/// One note: start and length in ticks, MIDI key, velocity 0 to 1.
#[derive(Clone, Copy, Debug)]
pub struct Note {
    pub start: i64,
    pub length: i64,
    pub key: u8,
    pub velocity: f32,
}

impl Note {
    fn end(&self) -> i64 {
        self.start + self.length
    }
}

/// Generates `count` notes of plausible music: a walking bass, chords and a
/// melody with triplets and syncopation, deterministically.
pub fn generate_notes(count: usize) -> Vec<Note> {
    let mut seed: u64 = 0x5EED_1234_ABCD_0001;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let scale = [0, 2, 3, 5, 7, 8, 10]; // natural minor
    let sixteenth = PPQ / 4;
    let triplet = PPQ / 3;
    let mut notes = Vec::with_capacity(count);
    let mut bar: i64 = 0;
    while notes.len() < count {
        let bar_start = bar * 4 * PPQ;
        let root = 45 + scale[(bar as usize * 3) % 7] as u8; // bass around A2
        // Bass: four quarter notes.
        for beat in 0..4 {
            notes.push(Note {
                start: bar_start + beat * PPQ,
                length: PPQ - sixteenth,
                key: root,
                velocity: 0.8,
            });
        }
        // Chord: three notes, two per bar.
        for half in 0..2 {
            for (i, interval) in [12_u8, 15, 19].iter().enumerate() {
                notes.push(Note {
                    start: bar_start + half * 2 * PPQ,
                    length: 2 * PPQ - sixteenth,
                    key: root + interval + (i as u8 % 2),
                    velocity: 0.55,
                });
            }
        }
        // Melody: sixteenths and triplets.
        let mut t = 0;
        while t < 4 * PPQ {
            let use_triplet = random() % 5 == 0;
            let step = if use_triplet {
                triplet
            } else {
                sixteenth * (1 + (random() % 3) as i64)
            };
            if random() % 4 != 0 {
                let degree = (random() % 14) as usize;
                let key = 69 + scale[degree % 7] as u8 + if degree >= 7 { 12 } else { 0 };
                notes.push(Note {
                    start: bar_start + t,
                    length: step,
                    key,
                    velocity: 0.4 + (random() % 60) as f32 / 100.0,
                });
            }
            t += step;
        }
        bar += 1;
    }
    notes.truncate(count);
    notes.sort_by_key(|n| (n.start, n.key));
    notes
}

/// Numbers from the last frame, for the overlay and the benchmark.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    pub visible_notes: usize,
}

pub struct PianoRoll {
    notes: Vec<Note>,
    longest_note: i64,
    /// Tick at the left edge of the grid.
    left_tick: f64,
    /// Horizontal zoom.
    ticks_per_point: f64,
    /// Key at the top edge of the grid (fractional, for smooth scrolling).
    top_key: f32,
    /// Vertical zoom.
    key_height: f32,
    beats_per_bar: i64,
    pub stats: FrameStats,
}

const KEYBOARD_WIDTH: f32 = 56.0;
const RULER_HEIGHT: f32 = 26.0;

impl PianoRoll {
    pub fn new(notes: Vec<Note>) -> Self {
        let longest_note = notes.iter().map(|n| n.length).max().unwrap_or(0);
        Self {
            notes,
            longest_note,
            left_tick: 0.0,
            ticks_per_point: (PPQ as f64) / 40.0, // a quarter note is 40 points wide
            top_key: 84.0,
            key_height: 14.0,
            beats_per_bar: 4,
            stats: FrameStats::default(),
        }
    }

    pub fn note_count(&self) -> usize {
        self.notes.len()
    }

    /// Ticks covered by the whole song.
    pub fn song_length(&self) -> i64 {
        self.notes.iter().map(Note::end).max().unwrap_or(0)
    }

    /// Zooms horizontally so the whole song fits in `width` points.
    pub fn show_everything(&mut self, width: f32) {
        self.left_tick = 0.0;
        self.ticks_per_point =
            self.song_length() as f64 / f64::from((width - KEYBOARD_WIDTH).max(1.0));
    }

    /// Moves the view as if someone were scrolling and zooming continuously:
    /// used by the window's autopilot to measure real frame rates.
    pub fn autopilot(&mut self, seconds: f64) {
        let zoom = 2.0_f64.powf(4.0 * (seconds * 0.5).sin()); // 16x in and out
        self.ticks_per_point = PPQ as f64 / 40.0 * zoom;
        self.left_tick = seconds * PPQ as f64 * 6.0;
        self.top_key = 84.0 + 18.0 * (seconds * 0.8).sin() as f32;
    }

    #[allow(
        clippy::too_many_lines,
        reason = "spike: one drawing routine, read top to bottom"
    )]
    pub fn ui(&mut self, ui: &mut Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let grid = Rect::from_min_max(
            pos2(rect.left() + KEYBOARD_WIDTH, rect.top() + RULER_HEIGHT),
            rect.max,
        );
        self.handle_input(ui, &response, grid);

        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        let dark = visuals.dark_mode;
        let background = if dark {
            Color32::from_gray(28)
        } else {
            Color32::from_gray(246)
        };
        let black_key_row = if dark {
            Color32::from_gray(22)
        } else {
            Color32::from_gray(232)
        };
        let line = if dark {
            Color32::from_gray(44)
        } else {
            Color32::from_gray(214)
        };
        let beat_line = if dark {
            Color32::from_gray(58)
        } else {
            Color32::from_gray(196)
        };
        let bar_line = if dark {
            Color32::from_gray(84)
        } else {
            Color32::from_gray(160)
        };
        let text = visuals.text_color();

        painter.rect_filled(rect, 0.0, background);

        // Key rows.
        let first_key = (self.top_key - grid.height() / self.key_height)
            .floor()
            .max(0.0) as i32;
        let last_key = (self.top_key.ceil() as i32).min(127);
        let grid_painter = ui.painter_at(grid);
        for key in first_key..=last_key {
            let y = self.key_to_y(grid, key as f32 + 1.0);
            let row =
                Rect::from_min_size(pos2(grid.left(), y), vec2(grid.width(), self.key_height));
            if is_black_key(key as u8) {
                grid_painter.rect_filled(row, 0.0, black_key_row);
            }
            if key % 12 == 0 {
                grid_painter.hline(grid.x_range(), row.bottom(), Stroke::new(1.0, line));
            }
        }

        // Vertical lines: bars always, beats and steps when there is room.
        let left = self.left_tick;
        let right = self.left_tick + f64::from(grid.width()) * self.ticks_per_point;
        let bar_ticks = self.beats_per_bar * PPQ;
        let points_per_beat = PPQ as f64 / self.ticks_per_point;
        // Level of detail: never draw lines closer than 8 points apart. When
        // even bars are that close, draw every 2nd, 4th, 8th... bar.
        let step_ticks = if points_per_beat / 4.0 >= 8.0 {
            PPQ / 4
        } else if points_per_beat >= 8.0 {
            PPQ
        } else {
            let points_per_bar = bar_ticks as f64 / self.ticks_per_point;
            let mut bars = 1;
            while points_per_bar * f64::from(bars) < 8.0 {
                bars *= 2;
            }
            bar_ticks * i64::from(bars)
        };
        let mut tick = (left / step_ticks as f64).floor() as i64 * step_ticks;
        let bar_label_every =
            ((60.0 / (bar_ticks as f64 / self.ticks_per_point)).ceil() as i64).max(1);
        while (tick as f64) <= right {
            let x = self.tick_to_x(grid, tick);
            let color = if tick % bar_ticks == 0 {
                bar_line
            } else if tick % PPQ == 0 {
                beat_line
            } else {
                line
            };
            if x >= grid.left() {
                grid_painter.vline(x, grid.y_range(), Stroke::new(1.0, color));
            }
            if tick % bar_ticks == 0 {
                let bar = tick / bar_ticks;
                if bar % bar_label_every == 0 && x >= grid.left() {
                    painter.text(
                        pos2(x + 4.0, rect.top() + RULER_HEIGHT / 2.0),
                        Align2::LEFT_CENTER,
                        format!("{}", bar + 1),
                        FontId::proportional(12.0),
                        text,
                    );
                }
            }
            tick += step_ticks;
        }

        // Notes, as one mesh. Notes are sorted by start, so the visible ones
        // are found by binary search; notes that started earlier but are
        // still sounding are caught by looking back by the longest length.
        let from = left as i64 - self.longest_note;
        let first = self.notes.partition_point(|n| n.start < from);
        let mut mesh = Mesh::default();
        let mut visible = 0;
        let accent = Color32::from_rgb(92, 176, 255);
        for note in &self.notes[first..] {
            if note.start as f64 > right {
                break;
            }
            if (note.end() as f64) < left
                || i32::from(note.key) < first_key
                || i32::from(note.key) > last_key
            {
                continue;
            }
            let x0 = self.tick_to_x(grid, note.start);
            let x1 = self.tick_to_x(grid, note.end()).max(x0 + 2.0);
            let y0 = self.key_to_y(grid, f32::from(note.key) + 1.0);
            let body = Rect::from_min_max(
                pos2(x0, y0 + 1.0),
                pos2(x1 - 1.0, y0 + self.key_height - 1.0),
            );
            let shade = 0.45 + 0.55 * note.velocity;
            let fill = Color32::from_rgb(
                (f32::from(accent.r()) * shade) as u8,
                (f32::from(accent.g()) * shade) as u8,
                (f32::from(accent.b()) * shade) as u8,
            );
            let edge = if dark {
                Color32::from_gray(10)
            } else {
                Color32::from_gray(60)
            };
            add_rect(&mut mesh, body.expand(0.5), edge);
            add_rect(&mut mesh, body.shrink(0.5), fill);
            visible += 1;
        }
        grid_painter.add(Shape::mesh(mesh));
        self.stats.visible_notes = visible;

        // Keyboard.
        let keys = Rect::from_min_max(
            pos2(rect.left(), grid.top()),
            pos2(grid.left(), rect.bottom()),
        );
        let key_painter = ui.painter_at(keys);
        for key in first_key..=last_key {
            let y = self.key_to_y(grid, key as f32 + 1.0);
            let key_rect =
                Rect::from_min_size(pos2(keys.left(), y), vec2(keys.width(), self.key_height));
            let black = is_black_key(key as u8);
            let fill = if black {
                Color32::from_gray(40)
            } else {
                Color32::from_gray(235)
            };
            key_painter.rect_filled(key_rect.shrink2(vec2(0.0, 0.5)), 2.0, fill);
            if key % 12 == 0 && self.key_height >= 9.0 {
                key_painter.text(
                    pos2(key_rect.right() - 4.0, key_rect.center().y),
                    Align2::RIGHT_CENTER,
                    format!("C{}", key / 12 - 1),
                    FontId::proportional((self.key_height - 2.0).min(12.0)),
                    Color32::from_gray(70),
                );
            }
        }
        painter.vline(grid.left(), rect.y_range(), Stroke::new(1.0, bar_line));
        painter.hline(rect.x_range(), grid.top(), Stroke::new(1.0, bar_line));
    }

    fn handle_input(&mut self, ui: &Ui, response: &egui::Response, grid: Rect) {
        let hover = response.hover_pos();
        let (scroll, zoom, alt) =
            ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.alt));
        if hover.is_some() {
            if (zoom - 1.0).abs() > f32::EPSILON {
                let anchor = hover.unwrap_or(grid.center());
                if alt {
                    let key = self.y_to_key(grid, anchor.y);
                    self.key_height = (self.key_height * zoom).clamp(4.0, 40.0);
                    self.top_key = key + (anchor.y - grid.top()) / self.key_height;
                } else {
                    let tick = self.x_to_tick(grid, anchor.x);
                    self.ticks_per_point = (self.ticks_per_point / f64::from(zoom))
                        .clamp(PPQ as f64 / 2000.0, PPQ as f64 * 64.0);
                    self.left_tick =
                        tick - f64::from(anchor.x - grid.left()) * self.ticks_per_point;
                }
            }
            self.left_tick -= f64::from(scroll.x) * self.ticks_per_point;
            self.top_key += scroll.y / self.key_height;
        }
        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = response.drag_delta();
            self.left_tick -= f64::from(delta.x) * self.ticks_per_point;
            self.top_key += delta.y / self.key_height;
        }
        self.left_tick = self.left_tick.max(0.0);
        let visible_keys = grid.height() / self.key_height;
        self.top_key = self.top_key.clamp(visible_keys.min(128.0) - 1.0, 127.0);
    }

    fn tick_to_x(&self, grid: Rect, tick: i64) -> f32 {
        grid.left() + ((tick as f64 - self.left_tick) / self.ticks_per_point) as f32
    }

    fn x_to_tick(&self, grid: Rect, x: f32) -> f64 {
        self.left_tick + f64::from(x - grid.left()) * self.ticks_per_point
    }

    fn key_to_y(&self, grid: Rect, key_top: f32) -> f32 {
        grid.top() + (self.top_key + 1.0 - key_top) * self.key_height
    }

    fn y_to_key(&self, grid: Rect, y: f32) -> f32 {
        self.top_key + 1.0 - (y - grid.top()) / self.key_height
    }
}

fn is_black_key(key: u8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

/// Adds an axis-aligned, solid rectangle (two triangles, no anti-aliasing:
/// edges are pixel-aligned anyway) to a mesh.
fn add_rect(mesh: &mut Mesh, rect: Rect, color: Color32) {
    let base = mesh.vertices.len() as u32;
    let uv = egui::epaint::WHITE_UV;
    for pos in [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ] {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: snap(pos),
            uv,
            color,
        });
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn snap(pos: Pos2) -> Pos2 {
    pos2(pos.x.round(), pos.y.round())
}

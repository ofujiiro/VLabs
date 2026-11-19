// SPDX-License-Identifier: GPL-3.0-or-later

//! The piano roll: keys down the left, time across, notes as blocks, a
//! ruler on top and the velocity of each note underneath.
//!
//! Mouse editing, all of it undoable:
//! - click an empty spot: a note appears there and plays; drag to stretch it;
//! - drag a note: move it (and the other selected notes), in time and pitch;
//! - drag a note's right edge: change its length;
//! - right-click or right-drag: delete the notes under the pointer;
//! - Ctrl- or Shift-drag on an empty spot: select with a rectangle;
//!   Ctrl- or Shift-click a note: add it to or remove it from the selection;
//! - hold Alt while dragging to ignore the grid;
//! - the wheel scrolls, Shift+wheel scrolls in time, Ctrl+wheel zooms in
//!   time, Alt+wheel zooms the keys; drag with the middle button to pan;
//! - click the ruler to move the play position; click a key to hear it;
//! - drag the tab at the end of the pattern (on the ruler) to make it end
//!   on any grid step (decision 0018);
//! - with the song's key set, rows outside the key are shaded; "Keep notes
//!   in the key" puts new and dragged notes on notes of the key; a chord
//!   stamp ("Draw") shows its notes under the pointer and places them all.
//! - the lane under the notes shows their velocity, or the song's tempo
//!   where the pattern plays (decision 0026; `tempo_lane`), edited there.
//!
//! Only what is visible is drawn, and all notes go into one triangle mesh
//! (from the phase 0 spike), so the cost per frame depends on what is on
//! screen, not on how many notes there are.

use std::collections::HashSet;

use daw_model::{MAX_KEY, MeterMap, Note, NoteId, Ticks};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Mesh, PointerButton, Pos2, Rect, Response,
    RichText, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};

use crate::session::{Draw, Grid, Session};
use crate::tempo_lane::{Axis, TempoLane};
use crate::text;
use crate::theme::{Palette, size};

const KEYS_WIDTH: f32 = 64.0;
const RULER_HEIGHT: f32 = 28.0;
const VELOCITY_HEIGHT: f32 = 72.0;
const VELOCITY_GAP: f32 = 6.0;
/// How close to a note's right end the pointer must be to resize it.
const EDGE_GRAB: f32 = 7.0;
/// How far the pointer must move before a click becomes a drag.
const DRAG_THRESHOLD: f32 = 3.0;
/// How close to the end of the pattern the pointer must be, on the ruler,
/// to take its handle.
const END_GRAB: f32 = 7.0;

/// What a mouse drag is doing.
#[derive(Clone, Debug, Default)]
enum Drag {
    #[default]
    None,
    /// Stretching a just-placed note or chord.
    Create {
        ids: Vec<NoteId>,
        start: Ticks,
        origin: Pos2,
    },
    /// Moving the selected notes.
    Move {
        origin: Pos2,
        originals: Vec<Note>,
        grabbed: Note,
        anchor_tick: f64,
        anchor_key: i32,
        last_key: u8,
    },
    /// Changing the selected notes' lengths.
    Resize {
        origin: Pos2,
        originals: Vec<Note>,
        grabbed: Note,
        anchor_tick: f64,
    },
    /// Selecting with a rectangle.
    Select {
        origin: Pos2,
        before: HashSet<NoteId>,
    },
    /// Deleting notes under the pointer.
    Erase,
    /// Changing velocities in the lane.
    Velocity { originals: Vec<Note>, grabbed: Note },
    /// Moving the play position along the ruler.
    Scrub,
    /// Moving the end of the pattern.
    End,
    /// Playing the keys on the left.
    Keys,
    /// Moving the view.
    Pan,
}

/// The piano roll's view: where it is scrolled and zoomed to, and the drag
/// in progress.
#[derive(Debug)]
pub struct PianoRoll {
    /// Tick at the left edge of the grid.
    left_tick: f64,
    /// Horizontal zoom.
    ticks_per_point: f64,
    /// Key at the top edge of the grid (fractional, for smooth scrolling).
    top_key: f32,
    /// Height of one key row.
    key_height: f32,
    drag: Drag,
    fitted: bool,
    /// When the view was last scrolled by hand (seconds), to stop following
    /// the playhead for a moment.
    scrolled_at: f64,
    hovered: Option<Note>,
    /// Scroll so every note is visible on the next frame.
    reveal: bool,
    /// Where the pattern ends (its handle on the ruler), this frame.
    pattern_end: Ticks,
    /// Whether the lane under the notes shows the song's tempo (rather
    /// than the notes' velocity).
    shows_tempo: bool,
    /// The tempo line, when the lane shows it.
    tempo: TempoLane,
}

impl Default for PianoRoll {
    fn default() -> Self {
        Self {
            left_tick: 0.0,
            #[allow(clippy::cast_precision_loss, reason = "a constant")]
            ticks_per_point: Ticks::QUARTER.0 as f64 / 48.0,
            top_key: 84.0,
            key_height: 16.0,
            drag: Drag::None,
            fitted: false,
            scrolled_at: f64::NEG_INFINITY,
            hovered: None,
            reveal: false,
            pattern_end: Ticks::WHOLE,
            shows_tempo: false,
            tempo: TempoLane::with_panels_above(),
        }
    }
}

/// The parts of the editor, worked out for each frame.
#[derive(Clone, Copy, Debug)]
struct Layout {
    whole: Rect,
    ruler: Rect,
    keys: Rect,
    grid: Rect,
    lane: Rect,
    lane_label: Rect,
    corner: Rect,
}

impl Layout {
    fn new(whole: Rect) -> Self {
        let left = whole.left() + KEYS_WIDTH;
        let lane_top = whole.bottom() - VELOCITY_HEIGHT;
        let grid_bottom = lane_top - VELOCITY_GAP;
        Self {
            whole,
            corner: Rect::from_min_max(whole.min, pos2(left, whole.top() + RULER_HEIGHT)),
            ruler: Rect::from_min_max(
                pos2(left, whole.top()),
                pos2(whole.right(), whole.top() + RULER_HEIGHT),
            ),
            keys: Rect::from_min_max(
                pos2(whole.left(), whole.top() + RULER_HEIGHT),
                pos2(left, grid_bottom),
            ),
            grid: Rect::from_min_max(
                pos2(left, whole.top() + RULER_HEIGHT),
                pos2(whole.right(), grid_bottom),
            ),
            lane: Rect::from_min_max(pos2(left, lane_top), whole.max),
            lane_label: Rect::from_min_max(
                pos2(whole.left(), lane_top),
                pos2(left, whole.bottom()),
            ),
        }
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "screen coordinates, clamped before converting"
)]
impl PianoRoll {
    /// Shows the editor in all the space `ui` has, and handles its mouse
    /// input.
    pub fn ui(&mut self, ui: &mut Ui, session: &mut Session) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, text::PIANO_ROLL));
        let layout = Layout::new(rect);
        self.pattern_end = session.pattern().length;
        if !self.fitted && layout.grid.width() > 50.0 {
            self.fit(session, layout.grid);
            self.fitted = true;
        }
        if std::mem::take(&mut self.reveal) {
            let notes = &session.pattern().notes;
            if let (Some(low), Some(high)) = (
                notes.iter().map(|n| n.key).min(),
                notes.iter().map(|n| n.key).max(),
            ) {
                self.show_keys(layout.grid, low, high);
            }
        }
        if session.is_recording() {
            for (key, _, _) in session.notes_being_recorded() {
                self.show_keys(layout.grid, key, key);
            }
        }
        self.handle_view_input(ui, &response, &layout);
        self.handle_pointer(ui, &response, &layout, session);
        self.follow_playhead(ui, session, &layout);
        self.clamp_view(&layout, session);

        let painter = ui.painter_at(rect);
        let p = Palette::of(ui.ctx());
        painter.rect_filled(rect, CornerRadius::ZERO, p.grid);
        self.paint_grid(ui, &layout, session);
        self.paint_notes(ui, &layout, session);
        self.paint_keys(ui, &layout, session);
        self.paint_ruler(ui, &layout, session);
        self.paint_lane(ui, &layout, session);
        self.lane_switch(ui, &layout);
        if self.shows_tempo {
            // The song's tempo where the pattern's start plays.
            let placement = session.placement();
            let zero = (placement.start - placement.offset).0 as f64;
            let axis = Axis {
                left: layout.grid.left(),
                left_tick: self.left_tick + zero,
                ticks_per_point: self.ticks_per_point,
            };
            self.tempo.ui(ui, session, layout.lane, &axis);
        }
        self.paint_overlay(ui, &layout, session);
        self.tempo.panel_ui(ui, session);
    }

    /// The buttons left of the lane: what it shows.
    fn lane_switch(&mut self, ui: &mut Ui, layout: &Layout) {
        let area = layout.lane_label.shrink2(vec2(3.0, 8.0));
        ui.scope_builder(UiBuilder::new().max_rect(area), |ui| {
            // One line each: the column is narrow.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.spacing_mut().button_padding.x = 3.0;
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.vertical(|ui| {
                let velocity = ui
                    .selectable_label(!self.shows_tempo, RichText::new(text::VELOCITY).small())
                    .on_hover_text(text::SHOW_VELOCITY);
                let showing = !self.shows_tempo;
                velocity.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::Button,
                        true,
                        showing,
                        text::LANE_VELOCITY,
                    )
                });
                if velocity.clicked() {
                    self.shows_tempo = false;
                    self.tempo.close_panel();
                }
                let tempo = ui
                    .selectable_label(self.shows_tempo, RichText::new(text::TEMPO_LANE).small())
                    .on_hover_text(text::SHOW_TEMPO);
                let showing = self.shows_tempo;
                tempo.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, true, showing, text::LANE_TEMPO)
                });
                if tempo.clicked() {
                    self.shows_tempo = true;
                }
            });
        });
    }

    /// Whether a drag is in progress (on the notes or on the tempo line).
    pub fn is_dragging(&self) -> bool {
        self.note_drag() || self.tempo.is_dragging()
    }

    /// Whether a drag of the editor's own is in progress.
    fn note_drag(&self) -> bool {
        !matches!(self.drag, Drag::None)
    }

    /// Closes the tempo line's panel, if one is open (Escape); returns
    /// whether one was.
    pub fn close_popups(&mut self) -> bool {
        self.tempo.close_panel()
    }

    /// Abandons the drag in progress, putting everything back. Returns
    /// whether there was one.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        if self.tempo.cancel_drag(session) {
            return true;
        }
        if !self.note_drag() {
            return false;
        }
        self.drag = Drag::None;
        session.document_mut().cancel_gesture();
        session.pointer_off();
        true
    }

    /// Zooms in time around the middle of the view (factor above 1 zooms in).
    pub fn zoom(&mut self, factor: f64) {
        self.ticks_per_point /= factor;
        self.ticks_per_point = self
            .ticks_per_point
            .clamp(min_ticks_per_point(), max_ticks_per_point());
    }

    /// Fits the pattern into the view on the next frame.
    pub fn refit(&mut self) {
        self.fitted = false;
    }

    /// Scrolls up or down, if needed, so the pattern's notes are in view on
    /// the next frame (after recording, for instance).
    pub fn reveal_notes(&mut self) {
        self.reveal = true;
    }

    /// Scrolls the keys so that `low..=high` is visible, if it fits.
    fn show_keys(&mut self, grid: Rect, low: u8, high: u8) {
        // Rows from `top_key` (the top row) down to `bottom` are visible.
        let visible = grid.height() / self.key_height;
        let bottom = self.top_key - visible + 1.0;
        let (low, high) = (f32::from(low), f32::from(high));
        if high - low + 1.0 > visible {
            self.top_key = high + 0.5;
        } else if high > self.top_key - 0.5 {
            self.top_key = high + 1.0;
        } else if low < bottom + 0.5 {
            self.top_key = low + visible - 2.0;
        }
    }

    /// Shows the whole pattern, centred on its notes (or on middle C).
    fn fit(&mut self, session: &Session, grid: Rect) {
        let pattern = session.pattern();
        let length = pattern.length.max(Ticks::QUARTER);
        self.left_tick = 0.0;
        self.ticks_per_point = (length.0 as f64 / f64::from(grid.width() - 8.0).max(1.0))
            .clamp(min_ticks_per_point(), max_ticks_per_point());
        let centre = if pattern.notes.is_empty() {
            62.0
        } else {
            let lowest = pattern.notes.iter().map(|n| n.key).min().unwrap_or(60);
            let highest = pattern.notes.iter().map(|n| n.key).max().unwrap_or(60);
            f32::midpoint(f32::from(lowest), f32::from(highest))
        };
        let visible = grid.height() / self.key_height;
        self.top_key = centre + visible / 2.0;
    }

    // ----- Coordinates

    fn tick_to_x(&self, grid: Rect, tick: Ticks) -> f32 {
        grid.left() + ((tick.0 as f64 - self.left_tick) / self.ticks_per_point) as f32
    }

    fn x_to_tick(&self, grid: Rect, x: f32) -> f64 {
        self.left_tick + f64::from(x - grid.left()) * self.ticks_per_point
    }

    /// The top edge of a key's row.
    fn key_top(&self, grid: Rect, key: u8) -> f32 {
        grid.top() + (self.top_key - f32::from(key)) * self.key_height
    }

    /// The key whose row contains `y`, as a fraction.
    fn y_to_key(&self, grid: Rect, y: f32) -> f32 {
        self.top_key - (y - grid.top()) / self.key_height + 1.0
    }

    fn key_at(&self, grid: Rect, y: f32) -> u8 {
        self.y_to_key(grid, y)
            .floor()
            .clamp(0.0, f32::from(MAX_KEY)) as u8
    }

    fn note_rect(&self, grid: Rect, note: &Note) -> Rect {
        let x0 = self.tick_to_x(grid, note.start);
        let x1 = self.tick_to_x(grid, note.end()).max(x0 + 3.0);
        let y0 = self.key_top(grid, note.key);
        Rect::from_min_max(pos2(x0, y0 + 1.0), pos2(x1, y0 + self.key_height - 1.0))
    }

    fn visible_ticks(&self, grid: Rect) -> (f64, f64) {
        (
            self.left_tick,
            self.left_tick + f64::from(grid.width()) * self.ticks_per_point,
        )
    }

    /// The notes that may be visible, as a range of indices (notes are
    /// sorted by start; notes that started earlier but still sound are
    /// caught by looking back by the longest note).
    fn visible_range(&self, notes: &[Note], grid: Rect) -> std::ops::Range<usize> {
        let (left, right) = self.visible_ticks(grid);
        let longest = notes.iter().map(|n| n.length.0).max().unwrap_or(0);
        let from = left as i64 - longest;
        let first = notes.partition_point(|n| n.start.0 < from);
        let last = notes.partition_point(|n| (n.start.0 as f64) <= right);
        first..last.max(first)
    }

    /// The topmost note under a point.
    fn note_at(&self, notes: &[Note], grid: Rect, pos: Pos2) -> Option<Note> {
        if !grid.contains(pos) {
            return None;
        }
        let range = self.visible_range(notes, grid);
        notes[range]
            .iter()
            .rev()
            .find(|n| {
                self.note_rect(grid, n)
                    .expand2(vec2(0.0, 1.0))
                    .contains(pos)
            })
            .copied()
    }

    /// The note whose velocity bar is nearest a point in the lane.
    fn stalk_at(
        &self,
        notes: &[Note],
        layout: &Layout,
        pos: Pos2,
        session: &Session,
    ) -> Option<Note> {
        if !layout.lane.contains(pos) {
            return None;
        }
        let range = self.visible_range(notes, layout.grid);
        let mut best: Option<(f32, bool, Note)> = None;
        for note in &notes[range] {
            let distance = (self.tick_to_x(layout.grid, note.start) - pos.x).abs();
            if distance > 6.0 {
                continue;
            }
            let selected = session.is_selected(note.id);
            let better = match best {
                None => true,
                Some((d, s, _)) => (selected && !s) || (selected == s && distance < d),
            };
            if better {
                best = Some((distance, selected, *note));
            }
        }
        best.map(|(_, _, n)| n)
    }

    // ----- Input

    fn handle_view_input(&mut self, ui: &Ui, response: &Response, layout: &Layout) {
        let Some(hover) = response.hover_pos() else {
            return;
        };
        let (scroll, zoom, alt, time) = ui.input(|i| {
            (
                i.smooth_scroll_delta,
                i.zoom_delta(),
                i.modifiers.alt,
                i.time,
            )
        });
        let grid = layout.grid;
        if (zoom - 1.0).abs() > f32::EPSILON {
            let tick = self.x_to_tick(grid, hover.x.max(grid.left()));
            self.ticks_per_point = (self.ticks_per_point / f64::from(zoom))
                .clamp(min_ticks_per_point(), max_ticks_per_point());
            self.left_tick =
                tick - f64::from(hover.x.max(grid.left()) - grid.left()) * self.ticks_per_point;
            self.scrolled_at = time;
        }
        if scroll != egui::Vec2::ZERO {
            if alt {
                let key = self.y_to_key(grid, hover.y);
                let factor = (scroll.y + scroll.x) / 200.0;
                self.key_height = (self.key_height * (1.0 + factor)).clamp(6.0, 40.0);
                self.top_key = key - 1.0 + (hover.y - grid.top()) / self.key_height;
            } else {
                self.left_tick -= f64::from(scroll.x) * self.ticks_per_point;
                self.top_key += scroll.y / self.key_height;
            }
            self.scrolled_at = time;
        }
    }

    fn clamp_view(&mut self, layout: &Layout, session: &Session) {
        let grid = layout.grid;
        let (_, right) = self.visible_ticks(grid);
        let width = right - self.left_tick;
        let end = session.pattern().length.0 as f64;
        self.left_tick = self.left_tick.min((end - width * 0.5).max(0.0)).max(0.0);
        let visible_keys = grid.height() / self.key_height;
        self.top_key = self
            .top_key
            .clamp((visible_keys - 1.0).min(127.0), 127.0 + 0.5);
    }

    fn follow_playhead(&mut self, ui: &Ui, session: &Session, layout: &Layout) {
        let time = ui.input(|i| i.time);
        if !session.is_playing() || self.note_drag() || time - self.scrolled_at < 1.5 {
            return;
        }
        let x = self.tick_to_x(layout.grid, session.position());
        if x > layout.grid.right() - 24.0 || x < layout.grid.left() {
            let width = f64::from(layout.grid.width()) * self.ticks_per_point;
            self.left_tick = (session.position().0 as f64 - width * 0.05).max(0.0);
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one place that decides what a press does"
    )]
    fn handle_pointer(
        &mut self,
        ui: &Ui,
        response: &Response,
        layout: &Layout,
        session: &mut Session,
    ) {
        let (
            pos,
            modifiers,
            primary_pressed,
            secondary_pressed,
            middle_pressed,
            any_down,
            delta,
            time,
        ) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.modifiers,
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.button_pressed(PointerButton::Secondary),
                i.pointer.button_pressed(PointerButton::Middle),
                i.pointer.any_down(),
                i.pointer.delta(),
                i.time,
            )
        });
        let pattern_now = session.pattern_arc();
        let notes: &[Note] = &pattern_now.notes;
        let grid = layout.grid;
        self.hovered = response
            .hover_pos()
            .and_then(|p| self.note_at(notes, grid, p));

        // Starting a drag.
        if !self.note_drag()
            && response.hovered()
            && let Some(pos) = pos
        {
            if middle_pressed {
                self.drag = Drag::Pan;
            } else if secondary_pressed
                && (grid.contains(pos) || (layout.lane.contains(pos) && !self.shows_tempo))
            {
                session
                    .document_mut()
                    .begin_gesture(text::EDIT_DELETE_NOTES);
                self.drag = Drag::Erase;
                self.erase_at(notes, grid, pos, session);
            } else if primary_pressed {
                self.press(pos, modifiers, layout, notes, session);
            }
        }

        let Some(pos) = pos else {
            return self.finish_pointer(
                ui, response, layout, notes, None, modifiers, any_down, session,
            );
        };
        let alt = modifiers.alt;
        let meter = session.pattern_meter();
        let pattern = session.pattern_id();
        match std::mem::take(&mut self.drag) {
            Drag::None => {}
            Drag::Pan => {
                self.left_tick -= f64::from(delta.x) * self.ticks_per_point;
                self.top_key += delta.y / self.key_height;
                self.scrolled_at = time;
                self.drag = Drag::Pan;
            }
            Drag::Erase => {
                self.erase_at(notes, grid, pos, session);
                self.drag = Drag::Erase;
            }
            Drag::Create { ids, start, origin } => {
                if (pos.x - origin.x).abs() > DRAG_THRESHOLD {
                    let tick = Ticks(self.x_to_tick(grid, pos.x) as i64);
                    let step = session.grid.step(&meter, start);
                    let length = if alt || session.grid == Grid::Off {
                        (tick - start).max(Ticks(step.0 / 4).max(Ticks(1)))
                    } else {
                        (tick - start).ceil_to(step).max(step)
                    };
                    session.document_mut().edit(text::EDIT_ADD_NOTE, |p| {
                        p.update_notes(pattern, &ids, |n| n.length = length);
                    });
                    session.note_length = length;
                    Self::grow_if_needed(session, start + length);
                }
                self.drag = Drag::Create { ids, start, origin };
            }
            Drag::End => {
                let tick = Ticks(self.x_to_tick(grid, pos.x).max(0.0) as i64);
                let step = session.grid.step(&meter, tick);
                let length = if alt || session.grid == Grid::Off {
                    tick
                } else {
                    session.grid.snap_nearest(&meter, tick).max(step)
                };
                session.set_pattern_length(length);
                self.drag = Drag::End;
            }
            Drag::Move {
                origin,
                originals,
                grabbed,
                anchor_tick,
                anchor_key,
                last_key,
            } => {
                if pos.distance(origin) < DRAG_THRESHOLD {
                    self.drag = Drag::Move {
                        origin,
                        originals,
                        grabbed,
                        anchor_tick,
                        anchor_key,
                        last_key,
                    };
                    return self.finish_pointer(
                        ui,
                        response,
                        layout,
                        notes,
                        Some(pos),
                        modifiers,
                        any_down,
                        session,
                    );
                }
                let raw = Ticks((self.x_to_tick(grid, pos.x) - anchor_tick) as i64);
                let target = grabbed.start + raw;
                let new_start = if alt {
                    target
                } else {
                    session.grid.snap_nearest(&meter, target)
                };
                let earliest = originals
                    .iter()
                    .map(|n| n.start)
                    .min()
                    .unwrap_or(Ticks::ZERO);
                let dt = (new_start - grabbed.start).max(-earliest);
                let lowest = originals
                    .iter()
                    .map(|n| i32::from(n.key))
                    .min()
                    .unwrap_or(0);
                let highest = originals
                    .iter()
                    .map(|n| i32::from(n.key))
                    .max()
                    .unwrap_or(0);
                let rows = i32::from(self.key_at(grid, pos.y)) - anchor_key;
                let dk = rows.clamp(-lowest, i32::from(MAX_KEY) - highest);
                // Kept in the key, every note moves by the same number of
                // steps of the scale (decision 0018); otherwise by semitones.
                let pointer = u8::try_from((i32::from(grabbed.key) + rows).clamp(0, 127))
                    .unwrap_or(grabbed.key);
                let keys: Vec<u8> = originals.iter().map(|n| n.key).collect();
                let moved: Vec<u8> = if session.keeps_in_key() {
                    session
                        .moved_keys(&keys, grabbed.key, pointer)
                        .unwrap_or_else(|| keys.clone())
                } else {
                    keys.iter()
                        .map(|&k| u8::try_from(i32::from(k) + dk).unwrap_or(k))
                        .collect()
                };
                let by_id: std::collections::HashMap<NoteId, (Note, u8)> = originals
                    .iter()
                    .zip(moved)
                    .map(|(n, key)| (n.id, (*n, key)))
                    .collect();
                let ids: Vec<NoteId> = originals.iter().map(|n| n.id).collect();
                let end = originals.iter().map(Note::end).max().unwrap_or(Ticks::ZERO) + dt;
                session.document_mut().edit(text::EDIT_MOVE_NOTES, |p| {
                    p.update_notes(pattern, &ids, |n| {
                        if let Some((o, key)) = by_id.get(&n.id) {
                            n.start = o.start + dt;
                            n.key = *key;
                        }
                    });
                });
                Self::grow_if_needed(session, end);
                let key = by_id.get(&grabbed.id).map_or(grabbed.key, |(_, key)| *key);
                if key != last_key {
                    session.pointer_on(key, grabbed.velocity);
                }
                self.drag = Drag::Move {
                    origin,
                    originals,
                    grabbed,
                    anchor_tick,
                    anchor_key,
                    last_key: key,
                };
            }
            Drag::Resize {
                origin,
                originals,
                grabbed,
                anchor_tick,
            } => {
                if pos.distance(origin) < DRAG_THRESHOLD {
                    self.drag = Drag::Resize {
                        origin,
                        originals,
                        grabbed,
                        anchor_tick,
                    };
                    return self.finish_pointer(
                        ui,
                        response,
                        layout,
                        notes,
                        Some(pos),
                        modifiers,
                        any_down,
                        session,
                    );
                }
                let raw = Ticks((self.x_to_tick(grid, pos.x) - anchor_tick) as i64);
                let step = session.grid.step(&meter, grabbed.start);
                let shortest = if alt || session.grid == Grid::Off {
                    Ticks((step.0 / 4).max(1))
                } else {
                    step
                };
                let target_end = grabbed.end() + raw;
                let new_end = if alt {
                    target_end
                } else {
                    session.grid.snap_nearest(&meter, target_end)
                };
                let change = (new_end - grabbed.start).max(shortest) - grabbed.length;
                let ids: Vec<NoteId> = originals.iter().map(|n| n.id).collect();
                let by_id: std::collections::HashMap<NoteId, Note> =
                    originals.iter().map(|n| (n.id, *n)).collect();
                let end = originals
                    .iter()
                    .map(|n| n.end() + change)
                    .max()
                    .unwrap_or(Ticks::ZERO);
                session.document_mut().edit(text::EDIT_RESIZE_NOTES, |p| {
                    p.update_notes(pattern, &ids, |n| {
                        if let Some(o) = by_id.get(&n.id) {
                            n.length = (o.length + change).max(shortest.min(o.length));
                        }
                    });
                });
                Self::grow_if_needed(session, end);
                session.note_length = (grabbed.length + change).max(shortest);
                self.drag = Drag::Resize {
                    origin,
                    originals,
                    grabbed,
                    anchor_tick,
                };
            }
            Drag::Velocity { originals, grabbed } => {
                let lane = layout.lane;
                let value =
                    ((lane.bottom() - 4.0 - pos.y) / (lane.height() - 10.0)).clamp(0.02, 1.0);
                let change = value - grabbed.velocity;
                let ids: Vec<NoteId> = originals.iter().map(|n| n.id).collect();
                let by_id: std::collections::HashMap<NoteId, Note> =
                    originals.iter().map(|n| (n.id, *n)).collect();
                session.document_mut().edit(text::EDIT_VELOCITY, |p| {
                    p.update_notes(pattern, &ids, |n| {
                        if let Some(o) = by_id.get(&n.id) {
                            n.velocity = (o.velocity + change).clamp(0.02, 1.0);
                        }
                    });
                });
                self.drag = Drag::Velocity { originals, grabbed };
            }
            Drag::Select { origin, before } => {
                let rect = Rect::from_two_pos(origin, pos);
                let (t0, t1) = (
                    self.x_to_tick(grid, rect.left()),
                    self.x_to_tick(grid, rect.right()),
                );
                let (k_top, k_bottom) = (
                    self.y_to_key(grid, rect.top()),
                    self.y_to_key(grid, rect.bottom()),
                );
                let inside = notes.iter().filter(|n| {
                    (n.end().0 as f64) > t0
                        && (n.start.0 as f64) < t1
                        && f32::from(n.key) + 1.0 > k_bottom
                        && f32::from(n.key) < k_top
                });
                let ids: Vec<NoteId> = before.iter().copied().chain(inside.map(|n| n.id)).collect();
                session.select_only(ids);
                self.drag = Drag::Select { origin, before };
            }
            Drag::Scrub => {
                let tick = self.x_to_tick(grid, pos.x.clamp(grid.left(), grid.right()));
                let at = session
                    .grid
                    .snap_nearest(&meter, Ticks(tick.max(0.0) as i64));
                session.locate(at);
                self.drag = Drag::Scrub;
            }
            Drag::Keys => {
                if layout.keys.contains(pos) || pos.x < layout.keys.right() {
                    let key = self.key_at(grid, pos.y);
                    session.pointer_on(key, session.keyboard_velocity());
                }
                self.drag = Drag::Keys;
            }
        }

        self.finish_pointer(
            ui,
            response,
            layout,
            notes,
            Some(pos),
            modifiers,
            any_down,
            session,
        );
    }

    /// Ends the drag when the buttons are up, and sets the mouse cursor.
    #[allow(clippy::too_many_arguments, reason = "the state of one frame's input")]
    fn finish_pointer(
        &mut self,
        ui: &Ui,
        response: &Response,
        layout: &Layout,
        notes: &[Note],
        pos: Option<Pos2>,
        modifiers: egui::Modifiers,
        any_down: bool,
        session: &mut Session,
    ) {
        if self.note_drag() && !any_down {
            self.drag = Drag::None;
            session.document_mut().end_gesture();
            session.pointer_off();
        }
        if let Some(pos) = pos {
            self.set_cursor(ui, response, layout, notes, pos, modifiers);
        }
    }

    /// A left press: decides what the drag will do.
    fn press(
        &mut self,
        pos: Pos2,
        modifiers: egui::Modifiers,
        layout: &Layout,
        notes: &[Note],
        session: &mut Session,
    ) {
        let grid = layout.grid;
        let additive = modifiers.shift || modifiers.command || modifiers.ctrl;
        if layout.ruler.contains(pos) {
            if self.near_end(layout, pos) {
                session
                    .document_mut()
                    .begin_gesture(text::EDIT_PATTERN_LENGTH);
                self.drag = Drag::End;
            } else {
                self.drag = Drag::Scrub;
            }
            return;
        }
        if layout.keys.contains(pos) {
            self.drag = Drag::Keys;
            return;
        }
        if layout.lane.contains(pos) {
            if self.shows_tempo {
                // The tempo line takes it.
                return;
            }
            if let Some(note) = self.stalk_at(notes, layout, pos, session) {
                if !session.is_selected(note.id) {
                    session.select_only([note.id]);
                }
                let originals = selected_or(notes, session, note);
                session.document_mut().begin_gesture(text::EDIT_VELOCITY);
                self.drag = Drag::Velocity {
                    originals,
                    grabbed: note,
                };
            }
            return;
        }
        if !grid.contains(pos) {
            return;
        }
        let tick = self.x_to_tick(grid, pos.x);
        if let Some(note) = self.note_at(notes, grid, pos) {
            if additive {
                session.toggle_selected(note.id);
                return;
            }
            if !session.is_selected(note.id) {
                session.select_only([note.id]);
            }
            let originals = selected_or(notes, session, note);
            let rect = self.note_rect(grid, &note);
            if pos.x > rect.right() - EDGE_GRAB && rect.width() > EDGE_GRAB * 1.5 {
                session
                    .document_mut()
                    .begin_gesture(text::EDIT_RESIZE_NOTES);
                self.drag = Drag::Resize {
                    origin: pos,
                    originals,
                    grabbed: note,
                    anchor_tick: tick,
                };
            } else {
                session.document_mut().begin_gesture(text::EDIT_MOVE_NOTES);
                session.pointer_on(note.key, note.velocity);
                self.drag = Drag::Move {
                    origin: pos,
                    originals,
                    grabbed: note,
                    anchor_tick: tick,
                    anchor_key: i32::from(self.key_at(grid, pos.y)),
                    last_key: note.key,
                };
            }
            return;
        }
        if additive {
            self.drag = Drag::Select {
                origin: pos,
                before: session.selection().clone(),
            };
            return;
        }
        // An empty spot: place a note (or the chosen chord) there and play
        // it.
        let start = Self::place_at(session, tick, modifiers.alt);
        let key = self.key_at(grid, pos.y);
        let velocity = session.keyboard_velocity();
        let label = match session.draw {
            Draw::Note => text::EDIT_ADD_NOTE,
            Draw::Chord(_) => text::EDIT_ADD_CHORD,
        };
        session.document_mut().begin_gesture(label);
        let ids = session.add_chord(start, key, velocity);
        if ids.is_empty() {
            session.document_mut().end_gesture();
        } else {
            self.drag = Drag::Create {
                ids,
                start,
                origin: pos,
            };
        }
    }

    /// Where a note placed at `tick` starts: on the grid (counted from the
    /// bar line), or exactly with Alt.
    fn place_at(session: &Session, tick: f64, alt: bool) -> Ticks {
        let raw = Ticks(tick.max(0.0) as i64);
        if alt {
            raw
        } else {
            session.grid.snap_down(&session.pattern_meter(), raw)
        }
    }

    /// Whether a point on the ruler is on the handle at the end of the
    /// pattern.
    fn near_end(&self, layout: &Layout, pos: Pos2) -> bool {
        let x = self.tick_to_x(layout.grid, self.pattern_end);
        layout.ruler.contains(pos) && (pos.x - x).abs() <= END_GRAB
    }

    fn erase_at(&self, notes: &[Note], grid: Rect, pos: Pos2, session: &mut Session) {
        if let Some(note) = self.note_at(notes, grid, pos) {
            let pattern = session.pattern_id();
            session.document_mut().edit(text::EDIT_DELETE_NOTES, |p| {
                p.remove_notes(pattern, &[note.id]);
            });
        }
    }

    fn grow_if_needed(session: &mut Session, end: Ticks) {
        if end > session.pattern().length {
            let bars = session.pattern_meter().position(end - Ticks(1)).bar + 1;
            session.set_pattern_bars(bars);
        }
    }

    fn set_cursor(
        &self,
        ui: &Ui,
        response: &Response,
        layout: &Layout,
        notes: &[Note],
        pos: Pos2,
        modifiers: egui::Modifiers,
    ) {
        if !response.hovered() && !self.note_drag() {
            return;
        }
        let icon = match &self.drag {
            Drag::Move { .. } => CursorIcon::Grabbing,
            Drag::Resize { .. } | Drag::Create { .. } | Drag::End => CursorIcon::ResizeHorizontal,
            Drag::Velocity { .. } => CursorIcon::ResizeVertical,
            Drag::Pan => CursorIcon::AllScroll,
            Drag::Erase | Drag::Select { .. } | Drag::Scrub | Drag::Keys => CursorIcon::Default,
            Drag::None => {
                if let Some(note) = self.note_at(notes, layout.grid, pos) {
                    let rect = self.note_rect(layout.grid, &note);
                    if pos.x > rect.right() - EDGE_GRAB
                        && rect.width() > EDGE_GRAB * 1.5
                        && !modifiers.shift
                    {
                        CursorIcon::ResizeHorizontal
                    } else {
                        CursorIcon::Grab
                    }
                } else if layout.grid.contains(pos) {
                    CursorIcon::Crosshair
                } else if self.near_end(layout, pos) {
                    CursorIcon::ResizeHorizontal
                } else if layout.ruler.contains(pos) || layout.keys.contains(pos) {
                    CursorIcon::PointingHand
                } else {
                    CursorIcon::Default
                }
            }
        };
        ui.ctx().set_cursor_icon(icon);
    }

    // ----- Painting

    fn paint_grid(&self, ui: &Ui, layout: &Layout, session: &Session) {
        let p = Palette::of(ui.ctx());
        let grid = layout.grid;
        let painter = ui.painter_at(grid);
        let first_key = self.y_to_key(grid, grid.bottom()).floor().max(0.0) as u8;
        let last_key = self.top_key.ceil().clamp(0.0, 127.0) as u8;
        // With a key, the rows outside it are shaded instead of the black
        // keys, and the tonic's rows are marked (decision 0018).
        let scale = session.pattern_key();
        for key in first_key..=last_key {
            let y = self.key_top(grid, key);
            let row =
                Rect::from_min_size(pos2(grid.left(), y), vec2(grid.width(), self.key_height));
            let shaded = scale.map_or_else(|| is_black_key(key), |s| !s.contains(key));
            if shaded {
                painter.rect_filled(row, CornerRadius::ZERO, p.grid_dark_row);
            }
            if scale.is_some_and(|s| s.is_tonic(key)) {
                painter.rect_filled(row, CornerRadius::ZERO, p.accent.gamma_multiply(0.10));
            }
            if scale.map_or(key % 12 == 0, |s| s.is_tonic(key)) {
                painter.hline(grid.x_range(), row.bottom(), Stroke::new(1.0, p.grid_beat));
            }
        }

        // Vertical lines: every grid step from each bar line, so the grid
        // meets the bars after an odd bar.
        let meter = session.pattern_meter();
        let (left, right) = self.visible_ticks(grid);
        let step = self.line_step(session, &meter);
        let bar_every = self.bar_every(&meter);
        let mut bar = meter.position(Ticks(left.max(0.0) as i64)).bar;
        loop {
            let bar_start = meter.bar_start(bar);
            if bar_start.0 as f64 > right {
                break;
            }
            let next = meter.bar_start(bar + 1);
            let beat = meter.signature_at_bar(bar).beat_length();
            if bar.is_multiple_of(bar_every) {
                let x = self.tick_to_x(grid, bar_start);
                if x >= grid.left() {
                    painter.vline(
                        x.round() + 0.5,
                        grid.y_range(),
                        Stroke::new(1.0, p.grid_bar),
                    );
                }
            }
            if let Some(step) = step {
                let mut tick = bar_start + step;
                while tick < next {
                    let x = self.tick_to_x(grid, tick);
                    if x >= grid.left() && x <= grid.right() {
                        let on_beat = (tick - bar_start).0 % beat.0 == 0;
                        let color = if on_beat { p.grid_beat } else { p.grid_line };
                        painter.vline(x.round() + 0.5, grid.y_range(), Stroke::new(1.0, color));
                    }
                    tick += step;
                }
            }
            bar += 1;
        }

        // After the end of the pattern: shaded, nothing plays there.
        let end_x = self.tick_to_x(grid, session.pattern().length);
        if end_x < grid.right() {
            let after = Rect::from_min_max(pos2(end_x.max(grid.left()), grid.top()), grid.max);
            painter.rect_filled(after, CornerRadius::ZERO, p.background.gamma_multiply(0.7));
            painter.vline(end_x, grid.y_range(), Stroke::new(2.0, p.grid_bar));
        }
    }

    /// The spacing of the lines inside bars: the grid, or the beat when the
    /// grid's lines would be closer than 8 points; `None` when even beats
    /// would be (only bar lines then).
    fn line_step(&self, session: &Session, meter: &MeterMap) -> Option<Ticks> {
        let beat = meter.initial().beat_length();
        let points = |t: Ticks| t.0 as f64 / self.ticks_per_point;
        let step = match session.grid {
            Grid::Off | Grid::Bar => beat,
            grid @ Grid::Note(_) => grid.step(meter, Ticks::ZERO),
        };
        if points(step) >= 8.0 {
            Some(step)
        } else if points(beat) >= 8.0 {
            Some(beat)
        } else {
            None
        }
    }

    /// Every how many bars a bar line is drawn, so they stay 8 points apart.
    fn bar_every(&self, meter: &MeterMap) -> u32 {
        let bar_points = meter.initial().bar_length().0 as f64 / self.ticks_per_point;
        ((8.0 / bar_points).ceil() as u32)
            .max(1)
            .next_power_of_two()
    }

    fn paint_notes(&self, ui: &Ui, layout: &Layout, session: &Session) {
        let p = Palette::of(ui.ctx());
        let grid = layout.grid;
        let painter = ui.painter_at(grid);
        let notes = &session.pattern().notes;
        let range = self.visible_range(notes, grid);
        let first_key = self.y_to_key(grid, grid.bottom()).floor() as i32 - 1;
        let last_key = self.top_key.ceil() as i32 + 1;
        let mut mesh = Mesh::default();
        let mut labels = Vec::new();
        let show_names = self.key_height >= 13.0 && range.len() < 400;
        let (left, _) = self.visible_ticks(grid);
        for note in &notes[range] {
            if (note.end().0 as f64) < left
                || i32::from(note.key) < first_key
                || i32::from(note.key) > last_key
            {
                continue;
            }
            let body = self.note_rect(grid, note);
            let selected = session.is_selected(note.id);
            let base = if selected { p.selected } else { p.accent };
            let fill = shade(base, 0.55 + 0.45 * note.velocity);
            add_rect(&mut mesh, body, p.note_edge);
            add_rect(&mut mesh, body.shrink(1.0), fill);
            if show_names && body.width() > 30.0 {
                labels.push((body, text::note_name(note.key)));
            }
        }
        // Notes being recorded, growing until their key comes up.
        if session.is_recording() {
            let now = session.position();
            for (key, _, start) in session.notes_being_recorded() {
                let end = now.max(start + Ticks(1));
                let note = Note {
                    id: NoteId(0),
                    start,
                    length: end - start,
                    key,
                    velocity: 1.0,
                    pitch_offset: 0.0,
                };
                let body = self.note_rect(grid, &note);
                add_rect(&mut mesh, body, p.note_edge);
                add_rect(&mut mesh, body.shrink(1.0), p.record);
            }
        }
        painter.add(Shape::mesh(mesh));
        self.paint_stamp(ui, layout, session);
        let font = FontId::proportional((self.key_height - 4.0).clamp(9.0, size::SMALL));
        for (body, name) in labels {
            painter.text(
                pos2(body.left() + 4.0, body.center().y),
                Align2::LEFT_CENTER,
                name,
                font.clone(),
                p.on_accent,
            );
        }

        if let Drag::Select { origin, .. } = &self.drag
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let rect = Rect::from_two_pos(*origin, pos);
            painter.rect(
                rect,
                CornerRadius::same(2),
                p.accent.gamma_multiply(0.12),
                Stroke::new(1.0, p.accent),
                StrokeKind::Inside,
            );
        }

        let x = self.tick_to_x(grid, session.position());
        if x >= grid.left() && x <= grid.right() {
            let color = if session.is_recording() {
                p.record
            } else {
                p.accent
            };
            painter.vline(x, grid.y_range(), Stroke::new(2.0, color));
        }
    }

    /// Under the pointer, on an empty spot: the notes a click would place
    /// when that is not simply the row under the pointer (a chord stamp, or
    /// a note kept in the key), with the chord's name.
    fn paint_stamp(&self, ui: &Ui, layout: &Layout, session: &Session) {
        if self.note_drag() || self.hovered.is_some() {
            return;
        }
        let Some(pos) = ui.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let grid = layout.grid;
        if !grid.contains(pos)
            || !(matches!(session.draw, Draw::Chord(_)) || session.keeps_in_key())
        {
            return;
        }
        let alt = ui.input(|i| i.modifiers.alt);
        let start = Self::place_at(session, self.x_to_tick(grid, pos.x), alt);
        let root = self.key_at(grid, pos.y);
        let keys = session.chord_keys(root);
        if keys == [root] {
            return;
        }
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(grid);
        for &key in &keys {
            let ghost = Note {
                id: NoteId(0),
                start,
                length: session.note_length.max(Ticks(1)),
                key,
                velocity: 1.0,
                pitch_offset: 0.0,
            };
            painter.rect(
                self.note_rect(grid, &ghost),
                CornerRadius::same(2),
                p.accent.gamma_multiply(0.25),
                Stroke::new(1.0, p.accent.gamma_multiply(0.8)),
                StrokeKind::Inside,
            );
        }
        if let Some(name) = session.chord_name(root) {
            let top = keys.iter().max().copied().unwrap_or(root);
            let x = self.tick_to_x(grid, start);
            painter.text(
                pos2(x, self.key_top(grid, top) - 3.0),
                Align2::LEFT_BOTTOM,
                name,
                FontId::proportional(size::SMALL),
                p.text,
            );
        }
    }

    fn paint_keys(&self, ui: &Ui, layout: &Layout, session: &Session) {
        let p = Palette::of(ui.ctx());
        let keys = layout.keys;
        let painter = ui.painter_at(keys);
        painter.rect_filled(keys, CornerRadius::ZERO, p.surface);
        let grid = layout.grid;
        let first_key = self.y_to_key(grid, grid.bottom()).floor().max(0.0) as u8;
        let last_key = self.top_key.ceil().clamp(0.0, 127.0) as u8;
        let hover_key = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|pos| grid.contains(*pos) || keys.contains(*pos))
            .map(|pos| self.key_at(grid, pos.y));
        let scale = session.pattern_key();
        for key in first_key..=last_key {
            let y = self.key_top(grid, key);
            let row =
                Rect::from_min_size(pos2(keys.left(), y), vec2(keys.width(), self.key_height));
            let black = is_black_key(key);
            let lit = session.is_key_lit(key);
            let fill = if lit {
                p.accent
            } else if black {
                p.key_black
            } else {
                p.key_white
            };
            let body = if black {
                Rect::from_min_max(
                    row.min,
                    pos2(row.left() + keys.width() * 0.62, row.bottom()),
                )
            } else {
                row
            };
            painter.rect_filled(body.shrink2(vec2(0.0, 0.5)), CornerRadius::same(2), fill);
            if Some(key) == hover_key && !lit {
                painter.rect_filled(
                    body.shrink2(vec2(0.0, 0.5)),
                    CornerRadius::same(2),
                    p.accent.gamma_multiply(0.25),
                );
            }
            let labelled = scale.map_or(key % 12 == 0, |s| s.is_tonic(key));
            if labelled && self.key_height >= 9.0 {
                painter.text(
                    pos2(row.right() - 6.0, row.center().y),
                    Align2::RIGHT_CENTER,
                    text::note_name(key),
                    FontId::proportional((self.key_height - 3.0).min(size::SMALL)),
                    Color32::from_gray(90),
                );
            } else if scale.is_some_and(|s| s.contains(key)) && self.key_height >= 9.0 {
                // A dot for each note of the key.
                painter.circle_filled(
                    pos2(row.right() - 8.0, row.center().y),
                    2.0,
                    Color32::from_gray(if black { 150 } else { 110 }),
                );
            }
        }
        painter.vline(
            keys.right() - 0.5,
            keys.y_range(),
            Stroke::new(1.0, p.divider),
        );
    }

    fn paint_ruler(&self, ui: &Ui, layout: &Layout, session: &Session) {
        let p = Palette::of(ui.ctx());
        let ruler = layout.ruler;
        let painter = ui.painter_at(ruler);
        painter.rect_filled(ruler, CornerRadius::ZERO, p.surface);
        let grid = layout.grid;
        let meter = &session.pattern_meter();
        if let Some(region) = session.loop_in_play() {
            let placement = session.placement();
            let x0 = self.tick_to_x(grid, placement.to_pattern(region.start));
            let x1 = self.tick_to_x(grid, placement.to_pattern(region.end));
            let band = Rect::from_min_max(pos2(x0, ruler.bottom() - 6.0), pos2(x1, ruler.bottom()));
            painter.rect_filled(band, CornerRadius::same(2), p.accent.gamma_multiply(0.7));
        }
        let (left, right) = self.visible_ticks(grid);
        let bar_length = meter.initial().bar_length();
        let bar_points = bar_length.0 as f64 / self.ticks_per_point;
        let every = ((48.0 / bar_points).ceil() as u32)
            .max(1)
            .next_power_of_two();
        let first_bar = meter.position(Ticks(left.max(0.0) as i64)).bar;
        let mut bar = first_bar;
        loop {
            let start = meter.bar_start(bar);
            if start.0 as f64 > right {
                break;
            }
            let x = self.tick_to_x(grid, start);
            if bar.is_multiple_of(every) {
                painter.vline(
                    x,
                    (ruler.top() + 8.0)..=ruler.bottom(),
                    Stroke::new(1.0, p.grid_bar),
                );
                painter.text(
                    pos2(x + 5.0, ruler.top() + 12.0),
                    Align2::LEFT_CENTER,
                    (bar + 1).to_string(),
                    FontId::proportional(size::SMALL),
                    p.text_weak,
                );
            }
            bar += 1;
        }
        // The handle at the end of the pattern (decision 0018).
        let end_x = self.tick_to_x(grid, session.pattern().length);
        if end_x >= ruler.left() - 6.0 && end_x <= ruler.right() + 6.0 {
            let hot = matches!(self.drag, Drag::End)
                || ui
                    .input(|i| i.pointer.hover_pos())
                    .is_some_and(|pos| self.near_end(layout, pos));
            let tab = Rect::from_min_max(
                pos2(end_x - 4.0, ruler.top() + 6.0),
                pos2(end_x + 4.0, ruler.bottom() - 2.0),
            );
            let color = if hot { p.accent } else { p.grid_bar };
            painter.rect_filled(tab, CornerRadius::same(2), color);
            painter.vline(
                end_x,
                (tab.top() + 4.0)..=(tab.bottom() - 4.0),
                Stroke::new(1.0, p.surface),
            );
        }
        let x = self.tick_to_x(grid, session.position());
        if x >= ruler.left() - 6.0 && x <= ruler.right() + 6.0 {
            let color = if session.is_recording() {
                p.record
            } else {
                p.accent
            };
            let points = vec![
                pos2(x - 6.0, ruler.bottom() - 10.0),
                pos2(x + 6.0, ruler.bottom() - 10.0),
                pos2(x, ruler.bottom()),
            ];
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        let corner = ui.painter_at(layout.corner);
        corner.rect_filled(layout.corner, CornerRadius::ZERO, p.surface);
        if let Some(note) = self.hovered {
            corner.text(
                layout.corner.center(),
                Align2::CENTER_CENTER,
                text::note_details(note.key, note.velocity),
                FontId::proportional(size::SMALL),
                p.text_weak,
            );
        }
        ui.painter().hline(
            layout.whole.x_range(),
            ruler.bottom() - 0.5,
            Stroke::new(1.0, p.divider),
        );
    }

    fn paint_lane(&self, ui: &Ui, layout: &Layout, session: &Session) {
        let p = Palette::of(ui.ctx());
        let lane = layout.lane;
        let whole_lane = Rect::from_min_max(
            pos2(layout.whole.left(), lane.top() - VELOCITY_GAP),
            layout.whole.max,
        );
        ui.painter()
            .rect_filled(whole_lane, CornerRadius::ZERO, p.surface);
        ui.painter().hline(
            whole_lane.x_range(),
            whole_lane.top() + 0.5,
            Stroke::new(1.0, p.divider),
        );
        let painter = ui.painter_at(lane);
        painter.rect_filled(lane.shrink2(vec2(0.0, 2.0)), CornerRadius::ZERO, p.grid);
        if self.shows_tempo {
            return;
        }
        let notes = &session.pattern().notes;
        let range = self.visible_range(notes, layout.grid);
        let usable = lane.height() - 10.0;
        for note in &notes[range] {
            let x = self.tick_to_x(layout.grid, note.start);
            if x < lane.left() - 4.0 || x > lane.right() + 4.0 {
                continue;
            }
            let top = lane.bottom() - 4.0 - usable * note.velocity;
            let color = if session.is_selected(note.id) {
                p.selected
            } else {
                p.accent
            };
            painter.vline(
                x + 1.0,
                top..=(lane.bottom() - 4.0),
                Stroke::new(2.0, color),
            );
            painter.circle_filled(pos2(x + 1.0, top), 3.5, color);
        }
    }

    fn paint_overlay(&self, ui: &Ui, layout: &Layout, session: &Session) {
        if !session.pattern().notes.is_empty() || self.note_drag() || session.is_recording() {
            return;
        }
        let p = Palette::of(ui.ctx());
        let centre = layout.grid.center();
        let painter = ui.painter_at(layout.grid);
        painter.text(
            centre - vec2(0.0, 14.0),
            Align2::CENTER_CENTER,
            text::EMPTY_PATTERN_HINT,
            FontId::proportional(size::HEADING),
            p.text_weak,
        );
        painter.text(
            centre + vec2(0.0, 14.0),
            Align2::CENTER_CENTER,
            text::EMPTY_PATTERN_HINT_2,
            FontId::proportional(size::BODY),
            p.text_weak.gamma_multiply(0.8),
        );
    }
}

/// The selected notes (which include `note`), or just `note`.
fn selected_or(notes: &[Note], session: &Session, note: Note) -> Vec<Note> {
    let selected: Vec<Note> = notes
        .iter()
        .filter(|n| session.is_selected(n.id))
        .copied()
        .collect();
    if selected.iter().any(|n| n.id == note.id) {
        selected
    } else {
        vec![note]
    }
}

#[allow(clippy::cast_precision_loss, reason = "a constant")]
fn min_ticks_per_point() -> f64 {
    Ticks::QUARTER.0 as f64 / 2000.0
}

#[allow(clippy::cast_precision_loss, reason = "a constant")]
fn max_ticks_per_point() -> f64 {
    Ticks::QUARTER.0 as f64 * 4.0
}

/// Whether a key is a black key on a piano.
pub fn is_black_key(key: u8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

fn shade(color: Color32, amount: f32) -> Color32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "in 0..=255"
    )]
    let scale = |c: u8| (f32::from(c) * amount).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(scale(color.r()), scale(color.g()), scale(color.b()))
}

/// Adds a solid rectangle to a mesh, aligned to whole pixels.
fn add_rect(mesh: &mut Mesh, rect: Rect, color: Color32) {
    let snapped = Rect::from_min_max(
        pos2(rect.left().round(), rect.top().round()),
        pos2(rect.right().round(), rect.bottom().round()),
    );
    mesh.add_colored_rect(snapped, color);
}

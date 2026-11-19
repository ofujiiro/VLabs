// SPDX-License-Identifier: GPL-3.0-or-later

//! The step sequencer (decision 0014): the kit's sounds down the left, the
//! steps of the pattern across, one square for each.
//!
//! - click a square: a hit appears there and plays; click it again to
//!   remove it; drag along the row to add (or remove) several at once;
//! - right-click a square to remove its hit;
//! - click a sound's name to hear it, choose it for the loudness strip, and
//!   change its sound (volume, pan, tune, length);
//! - drag the bars in the loudness strip to change how hard hits play;
//! - drag the tab at the end of the steps (on the ruler) to give the
//!   pattern any number of steps (decision 0018); with swing, the hits of
//!   swung steps are drawn late in their squares, as they sound.
//!
//! Each square is also in the accessibility tree ("Kick, step 1"), so
//! screen readers and the interface tests can find it.
//!
//! The rows are a bundled kit's fifteen sounds, or a custom kit's pads
//! (decision 0025), as many as it has: the wheel over the names scrolls
//! them.

use daw_dsp::drums::PadSound;
use daw_model::{NoteId, PadSetting, Ticks};
use egui::{
    Align2, CornerRadius, CursorIcon, FontId, PointerButton, Pos2, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::keys::{KeyLabels, NOTE_KEYS};
use crate::library_panel::LibraryDrag;
use crate::session::Session;
use crate::text;
use crate::theme::{Palette, size};
use crate::widgets::Gesture;

/// The sounds, top to bottom: the ones most beats use first.
pub const PAD_ORDER: [PadSound; 15] = [
    PadSound::Kick,
    PadSound::Snare,
    PadSound::Clap,
    PadSound::ClosedHat,
    PadSound::OpenHat,
    PadSound::PedalHat,
    PadSound::LowTom,
    PadSound::MidTom,
    PadSound::HighTom,
    PadSound::Crash,
    PadSound::Ride,
    PadSound::RideBell,
    PadSound::Rimshot,
    PadSound::SideStick,
    PadSound::Cowbell,
];

const NAMES_WIDTH: f32 = 188.0;
const RULER_HEIGHT: f32 = 26.0;
const STRIP_HEIGHT: f32 = 72.0;
const STRIP_GAP: f32 = 6.0;
const MAX_STEP_WIDTH: f32 = 64.0;
const MIN_STEP_WIDTH: f32 = 18.0;
const MAX_ROW_HEIGHT: f32 = 34.0;
const MIN_ROW_HEIGHT: f32 = 14.0;

/// What a mouse drag is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Drag {
    #[default]
    None,
    /// Adding (`on`) or removing hits along a row.
    Paint { row: usize, on: bool, last: usize },
    /// Setting loudness in the strip.
    Loudness,
    /// Holding a sound's name: it plays.
    Pad,
    /// Moving the end of the pattern.
    End,
}

/// The step sequencer's state between frames.
#[derive(Debug)]
pub struct StepGrid {
    /// The sound whose loudness the strip shows (its row's key).
    chosen: u8,
    drag: Drag,
    /// The sound whose settings are open (its row's key), and where its
    /// name is.
    settings: Option<(u8, Pos2)>,
    /// How far the steps are scrolled to the left, in points.
    scroll: f32,
    /// How far the rows are scrolled up, in points.
    row_scroll: f32,
    /// The width of a step while the end of the pattern is dragged, so the
    /// squares keep their size under the pointer.
    frozen_width: Option<f32>,
}

impl Default for StepGrid {
    fn default() -> Self {
        Self {
            chosen: PadSound::Kick.key(),
            drag: Drag::None,
            settings: None,
            scroll: 0.0,
            row_scroll: 0.0,
            frozen_width: None,
        }
    }
}

/// How close to the end of the steps the pointer must be, on the ruler, to
/// take its handle.
const END_GRAB: f32 = 7.0;

/// The parts of the editor, worked out for each frame.
#[derive(Clone, Copy, Debug)]
struct Layout {
    whole: Rect,
    names: Rect,
    ruler: Rect,
    grid: Rect,
    strip: Rect,
    strip_label: Rect,
    row_height: f32,
    step_width: f32,
    steps: usize,
    step: Ticks,
    scroll: f32,
    rows: usize,
    row_scroll: f32,
}

/// One row of the grid: the key its sound plays on, and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    key: u8,
    name: String,
}

/// The rows the track in focus shows: a bundled kit's fifteen sounds, or a
/// custom kit's pads.
fn rows(session: &Session) -> Vec<Row> {
    let pads = session.kit_pads(session.track_id());
    if session.is_custom_kit(session.track_id()) {
        pads.iter()
            .map(|pad| Row {
                key: pad.setting.key,
                name: pad.name.clone(),
            })
            .collect()
    } else {
        PAD_ORDER
            .iter()
            .map(|&sound| Row {
                key: sound.key(),
                name: text::pad_name(sound).to_owned(),
            })
            .collect()
    }
}

/// What the grid shows this frame: its rows, whether they are a custom
/// kit's, and where everything is.
struct Grid<'a> {
    rows: &'a [Row],
    custom: bool,
    layout: &'a Layout,
}

/// The row a note's key plays in: for a bundled kit, a sound's other keys
/// play in its row (35 is a kick too).
fn row_key(custom: bool, key: u8) -> Option<u8> {
    if custom {
        Some(key)
    } else {
        PadSound::from_key(key).map(PadSound::key)
    }
}

impl Layout {
    fn new(
        whole: Rect,
        session: &Session,
        (scroll, row_scroll): (f32, f32),
        frozen_width: Option<f32>,
        rows: usize,
    ) -> Self {
        let step = session.step();
        let length = session.pattern().length.max(step);
        let steps = usize::try_from((length.0 + step.0 - 1) / step.0.max(1))
            .unwrap_or(1)
            .max(1);
        let left = whole.left() + NAMES_WIDTH;
        let strip_top = whole.bottom() - STRIP_HEIGHT;
        let grid_bottom = strip_top - STRIP_GAP;
        let room = (grid_bottom - whole.top() - RULER_HEIGHT).max(1.0);
        #[allow(clippy::cast_precision_loss, reason = "small counts")]
        let row_height = (room / rows.max(1) as f32).clamp(MIN_ROW_HEIGHT, MAX_ROW_HEIGHT);
        #[allow(clippy::cast_precision_loss, reason = "small counts")]
        let step_width = frozen_width.unwrap_or_else(|| {
            ((whole.right() - left - 8.0) / steps as f32).clamp(MIN_STEP_WIDTH, MAX_STEP_WIDTH)
        });
        #[allow(clippy::cast_precision_loss, reason = "small counts")]
        let rows_bottom = whole.top() + RULER_HEIGHT + row_height * rows.max(1) as f32;
        #[allow(clippy::cast_precision_loss, reason = "small counts")]
        let row_scroll = row_scroll.clamp(0.0, (row_height * rows as f32 - room).max(0.0));
        #[allow(clippy::cast_precision_loss, reason = "small counts")]
        let content = step_width * steps as f32;
        let visible = whole.right() - left;
        let scroll = scroll.clamp(0.0, (content - visible + 8.0).max(0.0));
        Self {
            whole,
            names: Rect::from_min_max(
                pos2(whole.left(), whole.top() + RULER_HEIGHT),
                pos2(left, rows_bottom.min(grid_bottom)),
            ),
            ruler: Rect::from_min_max(
                pos2(left, whole.top()),
                pos2(whole.right(), whole.top() + RULER_HEIGHT),
            ),
            grid: Rect::from_min_max(
                pos2(left, whole.top() + RULER_HEIGHT),
                pos2(whole.right(), rows_bottom.min(grid_bottom)),
            ),
            strip: Rect::from_min_max(pos2(left, strip_top), whole.max),
            strip_label: Rect::from_min_max(
                pos2(whole.left(), strip_top),
                pos2(left, whole.bottom()),
            ),
            row_height,
            step_width,
            steps,
            step,
            scroll,
            rows,
            row_scroll,
        }
    }

    #[allow(clippy::cast_precision_loss, reason = "small counts")]
    fn step_x(&self, step: usize) -> f32 {
        self.grid.left() + step as f32 * self.step_width - self.scroll
    }

    #[allow(clippy::cast_precision_loss, reason = "small counts")]
    fn row_y(&self, row: usize) -> f32 {
        self.grid.top() + row as f32 * self.row_height - self.row_scroll
    }

    /// Whether a row is (at least partly) in view.
    fn row_visible(&self, row: usize) -> bool {
        let y = self.row_y(row);
        y + self.row_height > self.grid.top() && y < self.grid.bottom()
    }

    fn cell(&self, row: usize, step: usize) -> Rect {
        Rect::from_min_size(
            pos2(self.step_x(step), self.row_y(row)),
            vec2(self.step_width, self.row_height),
        )
    }

    /// The step under an x position (unclamped: may be past either end).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped before converting"
    )]
    #[allow(clippy::cast_precision_loss, reason = "a small count")]
    fn step_at(&self, x: f32) -> Option<usize> {
        let at = ((x - self.grid.left() + self.scroll) / self.step_width).floor();
        (at >= 0.0 && at < self.steps as f32).then_some(at as usize)
    }

    /// The row under a y position.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped before converting"
    )]
    fn row_at(&self, y: f32) -> Option<usize> {
        let at = ((y - self.grid.top() + self.row_scroll) / self.row_height).floor();
        (at >= 0.0 && (at as usize) < self.rows && y < self.grid.bottom()).then_some(at as usize)
    }

    fn step_start(&self, step: usize) -> Ticks {
        self.step * i64::try_from(step).unwrap_or(0)
    }

    /// Whether a point on the ruler is on the handle at the end of the
    /// steps.
    fn near_end(&self, pos: Pos2) -> bool {
        self.ruler.contains(pos) && (pos.x - self.step_x(self.steps)).abs() <= END_GRAB
    }
}

impl StepGrid {
    /// Shows the step sequencer in all the space `ui` has.
    pub fn ui(&mut self, ui: &mut Ui, session: &mut Session, labels: &KeyLabels) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, text::STEP_GRID));
        let rows = rows(session);
        let custom = session.is_custom_kit(session.track_id());
        if !rows.iter().any(|r| r.key == self.chosen)
            && let Some(first) = rows.first()
        {
            self.chosen = first.key;
        }
        let layout = Layout::new(
            rect,
            session,
            (self.scroll, self.row_scroll),
            self.frozen_width,
            rows.len(),
        );
        self.scroll = layout.scroll;
        self.row_scroll = layout.row_scroll;
        self.handle_scroll(ui, &layout);
        let grid = Grid {
            rows: &rows,
            custom,
            layout: &layout,
        };
        self.pad_names(ui, session, &grid);
        self.handle_pointer(ui, &response, &grid, session);
        Self::describe_cells(ui, session, &grid, response.id);

        let p = Palette::of(ui.ctx());
        ui.painter_at(rect)
            .rect_filled(rect, CornerRadius::ZERO, p.grid);
        self.paint_cells(ui, session, &grid);
        self.paint_ruler(ui, session, &layout);
        self.paint_names(ui, session, labels, &grid);
        self.paint_strip(ui, session, &grid);
        self.paint_hint(ui, session, labels, &grid);
        self.pad_settings(ui, session, &grid);
        if custom {
            Self::take_library_drop(ui, session, rect);
        }
    }

    /// Sounds dragged from the sound library and let go over a custom kit's
    /// grid join the kit (decision 0025); while they are dragged (or files
    /// from the desktop), the grid says so.
    fn take_library_drop(ui: &Ui, session: &mut Session, rect: Rect) {
        let library = egui::DragAndDrop::has_payload_of_type::<LibraryDrag>(ui.ctx());
        let files = ui.input(|i| !i.raw.hovered_files.is_empty());
        if !library && !files {
            return;
        }
        let over = ui
            .input(|i| i.pointer.latest_pos())
            .is_some_and(|pos| rect.contains(pos));
        if library
            && over
            && ui.input(|i| i.pointer.any_released())
            && let Some(dragged) = egui::DragAndDrop::payload::<LibraryDrag>(ui.ctx())
        {
            egui::DragAndDrop::clear_payload(ui.ctx());
            session.add_kit_sounds(session.track_id(), dragged.0.clone());
            return;
        }
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, p.accent.gamma_multiply(0.12));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            text::DROP_ON_KIT,
            FontId::proportional(size::HEADING),
            p.text,
        );
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag != Drag::None
    }

    /// Abandons the drag in progress, putting everything back.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        if self.drag == Drag::None {
            return false;
        }
        if matches!(self.drag, Drag::Paint { .. } | Drag::Loudness) {
            session.cancel_gesture();
        }
        self.drag = Drag::None;
        session.pointer_off();
        true
    }

    /// The wheel: over the names it scrolls the rows (a custom kit may
    /// have many), elsewhere the steps.
    fn handle_scroll(&mut self, ui: &Ui, layout: &Layout) {
        if !ui.rect_contains_pointer(layout.whole) {
            return;
        }
        let delta = ui.input(|i| i.smooth_scroll_delta);
        let over_names = ui
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|pos| pos.x < layout.names.right());
        if over_names {
            self.row_scroll = (self.row_scroll - delta.y).max(0.0);
        } else {
            self.scroll = (self.scroll - delta.x - delta.y).max(0.0);
        }
    }

    /// The hits of a row's sound in a step.
    fn hits(session: &Session, grid: &Grid<'_>, key: u8, step: usize) -> Vec<NoteId> {
        let from = grid.layout.step_start(step);
        let to = from + grid.layout.step;
        session
            .pattern()
            .notes
            .iter()
            .filter(|n| n.start >= from && n.start < to)
            .filter(|n| row_key(grid.custom, n.key) == Some(key))
            .map(|n| n.id)
            .collect()
    }

    /// How hard the hit in a step plays, if there is one.
    fn hit_velocity(session: &Session, grid: &Grid<'_>, key: u8, step: usize) -> Option<f32> {
        let from = grid.layout.step_start(step);
        let to = from + grid.layout.step;
        session
            .pattern()
            .notes
            .iter()
            .filter(|n| n.start >= from && n.start < to)
            .filter(|n| row_key(grid.custom, n.key) == Some(key))
            .map(|n| n.velocity)
            .reduce(f32::max)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one place that decides what a press does"
    )]
    fn handle_pointer(
        &mut self,
        ui: &Ui,
        response: &Response,
        grid: &Grid<'_>,
        session: &mut Session,
    ) {
        let layout = grid.layout;
        let (pos, primary_pressed, secondary_pressed, primary_down) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.button_pressed(PointerButton::Secondary),
                i.pointer.primary_down(),
            )
        });
        let pressed_here = response.is_pointer_button_down_on() || response.clicked();
        if let Some(pos) = pos
            && pressed_here
            && self.drag == Drag::None
            && primary_pressed
        {
            if layout.near_end(pos) {
                session.begin_gesture(text::EDIT_PATTERN_LENGTH);
                self.frozen_width = Some(layout.step_width);
                self.drag = Drag::End;
            } else if let (Some(row), Some(step)) = (layout.row_at(pos.y), layout.step_at(pos.x))
                && layout.grid.contains(pos)
            {
                let key = grid.rows[row].key;
                let existing = Self::hits(session, grid, key, step);
                let on = existing.is_empty();
                session.begin_gesture(if on {
                    text::EDIT_PAINT_HITS
                } else {
                    text::EDIT_REMOVE_HIT
                });
                if on {
                    session.add_hit(key, layout.step_start(step), session.keyboard_velocity());
                    session.pointer_on(key, session.keyboard_velocity());
                } else {
                    session.remove_notes(&existing);
                }
                self.chosen = key;
                self.drag = Drag::Paint {
                    row,
                    on,
                    last: step,
                };
            } else if layout.strip.contains(pos) {
                session.begin_gesture(text::EDIT_VELOCITY);
                self.drag = Drag::Loudness;
            }
        }
        if let Some(pos) = pos
            && secondary_pressed
            && ui.rect_contains_pointer(layout.whole)
            && let (Some(row), Some(step)) = (layout.row_at(pos.y), layout.step_at(pos.x))
            && layout.grid.contains(pos)
        {
            let existing = Self::hits(session, grid, grid.rows[row].key, step);
            session.remove_notes(&existing);
        }
        match self.drag {
            Drag::Paint { row, on, last } if primary_down => {
                if let Some(step) = pos.and_then(|p| layout.step_at(p.x))
                    && step != last
                {
                    let key = grid.rows[row].key;
                    let (low, high) = if step > last {
                        (last + 1, step)
                    } else {
                        (step, last - 1)
                    };
                    for step in low..=high {
                        let existing = Self::hits(session, grid, key, step);
                        if on && existing.is_empty() {
                            session.add_hit(
                                key,
                                layout.step_start(step),
                                session.keyboard_velocity(),
                            );
                        } else if !on {
                            session.remove_notes(&existing);
                        }
                    }
                    self.drag = Drag::Paint {
                        row,
                        on,
                        last: step,
                    };
                }
            }
            Drag::Loudness if primary_down => {
                if let Some(pos) = pos
                    && let Some(step) = layout.step_at(pos.x)
                {
                    let ids = Self::hits(session, grid, self.chosen, step);
                    let usable = layout.strip.height() - 12.0;
                    let velocity =
                        ((layout.strip.bottom() - 6.0 - pos.y) / usable).clamp(0.01, 1.0);
                    session.set_velocities(&ids, velocity);
                }
            }
            Drag::End if primary_down => {
                if let Some(pos) = pos {
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "a small count of steps, clamped"
                    )]
                    let steps = ((pos.x - layout.grid.left() + layout.scroll) / layout.step_width)
                        .round()
                        .clamp(1.0, 4096.0) as i64;
                    session.set_pattern_length(layout.step * steps);
                }
            }
            Drag::Pad if primary_down => {}
            Drag::None => {}
            _ => {
                // The button came up: the drag is over.
                if matches!(self.drag, Drag::Paint { .. } | Drag::Loudness | Drag::End) {
                    session.end_gesture();
                }
                self.frozen_width = None;
                session.pointer_off();
                self.drag = Drag::None;
            }
        }
        if let Some(pos) = pos.filter(|_| ui.rect_contains_pointer(layout.whole)) {
            let icon = if self.drag == Drag::End || layout.near_end(pos) {
                CursorIcon::ResizeHorizontal
            } else if layout.grid.contains(pos) || layout.strip.contains(pos) {
                CursorIcon::PointingHand
            } else {
                CursorIcon::Default
            };
            ui.ctx().set_cursor_icon(icon);
        }
    }

    /// The sounds' names: click to hear and adjust.
    fn pad_names(&mut self, ui: &Ui, session: &mut Session, grid: &Grid<'_>) {
        let layout = grid.layout;
        for (row, pad) in grid.rows.iter().enumerate() {
            if !layout.row_visible(row) {
                continue;
            }
            let rect = Rect::from_min_max(
                pos2(layout.names.left(), layout.row_y(row)),
                pos2(layout.names.right(), layout.row_y(row) + layout.row_height),
            )
            .intersect(layout.names);
            let id = ui.scope_id().with(("pad", row));
            let response = ui.interact(rect, id, Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    self.chosen == pad.key,
                    &pad.name,
                )
            });
            // The sound plays on the press itself, once per click (also a
            // click so quick it is pressed and released between two frames).
            let pressed = response.hovered()
                && ui.input(|i| i.pointer.button_pressed(PointerButton::Primary));
            if pressed && self.drag == Drag::None {
                session.pointer_on(pad.key, session.keyboard_velocity());
                self.drag = Drag::Pad;
            }
            if response.clicked() {
                self.chosen = pad.key;
                self.settings = match self.settings {
                    Some((open, _)) if open == pad.key => None,
                    _ => Some((pad.key, rect.right_top())),
                };
            }
            response.on_hover_text(text::PAD_HINT);
        }
    }

    /// Gives every square a place in the accessibility tree.
    fn describe_cells(ui: &Ui, session: &Session, grid: &Grid<'_>, canvas: egui::Id) {
        // Only when something reads the accessibility tree.
        if ui.ctx().accesskit_node_builder(canvas, |_| ()).is_none() {
            return;
        }
        let layout = grid.layout;
        for (row, pad) in grid.rows.iter().enumerate() {
            for step in 0..layout.steps {
                let rect = layout.cell(row, step);
                if !layout.grid.intersects(rect) {
                    continue;
                }
                let id = ui.scope_id().with(("cell", row, step));
                let on = Self::hit_velocity(session, grid, pad.key, step).is_some();
                let response = ui.interact(rect.intersect(layout.grid), id, Sense::hover());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::CheckBox,
                        true,
                        on,
                        text::step_cell(&pad.name, step + 1),
                    )
                });
            }
        }
    }

    fn paint_cells(&self, ui: &Ui, session: &Session, grid: &Grid<'_>) {
        let layout = grid.layout;
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(layout.grid);
        let meter = session.pattern_meter();
        let swing = session.swing();
        let playing = session.is_playing();
        let play_step = playing
            .then(|| usize::try_from(session.position().0 / layout.step.0.max(1)).ok())
            .flatten();
        let hover = ui.input(|i| i.pointer.hover_pos());
        for (row, pad) in grid.rows.iter().enumerate() {
            if !layout.row_visible(row) {
                continue;
            }
            let muted = session.pad(session.track_id(), pad.key).muted;
            for step in 0..layout.steps {
                let cell = layout.cell(row, step);
                if !layout.grid.intersects(cell) {
                    continue;
                }
                let start = layout.step_start(step);
                let at = meter.position(start);
                let shade = if at.beat.is_multiple_of(2) {
                    p.raised
                } else {
                    p.raised_hover
                };
                let inner = cell.shrink2(vec2(2.0, 2.0));
                let velocity = Self::hit_velocity(session, grid, pad.key, step);
                match velocity {
                    Some(v) => {
                        let color = if muted { p.text_weak } else { p.accent };
                        // A swung hit sits late in its square, as it sounds.
                        #[allow(clippy::cast_precision_loss, reason = "a fraction of a step")]
                        let late = ((swing.apply(start) - start).0 as f32
                            / layout.step.0.max(1) as f32
                            * cell.width())
                        .min(inner.width() * 0.6);
                        if late > 0.5 {
                            painter.rect_filled(inner, CornerRadius::same(4), shade);
                        }
                        let hit =
                            Rect::from_min_max(pos2(inner.left() + late, inner.top()), inner.max);
                        painter.rect_filled(
                            hit,
                            CornerRadius::same(4),
                            color.gamma_multiply(0.45 + 0.55 * v),
                        );
                    }
                    None => {
                        painter.rect_filled(inner, CornerRadius::same(4), shade);
                    }
                }
                if play_step == Some(step) {
                    painter.rect_stroke(
                        inner,
                        CornerRadius::same(4),
                        Stroke::new(1.5, p.text.gamma_multiply(0.6)),
                        StrokeKind::Inside,
                    );
                }
                if hover.is_some_and(|h| cell.contains(h)) && self.drag == Drag::None {
                    painter.rect_stroke(
                        inner,
                        CornerRadius::same(4),
                        Stroke::new(1.0, p.accent),
                        StrokeKind::Inside,
                    );
                }
            }
        }
        // Bars.
        for step in 1..layout.steps {
            let at = meter.position(layout.step_start(step));
            if at.beat == 0 && at.ticks == Ticks::ZERO {
                let x = layout.step_x(step);
                painter.vline(x, layout.grid.y_range(), Stroke::new(2.0, p.grid_bar));
            }
        }
        // The rows of the chosen sound and of the open settings.
        let chosen_row = grid.rows.iter().position(|r| r.key == self.chosen);
        if let Some(row) = chosen_row {
            let y = layout.row_y(row);
            let band = Rect::from_min_max(
                pos2(layout.grid.left(), y),
                pos2(layout.grid.right(), y + layout.row_height),
            );
            painter.rect_stroke(
                band,
                CornerRadius::same(4),
                Stroke::new(1.0, p.accent.gamma_multiply(0.35)),
                StrokeKind::Inside,
            );
        }
    }

    fn paint_ruler(&self, ui: &Ui, session: &Session, layout: &Layout) {
        let p = Palette::of(ui.ctx());
        let ruler = layout.ruler;
        let painter = ui.painter_at(ruler);
        painter.rect_filled(ruler, CornerRadius::ZERO, p.surface);
        let meter = session.pattern_meter();
        for step in 0..layout.steps {
            let at = meter.position(layout.step_start(step));
            if at.ticks != Ticks::ZERO {
                continue;
            }
            let x = layout.step_x(step);
            let label = if at.beat == 0 {
                (at.bar + 1).to_string()
            } else {
                format!("{}.{}", at.bar + 1, at.beat + 1)
            };
            let color = if at.beat == 0 { p.text } else { p.text_weak };
            painter.text(
                pos2(x + 6.0, ruler.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(size::SMALL),
                color,
            );
        }
        if session.is_playing() {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "a screen position"
            )]
            let x = layout.grid.left()
                + ((session.position().0 as f64 / layout.step.0.max(1) as f64)
                    * f64::from(layout.step_width)) as f32
                - layout.scroll;
            let color = if session.is_recording() {
                p.record
            } else {
                p.accent
            };
            painter.vline(x, ruler.y_range(), Stroke::new(2.0, color));
        }
        // The handle at the end of the steps (decision 0018).
        let end_x = layout.step_x(layout.steps);
        if end_x <= ruler.right() + 6.0 {
            let hot = self.drag == Drag::End
                || ui
                    .input(|i| i.pointer.hover_pos())
                    .is_some_and(|pos| layout.near_end(pos));
            let tab = Rect::from_min_max(
                pos2(end_x - 4.0, ruler.top() + 5.0),
                pos2(end_x + 4.0, ruler.bottom() - 2.0),
            );
            painter.rect_filled(
                tab,
                CornerRadius::same(2),
                if hot { p.accent } else { p.grid_bar },
            );
        }
        let corner =
            Rect::from_min_max(layout.whole.min, pos2(layout.names.right(), ruler.bottom()));
        ui.painter_at(corner)
            .rect_filled(corner, CornerRadius::ZERO, p.surface);
        ui.painter().hline(
            layout.whole.x_range(),
            ruler.bottom() - 0.5,
            Stroke::new(1.0, p.divider),
        );
    }

    fn paint_names(&self, ui: &Ui, session: &Session, labels: &KeyLabels, grid: &Grid<'_>) {
        let layout = grid.layout;
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(layout.names);
        painter.rect_filled(layout.names, CornerRadius::ZERO, p.surface);
        let font = FontId::proportional(if layout.row_height < 20.0 {
            size::SMALL
        } else {
            size::BODY
        });
        let lit_color = p.accent;
        for (row, pad) in grid.rows.iter().enumerate() {
            if !layout.row_visible(row) {
                continue;
            }
            let y = layout.row_y(row);
            let rect = Rect::from_min_max(
                pos2(layout.names.left(), y),
                pos2(layout.names.right(), y + layout.row_height),
            )
            .shrink2(vec2(6.0, 1.0));
            let setting = session.pad(session.track_id(), pad.key);
            let lit = session.is_key_lit(pad.key);
            let open = self.settings.is_some_and(|(key, _)| key == pad.key);
            if self.chosen == pad.key || open {
                painter.rect_filled(rect, CornerRadius::same(4), p.raised);
            }
            // The computer key that plays it, in a small key cap.
            let cap = Rect::from_min_size(
                pos2(rect.left() + 2.0, rect.center().y - 9.0),
                vec2(22.0, 18.0),
            );
            if let Some(key) = computer_key(session, labels, pad.key) {
                let fill = if lit { lit_color } else { p.background };
                painter.rect_filled(cap, CornerRadius::same(3), fill);
                painter.text(
                    cap.center(),
                    Align2::CENTER_CENTER,
                    key,
                    FontId::monospace(size::SMALL - 1.0),
                    if lit { p.on_accent } else { p.text_weak },
                );
            }
            let name_color = if setting.muted { p.text_weak } else { p.text };
            painter.text(
                pos2(cap.right() + 8.0, rect.center().y),
                Align2::LEFT_CENTER,
                &pad.name,
                font.clone(),
                name_color,
            );
            if setting != PadSetting::new(pad.key) {
                // A dot: this sound was adjusted.
                painter.circle_filled(pos2(rect.right() - 8.0, rect.center().y), 3.0, p.accent);
            }
        }
    }

    fn paint_strip(&self, ui: &Ui, session: &Session, grid: &Grid<'_>) {
        let layout = grid.layout;
        let p = Palette::of(ui.ctx());
        let whole = Rect::from_min_max(
            pos2(layout.whole.left(), layout.strip.top() - STRIP_GAP),
            layout.whole.max,
        );
        ui.painter()
            .rect_filled(whole, CornerRadius::ZERO, p.surface);
        ui.painter().hline(
            whole.x_range(),
            whole.top() + 0.5,
            Stroke::new(1.0, p.divider),
        );
        let label = layout.strip_label;
        ui.painter().text(
            pos2(label.left() + 10.0, label.center().y - 9.0),
            Align2::LEFT_CENTER,
            text::LOUDNESS,
            FontId::proportional(size::SMALL),
            p.text_weak,
        );
        ui.painter().text(
            pos2(label.left() + 10.0, label.center().y + 9.0),
            Align2::LEFT_CENTER,
            grid.rows
                .iter()
                .find(|r| r.key == self.chosen)
                .map_or("", |r| r.name.as_str()),
            FontId::proportional(size::BODY),
            p.text,
        );
        let strip = layout.strip;
        let painter = ui.painter_at(strip);
        painter.rect_filled(strip.shrink2(vec2(0.0, 2.0)), CornerRadius::ZERO, p.grid);
        let usable = strip.height() - 12.0;
        for step in 0..layout.steps {
            let Some(velocity) = Self::hit_velocity(session, grid, self.chosen, step) else {
                continue;
            };
            let x0 = layout.step_x(step) + 4.0;
            let x1 = layout.step_x(step) + layout.step_width - 4.0;
            let top = strip.bottom() - 6.0 - usable * velocity;
            let bar = Rect::from_min_max(pos2(x0, top), pos2(x1, strip.bottom() - 6.0));
            painter.rect_filled(bar, CornerRadius::same(3), p.accent.gamma_multiply(0.85));
        }
        let response = ui.interact(strip, ui.scope_id().with("loudness"), Sense::hover());
        response.on_hover_text(text::LOUDNESS_HINT);
    }

    fn paint_hint(&self, ui: &Ui, session: &Session, labels: &KeyLabels, grid: &Grid<'_>) {
        let layout = grid.layout;
        let p = Palette::of(ui.ctx());
        // A custom kit with no sounds yet says how to add some.
        if grid.custom && grid.rows.is_empty() {
            let painter = ui.painter_at(layout.whole);
            let middle = layout.whole.center();
            painter.text(
                middle - vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                text::EMPTY_KIT_HINT,
                FontId::proportional(size::BODY),
                p.text_weak,
            );
            painter.text(
                middle + vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                text::EMPTY_KIT_DETAIL,
                FontId::proportional(size::SMALL),
                p.text_weak.gamma_multiply(0.8),
            );
            return;
        }
        if !session.pattern().notes.is_empty() || self.is_dragging() || session.is_recording() {
            return;
        }
        // In the loudness strip, which is empty until there are hits.
        let below = layout.strip.center();
        let painter = ui.painter_at(layout.strip);
        let name = |sound: PadSound| computer_key(session, labels, sound.key()).unwrap_or_default();
        painter.text(
            below - vec2(0.0, 12.0),
            Align2::CENTER_CENTER,
            text::EMPTY_BEAT_HINT,
            FontId::proportional(size::BODY),
            p.text_weak,
        );
        let keys = match grid.rows.first() {
            Some(first) if grid.custom => text::kit_keys_hint(
                &computer_key(session, labels, first.key).unwrap_or_default(),
                &first.name,
            ),
            _ => text::beat_keys_hint(
                &name(PadSound::Kick),
                &name(PadSound::Snare),
                &name(PadSound::ClosedHat),
            ),
        };
        painter.text(
            below + vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            keys,
            FontId::proportional(size::SMALL),
            p.text_weak.gamma_multiply(0.8),
        );
    }

    /// The settings of the sound whose name was clicked, in a small panel.
    fn pad_settings(&mut self, ui: &Ui, session: &mut Session, grid: &Grid<'_>) {
        let Some((key, anchor)) = self.settings else {
            return;
        };
        let Some(name) = grid
            .rows
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.name.clone())
        else {
            self.settings = None; // Its pad was removed.
            return;
        };
        let track = session.track_id();
        let mut setting = session.pad(track, key);
        let before = setting;
        let mut close = false;
        let mut remove = false;
        let mut gesture = Gesture::default();
        let area = egui::Area::new(ui.scope_id().with("pad-settings"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor + vec2(8.0, 0.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(260.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&name).strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button(text::CLOSE).clicked() {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.add_space(4.0);
                        pad_sliders(ui, &mut setting, &mut gesture);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut setting.muted, text::PAD_MUTED);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button(text::RESET_SOUND).clicked() {
                                        setting = PadSetting::new(key);
                                    }
                                },
                            );
                        });
                        if grid.custom {
                            ui.add_space(6.0);
                            if ui
                                .button(text::REMOVE_PAD)
                                .on_hover_text(text::REMOVE_PAD_HINT)
                                .clicked()
                            {
                                remove = true;
                            }
                        }
                    });
            });
        if gesture.started {
            session.begin_gesture(text::EDIT_PAD);
        }
        if setting != before {
            session.set_pad(track, setting);
        }
        if gesture.stopped {
            session.end_gesture();
        }
        if remove {
            session.remove_kit_pad(track, key);
            self.settings = None;
            return;
        }
        let clicked_elsewhere = ui.input(|i| i.pointer.any_pressed())
            && !area.response.contains_pointer()
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| p.x > anchor.x);
        if close || clicked_elsewhere {
            self.settings = None;
        }
    }
}

/// A pad's volume, position, pitch and length sliders.
fn pad_sliders(ui: &mut Ui, setting: &mut PadSetting, gesture: &mut Gesture) {
    egui::Grid::new("pad-sliders")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            let sliders: [Slider<'_>; 4] = [
                (
                    text::PAD_VOLUME,
                    &mut setting.gain_db,
                    -30.0..=12.0,
                    text::db,
                ),
                (text::PAD_PAN, &mut setting.pan, -1.0..=1.0, text::pan),
                (
                    text::PAD_TUNE,
                    &mut setting.tune,
                    -12.0..=12.0,
                    text::semitones,
                ),
                (
                    text::PAD_LENGTH,
                    &mut setting.length,
                    0.05..=1.0,
                    text::percent,
                ),
            ];
            for (label, value, range, format) in sliders {
                gesture.add(&pad_slider(ui, label, value, range, format));
            }
        });
}

/// A pad setting's slider: label, value, range and how the value reads.
type Slider<'a> = (
    &'static str,
    &'a mut f32,
    std::ops::RangeInclusive<f32>,
    fn(f32) -> String,
);

/// A labelled slider in a two-column grid.
fn pad_slider(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    format: fn(f32) -> String,
) -> Response {
    let name = ui.label(label);
    let response = ui
        .add(
            egui::Slider::new(value, range)
                .custom_formatter(move |v, _| {
                    #[allow(clippy::cast_possible_truncation, reason = "a display value")]
                    let v = v as f32;
                    format(v)
                })
                .clamping(egui::SliderClamping::Always),
        )
        .labelled_by(name.id);
    ui.end_row();
    response
}

/// The label of the computer key that plays `key` on this track, if any.
fn computer_key(session: &Session, labels: &KeyLabels, key: u8) -> Option<String> {
    let offset = session.computer_key_for(key)?;
    NOTE_KEYS
        .iter()
        .find(|(_, o)| *o == offset)
        .map(|(physical, _)| labels.label(*physical))
}

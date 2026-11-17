// SPDX-License-Identifier: GPL-3.0-or-later

//! The step sequencer (decision 0014): the kit's sounds down the left, the
//! steps of the pattern across, one square for each.
//!
//! - click a square: a hit appears there and plays; click it again to
//!   remove it; drag along the row to add (or remove) several at once;
//! - right-click a square to remove its hit;
//! - click a sound's name to hear it, choose it for the loudness strip, and
//!   change its sound (volume, pan, tune, length);
//! - drag the bars in the loudness strip to change how hard hits play.
//!
//! Each square is also in the accessibility tree ("Kick, step 1"), so
//! screen readers and the interface tests can find it.

use daw_dsp::drums::PadSound;
use daw_model::{NoteId, PadSetting, Ticks};
use egui::{
    Align2, CornerRadius, CursorIcon, FontId, PointerButton, Pos2, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::keys::{KeyLabels, NOTE_KEYS};
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
}

/// The step sequencer's state between frames.
#[derive(Debug)]
pub struct StepGrid {
    /// The sound whose loudness the strip shows.
    chosen: PadSound,
    drag: Drag,
    /// The sound whose settings are open, and where its name is.
    settings: Option<(PadSound, Pos2)>,
    /// How far the steps are scrolled to the left, in points.
    scroll: f32,
}

impl Default for StepGrid {
    fn default() -> Self {
        Self {
            chosen: PadSound::Kick,
            drag: Drag::None,
            settings: None,
            scroll: 0.0,
        }
    }
}

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
}

impl Layout {
    fn new(whole: Rect, session: &Session, scroll: f32) -> Self {
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
        let row_height = (room / PAD_ORDER.len() as f32).clamp(MIN_ROW_HEIGHT, MAX_ROW_HEIGHT);
        #[allow(clippy::cast_precision_loss, reason = "small counts")]
        let step_width =
            ((whole.right() - left - 8.0) / steps as f32).clamp(MIN_STEP_WIDTH, MAX_STEP_WIDTH);
        #[allow(clippy::cast_precision_loss, reason = "small counts")]
        let rows_bottom = whole.top() + RULER_HEIGHT + row_height * PAD_ORDER.len() as f32;
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
        }
    }

    #[allow(clippy::cast_precision_loss, reason = "small counts")]
    fn step_x(&self, step: usize) -> f32 {
        self.grid.left() + step as f32 * self.step_width - self.scroll
    }

    #[allow(clippy::cast_precision_loss, reason = "small counts")]
    fn row_y(&self, row: usize) -> f32 {
        self.grid.top() + row as f32 * self.row_height
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
        let at = ((y - self.grid.top()) / self.row_height).floor();
        (at >= 0.0 && (at as usize) < PAD_ORDER.len()).then_some(at as usize)
    }

    fn step_start(&self, step: usize) -> Ticks {
        self.step * i64::try_from(step).unwrap_or(0)
    }
}

impl StepGrid {
    /// Shows the step sequencer in all the space `ui` has.
    pub fn ui(&mut self, ui: &mut Ui, session: &mut Session, labels: &KeyLabels) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, text::STEP_GRID));
        let layout = Layout::new(rect, session, self.scroll);
        self.scroll = layout.scroll;
        self.handle_scroll(ui, &layout);
        self.pad_names(ui, session, &layout);
        self.handle_pointer(ui, &response, &layout, session);
        Self::describe_cells(ui, session, &layout, response.id);

        let p = Palette::of(ui.ctx());
        ui.painter_at(rect)
            .rect_filled(rect, CornerRadius::ZERO, p.grid);
        self.paint_cells(ui, session, &layout);
        Self::paint_ruler(ui, session, &layout);
        self.paint_names(ui, session, labels, &layout);
        self.paint_strip(ui, session, &layout);
        self.paint_hint(ui, session, labels, &layout);
        self.pad_settings(ui, session);
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

    fn handle_scroll(&mut self, ui: &Ui, layout: &Layout) {
        if !ui.rect_contains_pointer(layout.whole) {
            return;
        }
        let delta = ui.input(|i| i.smooth_scroll_delta);
        self.scroll = (self.scroll - delta.x - delta.y).max(0.0);
    }

    /// The hits of a sound in a step.
    fn hits(session: &Session, sound: PadSound, layout: &Layout, step: usize) -> Vec<NoteId> {
        let from = layout.step_start(step);
        let to = from + layout.step;
        session
            .pattern()
            .notes
            .iter()
            .filter(|n| n.start >= from && n.start < to)
            .filter(|n| PadSound::from_key(n.key) == Some(sound))
            .map(|n| n.id)
            .collect()
    }

    /// How hard the hit in a step plays, if there is one.
    fn hit_velocity(
        session: &Session,
        sound: PadSound,
        layout: &Layout,
        step: usize,
    ) -> Option<f32> {
        let from = layout.step_start(step);
        let to = from + layout.step;
        session
            .pattern()
            .notes
            .iter()
            .filter(|n| n.start >= from && n.start < to)
            .filter(|n| PadSound::from_key(n.key) == Some(sound))
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
        layout: &Layout,
        session: &mut Session,
    ) {
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
            if let (Some(row), Some(step)) = (layout.row_at(pos.y), layout.step_at(pos.x))
                && layout.grid.contains(pos)
            {
                let sound = PAD_ORDER[row];
                let existing = Self::hits(session, sound, layout, step);
                let on = existing.is_empty();
                session.begin_gesture(if on {
                    text::EDIT_PAINT_HITS
                } else {
                    text::EDIT_REMOVE_HIT
                });
                if on {
                    session.add_hit(
                        sound.key(),
                        layout.step_start(step),
                        session.keyboard_velocity(),
                    );
                    session.pointer_on(sound.key(), session.keyboard_velocity());
                } else {
                    session.remove_notes(&existing);
                }
                self.chosen = sound;
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
            let existing = Self::hits(session, PAD_ORDER[row], layout, step);
            session.remove_notes(&existing);
        }
        match self.drag {
            Drag::Paint { row, on, last } if primary_down => {
                if let Some(step) = pos.and_then(|p| layout.step_at(p.x))
                    && step != last
                {
                    let sound = PAD_ORDER[row];
                    let (low, high) = if step > last {
                        (last + 1, step)
                    } else {
                        (step, last - 1)
                    };
                    for step in low..=high {
                        let existing = Self::hits(session, sound, layout, step);
                        if on && existing.is_empty() {
                            session.add_hit(
                                sound.key(),
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
                    let ids = Self::hits(session, self.chosen, layout, step);
                    let usable = layout.strip.height() - 12.0;
                    let velocity =
                        ((layout.strip.bottom() - 6.0 - pos.y) / usable).clamp(0.01, 1.0);
                    session.set_velocities(&ids, velocity);
                }
            }
            Drag::Pad if primary_down => {}
            Drag::None => {}
            _ => {
                // The button came up: the drag is over.
                if matches!(self.drag, Drag::Paint { .. } | Drag::Loudness) {
                    session.end_gesture();
                }
                session.pointer_off();
                self.drag = Drag::None;
            }
        }
        if let Some(pos) = pos.filter(|_| ui.rect_contains_pointer(layout.whole)) {
            let icon = if layout.grid.contains(pos) || layout.strip.contains(pos) {
                CursorIcon::PointingHand
            } else {
                CursorIcon::Default
            };
            ui.ctx().set_cursor_icon(icon);
        }
    }

    /// The sounds' names: click to hear and adjust.
    fn pad_names(&mut self, ui: &Ui, session: &mut Session, layout: &Layout) {
        for (row, &sound) in PAD_ORDER.iter().enumerate() {
            let rect = Rect::from_min_max(
                pos2(layout.names.left(), layout.row_y(row)),
                pos2(layout.names.right(), layout.row_y(row) + layout.row_height),
            );
            if rect.bottom() > layout.names.bottom() + 0.5 {
                break;
            }
            let id = ui.scope_id().with(("pad", row));
            let response = ui.interact(rect, id, Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    self.chosen == sound,
                    text::pad_name(sound),
                )
            });
            if response.is_pointer_button_down_on() && self.drag == Drag::None {
                session.pointer_on(sound.key(), session.keyboard_velocity());
                self.drag = Drag::Pad;
            }
            if response.clicked() {
                self.chosen = sound;
                self.settings = match self.settings {
                    Some((open, _)) if open == sound => None,
                    _ => Some((sound, rect.right_top())),
                };
            }
            response.on_hover_text(text::PAD_HINT);
        }
    }

    /// Gives every square a place in the accessibility tree.
    fn describe_cells(ui: &Ui, session: &Session, layout: &Layout, canvas: egui::Id) {
        // Only when something reads the accessibility tree.
        if ui.ctx().accesskit_node_builder(canvas, |_| ()).is_none() {
            return;
        }
        for (row, &sound) in PAD_ORDER.iter().enumerate() {
            for step in 0..layout.steps {
                let rect = layout.cell(row, step);
                if !layout.grid.intersects(rect) {
                    continue;
                }
                let id = ui.scope_id().with(("cell", row, step));
                let on = Self::hit_velocity(session, sound, layout, step).is_some();
                let response = ui.interact(rect.intersect(layout.grid), id, Sense::hover());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::CheckBox,
                        true,
                        on,
                        text::step_cell(text::pad_name(sound), step + 1),
                    )
                });
            }
        }
    }

    fn paint_cells(&self, ui: &Ui, session: &Session, layout: &Layout) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(layout.grid);
        let meter = &session.project().meter;
        let signature = meter.initial();
        let beat = signature.beat_length();
        let bar = signature.bar_length();
        let playing = session.is_playing();
        let play_step = playing
            .then(|| usize::try_from(session.position().0 / layout.step.0.max(1)).ok())
            .flatten();
        let hover = ui.input(|i| i.pointer.hover_pos());
        for (row, &sound) in PAD_ORDER.iter().enumerate() {
            let muted = session.pad(session.track_id(), sound.key()).muted;
            for step in 0..layout.steps {
                let cell = layout.cell(row, step);
                if !layout.grid.intersects(cell) {
                    continue;
                }
                let start = layout.step_start(step);
                let beat_index = start.0 / beat.0.max(1);
                let shade = if beat_index % 2 == 0 {
                    p.raised
                } else {
                    p.raised_hover
                };
                let inner = cell.shrink2(vec2(2.0, 2.0));
                let velocity = Self::hit_velocity(session, sound, layout, step);
                let fill = match velocity {
                    Some(v) => {
                        let color = if muted { p.text_weak } else { p.accent };
                        color.gamma_multiply(0.45 + 0.55 * v)
                    }
                    None => shade,
                };
                painter.rect_filled(inner, CornerRadius::same(4), fill);
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
        // Bars and beats.
        for step in 1..layout.steps {
            let start = layout.step_start(step);
            let x = layout.step_x(step);
            if start.0 % bar.0.max(1) == 0 {
                painter.vline(x, layout.grid.y_range(), Stroke::new(2.0, p.grid_bar));
            }
        }
        // The rows of the chosen sound and of the open settings.
        let chosen_row = PAD_ORDER.iter().position(|&s| s == self.chosen);
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

    fn paint_ruler(ui: &Ui, session: &Session, layout: &Layout) {
        let p = Palette::of(ui.ctx());
        let ruler = layout.ruler;
        let painter = ui.painter_at(ruler);
        painter.rect_filled(ruler, CornerRadius::ZERO, p.surface);
        let meter = &session.project().meter;
        let signature = meter.initial();
        let beat = signature.beat_length();
        for step in 0..layout.steps {
            let start = layout.step_start(step);
            if start.0 % beat.0.max(1) != 0 {
                continue;
            }
            let at = meter.position(start);
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

    fn paint_names(&self, ui: &Ui, session: &Session, labels: &KeyLabels, layout: &Layout) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(layout.names);
        painter.rect_filled(layout.names, CornerRadius::ZERO, p.surface);
        let font = FontId::proportional(if layout.row_height < 20.0 {
            size::SMALL
        } else {
            size::BODY
        });
        let lit_color = p.accent;
        for (row, &sound) in PAD_ORDER.iter().enumerate() {
            let y = layout.row_y(row);
            let rect = Rect::from_min_max(
                pos2(layout.names.left(), y),
                pos2(layout.names.right(), y + layout.row_height),
            )
            .shrink2(vec2(6.0, 1.0));
            let setting = session.pad(session.track_id(), sound.key());
            let lit = session.is_key_lit(sound.key());
            let open = self.settings.is_some_and(|(s, _)| s == sound);
            if self.chosen == sound || open {
                painter.rect_filled(rect, CornerRadius::same(4), p.raised);
            }
            // The computer key that plays it, in a small key cap.
            let cap = Rect::from_min_size(
                pos2(rect.left() + 2.0, rect.center().y - 9.0),
                vec2(22.0, 18.0),
            );
            if let Some(key) = computer_key(session, labels, sound.key()) {
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
                text::pad_name(sound),
                font.clone(),
                name_color,
            );
            if setting != PadSetting::new(sound.key()) {
                // A dot: this sound was adjusted.
                painter.circle_filled(pos2(rect.right() - 8.0, rect.center().y), 3.0, p.accent);
            }
        }
    }

    fn paint_strip(&self, ui: &Ui, session: &Session, layout: &Layout) {
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
            text::pad_name(self.chosen),
            FontId::proportional(size::BODY),
            p.text,
        );
        let strip = layout.strip;
        let painter = ui.painter_at(strip);
        painter.rect_filled(strip.shrink2(vec2(0.0, 2.0)), CornerRadius::ZERO, p.grid);
        let usable = strip.height() - 12.0;
        for step in 0..layout.steps {
            let Some(velocity) = Self::hit_velocity(session, self.chosen, layout, step) else {
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

    fn paint_hint(&self, ui: &Ui, session: &Session, labels: &KeyLabels, layout: &Layout) {
        if !session.pattern().notes.is_empty() || self.is_dragging() || session.is_recording() {
            return;
        }
        let p = Palette::of(ui.ctx());
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
        painter.text(
            below + vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            text::beat_keys_hint(
                &name(PadSound::Kick),
                &name(PadSound::Snare),
                &name(PadSound::ClosedHat),
            ),
            FontId::proportional(size::SMALL),
            p.text_weak.gamma_multiply(0.8),
        );
    }

    /// The settings of the sound whose name was clicked, in a small panel.
    fn pad_settings(&mut self, ui: &Ui, session: &mut Session) {
        let Some((sound, anchor)) = self.settings else {
            return;
        };
        let track = session.track_id();
        let mut setting = session.pad(track, sound.key());
        let before = setting;
        let mut close = false;
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
                            ui.label(RichText::new(text::pad_name(sound)).strong());
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
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut setting.muted, text::PAD_MUTED);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button(text::RESET_SOUND).clicked() {
                                        setting = PadSetting::new(sound.key());
                                    }
                                },
                            );
                        });
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! The sung pitch of a voice track (decision 0028), drawn over the piano
//! roll's notes: the line the voice sings, with its glides and vibrato,
//! faint while the notes are edited and with its handles in the Pitch mode.
//!
//! In the Pitch mode:
//! - click over a note: a pitch point there, at the height of the pointer
//!   (keep the button down to go on moving it);
//! - drag a point in time and pitch (a note's first point, after another
//!   note, leaves from wherever the voice is: it moves in time only); click
//!   it for its exact values and the curve after it; right-click it to
//!   remove it;
//! - a note's vibrato has a diamond where it starts, as high above the note
//!   as the vibrato is deep: drag it sideways to start it earlier or later,
//!   up or down to change its depth; click it for its panel; right-click it
//!   to remove the vibrato.

use daw_model::NoteId;
use daw_model::expression::{MAX_VIBRATO_DEPTH, PitchShape, VIBRATO_RATES, Vibrato};
use egui::{
    Align2, CursorIcon, DragValue, PointerButton, Pos2, Rect, Response, RichText, Sense, Shape,
    Stroke, Ui, pos2, vec2,
};

use crate::session::{LineNote, Session, SungLine, point_from};
use crate::text;
use crate::theme::Palette;

/// How close to a handle the pointer must be to take it.
const GRAB: f32 = 7.0;
/// How far the pointer must move before a press on a handle drags it.
const DRAG_THRESHOLD: f32 = 3.0;
/// The panels' width. Fixed, so nothing in them can grow them to the edge
/// of the window (which would make them slide as the window keeps them on
/// screen).
const PANEL_WIDTH: f32 = 250.0;
/// How far apart the line's points are drawn, in points.
const LINE_STEP: f32 = 2.0;

/// The shapes a point's panel offers.
const SHAPES: [(PitchShape, &str, &str); 4] = [
    (
        PitchShape::Smooth,
        text::SHAPE_SMOOTH,
        text::SHAPE_SMOOTH_HINT,
    ),
    (
        PitchShape::Straight,
        text::SHAPE_STRAIGHT,
        text::SHAPE_STRAIGHT_HINT,
    ),
    (
        PitchShape::SlowStart,
        text::SHAPE_SLOW_START,
        text::SHAPE_SLOW_START_HINT,
    ),
    (
        PitchShape::SlowEnd,
        text::SHAPE_SLOW_END,
        text::SHAPE_SLOW_END_HINT,
    ),
];

/// How the piano roll maps the pattern's time and pitch to the screen,
/// this frame.
#[derive(Clone, Copy, Debug)]
pub struct RollAxes {
    /// The grid of notes.
    pub grid: Rect,
    /// The pattern's tick at the grid's left edge.
    pub left_tick: f64,
    /// Horizontal zoom.
    pub ticks_per_point: f64,
    /// The key at the top edge of the grid (fractional).
    pub top_key: f32,
    /// The height of one key's row.
    pub key_height: f32,
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "screen coordinates from small numbers"
)]
impl RollAxes {
    /// Where a tick of the pattern is on screen.
    pub fn x(&self, tick: f64) -> f32 {
        self.grid.left() + ((tick - self.left_tick) / self.ticks_per_point) as f32
    }

    /// The tick of the pattern under `x`.
    pub fn tick(&self, x: f32) -> f64 {
        self.left_tick + f64::from(x - self.grid.left()) * self.ticks_per_point
    }

    /// The middle of the row of a (fractional) key.
    pub fn y(&self, pitch: f64) -> f32 {
        self.grid.top() + ((f64::from(self.top_key) - pitch) as f32 + 0.5) * self.key_height
    }

    /// The (fractional) key whose row's middle is at `y`.
    pub fn pitch(&self, y: f32) -> f64 {
        f64::from(self.top_key) - f64::from((y - self.grid.top()) / self.key_height - 0.5)
    }
}

/// What a handle moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grip {
    /// A pitch point, by its place among the note's points.
    Point(usize),
    /// The note's vibrato.
    Vibrato,
}

/// A handle on the line, this frame.
#[derive(Clone, Copy, Debug)]
struct Handle {
    grip: Grip,
    note: LineNote,
    at: Pos2,
    /// A first point leaving from wherever the voice is (moves in time
    /// only).
    leaves: bool,
}

/// What a mouse drag is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Drag {
    #[default]
    None,
    /// Holding a handle: a click opens its panel, a drag moves it.
    Holding {
        note: NoteId,
        grip: Grip,
        origin: Pos2,
        leaves: bool,
        /// Whether a gesture is open (the press added a point, or the drag
        /// started).
        open: bool,
        /// Whether the press added the point.
        added: bool,
        moved: bool,
    },
}

/// The small panel open for a point or a vibrato.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Panel {
    note: NoteId,
    grip: Grip,
    leaves: bool,
}

/// The pitch line's state between frames.
#[derive(Debug, Default)]
pub struct PitchLine {
    drag: Drag,
    panel: Option<(Panel, Pos2)>,
    /// Set on the frame a panel opens, so the press that opened it does not
    /// count as a press outside it.
    panel_fresh: bool,
}

impl PitchLine {
    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag != Drag::None
    }

    /// Abandons the drag in progress, putting everything back; returns
    /// whether there was one.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        let Drag::Holding { open, .. } = self.drag else {
            return false;
        };
        if open {
            session.cancel_gesture();
        }
        self.drag = Drag::None;
        true
    }

    /// Closes the open panel, if any; returns whether one was open.
    pub fn close_panel(&mut self) -> bool {
        self.panel.take().is_some()
    }

    /// The handles of the visible line: each note's pitch points, then its
    /// vibrato's diamond.
    fn handles(line: &SungLine, axes: &RollAxes) -> Vec<Handle> {
        let mut handles = Vec::new();
        let visible = axes.grid.expand(GRAB);
        for phrase in &line.phrases {
            for (index, note) in phrase.notes.iter().enumerate() {
                for (point, (seconds, pitch)) in phrase.pitch.points(index).into_iter().enumerate()
                {
                    let at = pos2(axes.x(line.tick(seconds)), axes.y(pitch));
                    if visible.contains(at) {
                        handles.push(Handle {
                            grip: Grip::Point(point),
                            note: *note,
                            at,
                            leaves: index > 0 && point == 0,
                        });
                    }
                }
                if let Some((seconds, pitch)) = phrase.pitch.vibrato_handle(index) {
                    let at = pos2(axes.x(line.tick(seconds)), axes.y(pitch));
                    if visible.contains(at) {
                        handles.push(Handle {
                            grip: Grip::Vibrato,
                            note: *note,
                            at,
                            leaves: false,
                        });
                    }
                }
            }
        }
        handles
    }

    /// The handle nearest a point, if one is close enough (a vibrato's
    /// diamond first: it sits apart from the points).
    fn handle_at(handles: &[Handle], pos: Pos2) -> Option<Handle> {
        handles
            .iter()
            .filter(|h| h.at.distance(pos) <= GRAB)
            .min_by(|a, b| {
                let rank = |h: &Handle| (h.grip != Grip::Vibrato, h.at.distance(pos));
                rank(a)
                    .partial_cmp(&rank(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
    }

    /// The note `id` of the line, as it is now.
    fn line_note(line: &SungLine, id: NoteId) -> Option<LineNote> {
        line.phrases
            .iter()
            .flat_map(|p| p.notes.iter())
            .find(|n| n.id == id)
            .copied()
    }

    /// Handles the pointer over the grid in the Pitch mode. Call it before
    /// the piano roll's own input, which leaves the grid's presses to it.
    pub fn input(
        &mut self,
        ui: &Ui,
        response: &Response,
        session: &mut Session,
        axes: &RollAxes,
        line: &SungLine,
    ) {
        let (pos, pressed, secondary, down, modifiers) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.button_pressed(PointerButton::Secondary),
                i.pointer.primary_down(),
                i.modifiers,
            )
        });
        let handles = Self::handles(line, axes);
        let selecting = modifiers.shift || modifiers.ctrl || modifiers.command;
        if let Some(pos) = pos
            && pressed
            && !selecting
            && response.hovered()
            && axes.grid.contains(pos)
            && self.drag == Drag::None
        {
            self.press(session, axes, line, &handles, pos);
        }
        if let Some(pos) = pos
            && secondary
            && response.hovered()
            && let Some(handle) = Self::handle_at(&handles, pos)
        {
            match handle.grip {
                Grip::Point(index) => session.remove_pitch_point(handle.note.id, index),
                Grip::Vibrato => session.set_vibrato(handle.note.id, None),
            }
            self.panel = None;
        }
        self.hold(session, axes, line, pos.filter(|_| down));
    }

    /// A left press on the grid: a handle is taken, or a point is added
    /// where the pointer is, over the note singing then.
    fn press(
        &mut self,
        session: &mut Session,
        axes: &RollAxes,
        line: &SungLine,
        handles: &[Handle],
        pos: Pos2,
    ) {
        if let Some(handle) = Self::handle_at(handles, pos) {
            session.select_only([handle.note.id]);
            self.drag = Drag::Holding {
                note: handle.note.id,
                grip: handle.grip,
                origin: pos,
                leaves: handle.leaves,
                open: false,
                added: false,
                moved: false,
            };
            return;
        }
        let seconds = line.seconds(axes.tick(pos.x));
        let Some((note, place, _)) = line.note_at(seconds) else {
            return;
        };
        let (ms, cents) = point_from(&note, seconds, axes.pitch(pos.y));
        session.begin_gesture(text::EDIT_ADD_PITCH_POINT);
        let Some(index) = session.add_pitch_point(note.id, ms, cents) else {
            session.end_gesture();
            return;
        };
        session.select_only([note.id]);
        self.drag = Drag::Holding {
            note: note.id,
            grip: Grip::Point(index),
            origin: pos,
            leaves: place > 0 && index == 0,
            open: true,
            added: true,
            moved: false,
        };
    }

    /// While a handle is held: dragging moves it (`pos`, while the button is
    /// down); let go without moving, it opens its panel.
    fn hold(&mut self, session: &mut Session, axes: &RollAxes, line: &SungLine, pos: Option<Pos2>) {
        let Drag::Holding {
            note,
            grip,
            origin,
            leaves,
            open,
            added,
            moved,
        } = self.drag
        else {
            return;
        };
        let Some(pos) = pos else {
            if open {
                session.end_gesture();
            }
            if !moved && !added {
                self.panel = Some((Panel { note, grip, leaves }, origin));
                self.panel_fresh = true;
            }
            self.drag = Drag::None;
            return;
        };
        let moved = moved || pos.distance(origin) > DRAG_THRESHOLD;
        let mut open = open;
        if moved && let Some(line_note) = Self::line_note(line, note) {
            if !open {
                session.begin_gesture(match grip {
                    Grip::Point(_) => text::EDIT_MOVE_PITCH_POINT,
                    Grip::Vibrato => text::EDIT_VIBRATO,
                });
                open = true;
            }
            let seconds = line.seconds(axes.tick(pos.x));
            let (ms, cents) = point_from(&line_note, seconds, axes.pitch(pos.y));
            match grip {
                Grip::Point(index) => {
                    // A point leaving from the voice keeps its own (unused)
                    // height.
                    let cents = if leaves {
                        session
                            .note_expression(note)
                            .points()
                            .get(index)
                            .map_or(0.0, |p| p.cents)
                    } else {
                        cents
                    };
                    session.move_pitch_point(note, index, ms, cents);
                }
                Grip::Vibrato => session.drag_vibrato(note, ms / 1000.0, cents),
            }
        }
        self.drag = Drag::Holding {
            note,
            grip,
            origin,
            leaves,
            open,
            added,
            moved,
        };
    }

    /// Draws the line over the notes; in the Pitch mode, with its handles.
    pub fn paint(&self, ui: &Ui, axes: &RollAxes, line: &SungLine, editing: bool) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(axes.grid);
        let colour = if editing {
            p.text
        } else {
            p.text.gamma_multiply(0.45)
        };
        let width = if editing { 2.0 } else { 1.5 };
        let (left, right) = (axes.grid.left() - 4.0, axes.grid.right() + 4.0);
        for phrase in &line.phrases {
            let (Some(first), Some(last)) = (phrase.notes.first(), phrase.notes.last()) else {
                continue;
            };
            let from = axes.x(line.tick(first.start)).max(left);
            let to = axes.x(line.tick(last.end)).min(right);
            if from >= to {
                continue;
            }
            let mut points = Vec::new();
            let mut x = from;
            loop {
                let seconds = line.seconds(axes.tick(x));
                points.push(pos2(x, axes.y(phrase.pitch.at(seconds))));
                if x >= to {
                    break;
                }
                x = (x + LINE_STEP).min(to);
            }
            painter.add(Shape::line(points, Stroke::new(width, colour)));
        }
        if !editing {
            return;
        }
        let handles = Self::handles(line, axes);
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|h| axes.grid.contains(*h) && ui.rect_contains_pointer(axes.grid));
        let hot = hover.and_then(|h| Self::handle_at(&handles, h));
        let held = match self.drag {
            Drag::Holding { note, grip, .. } => Some((note, grip)),
            Drag::None => None,
        };
        let open = self.panel.map(|(panel, _)| (panel.note, panel.grip));
        for handle in &handles {
            let which = Some((handle.note.id, handle.grip));
            let active = which == held
                || which == open
                || hot.is_some_and(|h| (h.note.id, h.grip) == (handle.note.id, handle.grip));
            match handle.grip {
                Grip::Point(index) => {
                    let radius = if active { 5.5 } else { 4.0 };
                    if handle.leaves {
                        painter.circle_stroke(
                            handle.at,
                            radius - 1.0,
                            Stroke::new(1.5, p.text_weak),
                        );
                    } else {
                        painter.circle(handle.at, radius, p.surface, Stroke::new(2.0, p.accent));
                    }
                    Self::label(ui, handle, &text::pitch_point(handle.note.key, index), axes);
                }
                Grip::Vibrato => {
                    let r = if active { 6.5 } else { 5.0 };
                    let centre = handle.at;
                    let base = pos2(centre.x, axes.y(f64::from(handle.note.key)));
                    painter.line_segment(
                        [base, centre],
                        Stroke::new(1.0, p.accent.gamma_multiply(0.6)),
                    );
                    painter.add(Shape::convex_polygon(
                        vec![
                            centre + vec2(0.0, -r),
                            centre + vec2(r, 0.0),
                            centre + vec2(0.0, r),
                            centre + vec2(-r, 0.0),
                        ],
                        p.surface,
                        Stroke::new(2.0, p.accent),
                    ));
                    Self::label(ui, handle, &text::vibrato_handle(handle.note.key), axes);
                }
            }
        }
        if hot.is_some() || held.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        } else if hover.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        }
    }

    /// Names a handle for screen readers (and the interface tests).
    fn label(ui: &Ui, handle: &Handle, label: &str, axes: &RollAxes) {
        let target = Rect::from_center_size(handle.at, vec2(2.0 * GRAB, 2.0 * GRAB));
        let id = ui.scope_id().with((
            "pitch-handle",
            handle.note.id.0,
            match handle.grip {
                Grip::Point(index) => index,
                Grip::Vibrato => usize::MAX,
            },
        ));
        ui.interact(target.intersect(axes.grid), id, Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    }

    // ----- The panel of a point or a vibrato

    /// Shows the open panel, if any. Call it after everything else the
    /// piano roll draws, so the panel is on top.
    pub fn panel_ui(&mut self, ui: &mut Ui, session: &mut Session) {
        let Some((panel, at)) = self.panel else {
            return;
        };
        let mut close = false;
        let shown = egui::Area::new(ui.scope_id().with("pitch-panel"))
            .order(egui::Order::Foreground)
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(at + vec2(GRAB, -GRAB))
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_width(PANEL_WIDTH);
                    close = match panel.grip {
                        Grip::Point(index) => {
                            Self::point_panel(ui, session, panel.note, index, panel.leaves)
                        }
                        Grip::Vibrato => Self::vibrato_panel(ui, session, panel.note),
                    };
                });
            });
        let fresh = std::mem::take(&mut self.panel_fresh);
        let pressed_outside =
            ui.input(|i| i.pointer.any_pressed()) && !fresh && !shown.response.contains_pointer();
        if close || pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.panel = None;
        }
    }

    /// A pitch point: its time and pitch, the curve after it; returns
    /// whether to close.
    fn point_panel(
        ui: &mut Ui,
        session: &mut Session,
        note: NoteId,
        index: usize,
        leaves: bool,
    ) -> bool {
        let Some(key) = session.pattern().note(note).map(|n| n.key) else {
            return true;
        };
        let points = session.note_expression(note).points().to_vec();
        let Some(point) = points.get(index).copied() else {
            return true;
        };
        ui.label(
            RichText::new(format!("{} · {}", text::PITCH_POINT, text::note_name(key))).strong(),
        );
        let gesture = |session: &mut Session, response: &egui::Response, label: &str| {
            if response.drag_started() {
                session.begin_gesture(label);
            }
            if response.drag_stopped() {
                session.end_gesture();
            }
        };
        ui.horizontal(|ui| {
            ui.label(text::POINT_TIME)
                .on_hover_text(text::POINT_TIME_HINT);
            let mut ms = f64::from(point.ms);
            let field = ui.add(
                DragValue::new(&mut ms)
                    .speed(1.0)
                    .range(-1000.0..=60_000.0)
                    .custom_formatter(|value, _| text::with_sign(value, text::MS)),
            );
            gesture(session, &field, text::EDIT_MOVE_PITCH_POINT);
            if field.changed() {
                #[allow(clippy::cast_possible_truncation, reason = "milliseconds")]
                session.move_pitch_point(note, index, ms as f32, point.cents);
            }
        });
        if leaves {
            ui.label(RichText::new(text::POINT_LEAVES).small().weak());
        } else {
            ui.horizontal(|ui| {
                ui.label(text::POINT_PITCH)
                    .on_hover_text(text::POINT_PITCH_HINT);
                let mut cents = f64::from(point.cents);
                let field = ui.add(
                    DragValue::new(&mut cents)
                        .speed(1.0)
                        .range(-2400.0..=2400.0)
                        .custom_formatter(|value, _| text::with_sign(value, text::CENTS)),
                );
                gesture(session, &field, text::EDIT_MOVE_PITCH_POINT);
                if field.changed() {
                    #[allow(clippy::cast_possible_truncation, reason = "cents")]
                    session.move_pitch_point(note, index, point.ms, cents as f32);
                }
            });
        }
        if index + 1 < points.len() {
            ui.add_space(4.0);
            ui.label(RichText::new(text::CURVE_AFTER).small().weak());
            ui.horizontal_wrapped(|ui| {
                for (shape, name, hint) in SHAPES {
                    let chosen = point.shape == shape;
                    let button = ui.selectable_label(chosen, name).on_hover_text(hint);
                    button.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            chosen,
                            text::curve_shape(name),
                        )
                    });
                    if button.clicked() {
                        session.set_pitch_shape(note, index, shape);
                    }
                }
            });
        }
        ui.separator();
        if ui.button(text::REMOVE_POINT).clicked() {
            session.remove_pitch_point(note, index);
            return true;
        }
        false
    }

    /// A note's vibrato: its depth, rate, start and growth; returns whether
    /// to close.
    fn vibrato_panel(ui: &mut Ui, session: &mut Session, note: NoteId) -> bool {
        let Some(key) = session.pattern().note(note).map(|n| n.key) else {
            return true;
        };
        let Some(vibrato) = session.note_expression(note).vibrato else {
            return true;
        };
        ui.label(RichText::new(format!("{} · {}", text::VIBRATO, text::note_name(key))).strong());
        let fields = [
            VibratoField {
                label: text::VIBRATO_DEPTH,
                hint: text::VIBRATO_DEPTH_HINT,
                value: vibrato.depth,
                range: 1.0..=f64::from(MAX_VIBRATO_DEPTH),
                speed: 0.5,
                unit: text::CENTS,
                decimals: 0,
            },
            VibratoField {
                label: text::VIBRATO_RATE,
                hint: text::VIBRATO_RATE_HINT,
                value: vibrato.rate,
                range: f64::from(*VIBRATO_RATES.start())..=f64::from(*VIBRATO_RATES.end()),
                speed: 0.02,
                unit: text::HZ,
                decimals: 1,
            },
            VibratoField {
                label: text::VIBRATO_START,
                hint: text::VIBRATO_START_HINT,
                value: vibrato.start * 1000.0,
                range: 0.0..=10_000.0,
                speed: 2.0,
                unit: text::MS,
                decimals: 0,
            },
            VibratoField {
                label: text::VIBRATO_GROWS,
                hint: text::VIBRATO_GROWS_HINT,
                value: vibrato.fade_in * 1000.0,
                range: 0.0..=5000.0,
                speed: 2.0,
                unit: text::MS,
                decimals: 0,
            },
        ];
        let mut changed = None;
        egui::Grid::new("vibrato-fields")
            .num_columns(2)
            .spacing(vec2(12.0, 4.0))
            .show(ui, |ui| {
                for (index, field) in fields.into_iter().enumerate() {
                    if let Some(value) = field.show(ui, session) {
                        changed = Some(match index {
                            0 => Vibrato {
                                depth: value,
                                ..vibrato
                            },
                            1 => Vibrato {
                                rate: value,
                                ..vibrato
                            },
                            2 => Vibrato {
                                start: value / 1000.0,
                                ..vibrato
                            },
                            _ => Vibrato {
                                fade_in: value / 1000.0,
                                ..vibrato
                            },
                        });
                    }
                }
            });
        if let Some(vibrato) = changed {
            session.set_vibrato(note, Some(vibrato));
        }
        ui.separator();
        if ui.button(text::REMOVE_THE_VIBRATO).clicked() {
            session.set_vibrato(note, None);
            return true;
        }
        false
    }
}

/// One number of a vibrato's panel.
struct VibratoField {
    label: &'static str,
    hint: &'static str,
    value: f32,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
    unit: &'static str,
    decimals: usize,
}

impl VibratoField {
    /// Shows it on a row of the panel's grid; its new value if changed.
    fn show(self, ui: &mut Ui, session: &mut Session) -> Option<f32> {
        ui.label(self.label).on_hover_text(self.hint);
        let mut number = f64::from(self.value);
        let response = ui.add(
            DragValue::new(&mut number)
                .range(self.range)
                .speed(self.speed)
                .fixed_decimals(self.decimals)
                .suffix(format!(" {}", self.unit)),
        );
        if response.drag_started() {
            session.begin_gesture(text::EDIT_VIBRATO);
        }
        if response.drag_stopped() {
            session.end_gesture();
        }
        ui.end_row();
        #[allow(clippy::cast_possible_truncation, reason = "small numbers")]
        response.changed().then_some(number as f32)
    }
}

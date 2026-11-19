// SPDX-License-Identifier: GPL-3.0-or-later

//! The tempo line (decisions 0018 and 0026): the song's tempo drawn as a
//! line over time, in the song view's signature row and in the lane under
//! the piano roll. Steps where the tempo changes suddenly, slopes where it
//! glides, bent by each glide's shape, with *rit.* or *accel.* beside them.
//!
//! - click an empty spot: a tempo change on the nearest beat (keep the
//!   button down and drag up or down to set its tempo; Alt: exactly where
//!   the pointer is);
//! - drag a point up or down to change its tempo, or sideways to move it;
//! - click a point for its exact tempo, whether the tempo arrives there
//!   gradually and whether it glides on; right-click it to remove it;
//! - drag the handle in the middle of a glide up or down to bend it, click
//!   it for its shapes (Early, Even, Late), right-click it to make the
//!   change sudden.

use daw_model::{TEMPO_RANGE, TempoPiece, Ticks, curve_through};
use egui::{
    Align2, CursorIcon, DragValue, FontId, PointerButton, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, Ui, pos2, vec2,
};

use crate::session::Session;
use crate::text;
use crate::theme::{Palette, size};

/// Room above and below the line.
const MARGIN: f32 = 10.0;
/// How close to a point or a glide's handle the pointer must be to take it.
const GRAB: f32 = 7.0;
/// The panel's width. Fixed, so nothing in it can grow it to the edge of
/// the window (which would make it slide over as the window keeps it on
/// screen).
const PANEL_WIDTH: f32 = 240.0;
const DRAG_THRESHOLD: f32 = 4.0;
/// How wide a glide must be on screen to show its handle and its word.
const HANDLE_ROOM: f32 = 3.0 * GRAB;
const WORD_ROOM: f32 = 70.0;

/// The shapes a glide's panel offers (decision 0026): bend, name, hint.
const SHAPES: [(f64, &str, &str); 3] = [
    (-0.5, text::GLIDE_EARLY, text::GLIDE_EARLY_HINT),
    (0.0, text::GLIDE_EVEN, text::GLIDE_EVEN_HINT),
    (0.5, text::GLIDE_LATE, text::GLIDE_LATE_HINT),
];

/// How a view maps the song's time to the screen, this frame.
#[derive(Clone, Copy, Debug)]
pub struct Axis {
    /// Where `left_tick` is on screen.
    pub left: f32,
    /// The song tick at `left`.
    pub left_tick: f64,
    /// Horizontal zoom.
    pub ticks_per_point: f64,
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "screen coordinates"
)]
impl Axis {
    /// Where a tick is on screen.
    pub fn x(&self, tick: Ticks) -> f32 {
        self.x_of(tick.0 as f64)
    }

    /// Where a fractional tick is on screen.
    pub fn x_of(&self, tick: f64) -> f32 {
        self.left + ((tick - self.left_tick) / self.ticks_per_point) as f32
    }

    /// The tick under `x` (never before the song).
    pub fn tick(&self, x: f32) -> Ticks {
        Ticks((self.left_tick + f64::from(x - self.left) * self.ticks_per_point).max(0.0) as i64)
    }
}

/// What a mouse drag is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Drag {
    #[default]
    None,
    /// Holding a tempo point: a click opens it; a drag changes its tempo
    /// (up and down) or moves it (sideways), whichever it starts with.
    Point {
        at: Ticks,
        origin: Pos2,
        bpm: f64,
        /// Set once the drag has chosen: `Some(true)` moves sideways.
        sideways: Option<bool>,
        /// Whether the press added the point (its gesture is already open).
        added: bool,
    },
    /// Holding the handle of the glide that starts at `at`: a click opens
    /// its shapes, a drag up or down bends it.
    Bend {
        at: Ticks,
        origin: Pos2,
        moved: bool,
    },
}

/// The small panel open for a point or a glide.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Panel {
    Point(Ticks),
    /// The glide that starts at this change.
    Glide(Ticks),
}

/// The tempo line's state between frames.
#[derive(Debug, Default)]
pub struct TempoLane {
    drag: Drag,
    panel: Option<(Panel, Pos2)>,
    /// Set on the frame a panel opens, so the press that opened it does
    /// not count as a press outside it.
    panel_fresh: bool,
    /// Set when a panel opens, until the host asks (to close its own).
    opened: bool,
    /// Whether panels open above the line (the piano roll's lane is at the
    /// bottom of the window) rather than below it.
    above: bool,
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "screen coordinates"
)]
impl TempoLane {
    /// A tempo line whose panels open above it.
    pub fn with_panels_above() -> Self {
        Self {
            above: true,
            ..Self::default()
        }
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag != Drag::None
    }

    /// Abandons the drag in progress, putting everything back; returns
    /// whether there was one.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        let gesture = match self.drag {
            Drag::None => return false,
            Drag::Point {
                sideways, added, ..
            } => added || sideways.is_some(),
            Drag::Bend { moved, .. } => moved,
        };
        if gesture {
            session.cancel_gesture();
        }
        self.drag = Drag::None;
        true
    }

    /// Closes the open panel, if any; returns whether one was open.
    pub fn close_panel(&mut self) -> bool {
        self.panel.take().is_some()
    }

    /// Whether a panel opened since the last call (so the host closes its
    /// own panels).
    pub fn take_opened(&mut self) -> bool {
        std::mem::take(&mut self.opened)
    }

    fn open_panel(&mut self, panel: Panel, at: Pos2) {
        self.panel = Some((panel, at));
        self.panel_fresh = true;
        self.opened = true;
    }

    // ----- Geometry

    /// The tempo range the line shows: the song's tempos with room around.
    fn range(session: &Session) -> (f64, f64) {
        let changes = session.tempo_changes();
        let low = changes.iter().map(|c| c.bpm).fold(f64::INFINITY, f64::min);
        let high = changes
            .iter()
            .map(|c| c.bpm)
            .fold(f64::NEG_INFINITY, f64::max);
        let middle = f64::midpoint(low, high);
        let half = ((high - low) / 2.0 + 10.0).max(15.0);
        (
            (middle - half).max(*TEMPO_RANGE.start()),
            (middle + half).min(*TEMPO_RANGE.end()),
        )
    }

    fn y(strip: Rect, range: (f64, f64), bpm: f64) -> f32 {
        let usable = f64::from(strip.height() - 2.0 * MARGIN);
        let fraction = ((bpm - range.0) / (range.1 - range.0).max(1e-6)).clamp(0.0, 1.0);
        strip.bottom() - MARGIN - (fraction * usable) as f32
    }

    /// The tempo at height `y` (not kept in the strip).
    fn bpm_at_y(strip: Rect, range: (f64, f64), y: f32) -> f64 {
        let usable = f64::from(strip.height() - 2.0 * MARGIN).max(1.0);
        range.0 + f64::from(strip.bottom() - MARGIN - y) / usable * (range.1 - range.0)
    }

    /// Where a tempo change added or moved to `x` goes: on a beat, or
    /// exactly with Alt.
    fn snapped(session: &Session, axis: &Axis, x: f32, alt: bool) -> Ticks {
        let at = axis.tick(x);
        if alt {
            return at;
        }
        let meter = &session.project().meter;
        meter.grid_round(at, meter.signature_at(at).beat_length())
    }

    fn point_at(
        session: &Session,
        strip: Rect,
        axis: &Axis,
        range: (f64, f64),
        pos: Pos2,
    ) -> Option<Ticks> {
        session
            .tempo_changes()
            .iter()
            .map(|c| (c.at, pos2(axis.x(c.at), Self::y(strip, range, c.bpm))))
            .filter(|(_, p)| p.distance(pos) <= GRAB)
            .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
            .map(|(at, _)| at)
    }

    /// The glides that move, each with the change it starts at.
    fn glides(session: &Session) -> Vec<(Ticks, TempoPiece)> {
        let tempo = &session.project().tempo;
        tempo
            .changes()
            .iter()
            .enumerate()
            .filter(|(_, c)| c.glide)
            .map(|(index, c)| (c.at, tempo.piece(index)))
            .filter(|(_, piece)| piece.glides())
            .collect()
    }

    /// The glide that starts at `at`, if it moves.
    fn glide_from(session: &Session, at: Ticks) -> Option<TempoPiece> {
        Self::glides(session)
            .into_iter()
            .find(|(start, _)| *start == at)
            .map(|(_, piece)| piece)
    }

    /// Where a glide's handle is: half-way through it in time, on the line;
    /// none when the glide is too narrow on screen to hold one.
    fn handle(piece: &TempoPiece, strip: Rect, axis: &Axis, range: (f64, f64)) -> Option<Pos2> {
        if axis.x_of(piece.end) - axis.x_of(piece.start) < HANDLE_ROOM {
            return None;
        }
        let middle = f64::midpoint(piece.start, piece.end);
        Some(pos2(
            axis.x_of(middle),
            Self::y(strip, range, piece.bpm_at(middle)),
        ))
    }

    fn handle_at(
        session: &Session,
        strip: Rect,
        axis: &Axis,
        range: (f64, f64),
        pos: Pos2,
    ) -> Option<Ticks> {
        Self::glides(session)
            .into_iter()
            .filter_map(|(at, piece)| Some((at, Self::handle(&piece, strip, axis, range)?)))
            .filter(|(_, p)| p.distance(pos) <= GRAB)
            .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
            .map(|(at, _)| at)
    }

    /// Where a panel for something at `x` opens: under the line, or over it.
    fn anchor(&self, strip: Rect, x: f32) -> Pos2 {
        pos2(
            x,
            if self.above {
                strip.top()
            } else {
                strip.bottom()
            },
        )
    }

    // ----- Input and drawing

    /// Draws the tempo line in `strip` and handles its input.
    #[allow(
        clippy::too_many_lines,
        reason = "one place that decides what a press does"
    )]
    pub fn ui(&mut self, ui: &mut Ui, session: &mut Session, strip: Rect, axis: &Axis) {
        let response = ui.interact(
            strip,
            ui.scope_id().with("tempo-strip"),
            Sense::click_and_drag(),
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, text::TEMPO_LINE));
        let range_id = ui.scope_id().with("tempo-range");
        // The scale holds still while something is dragged.
        let range = if self.is_dragging() {
            ui.data(|d| d.get_temp::<(f64, f64)>(range_id))
                .unwrap_or_else(|| Self::range(session))
        } else {
            Self::range(session)
        };
        let (pos, pressed, secondary, down, alt) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.button_pressed(PointerButton::Secondary),
                i.pointer.primary_down(),
                i.modifiers.alt,
            )
        });
        let here = response.is_pointer_button_down_on() || response.clicked();
        if let Some(pos) = pos
            && pressed
            && here
            && self.drag == Drag::None
            && strip.contains(pos)
        {
            ui.data_mut(|d| d.insert_temp(range_id, range));
            if let Some(at) = Self::point_at(session, strip, axis, range, pos) {
                let bpm = session
                    .tempo_changes()
                    .iter()
                    .find(|c| c.at == at)
                    .map_or(120.0, |c| c.bpm);
                self.drag = Drag::Point {
                    at,
                    origin: pos,
                    bpm,
                    sideways: None,
                    added: false,
                };
            } else if let Some(at) = Self::handle_at(session, strip, axis, range, pos) {
                self.drag = Drag::Bend {
                    at,
                    origin: pos,
                    moved: false,
                };
            } else {
                let at = Self::snapped(session, axis, pos.x, alt);
                session.begin_gesture(text::EDIT_ADD_TEMPO_CHANGE);
                let at = session.add_tempo_change(at);
                let bpm = session.project().tempo.bpm_at(at);
                self.drag = Drag::Point {
                    at,
                    origin: pos,
                    bpm,
                    sideways: Some(false),
                    added: true,
                };
            }
        }
        if let Some(pos) = pos
            && secondary
            && ui.rect_contains_pointer(strip)
        {
            if let Some(at) = Self::point_at(session, strip, axis, range, pos) {
                session.remove_tempo_change(at);
            } else if let Some(at) = Self::handle_at(session, strip, axis, range, pos) {
                session.set_tempo_glide(at, false);
            }
        }
        match self.drag {
            Drag::None => {}
            Drag::Point {
                at,
                origin,
                bpm,
                sideways,
                added,
            } => match pos {
                Some(pos) if down => {
                    let delta = pos - origin;
                    let sideways = sideways.or_else(|| {
                        (delta.length() > DRAG_THRESHOLD).then(|| {
                            session.begin_gesture(text::EDIT_MOVE_TEMPO_CHANGE);
                            delta.x.abs() > delta.y.abs() && at > Ticks::ZERO
                        })
                    });
                    let at = match sideways {
                        Some(true) => {
                            let to = Self::snapped(session, axis, pos.x, alt);
                            if to > Ticks::ZERO && to != at {
                                session.move_tempo_change(at, to)
                            } else {
                                at
                            }
                        }
                        Some(false) if (pos.y - origin.y).abs() > 0.5 || added => {
                            let usable = f64::from(strip.height() - 2.0 * MARGIN);
                            let per_point = (range.1 - range.0) / usable.max(1.0);
                            let new = (bpm + f64::from(origin.y - pos.y) * per_point).round();
                            session.set_tempo_at(at, new);
                            at
                        }
                        _ => at,
                    };
                    self.drag = Drag::Point {
                        at,
                        origin,
                        bpm,
                        sideways,
                        added,
                    };
                }
                _ => {
                    // The button came up. A gesture is open if the press
                    // added the point or the drag started moving it.
                    if added || sideways.is_some() {
                        session.end_gesture();
                    }
                    if pos.is_none_or(|p| p.distance(origin) <= DRAG_THRESHOLD) {
                        let anchor = self.anchor(strip, axis.x(at));
                        self.open_panel(Panel::Point(at), anchor);
                    }
                    self.drag = Drag::None;
                }
            },
            Drag::Bend { at, origin, moved } => match pos {
                Some(pos) if down => {
                    if moved || (pos.y - origin.y).abs() > DRAG_THRESHOLD {
                        if !moved {
                            session.begin_gesture(text::EDIT_GLIDE_SHAPE);
                        }
                        if let Some(piece) = Self::glide_from(session, at) {
                            let middle = Self::bpm_at_y(strip, range, pos.y);
                            let curve = curve_through(piece.bpm, piece.next_bpm, middle);
                            session.set_tempo_curve(at, curve);
                        }
                        self.drag = Drag::Bend {
                            at,
                            origin,
                            moved: true,
                        };
                    }
                }
                _ => {
                    if moved {
                        session.end_gesture();
                    } else if let Some(piece) = Self::glide_from(session, at) {
                        let x = axis.x_of(f64::midpoint(piece.start, piece.end));
                        let anchor = self.anchor(strip, x);
                        self.open_panel(Panel::Glide(at), anchor);
                    }
                    self.drag = Drag::None;
                }
            },
        }
        self.paint(ui, session, strip, axis, range, response);
    }

    #[allow(clippy::too_many_lines, reason = "the line, its glides, its points")]
    fn paint(
        &self,
        ui: &Ui,
        session: &Session,
        strip: Rect,
        axis: &Axis,
        range: (f64, f64),
        response: Response,
    ) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(strip);
        let project = session.project();
        let tempo = &project.tempo;
        let changes = tempo.changes();
        // The line: steps at sudden changes, curves where it glides (drawn
        // where it is on screen, a few points apart).
        let visible = (
            axis.left_tick + f64::from(strip.left() - 10.0 - axis.left) * axis.ticks_per_point,
            axis.left_tick + f64::from(strip.right() + 10.0 - axis.left) * axis.ticks_per_point,
        );
        let mut points = Vec::new();
        for (index, change) in changes.iter().enumerate() {
            let piece = tempo.piece(index);
            let y0 = Self::y(strip, range, change.bpm);
            points.push(pos2(axis.x(change.at), y0));
            match changes.get(index + 1) {
                Some(next) if piece.glides() => {
                    let (from, to) = (piece.start.max(visible.0), piece.end.min(visible.1));
                    if from < to {
                        let width = (to - from) / axis.ticks_per_point;
                        let steps = (width / 3.0).ceil().clamp(1.0, 600.0) as usize;
                        for step in 0..=steps {
                            let tick = from + (to - from) * step as f64 / steps as f64;
                            points.push(pos2(
                                axis.x_of(tick),
                                Self::y(strip, range, piece.bpm_at(tick)),
                            ));
                        }
                    }
                    points.push(pos2(axis.x(next.at), Self::y(strip, range, next.bpm)));
                }
                Some(next) => points.push(pos2(axis.x(next.at), y0)),
                None => points.push(pos2(strip.right() + 10.0, y0)),
            }
        }
        painter.add(Shape::line(points, Stroke::new(2.0, p.accent)));

        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|h| strip.contains(*h) && ui.rect_contains_pointer(strip));
        let hot_point = hover.and_then(|h| Self::point_at(session, strip, axis, range, h));
        let hot_handle = if hot_point.is_none() {
            hover.and_then(|h| Self::handle_at(session, strip, axis, range, h))
        } else {
            None
        };
        let open = self.panel.map(|(panel, _)| panel);

        // Each glide's handle, and the word a score would write beside it.
        for (at, piece) in Self::glides(session) {
            let Some(centre) = Self::handle(&piece, strip, axis, range) else {
                continue;
            };
            let slows = piece.next_bpm < piece.bpm;
            let active = open == Some(Panel::Glide(at))
                || matches!(self.drag, Drag::Bend { at: dragged, .. } if dragged == at);
            let r = if active || hot_handle == Some(at) {
                6.0
            } else {
                4.5
            };
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
            if axis.x_of(piece.end) - axis.x_of(piece.start) >= WORD_ROOM {
                let y = if centre.y - strip.top() > 18.0 {
                    centre.y - 10.0
                } else {
                    centre.y + 10.0
                };
                painter.text(
                    pos2(centre.x + 9.0, y),
                    Align2::LEFT_CENTER,
                    if slows { text::RIT } else { text::ACCEL },
                    FontId::proportional(size::SMALL),
                    p.text_weak,
                );
            }
            let meter = &project.meter;
            let label = text::glide_handle(
                slows,
                meter.position(Ticks(piece.start as i64)).bar,
                meter.position(Ticks(piece.end as i64)).bar,
            );
            let target = Rect::from_center_size(centre, vec2(2.0 * GRAB, 2.0 * GRAB));
            ui.interact(
                target.intersect(strip),
                ui.scope_id().with(("glide-handle", at.0)),
                Sense::hover(),
            )
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        }

        // The points, with their tempos.
        for change in changes {
            let centre = pos2(axis.x(change.at), Self::y(strip, range, change.bpm));
            if centre.x < strip.left() - 40.0 || centre.x > strip.right() + 4.0 {
                continue;
            }
            let active = open == Some(Panel::Point(change.at))
                || matches!(self.drag, Drag::Point { at, .. } if at == change.at);
            let radius = if active || hot_point == Some(change.at) {
                5.5
            } else {
                4.0
            };
            painter.circle(centre, radius, p.surface, Stroke::new(2.0, p.accent));
            let label_y = if centre.y - strip.top() > 16.0 {
                centre.y - 9.0
            } else {
                centre.y + 9.0
            };
            painter.text(
                pos2(centre.x + 7.0, label_y),
                Align2::LEFT_CENTER,
                text::bpm(change.bpm),
                FontId::proportional(size::SMALL),
                p.text_weak,
            );
            let bar = project.meter.position(change.at).bar;
            let id = ui.scope_id().with(("tempo-point", change.at.0));
            let target = Rect::from_center_size(centre, vec2(2.0 * GRAB, 2.0 * GRAB));
            ui.interact(target.intersect(strip), id, Sense::hover())
                .widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        true,
                        text::tempo_point(change.bpm, bar),
                    )
                });
        }

        if hot_point.is_some() || matches!(self.drag, Drag::Point { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        } else if hot_handle.is_some() || matches!(self.drag, Drag::Bend { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        } else if hover.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            response.on_hover_text(text::TEMPO_LANE_HINT);
        }
    }

    // ----- The panel of a point or a glide

    /// Shows the open panel, if any. Call it after everything else the view
    /// draws, so the panel is on top.
    pub fn panel_ui(&mut self, ui: &mut Ui, session: &mut Session) {
        let Some((panel, at)) = self.panel else {
            return;
        };
        let mut close = false;
        let area =
            egui::Area::new(ui.scope_id().with("tempo-panel")).order(egui::Order::Foreground);
        let area = if self.above {
            area.pivot(Align2::LEFT_BOTTOM)
                .fixed_pos(at - vec2(0.0, 2.0))
        } else {
            area.fixed_pos(at + vec2(0.0, 2.0))
        };
        let shown = area.show(ui.ctx(), |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| match panel {
                Panel::Point(at) => close = Self::point_panel(ui, session, at),
                Panel::Glide(at) => close = Self::glide_panel(ui, session, at),
            });
        });
        // Only the frame the panel opened on is spared.
        let fresh = std::mem::take(&mut self.panel_fresh);
        let pressed_outside = ui.input(|i| i.pointer.any_pressed())
            && !fresh
            && !shown.response.contains_pointer()
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| p != at);
        if close || pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.panel = None;
        }
    }

    /// A tempo point: its exact tempo, whether the tempo arrives there
    /// gradually and whether it glides on; returns whether to close.
    fn point_panel(ui: &mut Ui, session: &mut Session, at: Ticks) -> bool {
        let changes = session.tempo_changes().to_vec();
        let Some(index) = changes.iter().position(|c| c.at == at) else {
            return true;
        };
        let change = changes[index];
        let last = index + 1 == changes.len();
        let bar = session.project().meter.position(at).bar;
        ui.set_width(PANEL_WIDTH);
        ui.label(RichText::new(text::from_bar(bar)).strong());
        ui.horizontal(|ui| {
            let mut bpm = change.bpm;
            let field = ui.add(
                DragValue::new(&mut bpm)
                    .range(TEMPO_RANGE)
                    .speed(0.2)
                    .custom_formatter(|value, _| text::bpm(value)),
            );
            if field.drag_started() {
                session.begin_gesture(text::EDIT_TEMPO);
            }
            if field.changed() {
                session.set_tempo_at(at, bpm);
            }
            if field.drag_stopped() {
                session.end_gesture();
            }
            ui.label(RichText::new(text::BPM).weak());
        });
        if let Some(previous) = index.checked_sub(1).map(|i| changes[i]) {
            let mut arrive = previous.glide;
            let checkbox = ui
                .checkbox(&mut arrive, text::ARRIVE_GRADUALLY)
                .on_hover_text(text::ARRIVE_GRADUALLY_HINT);
            if checkbox.changed() {
                session.set_tempo_glide(previous.at, arrive);
            }
        }
        let mut glide = change.glide;
        let checkbox = ui
            .add_enabled(!last, egui::Checkbox::new(&mut glide, text::GLIDE))
            .on_hover_text(text::GLIDE_HINT);
        if checkbox.changed() {
            session.set_tempo_glide(at, glide);
        }
        let mut close = false;
        if at > Ticks::ZERO {
            ui.separator();
            if ui.button(text::REMOVE_TEMPO_CHANGE).clicked() {
                session.remove_tempo_change(at);
                close = true;
            }
        }
        close
    }

    /// A glide: where it goes, its shape; returns whether to close.
    fn glide_panel(ui: &mut Ui, session: &mut Session, at: Ticks) -> bool {
        let project = session.project();
        let changes = project.tempo.changes();
        let Some(index) = changes.iter().position(|c| c.at == at) else {
            return true;
        };
        let change = changes[index];
        let Some(next) = changes.get(index + 1).copied() else {
            return true;
        };
        if !change.glide {
            return true;
        }
        let from = project.meter.position(at).bar;
        let to = project.meter.position(next.at).bar;
        ui.set_width(PANEL_WIDTH);
        ui.label(RichText::new(text::glide_handle(next.bpm < change.bpm, from, to)).strong());
        ui.label(RichText::new(text::glide_tempos(change.bpm, next.bpm)).weak());
        ui.add_space(4.0);
        ui.label(RichText::new(text::GLIDE_SHAPE).small().weak());
        ui.horizontal(|ui| {
            for (curve, name, hint) in SHAPES {
                let chosen = (change.curve - curve).abs() < 0.005;
                if ui
                    .selectable_label(chosen, name)
                    .on_hover_text(hint)
                    .clicked()
                {
                    session.set_tempo_curve(at, curve);
                }
            }
        });
        ui.label(RichText::new(text::GLIDE_SHAPE_HINT).small().weak());
        ui.separator();
        let mut close = false;
        if ui.button(text::MAKE_SUDDEN).clicked() {
            session.set_tempo_glide(at, false);
            close = true;
        }
        close
    }
}

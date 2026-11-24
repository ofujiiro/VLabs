// SPDX-License-Identifier: GPL-3.0-or-later

//! A controller's curve in the piano roll's lane (decision 0033): what the
//! pitch wheel, the modulation wheel or another controller does as the
//! pattern plays, recorded from a MIDI controller or drawn. A click adds a
//! point, a drag moves it (on the grid unless Alt is held), a right-click
//! removes it; in between the curve is a straight line.

use daw_model::{ControlKind, ControlPoint, Ticks};
use egui::{PointerButton, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::session::Session;
use crate::tempo_lane::Axis;
use crate::text;
use crate::theme::Palette;

/// Room kept above and below the curve.
const MARGIN: f32 = 5.0;
/// How near a point the pointer must be to take it.
const GRAB: f32 = 7.0;
/// How far the pointer moves before a press on a point becomes a drag.
const DRAG_THRESHOLD: f32 = 3.0;
/// How near its resting place (as a share of the range) a dragged value
/// lands on it: a bend let go near the middle is exactly unbent.
const REST_SNAP: f32 = 0.03;

/// A point being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    control: ControlKind,
    index: usize,
    origin: Pos2,
    /// Whether it moved (or was just added, its gesture open).
    moving: bool,
}

/// The curve editor's state between frames.
#[derive(Debug, Default)]
pub struct ControlLaneView {
    drag: Option<Drag>,
}

#[allow(
    clippy::cast_precision_loss,
    reason = "screen coordinates and tick positions"
)]
impl ControlLaneView {
    /// Whether a point is being dragged.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Abandons a drag, putting the curve back as it was.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        match self.drag.take() {
            Some(drag) => {
                if drag.moving {
                    session.cancel_gesture();
                }
                true
            }
            None => false,
        }
    }

    /// The height of a value in `strip`.
    fn y_of(control: ControlKind, strip: Rect, value: f32) -> f32 {
        let (low, high) = control.range();
        let share = ((value - low) / (high - low)).clamp(0.0, 1.0);
        strip.bottom() - MARGIN - share * (strip.height() - 2.0 * MARGIN)
    }

    /// The value at a height of `strip`, landing on the resting value when
    /// near it.
    fn value_at(control: ControlKind, strip: Rect, y: f32) -> f32 {
        let (low, high) = control.range();
        let usable = (strip.height() - 2.0 * MARGIN).max(1.0);
        let share = ((strip.bottom() - MARGIN - y) / usable).clamp(0.0, 1.0);
        let value = low + share * (high - low);
        match control.rest() {
            Some(rest) if (value - rest).abs() < REST_SNAP * (high - low) => rest,
            _ => value,
        }
    }

    /// Where a press at `x` puts a point: on the grid, or exactly with Alt.
    fn tick_at(session: &Session, axis: &Axis, x: f32, exact: bool) -> Ticks {
        let tick = axis.tick(x);
        if exact {
            tick
        } else {
            session.grid.snap_nearest(&session.pattern_meter(), tick)
        }
    }

    /// The point of the curve under `pos`, if any.
    fn point_at(
        points: &[ControlPoint],
        control: ControlKind,
        strip: Rect,
        axis: &Axis,
        pos: Pos2,
    ) -> Option<usize> {
        points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let at = pos2(axis.x(p.at), Self::y_of(control, strip, p.value));
                (i, at.distance(pos))
            })
            .filter(|&(_, distance)| distance <= GRAB)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Draws `control`'s curve of the pattern in focus in `strip` and
    /// handles its input. `axis` is the piano roll's (pattern time).
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        strip: Rect,
        axis: &Axis,
        control: ControlKind,
    ) {
        if self.drag.is_some_and(|d| d.control != control) {
            self.cancel_drag(session);
        }
        let response = ui.interact(
            strip,
            ui.scope_id().with("control-curve"),
            Sense::click_and_drag(),
        );
        let label = text::control_curve(control);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, &label));
        let points = session.control_points(control);
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
            && self.drag.is_none()
            && strip.contains(pos)
        {
            if let Some(index) = Self::point_at(&points, control, strip, axis, pos) {
                self.drag = Some(Drag {
                    control,
                    index,
                    origin: pos,
                    moving: false,
                });
            } else {
                session.begin_gesture(text::EDIT_CONTROL_CURVE);
                let at = Self::tick_at(session, axis, pos.x, alt);
                let value = Self::value_at(control, strip, pos.y);
                let index = session.add_control_point(control, at, value);
                self.drag = Some(Drag {
                    control,
                    index,
                    origin: pos,
                    moving: true,
                });
            }
        }
        if let Some(pos) = pos
            && secondary
            && self.drag.is_none()
            && ui.rect_contains_pointer(strip)
            && let Some(index) = Self::point_at(&points, control, strip, axis, pos)
        {
            session.remove_control_point(control, index);
        }
        if let Some(mut drag) = self.drag {
            match pos {
                Some(pos) if down => {
                    if !drag.moving && (pos - drag.origin).length() > DRAG_THRESHOLD {
                        session.begin_gesture(text::EDIT_CONTROL_CURVE);
                        drag.moving = true;
                    }
                    if drag.moving {
                        let at = Self::tick_at(session, axis, pos.x, alt);
                        let value = Self::value_at(control, strip, pos.y);
                        drag.index = session.move_control_point(control, drag.index, at, value);
                    }
                    self.drag = Some(drag);
                }
                _ => {
                    if drag.moving {
                        session.end_gesture();
                    }
                    self.drag = None;
                }
            }
        }
        if self.drag.is_none() && response.hovered() {
            response.on_hover_text(text::control_hint(control));
        }
        self.paint(ui, session, strip, axis, control);
    }

    fn paint(&self, ui: &Ui, session: &Session, strip: Rect, axis: &Axis, control: ControlKind) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(strip);
        // The resting place, faint (the middle for the wheel).
        if let Some(rest) = control.rest() {
            let y = Self::y_of(control, strip, rest);
            painter.hline(strip.x_range(), y, Stroke::new(1.0, p.grid_bar));
        }
        let points = session.control_points(control);
        let (Some(first), Some(last)) = (points.first(), points.last()) else {
            return;
        };
        // The curve: held before its first point and after its last.
        let mut line = vec![pos2(strip.left(), Self::y_of(control, strip, first.value))];
        line.extend(
            points
                .iter()
                .map(|p| pos2(axis.x(p.at), Self::y_of(control, strip, p.value))),
        );
        line.push(pos2(strip.right(), Self::y_of(control, strip, last.value)));
        painter.add(egui::Shape::line(line, Stroke::new(2.0, p.accent)));
        let hovered = ui
            .input(|i| i.pointer.hover_pos())
            .and_then(|pos| Self::point_at(&points, control, strip, axis, pos));
        for (index, point) in points.iter().enumerate() {
            let at = pos2(axis.x(point.at), Self::y_of(control, strip, point.value));
            if !strip.expand(GRAB).contains(at) {
                continue;
            }
            let big = hovered == Some(index) || self.drag.is_some_and(|d| d.index == index);
            let radius = if big { 5.0 } else { 3.5 };
            painter.circle(at, radius, p.accent, Stroke::new(1.5, p.surface));
            // Named for screen readers and tests.
            let rect = Rect::from_center_size(at, vec2(GRAB * 2.0, GRAB * 2.0));
            let name = text::control_point(control, index);
            ui.interact(
                rect.intersect(strip),
                ui.scope_id().with(("control-point", index)),
                Sense::hover(),
            )
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, &name));
        }
    }
}

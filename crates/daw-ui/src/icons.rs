// SPDX-License-Identifier: GPL-3.0-or-later

//! Our own icons, drawn from simple shapes so they stay sharp at any size
//! and follow the theme's colours, and the round icon button that shows
//! them.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind,
    Ui, Vec2, WidgetInfo, pos2, vec2,
};

use crate::theme::Palette;

/// The icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// A triangle pointing right.
    Play,
    /// A square.
    Stop,
    /// A filled circle.
    Record,
    /// A bar with a triangle pointing left.
    ToStart,
    /// Two arrows chasing each other.
    Loop,
    /// A metronome.
    Metronome,
    /// A curved arrow to the left.
    Undo,
    /// A curved arrow to the right.
    Redo,
    /// Three lines.
    Menu,
    /// A few piano keys.
    Keyboard,
    /// A plus sign.
    Plus,
    /// A minus sign.
    Minus,
    /// A small arrow pointing down.
    ChevronDown,
    /// A small arrow pointing up.
    ChevronUp,
}

/// Draws an icon into `rect` with `color`.
#[allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "a table of small drawings in the usual geometry shorthand"
)]
pub fn paint(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = Stroke::new((s * 0.11).max(1.5), color);
    match icon {
        Icon::Play => {
            let h = s * 0.62;
            let points = vec![
                pos2(c.x - h * 0.36, c.y - h / 2.0),
                pos2(c.x + h * 0.52, c.y),
                pos2(c.x - h * 0.36, c.y + h / 2.0),
            ];
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        Icon::Stop => {
            let r = Rect::from_center_size(c, Vec2::splat(s * 0.46));
            painter.rect_filled(r, CornerRadius::same(2), color);
        }
        Icon::Record => {
            painter.circle_filled(c, s * 0.26, color);
        }
        Icon::ToStart => {
            let h = s * 0.5;
            let bar = Rect::from_min_size(pos2(c.x - h * 0.55, c.y - h / 2.0), vec2(s * 0.09, h));
            painter.rect_filled(bar, CornerRadius::same(1), color);
            let points = vec![
                pos2(c.x + h * 0.55, c.y - h / 2.0),
                pos2(c.x + h * 0.55, c.y + h / 2.0),
                pos2(c.x - h * 0.3, c.y),
            ];
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        Icon::Loop => {
            let r = s * 0.27;
            arc(painter, c, r, PI * 0.15, PI * 0.85, stroke);
            arc(painter, c, r, PI * 1.15, PI * 1.85, stroke);
            arrow_head(painter, c, r, PI * 0.85, true, stroke.width * 2.2, color);
            arrow_head(painter, c, r, PI * 1.85, true, stroke.width * 2.2, color);
        }
        Icon::Metronome => {
            let h = s * 0.56;
            let top = c.y - h / 2.0;
            let bottom = c.y + h / 2.0;
            let outline = vec![
                pos2(c.x - h * 0.16, top),
                pos2(c.x + h * 0.16, top),
                pos2(c.x + h * 0.38, bottom),
                pos2(c.x - h * 0.38, bottom),
            ];
            painter.add(Shape::closed_line(outline, stroke));
            painter.line_segment(
                [
                    pos2(c.x, bottom - h * 0.22),
                    pos2(c.x + h * 0.3, top + h * 0.05),
                ],
                stroke,
            );
        }
        Icon::Undo | Icon::Redo => {
            // The top half of a circle, with the arrow head at the left end
            // (undo) or the right end (redo), pointing down.
            let r = s * 0.24;
            let centre = pos2(c.x, c.y + s * 0.1);
            arc(painter, centre, r, PI, TAU, stroke);
            let (angle, clockwise) = if icon == Icon::Undo {
                (PI, false)
            } else {
                (TAU, true)
            };
            arrow_head(
                painter,
                centre,
                r,
                angle,
                clockwise,
                stroke.width * 2.4,
                color,
            );
        }
        Icon::Menu => {
            let w = s * 0.5;
            for dy in [-0.18, 0.0, 0.18] {
                let y = c.y + dy * s;
                painter.line_segment([pos2(c.x - w / 2.0, y), pos2(c.x + w / 2.0, y)], stroke);
            }
        }
        Icon::Keyboard => {
            let w = s * 0.62;
            let h = s * 0.44;
            let body = Rect::from_center_size(c, vec2(w, h));
            painter.rect_stroke(body, CornerRadius::same(2), stroke, StrokeKind::Middle);
            for i in 1..4 {
                #[allow(clippy::cast_precision_loss, reason = "a small count")]
                let x = body.left() + w * i as f32 / 4.0;
                painter.line_segment(
                    [pos2(x, body.top()), pos2(x, body.bottom())],
                    Stroke::new(stroke.width * 0.7, color),
                );
            }
            for i in [1.0, 2.0, 3.0] {
                let x = body.left() + w * i / 4.0;
                let key =
                    Rect::from_center_size(pos2(x, body.top() + h * 0.3), vec2(w * 0.11, h * 0.6));
                painter.rect_filled(key, CornerRadius::ZERO, color);
            }
        }
        Icon::Plus | Icon::Minus => {
            let w = s * 0.42;
            painter.line_segment([pos2(c.x - w / 2.0, c.y), pos2(c.x + w / 2.0, c.y)], stroke);
            if icon == Icon::Plus {
                painter.line_segment([pos2(c.x, c.y - w / 2.0), pos2(c.x, c.y + w / 2.0)], stroke);
            }
        }
        Icon::ChevronDown | Icon::ChevronUp => {
            let w = s * 0.32;
            let dy = if icon == Icon::ChevronDown {
                w * 0.5
            } else {
                -w * 0.5
            };
            let points = vec![
                pos2(c.x - w, c.y - dy * 0.5),
                pos2(c.x, c.y + dy * 1.0),
                pos2(c.x + w, c.y - dy * 0.5),
            ];
            painter.add(Shape::line(points, stroke));
        }
    }
}

fn arc(painter: &Painter, centre: Pos2, radius: f32, from: f32, to: f32, stroke: Stroke) {
    let steps = 24;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "a small count")]
            let t = from + (to - from) * i as f32 / steps as f32;
            pos2(centre.x + radius * t.cos(), centre.y + radius * t.sin())
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

/// An arrow head on a circle of `radius` at `angle`, pointing along the
/// circle (clockwise on screen if `clockwise`).
fn arrow_head(
    painter: &Painter,
    centre: Pos2,
    radius: f32,
    angle: f32,
    clockwise: bool,
    size: f32,
    color: Color32,
) {
    let tip = pos2(
        centre.x + radius * angle.cos(),
        centre.y + radius * angle.sin(),
    );
    let along = if clockwise {
        angle + FRAC_PI_2
    } else {
        angle - FRAC_PI_2
    };
    let dir = vec2(along.cos(), along.sin());
    let normal = vec2(-dir.y, dir.x);
    let front = tip + dir * size * 0.6;
    let back = tip - dir * size * 0.3;
    let points = vec![
        front,
        back + normal * size * 0.55,
        back - normal * size * 0.55,
    ];
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// How an icon button looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A plain button.
    Normal,
    /// A toggle that is on, or the button for what is happening now.
    Active,
    /// The record button while recording.
    Recording,
}

impl Look {
    /// [`Look::Active`] when `on`, [`Look::Normal`] otherwise.
    pub fn on(on: bool) -> Self {
        if on { Self::Active } else { Self::Normal }
    }
}

/// A square button showing an icon. `label` is its accessible name and the
/// start of its tooltip (`hint` adds the shortcut or an explanation).
pub fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    hint: &str,
    look: Look,
    size: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let selected = look != Look::Normal;
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, label));
    if ui.is_rect_visible(rect) {
        let p = Palette::of(ui.ctx());
        let hovered = response.hovered() && ui.is_enabled();
        let (fill, color) = match look {
            Look::Normal if hovered => (p.raised_hover, p.text),
            Look::Normal => (p.raised, p.text),
            Look::Active => (p.accent, p.on_accent),
            Look::Recording => (p.record, Color32::WHITE),
        };
        let color = if icon == Icon::Record && look == Look::Normal {
            p.record
        } else if ui.is_enabled() {
            color
        } else {
            p.text_weak
        };
        let fill = if response.is_pointer_button_down_on() {
            fill.gamma_multiply(0.85)
        } else {
            fill
        };
        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
        paint(ui.painter(), icon, rect.shrink(size * 0.18), color);
    }
    let tooltip = if hint.is_empty() {
        label.to_owned()
    } else {
        format!("{label}\n{hint}")
    };
    response.on_hover_text(tooltip)
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The song view's signature row (decision 0018): tempo, time-signature
//! and key changes along the song, pinned under the ruler. Folded, it is
//! one line that says what the song starts with and marks where it
//! changes; opened, it shows two strips:
//!
//! - signatures: a tag at each bar where the time signature or the key
//!   changes. Click a bar to add a change there, click a tag to change or
//!   remove it, drag a tag to another bar;
//! - tempo: the tempo drawn as a line over the song. Click to add a tempo
//!   change on a beat (keep the button down and drag up or down to set its
//!   tempo), drag a point up or down to change its tempo or sideways to
//!   move it (Alt: exactly where the pointer is), click a point for its
//!   exact tempo and its glide, right-click it to remove it.

use daw_model::{TEMPO_RANGE, Ticks};
use egui::{
    Align2, CornerRadius, CursorIcon, DragValue, FontId, PointerButton, Pos2, Rect, RichText,
    Sense, Shape, Stroke, Ui, UiBuilder, pos2, vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::session::Session;
use crate::text;
use crate::theme::{Palette, size};
use crate::transport::{key_choices, signature_choices};

const FOLDED_HEIGHT: f32 = 28.0;
const TAGS_HEIGHT: f32 = 28.0;
const TEMPO_HEIGHT: f32 = 56.0;
/// Room above and below the tempo line.
const TEMPO_MARGIN: f32 = 10.0;
/// How close to a tempo point the pointer must be to take it.
const POINT_GRAB: f32 = 7.0;
/// The widths of the panel's columns. Fixed, so nothing in the panel can
/// grow it to the edge of the window (which would make it slide over as
/// the window keeps it on screen).
const SIGNATURE_COLUMN: f32 = 200.0;
const KEY_COLUMN: f32 = 280.0;
const TEMPO_PANEL: f32 = 240.0;
const DRAG_THRESHOLD: f32 = 4.0;

/// How the song view maps time to the screen, this frame.
#[derive(Clone, Copy, Debug)]
pub struct Axis {
    /// The timeline's left edge on screen.
    pub left: f32,
    /// The tick at the left edge.
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
    fn x(&self, tick: Ticks) -> f32 {
        self.left + ((tick.0 as f64 - self.left_tick) / self.ticks_per_point) as f32
    }

    fn tick(&self, x: f32) -> Ticks {
        Ticks((self.left_tick + f64::from(x - self.left) * self.ticks_per_point).max(0.0) as i64)
    }
}

/// What a mouse drag is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Drag {
    #[default]
    None,
    /// Holding a signature tag: a click opens it, a drag moves it.
    Tag { bar: u32, origin: Pos2, moved: bool },
    /// Holding a tempo point: a click opens it; a drag changes its tempo
    /// (up and down) or moves it (sideways), whichever it starts with.
    Tempo {
        at: Ticks,
        origin: Pos2,
        bpm: f64,
        /// Set once the drag has chosen: `Some(true)` moves sideways.
        sideways: Option<bool>,
        /// Whether the press added the point (its gesture is already open).
        added: bool,
    },
}

/// The small panel open for one change.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Panel {
    /// The time signature and key from a bar.
    Bar(u32),
    /// A tempo change.
    Tempo(Ticks),
}

/// The signature row's state between frames.
#[derive(Debug, Default)]
pub struct SignatureRow {
    open: bool,
    drag: Drag,
    panel: Option<(Panel, Pos2)>,
    /// Set on the frame a panel opens, so the press that opened it does
    /// not count as a press outside it.
    panel_fresh: bool,
}

/// A signature tag on screen: its bar and where it is drawn.
struct Tag {
    bar: u32,
    rect: Rect,
    text: String,
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "screen coordinates"
)]
impl SignatureRow {
    /// The row's height: one line folded, two strips open.
    pub fn height(&self) -> f32 {
        if self.open {
            TAGS_HEIGHT + TEMPO_HEIGHT
        } else {
            FOLDED_HEIGHT
        }
    }

    /// Whether the strips are shown.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag != Drag::None
    }

    /// Abandons the drag in progress, putting everything back.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        let gesture = match self.drag {
            Drag::None => return false,
            Drag::Tag { moved, .. } => moved,
            Drag::Tempo {
                sideways, added, ..
            } => added || sideways.is_some(),
        };
        if gesture {
            session.cancel_gesture();
        }
        self.drag = Drag::None;
        true
    }

    /// Draws the row in `rect` (the whole width of the view; the track
    /// headers' column ends at `header_right`) and handles its input.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        rect: Rect,
        header_right: f32,
        axis: Axis,
    ) {
        let p = Palette::of(ui.ctx());
        ui.painter_at(rect)
            .rect_filled(rect, CornerRadius::ZERO, p.surface);
        let header = Rect::from_min_max(rect.min, pos2(header_right, rect.bottom()));
        let lane = Rect::from_min_max(pos2(header_right, rect.top()), rect.max);
        self.header(ui, header);
        if self.open {
            let tags = Rect::from_min_max(lane.min, pos2(lane.right(), lane.top() + TAGS_HEIGHT));
            let tempo = Rect::from_min_max(pos2(lane.left(), tags.bottom()), lane.max);
            self.tags(ui, session, tags, &axis);
            self.tempo(ui, session, tempo, &axis);
            ui.painter().hline(
                lane.x_range(),
                tags.bottom() - 0.5,
                Stroke::new(1.0, p.divider.gamma_multiply(0.5)),
            );
        } else {
            self.folded(ui, session, lane, &axis);
        }
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, p.divider),
        );
        self.panel(ui, session);
    }

    /// The left part: the button that opens and folds the row, and what
    /// each strip is.
    fn header(&mut self, ui: &mut Ui, header: Rect) {
        let p = Palette::of(ui.ctx());
        let top = Rect::from_min_max(
            header.min,
            pos2(
                header.right(),
                header.top() + FOLDED_HEIGHT.min(TAGS_HEIGHT),
            ),
        );
        let (icon, label) = if self.open {
            (Icon::ChevronUp, text::HIDE_SIGNATURES)
        } else {
            (Icon::ChevronDown, text::SHOW_SIGNATURES)
        };
        let mut toggled = false;
        ui.scope_builder(
            UiBuilder::new().max_rect(top.shrink2(vec2(8.0, 2.0))),
            |ui| {
                ui.horizontal_centered(|ui| {
                    if icon_button(
                        ui,
                        icon,
                        label,
                        text::SIGNATURE_ROW_HINT,
                        Look::Normal,
                        22.0,
                    )
                    .clicked()
                    {
                        toggled = true;
                    }
                    let title = if self.open {
                        text::SIGNATURES
                    } else {
                        text::SIGNATURE_ROW
                    };
                    ui.label(RichText::new(title).small().color(p.text_weak))
                        .on_hover_text(if self.open {
                            text::SIGNATURES_HINT
                        } else {
                            text::SIGNATURE_ROW_HINT
                        });
                });
            },
        );
        if self.open {
            ui.painter_at(header).text(
                pos2(
                    header.left() + 38.0,
                    header.top() + TAGS_HEIGHT + TEMPO_HEIGHT / 2.0,
                ),
                Align2::LEFT_CENTER,
                text::TEMPO_LANE,
                FontId::proportional(size::SMALL),
                p.text_weak,
            );
        }
        if toggled {
            self.open = !self.open;
            self.drag = Drag::None;
            self.panel = None;
        }
    }

    /// Folded: what the song starts with, a mark where something changes;
    /// a click opens the row.
    fn folded(&mut self, ui: &mut Ui, session: &Session, lane: Rect, axis: &Axis) {
        let p = Palette::of(ui.ctx());
        let project = session.project();
        let first = project.meter.initial();
        let summary = text::signature_summary(
            project.tempo.initial_bpm(),
            first.numerator,
            first.denominator,
            &text::song_key_name(project.key.at_bar(0)),
        );
        // A click on the line opens the row, like the button beside it.
        let response = ui.interact(lane, ui.scope_id().with("signature-folded"), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &summary));
        let painter = ui.painter_at(lane);
        painter.text(
            pos2(lane.left() + 10.0, lane.center().y),
            Align2::LEFT_CENTER,
            summary,
            FontId::proportional(size::SMALL),
            p.text_weak,
        );
        let marks = project.tempo.changes().iter().skip(1).map(|c| c.at).chain(
            session
                .signature_bars()
                .into_iter()
                .skip(1)
                .map(|bar| project.meter.bar_start(bar)),
        );
        for at in marks {
            let x = axis.x(at);
            if x > lane.left() && x < lane.right() {
                painter.circle_filled(pos2(x, lane.bottom() - 6.0), 3.0, p.accent);
            }
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            self.open = true;
        }
    }

    // ----- Signatures

    fn tag_list(ui: &Ui, session: &Session, strip: Rect, axis: &Axis) -> Vec<Tag> {
        let project = session.project();
        let font = FontId::proportional(size::SMALL);
        session
            .signature_bars()
            .into_iter()
            .map(|bar| {
                let mut parts = Vec::new();
                if let Some(signature) = session.meter_change_at(bar) {
                    parts.push(text::time_signature(
                        signature.numerator,
                        signature.denominator,
                    ));
                }
                if let Some(scale) = session.key_change_at(bar)
                    && (bar > 0 || scale.is_some())
                {
                    parts.push(text::song_key_name(scale));
                }
                let text = parts.join("  ·  ");
                let width = ui
                    .painter()
                    .layout_no_wrap(text.clone(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x;
                let x = axis.x(project.meter.bar_start(bar));
                let rect = Rect::from_min_size(
                    pos2(x + 2.0, strip.top() + 4.0),
                    vec2(width + 14.0, strip.height() - 8.0),
                );
                Tag { bar, rect, text }
            })
            .collect()
    }

    /// The bar whose line is nearest `x`.
    fn bar_near(session: &Session, axis: &Axis, x: f32) -> u32 {
        let meter = &session.project().meter;
        let at = axis.tick(x);
        let bar = meter.position(at).bar;
        let (this, next) = (meter.bar_start(bar), meter.bar_start(bar + 1));
        if (at - this) * 2 > next - this {
            bar + 1
        } else {
            bar
        }
    }

    fn tags(&mut self, ui: &mut Ui, session: &mut Session, strip: Rect, axis: &Axis) {
        let response = ui.interact(
            strip,
            ui.scope_id().with("signature-tags"),
            Sense::click_and_drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Canvas, true, text::SIGNATURE_TAGS)
        });
        let tags = Self::tag_list(ui, session, strip, axis);
        let (pos, pressed, down) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.primary_down(),
            )
        });
        let here = response.is_pointer_button_down_on() || response.clicked();
        if let Some(pos) = pos
            && pressed
            && here
            && self.drag == Drag::None
            && strip.contains(pos)
        {
            if let Some(tag) = tags.iter().rev().find(|t| t.rect.contains(pos)) {
                self.drag = Drag::Tag {
                    bar: tag.bar,
                    origin: pos,
                    moved: false,
                };
            } else {
                let bar = session.project().meter.position(axis.tick(pos.x)).bar;
                let x = axis.x(session.project().meter.bar_start(bar));
                self.panel = Some((Panel::Bar(bar), pos2(x, strip.bottom())));
                self.panel_fresh = true;
            }
        }
        if let Drag::Tag { bar, origin, moved } = self.drag {
            match pos {
                Some(pos) if down => {
                    let far = (pos.x - origin.x).abs() > DRAG_THRESHOLD;
                    if bar > 0 && (moved || far) {
                        if !moved {
                            session.begin_gesture(text::EDIT_MOVE_SIGNATURE);
                        }
                        let target = Self::bar_near(session, axis, pos.x).max(1);
                        let bar = if target == bar {
                            bar
                        } else {
                            session.move_signature_changes(bar, target)
                        };
                        self.drag = Drag::Tag {
                            bar,
                            origin,
                            moved: true,
                        };
                    }
                }
                _ => {
                    if moved {
                        session.end_gesture();
                    } else if let Some(tag) = tags.iter().find(|t| t.bar == bar) {
                        self.panel = Some((Panel::Bar(bar), tag.rect.left_bottom()));
                    }
                    self.drag = Drag::None;
                }
            }
        }
        self.paint_tags(ui, session, strip, axis, &tags);
    }

    /// Draws the tags, the place a click would add a change, and their
    /// entries in the accessibility tree.
    fn paint_tags(&self, ui: &Ui, session: &Session, strip: Rect, axis: &Axis, tags: &[Tag]) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(strip);
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|h| strip.contains(*h) && ui.rect_contains_pointer(strip));
        let open_bar = match self.panel {
            Some((Panel::Bar(bar), _)) => Some(bar),
            _ => None,
        };
        let over_tag = hover.is_some_and(|h| tags.iter().any(|t| t.rect.contains(h)));
        if let Some(h) = hover
            && !over_tag
            && self.drag == Drag::None
        {
            // Where a click would add a change: the bar under the pointer.
            let meter = &session.project().meter;
            let x = axis.x(meter.bar_start(meter.position(axis.tick(h.x)).bar));
            painter.vline(
                x,
                strip.y_range(),
                Stroke::new(1.0, p.accent.gamma_multiply(0.5)),
            );
            painter.text(
                pos2(x + 6.0, strip.center().y),
                Align2::LEFT_CENTER,
                "+",
                FontId::proportional(size::BODY),
                p.accent,
            );
        }
        for tag in tags {
            if tag.rect.right() < strip.left() || tag.rect.left() > strip.right() {
                continue;
            }
            let active = open_bar == Some(tag.bar)
                || matches!(self.drag, Drag::Tag { bar, .. } if bar == tag.bar);
            let hot = hover.is_some_and(|h| tag.rect.contains(h));
            let fill = if active {
                p.accent.gamma_multiply(0.35)
            } else if hot {
                p.raised_hover
            } else {
                p.raised
            };
            painter.vline(
                tag.rect.left() - 2.0,
                strip.y_range(),
                Stroke::new(1.0, p.grid_bar),
            );
            painter.rect_filled(tag.rect, CornerRadius::same(4), fill);
            painter.text(
                pos2(tag.rect.left() + 7.0, tag.rect.center().y),
                Align2::LEFT_CENTER,
                &tag.text,
                FontId::proportional(size::SMALL),
                p.text,
            );
            let id = ui.scope_id().with(("signature-tag", tag.bar));
            ui.interact(tag.rect.intersect(strip), id, Sense::hover())
                .widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        true,
                        text::signature_tag(tag.bar, &tag.text),
                    )
                });
        }
        if over_tag || matches!(self.drag, Drag::Tag { moved: true, .. }) {
            ui.ctx().set_cursor_icon(if self.is_dragging() {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            });
        } else if hover.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
    }

    // ----- Tempo

    /// The tempo range the strip shows: the song's tempos with room around.
    fn tempo_range(session: &Session) -> (f64, f64) {
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

    fn tempo_y(strip: Rect, range: (f64, f64), bpm: f64) -> f32 {
        let usable = f64::from(strip.height() - 2.0 * TEMPO_MARGIN);
        let fraction = ((bpm - range.0) / (range.1 - range.0).max(1e-6)).clamp(0.0, 1.0);
        strip.bottom() - TEMPO_MARGIN - (fraction * usable) as f32
    }

    /// Where a tempo change dragged or added at `x` goes: on a beat, or
    /// exactly with Alt.
    fn tempo_tick(session: &Session, axis: &Axis, x: f32, alt: bool) -> Ticks {
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
            .map(|c| (c.at, pos2(axis.x(c.at), Self::tempo_y(strip, range, c.bpm))))
            .filter(|(_, p)| p.distance(pos) <= POINT_GRAB)
            .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
            .map(|(at, _)| at)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one strip: its input, then its drawing"
    )]
    fn tempo(&mut self, ui: &mut Ui, session: &mut Session, strip: Rect, axis: &Axis) {
        let p = Palette::of(ui.ctx());
        let response = ui.interact(
            strip,
            ui.scope_id().with("tempo-strip"),
            Sense::click_and_drag(),
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, text::TEMPO_LINE));
        let range = match self.drag {
            // The scale holds still while a point is dragged.
            Drag::Tempo { .. } => ui
                .data(|d| d.get_temp::<(f64, f64)>(ui.scope_id().with("tempo-range")))
                .unwrap_or_else(|| Self::tempo_range(session)),
            _ => Self::tempo_range(session),
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
            ui.data_mut(|d| d.insert_temp(ui.scope_id().with("tempo-range"), range));
            if let Some(at) = Self::point_at(session, strip, axis, range, pos) {
                let bpm = session
                    .tempo_changes()
                    .iter()
                    .find(|c| c.at == at)
                    .map_or(120.0, |c| c.bpm);
                self.drag = Drag::Tempo {
                    at,
                    origin: pos,
                    bpm,
                    sideways: None,
                    added: false,
                };
            } else {
                let at = Self::tempo_tick(session, axis, pos.x, alt);
                session.begin_gesture(text::EDIT_ADD_TEMPO_CHANGE);
                let at = session.add_tempo_change(at);
                let bpm = session.project().tempo.bpm_at(at);
                self.drag = Drag::Tempo {
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
            && let Some(at) = Self::point_at(session, strip, axis, range, pos)
        {
            session.remove_tempo_change(at);
        }
        if let Drag::Tempo {
            at,
            origin,
            bpm,
            sideways,
            added,
        } = self.drag
        {
            match pos {
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
                            let to = Self::tempo_tick(session, axis, pos.x, alt);
                            if to > Ticks::ZERO && to != at {
                                session.move_tempo_change(at, to)
                            } else {
                                at
                            }
                        }
                        Some(false) if (pos.y - origin.y).abs() > 0.5 || added => {
                            let usable = f64::from(strip.height() - 2.0 * TEMPO_MARGIN);
                            let per_point = (range.1 - range.0) / usable.max(1.0);
                            let new = (bpm + f64::from(origin.y - pos.y) * per_point).round();
                            session.set_tempo_at(at, new);
                            at
                        }
                        _ => at,
                    };
                    self.drag = Drag::Tempo {
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
                        self.panel = Some((Panel::Tempo(at), pos2(axis.x(at), strip.bottom())));
                    }
                    self.drag = Drag::None;
                }
            }
        }

        // The line: steps at sudden changes, slopes where it glides.
        let painter = ui.painter_at(strip);
        let changes = session.tempo_changes().to_vec();
        let mut points = Vec::new();
        for (index, change) in changes.iter().enumerate() {
            let x0 = axis.x(change.at);
            let y0 = Self::tempo_y(strip, range, change.bpm);
            let (x1, y1) = match changes.get(index + 1) {
                Some(next) if change.glide => {
                    (axis.x(next.at), Self::tempo_y(strip, range, next.bpm))
                }
                Some(next) => (axis.x(next.at), y0),
                None => (strip.right() + 10.0, y0),
            };
            points.push(pos2(x0, y0));
            points.push(pos2(x1, y1));
        }
        painter.add(Shape::line(points, Stroke::new(2.0, p.accent)));
        let open_at = match self.panel {
            Some((Panel::Tempo(at), _)) => Some(at),
            _ => None,
        };
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|h| strip.contains(*h) && ui.rect_contains_pointer(strip));
        let hot = hover.and_then(|h| Self::point_at(session, strip, axis, range, h));
        for change in &changes {
            let centre = pos2(axis.x(change.at), Self::tempo_y(strip, range, change.bpm));
            if centre.x < strip.left() - 40.0 || centre.x > strip.right() + 4.0 {
                continue;
            }
            let active = open_at == Some(change.at)
                || matches!(self.drag, Drag::Tempo { at, .. } if at == change.at);
            let radius = if active || hot == Some(change.at) {
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
            let bar = session.project().meter.position(change.at).bar;
            let id = ui.scope_id().with(("tempo-point", change.at.0));
            let target = Rect::from_center_size(centre, vec2(2.0 * POINT_GRAB, 2.0 * POINT_GRAB));
            ui.interact(target.intersect(strip), id, Sense::hover())
                .widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        true,
                        text::tempo_point(change.bpm, bar),
                    )
                });
        }
        if hot.is_some() || self.is_dragging() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        } else if hover.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            response.on_hover_text(text::TEMPO_LANE_HINT);
        }
    }

    // ----- The panel of one change

    fn panel(&mut self, ui: &mut Ui, session: &mut Session) {
        let Some((panel, at)) = self.panel else {
            return;
        };
        let mut close = false;
        let area = egui::Area::new(ui.scope_id().with("signature-panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(at + vec2(0.0, 2.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| match panel {
                    Panel::Bar(bar) => close = Self::bar_panel(ui, session, bar),
                    Panel::Tempo(at) => close = Self::tempo_panel(ui, session, at),
                });
            });
        let pressed_outside = ui.input(|i| i.pointer.any_pressed())
            && !std::mem::take(&mut self.panel_fresh)
            && !area.response.contains_pointer()
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| p != at);
        if close || pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.panel = None;
        }
    }

    /// The time signature and key from a bar; returns whether to close.
    fn bar_panel(ui: &mut Ui, session: &mut Session, bar: u32) -> bool {
        let mut close = false;
        ui.label(RichText::new(text::from_bar(bar)).strong());
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(SIGNATURE_COLUMN);
                ui.label(RichText::new(text::TIME_SIGNATURE).small().weak());
                let change = session.meter_change_at(bar);
                if bar > 0
                    && ui
                        .selectable_label(change.is_none(), text::SAME_AS_BEFORE)
                        .clicked()
                {
                    session.set_meter_change(bar, None);
                }
                let current = session.project().meter.signature_at_bar(bar);
                let shown = change.unwrap_or(current);
                signature_choices(ui, shown, |signature| {
                    session.set_meter_change(bar, Some(signature));
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(KEY_COLUMN);
                ui.label(RichText::new(text::KEY).small().weak());
                let change = session.key_change_at(bar);
                if bar > 0
                    && ui
                        .selectable_label(change.is_none(), text::SAME_AS_BEFORE)
                        .clicked()
                {
                    session.set_key_change(bar, None);
                }
                let current = session.project().key.at_bar(bar);
                key_choices(ui, current, |scale| {
                    session.set_key_change(bar, Some(scale));
                });
            });
        });
        let has_changes =
            session.meter_change_at(bar).is_some() || session.key_change_at(bar).is_some();
        if bar > 0 && has_changes {
            ui.separator();
            if ui.button(text::REMOVE_CHANGES).clicked() {
                session.remove_signature_changes(bar);
                close = true;
            }
        }
        close
    }

    /// A tempo change: its exact tempo, its glide; returns whether to close.
    fn tempo_panel(ui: &mut Ui, session: &mut Session, at: Ticks) -> bool {
        let changes = session.tempo_changes();
        let Some(index) = changes.iter().position(|c| c.at == at) else {
            return true;
        };
        let change = changes[index];
        let last = index + 1 == changes.len();
        let bar = session.project().meter.position(at).bar;
        ui.set_width(TEMPO_PANEL);
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
}

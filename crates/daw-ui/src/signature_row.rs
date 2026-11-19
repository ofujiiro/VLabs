// SPDX-License-Identifier: GPL-3.0-or-later

//! The song view's signature row (decision 0018): tempo, time-signature
//! and key changes along the song, pinned under the ruler. Folded, it is
//! one line that says what the song starts with and marks where it
//! changes; opened, it shows two strips:
//!
//! - signatures: a tag at each bar where the time signature or the key
//!   changes. Click a bar to add a change there, click a tag to change or
//!   remove it, drag a tag to another bar;
//! - tempo: the song's tempo line (`tempo_lane`, decisions 0018 and 0026).

use egui::{
    Align2, CornerRadius, CursorIcon, FontId, PointerButton, Pos2, Rect, RichText, Sense, Stroke,
    Ui, UiBuilder, pos2, vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::session::Session;
use crate::tempo_lane::TempoLane;
use crate::text;
use crate::theme::{Palette, size};
use crate::transport::{key_choices, signature_choices};

const FOLDED_HEIGHT: f32 = 28.0;
const TAGS_HEIGHT: f32 = 28.0;
const TEMPO_HEIGHT: f32 = 64.0;
/// The widths of the panel's columns. Fixed, so nothing in the panel can
/// grow it to the edge of the window (which would make it slide over as
/// the window keeps it on screen).
const SIGNATURE_COLUMN: f32 = 200.0;
const KEY_COLUMN: f32 = 280.0;
const DRAG_THRESHOLD: f32 = 4.0;

pub use crate::tempo_lane::Axis;

/// What a mouse drag is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Drag {
    #[default]
    None,
    /// Holding a signature tag: a click opens it, a drag moves it.
    Tag { bar: u32, origin: Pos2, moved: bool },
}

/// The small panel open for one change.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Panel {
    /// The time signature and key from a bar.
    Bar(u32),
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
    /// The tempo strip.
    tempo: TempoLane,
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
        self.drag != Drag::None || self.tempo.is_dragging()
    }

    /// Opens the panel of a change at `at`. The press that opened it (on
    /// this frame) does not count as a press outside it, even when press
    /// and release come in the same frame.
    fn open_panel(&mut self, panel: Panel, at: Pos2) {
        self.panel = Some((panel, at));
        self.panel_fresh = true;
        self.tempo.close_panel();
    }

    /// Closes the panel of a change, if one is open (Escape); returns
    /// whether one was.
    pub fn close_panel(&mut self) -> bool {
        let bar = self.panel.take().is_some();
        self.tempo.close_panel() || bar
    }

    /// Abandons the drag in progress, putting everything back.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        match self.drag {
            Drag::None => self.tempo.cancel_drag(session),
            Drag::Tag { moved, .. } => {
                if moved {
                    session.cancel_gesture();
                }
                self.drag = Drag::None;
                true
            }
        }
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
            self.tempo.ui(ui, session, tempo, &axis);
            if self.tempo.take_opened() {
                self.panel = None;
            }
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
        self.tempo.panel_ui(ui, session);
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
            self.tempo.close_panel();
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
                self.open_panel(Panel::Bar(bar), pos2(x, strip.bottom()));
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
                        self.open_panel(Panel::Bar(bar), tag.rect.left_bottom());
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
                });
            });
        // Only the frame the panel opened on is spared.
        let fresh = std::mem::take(&mut self.panel_fresh);
        let pressed_outside = ui.input(|i| i.pointer.any_pressed())
            && !fresh
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
                // The signature in effect is the base for "Other…"; a choice
                // is highlighted only when this bar has its own change.
                let current = change.unwrap_or(session.project().meter.signature_at_bar(bar));
                signature_choices(ui, current, change.is_some(), |signature| {
                    session.set_meter_change(bar, Some(signature));
                });
            });
            // (Space, not a separator: a vertical separator would take all
            // the height there is and grow the panel to the window's edge.)
            ui.add_space(16.0);
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
                key_choices(ui, current, change.is_some(), |scale| {
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
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The note shape panel (decision 0031): how a track's notes start and
//! end, for the instruments that play recorded sounds. A button opens it
//! (in the track's header and in the pattern's location line): a picture
//! of a note's shape, the attack and the release (each either the
//! instrument's own or a time), and four quick choices, among them "Cut at
//! note end", which cuts every note off as its key comes up.

use daw_model::{NoteShape, TrackId};
use egui::{
    Align2, Button, FontId, PopupCloseBehavior, RichText, Sense, Shape, Slider, Stroke, Ui, pos2,
    vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::session::Session;
use crate::text;
use crate::theme::{Palette, size};

/// The panel's width.
const WIDTH: f32 = 300.0;
/// The picture's height.
const PICTURE_HEIGHT: f32 = 64.0;
/// What a slider shows while the instrument keeps its own time.
const SHOWN_ATTACK: f32 = 0.005;
const SHOWN_RELEASE: f32 = 0.3;
/// The quick choices' times.
const SOFT_ATTACK: f32 = 0.15;
const LONG_RELEASE: f32 = 2.5;

/// The button that opens a track's note shape panel, lit when the shape is
/// not the instrument's own. Nothing for instruments without one.
pub fn shape_button(ui: &mut Ui, session: &mut Session, track: TrackId, size: f32) {
    let Some(shape) = session.track_shape(track) else {
        return;
    };
    let button = icon_button(
        ui,
        Icon::Shape,
        text::NOTE_SHAPE,
        text::NOTE_SHAPE_HINT,
        Look::on(!shape.is_natural()),
        size,
    );
    egui::Popup::menu(&button)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| panel(ui, session, track));
}

/// The panel's contents.
fn panel(ui: &mut Ui, session: &mut Session, track: TrackId) {
    let Some(shape) = session.track_shape(track) else {
        return;
    };
    let kit = session
        .project()
        .track(track)
        .is_some_and(|t| t.instrument.is_drums());
    ui.set_width(WIDTH);
    // Buttons that look like buttons, inside a menu.
    let raised = Palette::of(ui.ctx()).raised;
    ui.visuals_mut().button_frame = true;
    ui.visuals_mut().widgets.inactive.weak_bg_fill = raised;
    ui.label(RichText::new(text::NOTE_SHAPE).strong().size(size::BODY));
    ui.add_space(4.0);
    picture(ui, shape);
    ui.add_space(6.0);
    times(ui, session, track, shape);
    ui.add_space(6.0);
    choices(ui, session, track, shape);
    if kit {
        ui.add_space(4.0);
        ui.label(
            RichText::new(text::SHAPE_KIT_NOTE)
                .small()
                .color(Palette::of(ui.ctx()).text_weak),
        );
    }
}

/// The attack and the release: each ticked to set it, with its time.
fn times(ui: &mut Ui, session: &mut Session, track: TrackId, mut shape: NoteShape) {
    let mut changed = false;
    let mut gesture: Option<bool> = None;
    egui::Grid::new("shape-times")
        .num_columns(2)
        .min_col_width(78.0)
        .spacing(vec2(8.0, 6.0))
        .show(ui, |ui| {
            for (label, hint, range, shown, value) in [
                (
                    text::SHAPE_ATTACK,
                    text::SHAPE_ATTACK_HINT,
                    (0.001, NoteShape::ATTACK_RANGE.1),
                    SHOWN_ATTACK,
                    &mut shape.attack,
                ),
                (
                    text::SHAPE_RELEASE,
                    text::SHAPE_RELEASE_HINT,
                    NoteShape::RELEASE_RANGE,
                    SHOWN_RELEASE,
                    &mut shape.release,
                ),
            ] {
                let mut own = value.is_some();
                let tick = ui.checkbox(&mut own, label).on_hover_text(hint);
                if tick.changed() {
                    *value = own.then_some(value.unwrap_or(shown));
                    changed = true;
                }
                let mut seconds = value.unwrap_or(shown).max(range.0);
                let slider = ui
                    .add_enabled(
                        own,
                        Slider::new(&mut seconds, range.0..=range.1)
                            .logarithmic(true)
                            .custom_formatter(|v, _| {
                                #[allow(clippy::cast_possible_truncation, reason = "seconds")]
                                let v = v as f32;
                                if own {
                                    text::shape_time(v)
                                } else {
                                    text::SHAPE_NATURAL_VALUE.to_owned()
                                }
                            })
                            .custom_parser(|_| None),
                    )
                    .labelled_by(tick.id)
                    .on_hover_text(hint);
                if slider.drag_started() {
                    gesture = Some(true);
                }
                if slider.changed() && own {
                    *value = Some(seconds);
                    changed = true;
                }
                if slider.drag_stopped() {
                    gesture = Some(false);
                }
                ui.end_row();
            }
        });
    if gesture == Some(true) {
        session.begin_gesture(text::EDIT_NOTE_SHAPE);
    }
    if changed {
        session.set_track_shape(track, shape);
    }
    if gesture == Some(false) {
        session.end_gesture();
    }
}

/// The quick choices, two by two.
fn choices(ui: &mut Ui, session: &mut Session, track: TrackId, shape: NoteShape) {
    let mut chosen = None;
    egui::Grid::new("shape-choices")
        .num_columns(2)
        .spacing(vec2(8.0, 6.0))
        .show(ui, |ui| {
            let choices = [
                (
                    text::SHAPE_CUT,
                    text::SHAPE_CUT_HINT,
                    NoteShape {
                        release: Some(NoteShape::CUT_RELEASE),
                        ..shape
                    },
                ),
                (
                    text::SHAPE_SOFT,
                    text::SHAPE_SOFT_HINT,
                    NoteShape {
                        attack: Some(SOFT_ATTACK),
                        ..shape
                    },
                ),
                (
                    text::SHAPE_LONG,
                    text::SHAPE_LONG_HINT,
                    NoteShape {
                        release: Some(LONG_RELEASE),
                        ..shape
                    },
                ),
                (
                    text::SHAPE_RESET,
                    text::SHAPE_RESET_HINT,
                    NoteShape::NATURAL,
                ),
            ];
            for (index, (label, hint, choice)) in choices.into_iter().enumerate() {
                let current = if label == text::SHAPE_RESET {
                    shape.is_natural()
                } else if label == text::SHAPE_CUT {
                    shape.is_cut()
                } else {
                    choice == shape
                };
                let button = Button::new(label)
                    .selected(current)
                    .min_size(vec2((WIDTH - 8.0) / 2.0, 26.0));
                if ui.add(button).on_hover_text(hint).clicked() {
                    chosen = Some(choice);
                }
                if index % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    if let Some(choice) = chosen {
        session.set_track_shape(track, choice);
    }
}

/// A picture of a note's shape: its rise, its level while the key is down,
/// the moment the key comes up, and its fall. Times the instrument keeps as
/// its own are drawn dashed.
fn picture(ui: &mut Ui, shape: NoteShape) {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(WIDTH, PICTURE_HEIGHT), Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, text::SHAPE_PICTURE)
    });
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, p.raised);
    let inner = rect.shrink2(vec2(10.0, 10.0));
    // Time runs on a gentle curve, so milliseconds and seconds both show.
    let width_of = |seconds: f32| (seconds / (seconds + 0.4)) * inner.width() * 0.4;
    let attack = shape.attack.unwrap_or(SHOWN_ATTACK);
    let release = shape.release.unwrap_or(SHOWN_RELEASE);
    let key_up = inner.left() + inner.width() * 0.55;
    let top = inner.top() + 4.0;
    let bottom = inner.bottom();
    let rise_end = (inner.left() + width_of(attack).max(1.0)).min(key_up - 8.0);
    let fall_end = (key_up + width_of(release).max(1.5)).min(inner.right());
    // The key-up line.
    painter.line_segment(
        [pos2(key_up, inner.top()), pos2(key_up, bottom)],
        Stroke::new(1.0, p.text_weak.gamma_multiply(0.6)),
    );
    painter.text(
        pos2(key_up - 4.0, bottom + 2.0),
        Align2::RIGHT_BOTTOM,
        text::SHAPE_KEY_UP,
        FontId::proportional(size::SMALL),
        p.text_weak,
    );
    let line = |painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, own: bool| {
        let stroke = Stroke::new(2.0, if own { p.accent } else { p.text_weak });
        if own {
            painter.line_segment([from, to], stroke);
        } else {
            painter.extend(Shape::dashed_line(&[from, to], stroke, 4.0, 3.0));
        }
    };
    line(
        &painter,
        pos2(inner.left(), bottom),
        pos2(rise_end, top),
        shape.attack.is_some(),
    );
    painter.line_segment(
        [pos2(rise_end, top), pos2(key_up, top)],
        Stroke::new(2.0, p.text),
    );
    // The fall: a curve for a release, as sound fades.
    let fall: Vec<egui::Pos2> = (0..=16)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "a small count")]
            let t = i as f32 / 16.0;
            let level = (1.0 - t).powi(2);
            pos2(
                key_up + (fall_end - key_up) * t,
                bottom - (bottom - top) * level,
            )
        })
        .collect();
    if shape.release.is_some() {
        painter.add(Shape::line(fall, Stroke::new(2.0, p.accent)));
    } else {
        for pair in fall.windows(2).step_by(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(2.0, p.text_weak));
        }
    }
}

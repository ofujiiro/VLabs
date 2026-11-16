// SPDX-License-Identifier: GPL-3.0-or-later

//! The mixer (decision 0014), in the bottom of the window: a strip for each
//! track and one for the master, each with its effects, pan, volume fader,
//! level meter, mute and solo. Effects open in small windows.

use std::collections::HashMap;

use daw_model::{ChainPlace, Effect, EffectId, EffectKind, TrackId};
use egui::{
    Align, Button, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};

use crate::session::MAX_GAIN;
use crate::session::Session;
use crate::text;
use crate::theme::{Palette, size, track_color};
use crate::widgets::{Gesture, db_to_gain, gain_to_db};
use crate::workspace::Intent;

/// The height of the mixer, below its header line.
pub const MIXER_HEIGHT: f32 = 304.0;
const STRIP_WIDTH: f32 = 124.0;
const EFFECT_ROWS: usize = 3;
const FADER_HEIGHT: f32 = 88.0;
const METER_WIDTH: f32 = 10.0;
/// The quietest level the meters and faders show, in dB.
const FLOOR_DB: f32 = -60.0;
/// The loudest a fader goes, in dB.
const TOP_DB: f32 = 6.0;

/// One strip's subject.
#[derive(Clone, Debug)]
struct StripInfo {
    place: ChainPlace,
    name: String,
    color: egui::Color32,
    gain: f32,
    pan: f32,
    mute: bool,
    solo: bool,
    effects: Vec<Effect>,
    level: f32,
}

/// The mixer's state between frames: the meters, which fall slowly.
#[derive(Debug, Default)]
pub struct MixerPanel {
    levels: HashMap<TrackId, f32>,
    master: f32,
}

impl MixerPanel {
    /// Shows the strips in all the space `ui` has.
    pub fn ui(&mut self, ui: &mut Ui, session: &mut Session, intents: &mut Vec<Intent>) {
        let whole = ui.available_rect_before_wrap();
        ui.allocate_rect(whole, Sense::hover());
        let mut strips: Vec<StripInfo> = Vec::new();
        for (index, track) in session.project().tracks.iter().enumerate() {
            let previous = self.levels.get(&track.id).copied().unwrap_or(0.0);
            let level = session.track_level(track.id).max(previous * 0.86);
            self.levels.insert(track.id, level);
            strips.push(StripInfo {
                place: ChainPlace::Track(track.id),
                name: track.name.clone(),
                color: track_color(index),
                gain: track.strip.gain,
                pan: track.strip.pan,
                mute: track.strip.mute,
                solo: track.strip.solo,
                effects: track.effects.clone(),
                level,
            });
        }
        self.master = session.master_level().max(self.master * 0.86);
        let master = StripInfo {
            place: ChainPlace::Master,
            name: text::MASTER.to_owned(),
            color: Palette::of(ui.ctx()).accent,
            gain: session.project().master.gain,
            pan: 0.0,
            mute: false,
            solo: false,
            effects: session.project().master.effects.clone(),
            level: self.master,
        };

        let master_rect = Rect::from_min_max(
            pos2(whole.right() - STRIP_WIDTH - 8.0, whole.top()),
            whole.max,
        );
        let tracks_rect =
            Rect::from_min_max(whole.min, pos2(master_rect.left() - 8.0, whole.bottom()));
        let p = Palette::of(ui.ctx());
        ui.painter().vline(
            master_rect.left() - 4.0,
            whole.y_range(),
            Stroke::new(1.0, p.divider),
        );
        let mut tracks_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(tracks_rect)
                .layout(Layout::left_to_right(Align::Min)),
        );
        egui::ScrollArea::horizontal()
            .id_salt("mixer-strips")
            .auto_shrink([false, false])
            .show(&mut tracks_ui, |ui| {
                ui.horizontal_top(|ui| {
                    for strip in &strips {
                        strip_ui(ui, session, strip, intents);
                    }
                });
            });
        let mut master_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(master_rect)
                .layout(Layout::left_to_right(Align::Min)),
        );
        strip_ui(&mut master_ui, session, &master, intents);
    }

    /// Whether a meter is still moving (the window keeps redrawing).
    pub fn is_moving(&self) -> bool {
        self.master > 1e-4 || self.levels.values().any(|&l| l > 1e-4)
    }
}

/// One channel strip.
#[allow(clippy::too_many_lines, reason = "one strip, top to bottom")]
fn strip_ui(ui: &mut Ui, session: &mut Session, strip: &StripInfo, intents: &mut Vec<Intent>) {
    let p = Palette::of(ui.ctx());
    let track = match strip.place {
        ChainPlace::Track(track) => Some(track),
        ChainPlace::Master => None,
    };
    ui.allocate_ui_with_layout(
        vec2(STRIP_WIDTH, MIXER_HEIGHT - 8.0),
        Layout::top_down(Align::Center),
        |ui| {
            ui.set_width(STRIP_WIDTH);
            let frame = egui::Frame::new()
                .fill(p.background)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(6, 6));
            frame.show(ui, |ui| {
                ui.set_width(STRIP_WIDTH - 12.0);
                ui.spacing_mut().item_spacing.y = 3.0;
                // The name, with the track's colour; a click puts it in focus.
                let focused = track.is_some_and(|t| t == session.track_id());
                let name = RichText::new(&strip.name).strong().color(if focused {
                    p.accent
                } else {
                    p.text
                });
                let response = ui.add(Button::new(name).frame(false).truncate());
                ui.painter().hline(
                    response.rect.x_range(),
                    response.rect.bottom() + 1.0,
                    Stroke::new(2.0, strip.color),
                );
                if response.clicked()
                    && let Some(track) = track
                {
                    session.focus_track(track);
                }
                effects_ui(ui, session, strip, intents);
                // Pan, with its position as text beside it.
                if let Some(track) = track {
                    let mut pan = strip.pan;
                    let mut gesture = Gesture::default();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let response = ui
                            .scope(|ui| {
                                ui.spacing_mut().slider_width = 62.0;
                                ui.add(
                                    egui::Slider::new(&mut pan, -1.0..=1.0)
                                        .show_value(false)
                                        .clamping(egui::SliderClamping::Always),
                                )
                            })
                            .inner
                            .on_hover_text(format!(
                                "{}: {}\n{}",
                                text::PAN,
                                text::pan(pan),
                                text::PAN_HINT
                            ));
                        gesture.add(&response);
                        if response.double_clicked() {
                            pan = 0.0;
                        }
                        ui.label(RichText::new(text::pan(pan)).small().color(p.text_weak));
                    });
                    apply(session, gesture, text::EDIT_PAN, |s| {
                        s.set_track_pan(track, pan);
                    });
                } else {
                    ui.add_space(20.0);
                }
                // Volume fader and meter.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    ui.add_space(4.0);
                    let mut db = gain_to_db(strip.gain);
                    let mut gesture = Gesture::default();
                    let response = ui
                        .scope(|ui| {
                            ui.spacing_mut().slider_width = FADER_HEIGHT;
                            ui.add(
                                egui::Slider::new(&mut db, FLOOR_DB..=TOP_DB)
                                    .vertical()
                                    .show_value(false)
                                    .clamping(egui::SliderClamping::Always),
                            )
                        })
                        .inner
                        .on_hover_text(format!(
                            "{}: {}\n{}",
                            text::VOLUME,
                            text::db(db),
                            text::VOLUME_HINT
                        ));
                    gesture.add(&response);
                    if response.double_clicked() {
                        db = 0.0;
                    }
                    let gain = db_to_gain(db).min(MAX_GAIN);
                    match track {
                        Some(track) => {
                            apply(session, gesture, text::EDIT_VOLUME, |s| {
                                s.set_track_gain(track, gain);
                            });
                        }
                        None => apply(session, gesture, text::EDIT_MASTER_VOLUME, |s| {
                            s.set_master_gain(gain);
                        }),
                    }
                    meter(ui, strip.level);
                    ui.label(
                        RichText::new(text::db(gain_to_db(strip.gain)))
                            .small()
                            .monospace()
                            .color(p.text_weak),
                    );
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if let Some(track) = track {
                        if ui
                            .add(
                                Button::new(text::MUTE_SHORT)
                                    .selected(strip.mute)
                                    .min_size(vec2(30.0, 22.0)),
                            )
                            .on_hover_text(text::MUTE_HINT)
                            .clicked()
                        {
                            session.toggle_mute(track);
                        }
                        if ui
                            .add(
                                Button::new(text::SOLO_SHORT)
                                    .selected(strip.solo)
                                    .min_size(vec2(30.0, 22.0)),
                            )
                            .on_hover_text(text::SOLO_HINT)
                            .clicked()
                        {
                            session.toggle_solo(track);
                        }
                    }
                });
            });
        },
    );
}

/// The effect chain: one row per effect, then an Add button.
fn effects_ui(ui: &mut Ui, session: &mut Session, strip: &StripInfo, intents: &mut Vec<Intent>) {
    let p = Palette::of(ui.ctx());
    let row_height = 20.0;
    #[allow(clippy::cast_precision_loss, reason = "a small count")]
    let height = row_height * EFFECT_ROWS as f32;
    egui::ScrollArea::vertical()
        .id_salt(("effects", format!("{:?}", strip.place)))
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if strip.effects.is_empty() {
                ui.label(RichText::new(text::NO_EFFECTS).small().color(p.text_weak));
            }
            let count = strip.effects.len();
            for (index, effect) in strip.effects.iter().enumerate() {
                let dot = if effect.enabled { "●" } else { "○" };
                let label = format!("{dot} {}", text::effect_name(&effect.kind));
                let color = if effect.enabled { p.text } else { p.text_weak };
                let response = ui
                    .add(
                        Button::new(RichText::new(label).color(color).size(size::SMALL))
                            .min_size(vec2(ui.available_width(), row_height - 2.0)),
                    )
                    .on_hover_text(format!(
                        "{}\n{}",
                        text::effect_description(&effect.kind),
                        text::EFFECT_HINT
                    ));
                if response.clicked() {
                    intents.push(Intent::OpenEffect(effect.id));
                }
                response.context_menu(|ui| {
                    effect_menu(ui, session, effect.id, effect.enabled, index, count);
                });
            }
        });
    let add = ui.add(
        Button::new(RichText::new(format!("+ {}", text::ADD_EFFECT)).size(size::SMALL))
            .min_size(vec2(ui.available_width(), row_height)),
    );
    egui::Popup::menu(&add).show(|ui| {
        ui.set_min_width(300.0);
        for kind in EffectKind::all() {
            let response = ui
                .button(text::effect_name(&kind))
                .on_hover_text(text::effect_description(&kind));
            if response.clicked() {
                if let Some(id) = session.add_effect(strip.place, kind.clone()) {
                    intents.push(Intent::OpenEffect(id));
                }
                ui.close();
            }
            ui.label(
                RichText::new(text::effect_description(&kind))
                    .small()
                    .color(p.text_weak),
            );
            ui.add_space(2.0);
        }
    });
}

/// The right-click menu of an effect.
fn effect_menu(
    ui: &mut Ui,
    session: &mut Session,
    id: EffectId,
    enabled: bool,
    index: usize,
    count: usize,
) {
    let mut on = enabled;
    if ui.checkbox(&mut on, text::EFFECT_ON).changed() {
        session.set_effect_enabled(id, on);
        ui.close();
    }
    if ui
        .add_enabled(index > 0, Button::new(text::MOVE_EARLIER))
        .clicked()
    {
        session.move_effect(id, index - 1);
        ui.close();
    }
    if ui
        .add_enabled(index + 1 < count, Button::new(text::MOVE_LATER))
        .clicked()
    {
        session.move_effect(id, index + 1);
        ui.close();
    }
    ui.separator();
    if ui.button(text::REMOVE_EFFECT).clicked() {
        session.remove_effect(id);
        ui.close();
    }
}

/// Applies a slider's change inside a gesture when it is being dragged.
fn apply(session: &mut Session, gesture: Gesture, label: &str, change: impl FnOnce(&mut Session)) {
    if gesture.started {
        session.begin_gesture(label);
    }
    change(session);
    if gesture.stopped {
        session.end_gesture();
    }
}

/// A level meter: green, then yellow above -6 dB, red at full scale.
fn meter(ui: &mut Ui, level: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(METER_WIDTH, FADER_HEIGHT), Sense::hover());
    let p = Palette::of(ui.ctx());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(2), p.grid);
    let db = gain_to_db(level);
    let fraction = ((db - FLOOR_DB) / (TOP_DB - FLOOR_DB)).clamp(0.0, 1.0);
    if fraction > 0.0 {
        let top = rect.bottom() - rect.height() * fraction;
        let color = if db >= 0.0 {
            p.record
        } else if db >= -6.0 {
            p.selected
        } else {
            p.accent
        };
        painter.rect_filled(
            Rect::from_min_max(pos2(rect.left(), top), rect.max),
            CornerRadius::same(2),
            color,
        );
    }
    // A mark at 0 dB.
    let zero = rect.bottom() - rect.height() * ((0.0 - FLOOR_DB) / (TOP_DB - FLOOR_DB));
    painter.hline(rect.x_range(), zero, Stroke::new(1.0, p.text_weak));
    response.on_hover_text(format!("{}: {}", text::LEVEL, text::db(db)));
}

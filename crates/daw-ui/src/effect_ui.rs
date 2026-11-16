// SPDX-License-Identifier: GPL-3.0-or-later

//! The small windows that adjust an effect (decision 0014): a few named
//! controls each, an On switch, and Remove. The EQ draws its curve. A
//! slider drag is one undo step.

use daw_dsp::effects::{Eq, EqParams};
use daw_model::{
    ChainPlace, CompressorSettings, DelaySettings, DelayTime, EffectId, EffectKind, EqSettings,
    LimiterSettings, ReverbSettings, Ticks,
};
use egui::{Align2, CornerRadius, FontId, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::session::Session;
use crate::text;
use crate::theme::{Palette, size};
use crate::widgets::{Gesture, Scale, slider_row};

/// Delay times offered when the delay follows the song: (name, length).
const SYNCED_TIMES: [(&str, i64, i64); 8] = [
    ("1/2", 1, 2),
    ("3/8", 3, 8),
    ("1/4", 1, 4),
    ("3/16", 3, 16),
    ("1/8", 1, 8),
    ("1/8 triplet", 1, 12),
    ("1/16", 1, 16),
    ("1/32", 1, 32),
];

/// The open effect windows.
#[derive(Debug, Default)]
pub struct EffectWindows {
    open: Vec<EffectId>,
}

impl EffectWindows {
    /// Opens an effect's window (or brings it back if already open).
    pub fn open(&mut self, id: EffectId) {
        if !self.open.contains(&id) {
            self.open.push(id);
        }
    }

    /// Whether an effect's window is open.
    pub fn is_open(&self, id: EffectId) -> bool {
        self.open.contains(&id)
    }

    /// Closes every window (when another song opens).
    pub fn close_all(&mut self) {
        self.open.clear();
    }

    /// Shows the open windows.
    pub fn ui(&mut self, ctx: &egui::Context, session: &mut Session) {
        self.open
            .retain(|&id| session.project().effect(id).is_some());
        let mut closed = Vec::new();
        for (index, id) in self.open.clone().into_iter().enumerate() {
            let Some(effect) = session.project().effect(id).cloned() else {
                continue;
            };
            let place = match session.project().effect_place(id) {
                Some((ChainPlace::Track(track), _)) => session
                    .project()
                    .track(track)
                    .map_or_else(String::new, |t| t.name.clone()),
                _ => text::MASTER.to_owned(),
            };
            let title = text::effect_title(text::effect_name(&effect.kind), &place);
            let mut open = true;
            #[allow(clippy::cast_precision_loss, reason = "a small count")]
            let offset = index as f32 * 24.0;
            egui::Window::new(title)
                .id(egui::Id::unique(("effect-window", id.0)))
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .default_pos(pos2(360.0 + offset, 160.0 + offset))
                .show(ctx, |ui| {
                    ui.set_width(400.0);
                    ui.spacing_mut().slider_width = 190.0;
                    editor(ui, session, id, &effect.kind, effect.enabled);
                });
            if !open {
                closed.push(id);
            }
        }
        self.open.retain(|id| !closed.contains(id));
    }
}

/// The contents of an effect's window.
fn editor(ui: &mut Ui, session: &mut Session, id: EffectId, kind: &EffectKind, enabled: bool) {
    let p = Palette::of(ui.ctx());
    ui.label(RichText::new(text::effect_description(kind)).color(p.text_weak));
    ui.add_space(4.0);
    let mut on = enabled;
    if ui.checkbox(&mut on, text::EFFECT_ON).changed() {
        session.set_effect_enabled(id, on);
    }
    ui.add_space(6.0);
    let mut changed = kind.clone();
    let mut gesture = Gesture::default();
    match &mut changed {
        EffectKind::Eq(eq) => eq_controls(ui, &mut gesture, eq),
        EffectKind::Compressor(c) => compressor_controls(ui, &mut gesture, c),
        EffectKind::Reverb(r) => reverb_controls(ui, &mut gesture, r),
        EffectKind::Delay(d) => delay_controls(ui, &mut gesture, d),
        EffectKind::Limiter(l) => limiter_controls(ui, &mut gesture, l),
    }
    if gesture.started {
        session.begin_gesture(text::EDIT_CHANGE_EFFECT);
    }
    if &changed != kind {
        session.set_effect(id, changed);
    }
    if gesture.stopped {
        session.end_gesture();
    }
    ui.add_space(8.0);
    ui.separator();
    if ui.button(text::REMOVE_EFFECT).clicked() {
        session.remove_effect(id);
    }
}

fn grid(ui: &mut Ui, name: &str, contents: impl FnOnce(&mut Ui)) {
    egui::Grid::new(name)
        .num_columns(2)
        .spacing([12.0, 6.0])
        .min_col_width(96.0)
        .show(ui, contents);
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "slider values shown as text"
)]
fn eq_controls(ui: &mut Ui, gesture: &mut Gesture, eq: &mut EqSettings) {
    eq_curve(ui, eq);
    ui.add_space(6.0);
    grid(ui, "eq", |ui| {
        let mut low_cut = eq.low_cut_hz.is_some();
        ui.checkbox(&mut low_cut, text::LOW_CUT);
        let mut frequency = eq.low_cut_hz.unwrap_or(80.0);
        let response = ui.add_enabled(
            low_cut,
            egui::Slider::new(&mut frequency, 20.0..=500.0)
                .logarithmic(true)
                .custom_formatter(|v, _| text::hz(v as f32)),
        );
        gesture.add(&response);
        ui.end_row();
        eq.low_cut_hz = low_cut.then_some(frequency);

        heading_row(ui, text::LOW);
        slider_row(
            ui,
            gesture,
            text::GAIN,
            &mut eq.low_gain_db,
            -15.0..=15.0,
            Scale::linear(text::db),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::FREQUENCY,
            &mut eq.low_hz,
            40.0..=500.0,
            Scale::log(text::hz),
            "",
        );
        heading_row(ui, text::MID);
        slider_row(
            ui,
            gesture,
            text::GAIN,
            &mut eq.mid_gain_db,
            -15.0..=15.0,
            Scale::linear(text::db),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::FREQUENCY,
            &mut eq.mid_hz,
            200.0..=8000.0,
            Scale::log(text::hz),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::WIDTH_Q,
            &mut eq.mid_q,
            0.3..=6.0,
            Scale::log(text::q),
            "",
        );
        heading_row(ui, text::HIGH);
        slider_row(
            ui,
            gesture,
            text::GAIN,
            &mut eq.high_gain_db,
            -15.0..=15.0,
            Scale::linear(text::db),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::FREQUENCY,
            &mut eq.high_hz,
            1500.0..=16000.0,
            Scale::log(text::hz),
            "",
        );

        let mut high_cut = eq.high_cut_hz.is_some();
        ui.checkbox(&mut high_cut, text::HIGH_CUT);
        let mut frequency = eq.high_cut_hz.unwrap_or(12_000.0);
        let response = ui.add_enabled(
            high_cut,
            egui::Slider::new(&mut frequency, 2000.0..=20_000.0)
                .logarithmic(true)
                .custom_formatter(|v, _| text::hz(v as f32)),
        );
        gesture.add(&response);
        ui.end_row();
        eq.high_cut_hz = high_cut.then_some(frequency);
    });
}

fn heading_row(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).strong());
    ui.end_row();
}

/// The EQ's frequency response, from 20 Hz to 20 kHz, ±18 dB.
#[allow(clippy::cast_precision_loss, reason = "screen coordinates")]
fn eq_curve(ui: &mut Ui, eq: &EqSettings) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 120.0), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, text::EQ_CURVE));
    let p = Palette::of(ui.ctx());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), p.grid);
    let params = EqParams {
        low_cut_hz: eq.low_cut_hz,
        low_gain_db: eq.low_gain_db,
        low_hz: eq.low_hz,
        mid_gain_db: eq.mid_gain_db,
        mid_hz: eq.mid_hz,
        mid_q: eq.mid_q,
        high_gain_db: eq.high_gain_db,
        high_hz: eq.high_hz,
        high_cut_hz: eq.high_cut_hz,
    };
    let filter = Eq::new(&params, 48_000);
    let (low, high) = (20.0_f32.ln(), 20_000.0_f32.ln());
    let x_of = |hz: f32| rect.left() + (hz.ln() - low) / (high - low) * rect.width();
    let range = 18.0;
    let y_of =
        |db: f32| rect.center().y - (db / range).clamp(-1.0, 1.0) * (rect.height() / 2.0 - 6.0);
    for hz in [100.0, 1000.0, 10_000.0] {
        painter.vline(x_of(hz), rect.y_range(), Stroke::new(1.0, p.grid_beat));
        let (x, align) = if hz > 5000.0 {
            (x_of(hz) - 3.0, Align2::RIGHT_BOTTOM)
        } else {
            (x_of(hz) + 3.0, Align2::LEFT_BOTTOM)
        };
        painter.text(
            pos2(x, rect.bottom() - 3.0),
            align,
            text::hz(hz),
            FontId::proportional(size::SMALL - 2.0),
            p.text_weak,
        );
    }
    painter.hline(rect.x_range(), y_of(0.0), Stroke::new(1.0, p.grid_bar));
    let points: Vec<egui::Pos2> = (0..=160)
        .map(|i| {
            let x = rect.left() + rect.width() * i as f32 / 160.0;
            let hz = (low + (high - low) * (x - rect.left()) / rect.width()).exp();
            pos2(x, y_of(filter.response_db(hz)))
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(2.0, p.accent)));
}

fn compressor_controls(ui: &mut Ui, gesture: &mut Gesture, c: &mut CompressorSettings) {
    grid(ui, "compressor", |ui| {
        slider_row(
            ui,
            gesture,
            text::THRESHOLD,
            &mut c.threshold_db,
            -60.0..=0.0,
            Scale::linear(text::db),
            text::THRESHOLD_HINT,
        );
        slider_row(
            ui,
            gesture,
            text::RATIO,
            &mut c.ratio,
            1.0..=20.0,
            Scale::log(text::ratio),
            text::RATIO_HINT,
        );
        slider_row(
            ui,
            gesture,
            text::ATTACK,
            &mut c.attack_ms,
            0.1..=100.0,
            Scale::log(text::ms),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::RELEASE,
            &mut c.release_ms,
            10.0..=1000.0,
            Scale::log(text::ms),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::KNEE,
            &mut c.knee_db,
            0.0..=24.0,
            Scale::linear(text::db),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::MAKEUP,
            &mut c.makeup_db,
            0.0..=24.0,
            Scale::linear(text::db),
            "",
        );
    });
}

fn reverb_controls(ui: &mut Ui, gesture: &mut Gesture, r: &mut ReverbSettings) {
    grid(ui, "reverb", |ui| {
        slider_row(
            ui,
            gesture,
            text::AMOUNT,
            &mut r.mix,
            0.0..=1.0,
            Scale::linear(text::percent),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::ROOM_SIZE,
            &mut r.size,
            0.0..=1.0,
            Scale::linear(text::percent),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::DAMPING,
            &mut r.damping,
            0.0..=1.0,
            Scale::linear(text::percent),
            text::DAMPING_HINT,
        );
        slider_row(
            ui,
            gesture,
            text::STEREO_WIDTH,
            &mut r.width,
            0.0..=1.0,
            Scale::linear(text::percent),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::PRE_DELAY,
            &mut r.pre_delay_ms,
            0.0..=100.0,
            Scale::linear(text::ms),
            "",
        );
    });
}

fn delay_controls(ui: &mut Ui, gesture: &mut Gesture, d: &mut DelaySettings) {
    let mut synced = matches!(d.time, DelayTime::Synced(_));
    if ui.checkbox(&mut synced, text::IN_TIME).changed() {
        d.time = if synced {
            DelayTime::Synced(Ticks::note_value(3, 16))
        } else {
            DelayTime::Seconds(0.3)
        };
    }
    grid(ui, "delay", |ui| {
        match &mut d.time {
            DelayTime::Synced(length) => {
                ui.label(text::TIME);
                let current = SYNCED_TIMES
                    .iter()
                    .find(|(_, n, of)| Ticks::note_value(*n, *of) == *length)
                    .map_or("—", |(name, _, _)| *name);
                egui::ComboBox::from_id_salt("delay-time")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (name, n, of) in SYNCED_TIMES {
                            let value = Ticks::note_value(n, of);
                            ui.selectable_value(length, value, name);
                        }
                    });
                ui.end_row();
            }
            DelayTime::Seconds(seconds) => {
                slider_row(
                    ui,
                    gesture,
                    text::TIME,
                    seconds,
                    0.01..=2.0,
                    Scale::log(text::seconds),
                    "",
                );
            }
        }
        slider_row(
            ui,
            gesture,
            text::REPEATS,
            &mut d.feedback,
            0.0..=0.95,
            Scale::linear(text::percent),
            text::REPEATS_HINT,
        );
        slider_row(
            ui,
            gesture,
            text::AMOUNT,
            &mut d.mix,
            0.0..=1.0,
            Scale::linear(text::percent),
            "",
        );
        slider_row(
            ui,
            gesture,
            text::TONE,
            &mut d.tone_hz,
            1000.0..=20_000.0,
            Scale::log(text::hz),
            text::TONE_HINT,
        );
    });
    ui.checkbox(&mut d.ping_pong, text::PING_PONG);
}

fn limiter_controls(ui: &mut Ui, gesture: &mut Gesture, l: &mut LimiterSettings) {
    grid(ui, "limiter", |ui| {
        slider_row(
            ui,
            gesture,
            text::DRIVE,
            &mut l.gain_db,
            0.0..=18.0,
            Scale::linear(text::db),
            text::DRIVE_HINT,
        );
        slider_row(
            ui,
            gesture,
            text::CEILING,
            &mut l.ceiling_db,
            -12.0..=0.0,
            Scale::linear(text::db),
            text::CEILING_HINT,
        );
        slider_row(
            ui,
            gesture,
            text::RELEASE,
            &mut l.release_ms,
            10.0..=1000.0,
            Scale::log(text::ms),
            "",
        );
    });
}

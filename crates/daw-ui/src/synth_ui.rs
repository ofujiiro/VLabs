// SPDX-License-Identifier: GPL-3.0-or-later

//! The synth's window (decision 0019): presets on top, then the controls
//! in the order the sound flows through them, left to right: oscillators,
//! the filter (its response drawn), the two envelopes (each drawn as its
//! shape), movement and playing. A window inside the program, so the
//! computer keyboard keeps playing the track while it is open. A slider
//! drag is one undo step.

use daw_model::{
    LfoShape, SynthEnvelope, SynthFilter, SynthOscillator, SynthPreset, SynthSettings, SynthWave,
    TrackId,
};
use egui::{CornerRadius, Response, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::icons::{Icon, Look, icon_button};
use crate::session::Session;
use crate::text;
use crate::theme::Palette;
use crate::widgets::{Gesture, Scale, slider_row};

/// Where a synth window first opens: right of the song view's track
/// headers, so the button that opened it stays in reach.
const WINDOW_LEFT: f32 = 262.0;

/// The width of each column of controls.
const COLUMN: f32 = 230.0;
/// The width of a slider inside a column (its value is shown beside it).
const SLIDER: f32 = 76.0;

/// The button that opens or closes a synth track's window (track header,
/// location line, mixer strip).
pub fn window_button(ui: &mut Ui, session: &mut Session, track: TrackId, size: f32) -> Response {
    let open = session.is_synth_window_open(track);
    let label = if open {
        text::CLOSE_SYNTH
    } else {
        text::OPEN_SYNTH
    };
    let hint = session
        .synth_settings(track)
        .map_or_else(String::new, |s| text::synth_description(&s));
    let response = icon_button(ui, Icon::Window, label, &hint, Look::on(open), size);
    if response.clicked() {
        session.toggle_synth_window(track);
    }
    response
}

/// Shows every open synth window.
pub fn show_windows(ctx: &egui::Context, session: &mut Session) {
    for (index, track) in session.synth_windows().into_iter().enumerate() {
        let Some(settings) = session.synth_settings(track) else {
            continue;
        };
        let name = session
            .project()
            .track(track)
            .map_or_else(String::new, |t| t.name.clone());
        let mut open = true;
        #[allow(clippy::cast_precision_loss, reason = "a small count")]
        let offset = index as f32 * 24.0;
        egui::Window::new(text::synth_title(&name))
            .id(egui::Id::unique(("synth-window", track.0)))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_pos(pos2(WINDOW_LEFT + offset, 118.0 + offset))
            .show(ctx, |ui| editor(ui, session, track, settings));
        if !open {
            session.close_synth_window(track);
        }
    }
}

/// The window's contents.
fn editor(ui: &mut Ui, session: &mut Session, track: TrackId, settings: SynthSettings) {
    // Compact rows: twenty-odd controls in a window that fits the
    // program's smallest size.
    ui.spacing_mut().slider_width = SLIDER;
    ui.spacing_mut().interact_size.y = 20.0;
    ui.spacing_mut().button_padding.y = 1.0;
    ui.spacing_mut().item_spacing.y = 3.0;
    preset_row(ui, session, track, &settings);
    ui.add_space(8.0);
    let mut changed = settings;
    let mut gesture = Gesture::default();
    // Three columns, in the order the sound flows: the oscillators; the
    // filter, then what moves it; the envelopes, then how notes play. They
    // fit the program's smallest window.
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        column(ui, |ui| {
            heading(ui, text::OSCILLATORS);
            oscillators(ui, &mut gesture, &mut changed, track);
        });
        column(ui, |ui| {
            heading(ui, text::FILTER);
            filter(ui, &mut gesture, &mut changed, track);
            ui.add_space(10.0);
            heading(ui, text::MOVEMENT);
            movement(ui, &mut gesture, &mut changed, track);
        });
        column(ui, |ui| {
            heading(ui, text::ENVELOPES);
            envelopes(ui, &mut gesture, &mut changed, track);
            ui.add_space(4.0);
            heading(ui, text::PLAYING);
            playing(ui, &mut gesture, &mut changed, track);
        });
    });
    if gesture.started {
        session.begin_gesture(text::EDIT_CHANGE_SYNTH);
    }
    if changed != settings {
        session.set_synth(track, changed);
    }
    if gesture.stopped {
        session.end_gesture();
    }
    // Each setting's slider can be given a MIDI controller's knob
    // (decision 0033).
    for (response, param) in &gesture.sliders {
        let target = daw_model::LinkTarget::Track {
            track,
            setting: daw_model::AutomationTarget::Instrument { param: *param },
        };
        crate::midi_ui::assign_menu(response, session, target);
    }
}

fn preset_row(ui: &mut Ui, session: &mut Session, track: TrackId, settings: &SynthSettings) {
    ui.horizontal(|ui| {
        let label = ui
            .label(RichText::new(text::SYNTH_PRESET).strong())
            .on_hover_text(text::SYNTH_PRESET_HINT);
        let current =
            SynthPreset::matching(settings).map_or(text::SYNTH_CUSTOM, text::synth_preset_name);
        let mut chosen = None;
        egui::ComboBox::from_id_salt(("synth-preset", track.0))
            .selected_text(current)
            .width(160.0)
            .show_ui(ui, |ui| {
                for preset in SynthPreset::ALL {
                    let selected = SynthPreset::matching(settings) == Some(preset);
                    if ui
                        .selectable_label(selected, text::synth_preset_name(preset))
                        .clicked()
                    {
                        chosen = Some(preset);
                    }
                }
            })
            .response
            .labelled_by(label.id);
        if let Some(preset) = chosen {
            session.set_synth_preset(track, preset);
        }
    });
}

/// A column of controls.
fn column(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.set_width(COLUMN);
        contents(ui);
    });
}

/// A section's heading.
fn heading(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).heading());
    ui.add_space(2.0);
}

/// A two-column grid of labelled controls.
fn grid(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, contents: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([6.0, 3.0])
        .min_col_width(64.0)
        .show(ui, contents);
}

fn sub_heading(ui: &mut Ui, label: &str) {
    let p = Palette::of(ui.ctx());
    ui.label(RichText::new(label).strong().color(p.text_weak));
    ui.end_row();
}

/// A row choosing one of a few values from a list.
fn choice_row<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    value: &mut T,
    all: &[T],
    name: fn(T) -> &'static str,
) {
    let label = ui.label(label);
    egui::ComboBox::from_id_salt(id)
        .selected_text(name(*value))
        .width(SLIDER + 46.0)
        .show_ui(ui, |ui| {
            for &option in all {
                ui.selectable_value(value, option, name(option));
            }
        })
        .response
        .labelled_by(label.id);
    ui.end_row();
}

/// A row with a slider of whole steps (octaves, semitones).
fn steps_row(
    ui: &mut Ui,
    gesture: &mut Gesture,
    label: &str,
    value: &mut i8,
    range: std::ops::RangeInclusive<i8>,
) {
    let name = ui.label(label);
    let mut wide = f32::from(*value);
    let response = ui
        .add(
            egui::Slider::new(
                &mut wide,
                f32::from(*range.start())..=f32::from(*range.end()),
            )
            .step_by(1.0)
            .custom_formatter(|v, _| {
                #[allow(clippy::cast_possible_truncation, reason = "whole steps")]
                let v = v.round() as i8;
                text::signed(v)
            }),
        )
        .labelled_by(name.id);
    gesture.add(&response);
    #[allow(clippy::cast_possible_truncation, reason = "whole steps in range")]
    {
        *value = wide.round() as i8;
    }
    ui.end_row();
}

fn oscillators(ui: &mut Ui, gesture: &mut Gesture, s: &mut SynthSettings, track: TrackId) {
    grid(ui, ("synth-oscillators", track.0), |ui| {
        for (index, (osc, heading)) in [
            (&mut s.osc1, text::OSCILLATOR_1),
            (&mut s.osc2, text::OSCILLATOR_2),
        ]
        .into_iter()
        .enumerate()
        {
            sub_heading(ui, heading);
            oscillator(ui, gesture, osc, (track.0, index));
        }
        sub_heading(ui, text::NOISE);
        slider_row(
            ui,
            gesture,
            text::LEVEL,
            &mut s.noise,
            0.0..=1.0,
            Scale::linear(text::percent),
            "",
            Some(8),
        );
        slider_row(
            ui,
            gesture,
            text::PULSE_WIDTH,
            &mut s.pulse_width,
            0.05..=0.95,
            Scale::linear(text::percent),
            text::PULSE_WIDTH_HINT,
            Some(9),
        );
    });
}

fn oscillator(ui: &mut Ui, gesture: &mut Gesture, osc: &mut SynthOscillator, id: (u64, usize)) {
    // Each oscillator's level and fine tune are automatable settings 4 to
    // 7, two each.
    let index = u32::try_from(id.1).unwrap_or(0);
    choice_row(
        ui,
        ("synth-wave", id),
        text::WAVE,
        &mut osc.wave,
        &SynthWave::ALL,
        text::synth_wave_name,
    );
    steps_row(ui, gesture, text::OCTAVE, &mut osc.octave, -3..=3);
    steps_row(ui, gesture, text::SEMITONES, &mut osc.semitones, -12..=12);
    slider_row(
        ui,
        gesture,
        text::FINE_TUNE,
        &mut osc.fine,
        -100.0..=100.0,
        Scale::linear(text::cents),
        text::FINE_TUNE_HINT,
        Some(5 + 2 * index),
    );
    slider_row(
        ui,
        gesture,
        text::LEVEL,
        &mut osc.level,
        0.0..=1.0,
        Scale::linear(text::percent),
        "",
        Some(4 + 2 * index),
    );
}

fn filter(ui: &mut Ui, gesture: &mut Gesture, s: &mut SynthSettings, track: TrackId) {
    filter_curve(ui, s);
    ui.add_space(6.0);
    grid(ui, ("synth-filter", track.0), |ui| {
        choice_row(
            ui,
            ("synth-filter-kind", track.0),
            text::FILTER_KIND,
            &mut s.filter,
            &SynthFilter::ALL,
            text::synth_filter_name,
        );
        slider_row(
            ui,
            gesture,
            text::CUTOFF,
            &mut s.cutoff_hz,
            20.0..=20_000.0,
            Scale::log(text::hz),
            text::CUTOFF_HINT,
            Some(0),
        );
        slider_row(
            ui,
            gesture,
            text::RESONANCE,
            &mut s.resonance,
            0.0..=1.0,
            Scale::linear(text::percent),
            text::RESONANCE_HINT,
            Some(1),
        );
        slider_row(
            ui,
            gesture,
            text::ENVELOPE_AMOUNT,
            &mut s.envelope_amount,
            -6.0..=6.0,
            Scale::linear(text::octaves),
            text::ENVELOPE_AMOUNT_HINT,
            Some(2),
        );
        slider_row(
            ui,
            gesture,
            text::KEY_TRACKING,
            &mut s.key_tracking,
            0.0..=1.0,
            Scale::linear(text::percent),
            text::KEY_TRACKING_HINT,
            Some(3),
        );
    });
}

fn envelopes(ui: &mut Ui, gesture: &mut Gesture, s: &mut SynthSettings, track: TrackId) {
    for (index, (envelope, heading)) in [
        (&mut s.amp_envelope, text::VOLUME_ENVELOPE),
        (&mut s.filter_envelope, text::FILTER_ENVELOPE),
    ]
    .into_iter()
    .enumerate()
    {
        let p = Palette::of(ui.ctx());
        ui.label(RichText::new(heading).strong().color(p.text_weak));
        envelope_shape(ui, envelope);
        // The volume envelope's settings are numbers 10 to 13, the
        // filter's 14 to 17.
        let base = if index == 0 { 10 } else { 14 };
        grid(ui, ("synth-envelope", track.0, index), |ui| {
            envelope_rows(ui, gesture, envelope, base);
        });
        ui.add_space(6.0);
    }
}

fn envelope_rows(ui: &mut Ui, gesture: &mut Gesture, envelope: &mut SynthEnvelope, base: u32) {
    for (param, label, value) in [
        (base, text::ATTACK, &mut envelope.attack),
        (base + 1, text::DECAY, &mut envelope.decay),
    ] {
        slider_row(
            ui,
            gesture,
            label,
            value,
            0.001..=10.0,
            Scale::log(text::time),
            "",
            Some(param),
        );
    }
    slider_row(
        ui,
        gesture,
        text::SUSTAIN,
        &mut envelope.sustain,
        0.0..=1.0,
        Scale::linear(text::percent),
        "",
        Some(base + 2),
    );
    slider_row(
        ui,
        gesture,
        text::RELEASE,
        &mut envelope.release,
        0.001..=10.0,
        Scale::log(text::time),
        "",
        Some(base + 3),
    );
}

fn movement(ui: &mut Ui, gesture: &mut Gesture, s: &mut SynthSettings, track: TrackId) {
    grid(ui, ("synth-movement", track.0), |ui| {
        choice_row(
            ui,
            ("synth-lfo-shape", track.0),
            text::LFO_SHAPE,
            &mut s.lfo_shape,
            &LfoShape::ALL,
            text::lfo_shape_name,
        );
        slider_row(
            ui,
            gesture,
            text::LFO_RATE,
            &mut s.lfo_rate_hz,
            0.05..=20.0,
            Scale::log(text::rate),
            "",
            Some(18),
        );
        slider_row(
            ui,
            gesture,
            text::VIBRATO,
            &mut s.vibrato_cents,
            0.0..=100.0,
            Scale::linear(text::cents),
            text::VIBRATO_HINT,
            Some(19),
        );
        slider_row(
            ui,
            gesture,
            text::WOBBLE,
            &mut s.wobble_octaves,
            0.0..=4.0,
            Scale::linear(text::octaves),
            text::WOBBLE_HINT,
            Some(20),
        );
        slider_row(
            ui,
            gesture,
            text::TREMOLO,
            &mut s.tremolo,
            0.0..=1.0,
            Scale::linear(text::percent),
            text::TREMOLO_HINT,
            Some(21),
        );
    });
}

fn playing(ui: &mut Ui, gesture: &mut Gesture, s: &mut SynthSettings, track: TrackId) {
    ui.checkbox(&mut s.mono, text::MONO)
        .on_hover_text(text::MONO_HINT);
    grid(ui, ("synth-playing", track.0), |ui| {
        // Glide only works with one note at a time.
        let label = ui.label(text::SYNTH_GLIDE);
        let response = ui
            .add_enabled(
                s.mono,
                egui::Slider::new(&mut s.glide_seconds, 0.0..=2.0)
                    .custom_formatter(|v, _| {
                        #[allow(clippy::cast_possible_truncation, reason = "a display value")]
                        let v = v as f32;
                        text::time(v)
                    })
                    .clamping(egui::SliderClamping::Always),
            )
            .labelled_by(label.id)
            .on_hover_text(text::SYNTH_GLIDE_HINT);
        gesture.add(&response);
        gesture.link(&response, 22);
        ui.end_row();
        slider_row(
            ui,
            gesture,
            text::VELOCITY_AMOUNT,
            &mut s.velocity,
            0.0..=1.0,
            Scale::linear(text::percent),
            text::VELOCITY_AMOUNT_HINT,
            Some(23),
        );
        slider_row(
            ui,
            gesture,
            text::VOLUME,
            &mut s.volume_db,
            -24.0..=6.0,
            Scale::linear(text::db),
            "",
            Some(24),
        );
    });
}

/// The filter's response, from 20 Hz to 20 kHz, with the cutoff marked.
#[allow(clippy::cast_precision_loss, reason = "screen coordinates")]
fn filter_curve(ui: &mut Ui, s: &SynthSettings) {
    let (rect, response) = ui.allocate_exact_size(vec2(COLUMN, 80.0), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, text::FILTER_CURVE));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let p = Palette::of(ui.ctx());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), p.grid);
    let (low, high) = (20.0_f32.ln(), 20_000.0_f32.ln());
    let x_of = |hz: f32| rect.left() + (hz.ln() - low) / (high - low) * rect.width();
    let (top_db, bottom_db) = (24.0_f32, -48.0_f32);
    let y_of = |db: f32| {
        let t = (top_db - db.clamp(bottom_db, top_db)) / (top_db - bottom_db);
        rect.top() + 4.0 + t * (rect.height() - 8.0)
    };
    for hz in [100.0, 1000.0, 10_000.0] {
        painter.vline(x_of(hz), rect.y_range(), Stroke::new(1.0, p.grid_beat));
    }
    painter.hline(rect.x_range(), y_of(0.0), Stroke::new(1.0, p.grid_bar));
    let cutoff = s.cutoff_hz.clamp(20.0, 20_000.0);
    painter.vline(
        x_of(cutoff),
        rect.y_range(),
        Stroke::new(1.0, p.accent.gamma_multiply(0.5)),
    );
    let points: Vec<egui::Pos2> = (0..=120)
        .map(|i| {
            let x = rect.left() + rect.width() * i as f32 / 120.0;
            let hz = (low + (high - low) * (x - rect.left()) / rect.width()).exp();
            pos2(x, y_of(filter_response_db(s, hz)))
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(2.0, p.accent)));
}

/// The level the filter leaves at a frequency, in dB (the analogue
/// response the synth's filter is modelled on).
fn filter_response_db(s: &SynthSettings, hz: f32) -> f32 {
    let resonance = s.resonance.clamp(0.0, 1.0);
    let damping = |base: f32| base + (0.05 - base) * resonance;
    let w = hz / s.cutoff_hz.clamp(20.0, 20_000.0);
    // |1 / (s² + k s + 1)| at s = jw.
    let stage = |k: f32| 1.0 / ((1.0 - w * w).powi(2) + (k * w).powi(2)).sqrt();
    let gain = 1.0 - 0.5 * resonance;
    let magnitude = match s.filter {
        SynthFilter::LowPass24 => stage(1.848) * stage(damping(0.765)),
        SynthFilter::LowPass12 => stage(damping(1.414)),
        SynthFilter::HighPass => w * w * stage(damping(1.414)),
        SynthFilter::BandPass => {
            let k = damping(1.414);
            k * w * stage(k)
        }
    } * gain;
    20.0 * magnitude.max(1e-6).log10()
}

/// An envelope drawn as its shape: rise, fall to the sustain level, hold,
/// release (longer times take more room, but not in proportion).
fn envelope_shape(ui: &mut Ui, envelope: &SynthEnvelope) {
    let (rect, response) = ui.allocate_exact_size(vec2(COLUMN, 36.0), Sense::hover());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, text::ENVELOPE_SHAPE));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), palette.grid);
    // Each time takes room growing with its square root; the hold takes a
    // fixed share.
    let room = |seconds: f32| seconds.clamp(0.001, 10.0).sqrt();
    let attack = room(envelope.attack);
    let decay = room(envelope.decay);
    let release = room(envelope.release);
    let hold = 0.6;
    let total = attack + decay + hold + release;
    let inner = rect.shrink2(vec2(6.0, 5.0));
    let along = |time: f32| inner.left() + time / total * inner.width();
    let height = |level: f32| inner.bottom() - level.clamp(0.0, 1.0) * inner.height();
    let sustain = envelope.sustain.clamp(0.0, 1.0);
    let points = vec![
        pos2(along(0.0), height(0.0)),
        pos2(along(attack), height(1.0)),
        pos2(along(attack + decay), height(sustain)),
        pos2(along(attack + decay + hold), height(sustain)),
        pos2(along(total), height(0.0)),
    ];
    painter.add(egui::Shape::line(points, Stroke::new(2.0, palette.accent)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawn_filter_passes_below_and_cuts_above_the_cutoff() {
        let mut s = SynthPreset::Plain.settings();
        s.cutoff_hz = 1000.0;
        assert!(filter_response_db(&s, 50.0).abs() < 0.5);
        assert!(filter_response_db(&s, 8000.0) < -60.0);
        s.filter = SynthFilter::HighPass;
        assert!(filter_response_db(&s, 50.0) < -40.0);
        assert!(filter_response_db(&s, 16_000.0).abs() < 0.5);
        s.filter = SynthFilter::BandPass;
        assert!(filter_response_db(&s, 1000.0).abs() < 0.5);
    }
}

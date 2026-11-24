// SPDX-License-Identifier: GPL-3.0-or-later

//! Small pieces shared by the mixer, the effect windows and the drum sound
//! settings: labelled sliders whose drags are single undo steps, and which
//! a MIDI controller's knob can be given (decision 0033).

use std::ops::RangeInclusive;

use egui::{Response, Ui};

/// Whether a slider drag started or ended this frame. The caller begins a
/// gesture before applying the change and ends it after, so a whole drag
/// becomes one undo step. It also collects the sliders of automatable
/// settings, by number, so the caller can offer each a MIDI control.
#[derive(Clone, Debug, Default)]
pub struct Gesture {
    /// A drag started.
    pub started: bool,
    /// A drag ended.
    pub stopped: bool,
    /// The sliders of automatable settings drawn: each with its setting's
    /// number (as automation numbers them).
    pub sliders: Vec<(Response, u32)>,
}

impl Gesture {
    /// Takes note of a slider's response.
    pub fn add(&mut self, response: &Response) {
        self.started |= response.drag_started();
        self.stopped |= response.drag_stopped();
    }

    /// Notes that a slider moves the automatable setting `param`.
    pub fn link(&mut self, response: &Response, param: u32) {
        self.sliders.push((response.clone(), param));
    }
}

/// How a slider's value is shown and scaled.
#[derive(Clone, Copy)]
pub struct Scale {
    /// Turns the value into text ("-3.0 dB").
    pub format: fn(f32) -> String,
    /// Whether the slider moves in equal ratios (frequencies, times).
    pub logarithmic: bool,
}

impl Scale {
    /// A linear slider.
    pub const fn linear(format: fn(f32) -> String) -> Self {
        Self {
            format,
            logarithmic: false,
        }
    }

    /// A logarithmic slider.
    pub const fn log(format: fn(f32) -> String) -> Self {
        Self {
            format,
            logarithmic: true,
        }
    }
}

/// A row of a two-column grid: a label, then a slider. Notes the slider in
/// `gesture` (with `param`, the automatable setting it moves, if it moves
/// one); `hint` (if any) is its tooltip.
#[allow(
    clippy::too_many_arguments,
    reason = "a row's label, value, range, scale, hint and setting"
)]
pub fn slider_row(
    ui: &mut Ui,
    gesture: &mut Gesture,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    scale: Scale,
    hint: &str,
    param: Option<u32>,
) -> Response {
    let name = ui.label(label);
    let format = scale.format;
    let mut response = ui
        .add(
            egui::Slider::new(value, range)
                .logarithmic(scale.logarithmic)
                .custom_formatter(move |v, _| {
                    #[allow(clippy::cast_possible_truncation, reason = "a display value")]
                    let v = v as f32;
                    format(v)
                })
                .clamping(egui::SliderClamping::Always),
        )
        .labelled_by(name.id);
    if !hint.is_empty() {
        response = response.on_hover_text(hint);
    }
    gesture.add(&response);
    if let Some(param) = param {
        gesture.link(&response, param);
    }
    ui.end_row();
    response
}

/// Linear gain as decibels (silence is -60 dB and below).
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.001 {
        -60.0
    } else {
        (20.0 * gain.log10()).max(-60.0)
    }
}

/// Decibels as linear gain (-60 dB and below is silence).
pub fn db_to_gain(db: f32) -> f32 {
    if db <= -59.9 {
        0.0
    } else {
        10.0_f32.powf(db / 20.0)
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Giving a MIDI controller's knob or fader a setting, from the slider's
//! own right-click menu (decision 0033).

use daw_model::LinkTarget;
use egui::{Response, RichText};

use crate::session::Session;
use crate::text;

/// Adds to a slider's right-click menu: which MIDI control moves its
/// setting (if one does), **Assign a MIDI control…** (the next knob moved
/// gets it) and **Forget the MIDI control**.
pub fn assign_menu(response: &Response, session: &mut Session, target: LinkTarget) {
    let control = session.midi_control_of(target);
    // A slider senses drags only: the menu opens on a right-click within
    // it without giving it a click's sense (which would change how its drag
    // starts, and split one drag into two undo steps).
    response.container_context_menu(|ui| {
        if let Some(control) = control {
            ui.label(RichText::new(text::moved_by(control)).small().weak());
        }
        let assign = if control.is_some() {
            text::ASSIGN_OTHER_MIDI_CONTROL
        } else {
            text::ASSIGN_MIDI_CONTROL
        };
        if ui
            .button(assign)
            .on_hover_text(text::ASSIGN_MIDI_CONTROL_HINT)
            .clicked()
        {
            session.learn_midi(target);
            ui.close();
        }
        if control.is_some() && ui.button(text::FORGET_MIDI_CONTROL).clicked() {
            session.forget_midi(target);
            ui.close();
        }
    });
}

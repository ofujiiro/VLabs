// SPDX-License-Identifier: GPL-3.0-or-later

//! The bars across the top: the transport (play, stop, record, where the
//! song is, tempo, time signature, metronome, loop, undo) and the location
//! line under it (what is being edited, its length, the grid).

use daw_engine::RecordState;
use daw_model::{TEMPO_RANGE, Ticks, TimeSignature};
use egui::{
    Align, Align2, Button, CornerRadius, DragValue, FontId, Layout, Popup, PopupCloseBehavior,
    RichText, Sense, Ui, vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::keys::{Command, Keymap};
use crate::session::{Grid, NOTE_LENGTHS, Session};
use crate::text;
use crate::theme::{Palette, size};

/// The size of the main transport buttons.
const BIG: f32 = 40.0;
/// The size of the smaller buttons.
const SMALL: f32 = 32.0;

/// Time signatures offered.
const SIGNATURES: [(u8, u8); 8] = [
    (2, 4),
    (3, 4),
    (4, 4),
    (5, 4),
    (6, 8),
    (7, 8),
    (9, 8),
    (12, 8),
];

/// Draws the transport bar; the commands its buttons run are added to
/// `actions`.
pub fn transport_bar(
    ui: &mut Ui,
    session: &mut Session,
    keymap: &Keymap,
    actions: &mut Vec<Command>,
) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        transport_buttons(ui, session, keymap, actions);
        recording_options(ui, session);
        ui.add_space(12.0);
        position_readout(ui, session);
        ui.add_space(12.0);
        tempo_field(ui, session);
        time_signature_button(ui, session);
        ui.add_space(12.0);
        let metronome_hint = with_hint(text::METRONOME_HINT, &keymap.text(Command::Metronome));
        let look = Look::on(session.metronome());
        if icon_button(
            ui,
            Icon::Metronome,
            text::METRONOME,
            &metronome_hint,
            look,
            SMALL,
        )
        .clicked()
        {
            actions.push(Command::Metronome);
        }
        metronome_options(ui, session);
        let look = Look::on(session.is_looping());
        if icon_button(
            ui,
            Icon::Loop,
            text::LOOP,
            &keymap.text(Command::Loop),
            look,
            SMALL,
        )
        .clicked()
        {
            actions.push(Command::Loop);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            undo_redo(ui, session, keymap, actions);
        });
    });
}

/// Back to start, play, stop, record.
fn transport_buttons(
    ui: &mut Ui,
    session: &mut Session,
    keymap: &Keymap,
    actions: &mut Vec<Command>,
) {
    let hint = |command: Command| keymap.text(command);
    if icon_button(
        ui,
        Icon::ToStart,
        text::BACK_TO_START,
        &hint(Command::ToStart),
        Look::Normal,
        SMALL,
    )
    .clicked()
    {
        actions.push(Command::ToStart);
    }
    let playing = session.is_playing() && !session.is_recording();
    if icon_button(
        ui,
        Icon::Play,
        text::PLAY,
        &hint(Command::PlayStop),
        Look::on(playing),
        BIG,
    )
    .clicked()
        && !playing
    {
        actions.push(Command::PlayStop);
    }
    if icon_button(
        ui,
        Icon::Stop,
        text::STOP,
        &hint(Command::PlayStop),
        Look::Normal,
        BIG,
    )
    .clicked()
    {
        session.stop();
    }
    let recording = session.is_recording();
    let (label, look) = if recording {
        (text::STOP_RECORDING, Look::Recording)
    } else {
        (text::RECORD, Look::Normal)
    };
    if icon_button(ui, Icon::Record, label, &hint(Command::Record), look, BIG).clicked() {
        actions.push(Command::Record);
    }
}

/// Undo and redo, with what they would undo in their tooltips.
fn undo_redo(ui: &mut Ui, session: &Session, keymap: &Keymap, actions: &mut Vec<Command>) {
    let doc = session.document();
    let redo_hint = match doc.redo_label() {
        Some(label) => text::with_shortcut(&text::redo_named(label), &keymap.text(Command::Redo)),
        None => text::NOTHING_TO_REDO.to_owned(),
    };
    let undo_hint = match doc.undo_label() {
        Some(label) => text::with_shortcut(&text::undo_named(label), &keymap.text(Command::Undo)),
        None => text::NOTHING_TO_UNDO.to_owned(),
    };
    let (can_undo, can_redo) = (doc.undo_label().is_some(), doc.redo_label().is_some());
    let redo = ui
        .add_enabled_ui(can_redo, |ui| {
            icon_button(ui, Icon::Redo, text::REDO, &redo_hint, Look::Normal, SMALL)
        })
        .inner;
    if redo.clicked() {
        actions.push(Command::Redo);
    }
    let undo = ui
        .add_enabled_ui(can_undo, |ui| {
            icon_button(ui, Icon::Undo, text::UNDO, &undo_hint, Look::Normal, SMALL)
        })
        .inner;
    if undo.clicked() {
        actions.push(Command::Undo);
    }
}

fn with_hint(explanation: &str, shortcut: &str) -> String {
    if shortcut.is_empty() {
        explanation.to_owned()
    } else {
        format!("{explanation}  ({shortcut})")
    }
}

/// A small chevron button that opens a popup of options.
fn options_button(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui)) {
    let response = icon_button(ui, Icon::ChevronDown, label, "", Look::Normal, 22.0);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(240.0);
            add_contents(ui);
        });
}

fn recording_options(ui: &mut Ui, session: &mut Session) {
    options_button(ui, text::RECORDING_OPTIONS, |ui| {
        ui.label(RichText::new(text::COUNT_IN).strong());
        ui.label(RichText::new(text::COUNT_IN_HINT).small().weak());
        ui.horizontal(|ui| {
            for bars in 0..=2 {
                let label = if bars == 0 {
                    text::OFF.to_owned()
                } else {
                    text::bars(bars)
                };
                if ui
                    .selectable_label(session.count_in_bars() == bars, label)
                    .clicked()
                {
                    session.set_count_in_bars(bars);
                }
            }
        });
        ui.separator();
        let mut snap = session.snap_recording();
        if ui
            .checkbox(&mut snap, text::QUANTIZE)
            .on_hover_text(text::QUANTIZE_HINT)
            .changed()
        {
            session.set_snap_recording(snap);
        }
        let mut click = session.metronome_when_recording();
        if ui
            .checkbox(&mut click, text::METRONOME_WHEN_RECORDING)
            .changed()
        {
            session.set_metronome_when_recording(click);
        }
    });
}

fn metronome_options(ui: &mut Ui, session: &mut Session) {
    options_button(ui, text::METRONOME_OPTIONS, |ui| {
        let label = ui.label(RichText::new(text::METRONOME_VOLUME).strong());
        let mut volume = session.metronome_volume();
        let slider = ui
            .add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false))
            .labelled_by(label.id);
        if slider.changed() {
            session.set_metronome_volume(volume);
        }
        let mut click = session.metronome_when_recording();
        if ui
            .checkbox(&mut click, text::METRONOME_WHEN_RECORDING)
            .changed()
        {
            session.set_metronome_when_recording(click);
        }
    });
}

/// Bar.beat.sixteenth in large figures, the clock under it; shows the
/// count-in while counting.
fn position_readout(ui: &mut Ui, session: &Session) {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(118.0, BIG + 4.0), Sense::hover());
    let project = session.project();
    let song_position = session.placement().to_song(session.position());
    let (main, small, color) = match session.record_state() {
        RecordState::CountIn(left) => {
            let beat = project.meter.signature_at(song_position).beat_length();
            let beats = u32::try_from((left.0 + beat.0 - 1) / beat.0.max(1))
                .unwrap_or(0)
                .max(1);
            (beats.to_string(), text::COUNT_IN.to_owned(), p.record)
        }
        state => {
            let at = project.meter.position(song_position);
            let sixteenth = Ticks::WHOLE.0 / 16;
            let six = u32::try_from(at.ticks.0 / sixteenth).unwrap_or(0);
            let seconds = project.tempo.seconds_at(song_position);
            let color = if state == RecordState::Recording {
                p.record
            } else {
                p.text
            };
            (
                text::position(at.bar + 1, at.beat + 1, six + 1),
                text::clock(seconds),
                color,
            )
        }
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), p.background);
    painter.text(
        rect.center_top() + vec2(0.0, 4.0),
        Align2::CENTER_TOP,
        main,
        FontId::monospace(size::READOUT),
        color,
    );
    painter.text(
        rect.center_bottom() - vec2(0.0, 4.0),
        Align2::CENTER_BOTTOM,
        small,
        FontId::monospace(size::SMALL),
        p.text_weak,
    );
    response.on_hover_text(text::POSITION);
}

fn tempo_field(ui: &mut Ui, session: &mut Session) {
    let mut bpm = session.tempo();
    let response = ui
        .scope(|ui| {
            ui.style_mut().override_font_id = Some(FontId::monospace(size::HEADING));
            ui.spacing_mut().interact_size = vec2(64.0, BIG - 6.0);
            ui.add(
                DragValue::new(&mut bpm)
                    .range(TEMPO_RANGE)
                    .speed(0.2)
                    .custom_formatter(|value, _| text::bpm(value)),
            )
        })
        .inner;
    if response.drag_started() {
        session.document_mut().begin_gesture(text::EDIT_TEMPO);
    }
    if response.changed() {
        session.set_tempo(bpm);
    }
    if response.drag_stopped() {
        session.document_mut().end_gesture();
    }
    response.on_hover_text(text::TEMPO);
    ui.label(RichText::new(text::BPM).small().weak());
}

fn time_signature_button(ui: &mut Ui, session: &mut Session) {
    let current = session.time_signature();
    let label = RichText::new(text::time_signature(current.numerator, current.denominator))
        .font(FontId::monospace(size::HEADING));
    let response = ui
        .add(Button::new(label).min_size(vec2(56.0, BIG - 6.0)))
        .on_hover_text(text::TIME_SIGNATURE);
    Popup::menu(&response).show(|ui| {
        for (numerator, denominator) in SIGNATURES {
            let selected = current.numerator == numerator && current.denominator == denominator;
            if ui
                .selectable_label(selected, text::time_signature(numerator, denominator))
                .clicked()
                && let Some(signature) = TimeSignature::new(numerator, denominator)
            {
                session.set_time_signature(signature);
                ui.close();
            }
        }
    });
}

/// The line under the transport: where you are, and the editor's grid.
pub fn location_bar(
    ui: &mut Ui,
    session: &mut Session,
    keyboard_shown: bool,
    actions: &mut Vec<Command>,
) {
    let p = Palette::of(ui.ctx());
    ui.horizontal_centered(|ui| {
        let pattern_name = session.pattern().name.clone();
        let place = text::location_prefix(&session.display_name(), session.track_name());
        ui.label(RichText::new(place).color(p.text_weak));
        ui.label(RichText::new(pattern_name).heading());

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let keyboard_label = if keyboard_shown {
                text::HIDE_KEYBOARD
            } else {
                text::SHOW_KEYBOARD
            };
            let look = if keyboard_shown {
                Look::Active
            } else {
                Look::Normal
            };
            if icon_button(ui, Icon::Keyboard, keyboard_label, "", look, 30.0).clicked() {
                actions.push(Command::ToggleKeyboard);
            }
            ui.add_space(12.0);
            note_length_menu(ui, session);
            ui.label(RichText::new(text::NOTE_LENGTH).weak());
            ui.add_space(12.0);
            grid_menu(ui, session);
            ui.label(RichText::new(text::SNAP).weak())
                .on_hover_text(text::SNAP_HINT);
            ui.add_space(12.0);
            pattern_length(ui, session);
        });
    });
}

fn grid_menu(ui: &mut Ui, session: &mut Session) {
    let response = ui
        .add(Button::new(text::grid_name(session.grid)).min_size(vec2(84.0, 28.0)))
        .on_hover_text(text::SNAP_HINT);
    Popup::menu(&response).show(|ui| {
        for grid in Grid::CHOICES {
            if ui
                .selectable_label(session.grid == grid, text::grid_name(grid))
                .clicked()
            {
                session.grid = grid;
                ui.close();
            }
        }
    });
}

fn note_length_menu(ui: &mut Ui, session: &mut Session) {
    let current = NOTE_LENGTHS
        .iter()
        .find(|&&n| Ticks(Ticks::WHOLE.0 / n) == session.note_length)
        .map_or_else(
            || text::CUSTOM_LENGTH.to_owned(),
            |&n| text::note_value_name(n),
        );
    let response = ui
        .add(Button::new(current).min_size(vec2(84.0, 28.0)))
        .on_hover_text(text::NOTE_LENGTH_HINT);
    Popup::menu(&response).show(|ui| {
        for n in NOTE_LENGTHS {
            let length = Ticks(Ticks::WHOLE.0 / n);
            if ui
                .selectable_label(session.note_length == length, text::note_value_name(n))
                .clicked()
            {
                session.note_length = length;
                ui.close();
            }
        }
    });
}

fn pattern_length(ui: &mut Ui, session: &mut Session) {
    let bars = session.pattern_bars();
    if icon_button(ui, Icon::Plus, text::LONGER, "", Look::Normal, 26.0).clicked() {
        session.set_pattern_bars(bars + 1);
    }
    ui.label(RichText::new(text::bars(bars)).monospace())
        .on_hover_text(text::PATTERN_LENGTH_HINT);
    let shorter = ui
        .add_enabled_ui(bars > 1, |ui| {
            icon_button(ui, Icon::Minus, text::SHORTER, "", Look::Normal, 26.0)
        })
        .inner;
    if shorter.clicked() {
        session.set_pattern_bars(bars - 1);
    }
    ui.label(RichText::new(text::PATTERN_LENGTH).weak());
}

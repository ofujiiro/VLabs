// SPDX-License-Identifier: GPL-3.0-or-later

//! The bars across the top: the transport (play, stop, record, where the
//! song is, tempo, time signature, metronome, loop, undo) and the location
//! line under it (where you are, the way around, and the settings of what
//! is on screen).

use daw_engine::RecordState;
use daw_model::{TEMPO_RANGE, Ticks, TimeSignature};
use egui::{
    Align, Align2, Button, CornerRadius, DragValue, FontId, Layout, Popup, PopupCloseBehavior,
    RichText, Sense, Ui, vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::keys::{Command, Keymap};
use crate::session::{Grid, NOTE_LENGTHS, Session, View};
use crate::text;
use crate::theme::{Palette, size};
use crate::workspace::Intent;

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
        let loop_label = if session.view() == View::Song {
            text::LOOP_SONG
        } else {
            text::LOOP
        };
        if icon_button(
            ui,
            Icon::Loop,
            loop_label,
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
    let song_position = session.song_position();
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

/// The line under the transport (decision 0014): where you are, as a
/// path that is also the way around (song › track › pattern), and the
/// settings of what is on screen. On the right, the switches for the
/// bottom of the window: the keyboard or the mixer.
pub fn location_bar(
    ui: &mut Ui,
    session: &mut Session,
    bottom: (bool, bool),
    actions: &mut Vec<Command>,
    intents: &mut Vec<Intent>,
) {
    let (keyboard_shown, mixer_shown) = bottom;
    let p = Palette::of(ui.ctx());
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        match session.view() {
            View::Song => {
                ui.label(RichText::new(session.display_name()).heading());
            }
            View::Pattern => {
                let song = ui
                    .add(
                        Button::new(RichText::new(session.display_name()).color(p.text_weak))
                            .frame(false),
                    )
                    .on_hover_text(text::SHOW_SONG);
                if song.clicked() {
                    session.show_song();
                }
                ui.label(RichText::new("›").color(p.text_weak));
                track_menu(ui, session);
                plugin_button(ui, session);
                ui.label(RichText::new("›").color(p.text_weak));
                pattern_menu(ui, session, intents);
                if !session.is_placed() {
                    ui.label(RichText::new(text::NOT_PLACED).small().color(p.selected))
                        .on_hover_text(text::NOT_PLACED_HINT);
                }
            }
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mixer_label = if mixer_shown {
                text::HIDE_MIXER
            } else {
                text::SHOW_MIXER
            };
            if icon_button(
                ui,
                Icon::Mixer,
                mixer_label,
                "",
                Look::on(mixer_shown),
                30.0,
            )
            .clicked()
            {
                actions.push(Command::ToggleMixer);
            }
            let keyboard_label = if keyboard_shown {
                text::HIDE_KEYBOARD
            } else {
                text::SHOW_KEYBOARD
            };
            if icon_button(
                ui,
                Icon::Keyboard,
                keyboard_label,
                "",
                Look::on(keyboard_shown),
                30.0,
            )
            .clicked()
            {
                actions.push(Command::ToggleKeyboard);
            }
            if session.view() == View::Song {
                return;
            }
            ui.add_space(12.0);
            if session.is_drum_track() {
                pattern_length(ui, session);
                ui.add_space(12.0);
                steps_menu(ui, session);
                ui.label(RichText::new(text::STEPS).weak())
                    .on_hover_text(text::STEPS_HINT);
                ui.add_space(12.0);
                kit_menu(ui, session);
                ui.label(RichText::new(text::KIT).weak())
                    .on_hover_text(text::KIT_HINT);
            } else {
                note_length_menu(ui, session);
                ui.label(RichText::new(text::NOTE_LENGTH).weak());
                ui.add_space(12.0);
                grid_menu(ui, session);
                ui.label(RichText::new(text::SNAP).weak())
                    .on_hover_text(text::SNAP_HINT);
                ui.add_space(12.0);
                pattern_length(ui, session);
            }
        });
    });
}

/// For a plugin track, the button that opens the plugin's own window.
fn plugin_button(ui: &mut Ui, session: &mut Session) {
    let Some(target) = session.track_plugin(session.track_id()) else {
        return;
    };
    if session.is_plugin_missing(target) || !session.has_plugin_window(target) {
        return;
    }
    let open = session.is_plugin_window_open(target);
    let label = if open {
        text::CLOSE_PLUGIN
    } else {
        text::OPEN_PLUGIN
    };
    let hint = session.plugin_name(target);
    if icon_button(ui, Icon::Window, label, &hint, Look::on(open), 26.0).clicked() {
        session.toggle_plugin_window(target);
    }
}

/// A button that opens a menu, with a small arrow.
fn dropdown(ui: &mut Ui, label: RichText, hint: &str) -> egui::Response {
    let response = ui
        .add(
            Button::new(label)
                .right_text("    ")
                .min_size(vec2(0.0, 28.0)),
        )
        .on_hover_text(hint);
    let arrow = egui::Rect::from_center_size(
        egui::pos2(response.rect.right() - 13.0, response.rect.center().y),
        vec2(12.0, 12.0),
    );
    crate::icons::paint(
        ui.painter(),
        Icon::ChevronDown,
        arrow,
        Palette::of(ui.ctx()).text_weak,
    );
    response
}

/// The track in the path: a menu of every track, to open another.
fn track_menu(ui: &mut Ui, session: &mut Session) {
    let name = RichText::new(session.track_name()).size(size::HEADING);
    let response = dropdown(ui, name, text::OPEN_TRACK_HINT);
    Popup::menu(&response).show(|ui| {
        ui.set_min_width(200.0);
        let tracks: Vec<_> = session
            .project()
            .tracks
            .iter()
            .map(|t| (t.id, t.name.clone()))
            .collect();
        for (track, name) in tracks {
            if ui
                .selectable_label(track == session.track_id(), name)
                .clicked()
            {
                session.open_track(track);
                ui.close();
            }
        }
    });
}

/// The pattern in the path: a menu to switch, add, copy, rename or delete.
fn pattern_menu(ui: &mut Ui, session: &mut Session, intents: &mut Vec<Intent>) {
    let name = RichText::new(session.pattern().name.clone()).heading();
    let response = dropdown(ui, name, text::PATTERN_MENU_HINT);
    Popup::menu(&response).show(|ui| {
        ui.set_min_width(220.0);
        let track = session.track_id();
        let patterns: Vec<_> = session
            .project()
            .track_patterns(track)
            .map(|p| (p.id, p.name.clone()))
            .collect();
        for (pattern, name) in patterns {
            if ui
                .selectable_label(pattern == session.pattern_id(), name)
                .clicked()
            {
                session.open_pattern(track, pattern);
                ui.close();
            }
        }
        ui.separator();
        if ui.button(text::NEW_PATTERN).clicked() {
            session.new_pattern();
            ui.close();
        }
        if ui.button(text::DUPLICATE_PATTERN).clicked() {
            session.duplicate_pattern();
            ui.close();
        }
        if ui.button(text::RENAME_PATTERN).clicked() {
            intents.push(Intent::RenamePattern(session.pattern_id()));
            ui.close();
        }
        if ui.button(text::DELETE_PATTERN).clicked() {
            let pattern = session.pattern_id();
            session.remove_pattern(pattern);
            ui.close();
        }
    });
}

/// Kits offered on drum tracks.
const KITS: [&str; 2] = [daw_model::ACOUSTIC_KIT, daw_model::ELECTRONIC_KIT];

fn kit_menu(ui: &mut Ui, session: &mut Session) {
    let track = session.track_id();
    let current = session
        .kit(track)
        .unwrap_or(daw_model::ACOUSTIC_KIT)
        .to_owned();
    let response = dropdown(ui, RichText::new(text::kit_name(&current)), text::KIT_HINT);
    Popup::menu(&response).show(|ui| {
        for kit in KITS {
            if ui
                .selectable_label(current == kit, text::kit_name(kit))
                .clicked()
            {
                session.set_kit(track, kit);
                ui.close();
            }
        }
    });
}

/// Step sizes offered for the step grid: 1/`n` of a whole note.
const STEP_SIZES: [i64; 5] = [8, 16, 32, 12, 24];

fn steps_menu(ui: &mut Ui, session: &mut Session) {
    let current = session.step();
    let name = STEP_SIZES
        .iter()
        .find(|&&n| Ticks::note_value(1, n) == current)
        .map_or_else(
            || text::CUSTOM_LENGTH.to_owned(),
            |&n| text::note_value_name(n),
        );
    let response = dropdown(ui, RichText::new(name), text::STEPS_HINT);
    Popup::menu(&response).show(|ui| {
        for n in STEP_SIZES {
            let step = Ticks::note_value(1, n);
            if ui
                .selectable_label(current == step, text::note_value_name(n))
                .clicked()
            {
                session.set_step(step);
                ui.close();
            }
        }
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! The bars across the top: the transport (play, stop, record, where the
//! song is, tempo, time signature, metronome, loop, undo) and the location
//! line under it (where you are, the way around, and the settings of what
//! is on screen).

use daw_engine::RecordState;
use daw_model::{
    ChordKind, ChordShape, Scale, ScaleKind, Swing, TEMPO_RANGE, Ticks, TimeSignature,
};
use egui::{
    Align, Align2, Button, CornerRadius, DragValue, FontId, Layout, Popup, PopupCloseBehavior,
    RichText, Sense, Slider, Ui, vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::keys::{Command, Keymap};
use crate::session::{Draw, Grid, NOTE_LENGTHS, Session, View};
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
        key_button(ui, session);
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
    } else if session.audio_track_in_focus() && session.view() == View::Song {
        (text::RECORD_SOUND, Look::Normal)
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
    // A drag changes the part of the song it started in, even if the play
    // position moves on into another (decision 0018).
    let target = response.id.with("tempo-target");
    if response.drag_started() {
        let at = session.tempo_change_at_position();
        ui.data_mut(|d| d.insert_temp(target, at));
        session.document_mut().begin_gesture(text::EDIT_TEMPO);
    }
    if response.changed() {
        match ui.data(|d| d.get_temp::<Ticks>(target)) {
            Some(at) if response.dragged() => session.set_tempo_at(at, bpm),
            _ => session.set_tempo(bpm),
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|d| d.remove::<Ticks>(target));
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
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            signature_choices(ui, current, true, |signature| {
                session.set_time_signature(signature);
            });
        });
}

/// The common time signatures, then any other: beats and note value. Each
/// choice calls `choose` (the transport, and the signature row's panel).
/// `current` is shown as chosen when `highlight` is set (the signature
/// row's panel does not, when its bar keeps the signature before it).
pub fn signature_choices(
    ui: &mut Ui,
    current: TimeSignature,
    highlight: bool,
    mut choose: impl FnMut(TimeSignature),
) {
    ui.set_min_width(150.0);
    for (numerator, denominator) in SIGNATURES {
        let selected =
            highlight && current.numerator == numerator && current.denominator == denominator;
        if ui
            .selectable_label(selected, text::time_signature(numerator, denominator))
            .clicked()
            && let Some(signature) = TimeSignature::new(numerator, denominator)
        {
            choose(signature);
            ui.close();
        }
    }
    ui.separator();
    ui.label(RichText::new(text::OTHER_SIGNATURE).weak());
    ui.horizontal(|ui| {
        let mut beats = current.numerator;
        let changed = ui
            .add(DragValue::new(&mut beats).range(1..=64).speed(0.1))
            .on_hover_text(text::BEATS)
            .changed();
        if changed && let Some(signature) = TimeSignature::new(beats, current.denominator) {
            choose(signature);
        }
        ui.label("/");
        for denominator in [2_u8, 4, 8, 16, 32] {
            if ui
                .selectable_label(
                    highlight && current.denominator == denominator,
                    denominator.to_string(),
                )
                .on_hover_text(text::NOTE_VALUE)
                .clicked()
                && let Some(signature) = TimeSignature::new(current.numerator, denominator)
            {
                choose(signature);
            }
        }
    });
}

/// The song's key at the play position (decision 0018).
fn key_button(ui: &mut Ui, session: &mut Session) {
    let current = session.key();
    let mut label = RichText::new(text::song_key_name(current)).size(size::BODY);
    if current.is_none() {
        label = label.weak();
    }
    let response = ui
        .add(Button::new(label).min_size(vec2(0.0, BIG - 6.0)))
        .on_hover_text(text::KEY_HINT);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            key_choices(ui, current, true, |scale| session.set_key(scale));
        });
}

/// No key, the twelve tonics and the scales. Each choice calls `choose`
/// (the transport, and the signature row's panel). `current` is shown as
/// chosen when `highlight` is set.
pub fn key_choices(
    ui: &mut Ui,
    current: Option<Scale>,
    highlight: bool,
    mut choose: impl FnMut(Option<Scale>),
) {
    ui.set_min_width(260.0);
    if ui
        .selectable_label(highlight && current.is_none(), text::NO_KEY)
        .clicked()
    {
        choose(None);
        ui.close();
    }
    ui.separator();
    let kind = current.map_or(ScaleKind::Major, |s| s.kind);
    ui.label(RichText::new(text::TONIC).weak());
    for row in [0_u8..6, 6..12] {
        ui.horizontal(|ui| {
            for tonic in row {
                let selected = highlight && current.is_some_and(|s| s.tonic == tonic);
                let name = text::tonic_name(tonic, kind);
                if ui
                    .add(Button::selectable(selected, name).min_size(vec2(34.0, 26.0)))
                    .clicked()
                {
                    choose(Some(Scale::new(tonic, kind)));
                }
            }
        });
    }
    ui.add_space(4.0);
    ui.label(RichText::new(text::SCALE).weak());
    let tonic = current.map_or(0, |s| s.tonic);
    egui::Grid::new("scale-kinds")
        .num_columns(2)
        .show(ui, |ui| {
            for (index, kind) in ScaleKind::ALL.into_iter().enumerate() {
                let selected = highlight && current.is_some_and(|s| s.kind == kind);
                if ui
                    .selectable_label(selected, text::scale_kind_name(kind))
                    .clicked()
                {
                    choose(Some(Scale::new(tonic, kind)));
                }
                if index % 2 == 1 {
                    ui.end_row();
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
    keymap: &Keymap,
    bottom: (bool, bool, bool),
    actions: &mut Vec<Command>,
    intents: &mut Vec<Intent>,
) {
    let (keyboard_shown, mixer_shown, library_shown) = bottom;
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
            let library_label = if library_shown {
                text::HIDE_LIBRARY
            } else {
                text::SHOW_LIBRARY
            };
            if icon_button(
                ui,
                Icon::Folder,
                library_label,
                "",
                Look::on(library_shown),
                30.0,
            )
            .clicked()
            {
                actions.push(Command::ToggleLibrary);
            }
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
            ui.add_space(12.0);
            if session.view() == View::Song {
                song_tools(ui, session);
                return;
            }
            pattern_tools(ui, session, keymap, intents);
        });
    });
}

/// The settings of the song view: whether clips land on bars and beats
/// (decision 0035).
fn song_tools(ui: &mut Ui, session: &mut Session) {
    let mut snap = session.snap_clips();
    if ui
        .checkbox(&mut snap, text::SNAP_CLIPS)
        .on_hover_text(text::SNAP_CLIPS_HINT)
        .changed()
    {
        session.set_snap_clips(snap);
    }
}

/// The settings of the open pattern, laid out from the right (decision
/// 0018 adds swing, chord stamps and the notes menu; decision 0028 the
/// switch between the notes and the sung pitch on voice tracks, in the
/// place of chord stamps, which one voice cannot sing).
fn pattern_tools(ui: &mut Ui, session: &mut Session, keymap: &Keymap, intents: &mut Vec<Intent>) {
    // Laid out from the right.
    if session.is_drum_track() {
        pattern_length(ui, session);
        ui.add_space(12.0);
        swing_menu(ui, session);
        ui.add_space(12.0);
        steps_menu(ui, session);
        ui.label(RichText::new(text::STEPS).weak())
            .on_hover_text(text::STEPS_HINT);
        ui.add_space(12.0);
        if session.is_custom_kit(session.track_id()) {
            // A custom kit takes more sounds (decision 0025).
            if ui
                .button(text::ADD_SOUNDS)
                .on_hover_text(text::ADD_SOUNDS_HINT)
                .clicked()
            {
                intents.push(Intent::AddKitSounds(session.track_id()));
            }
        } else {
            kit_menu(ui, session);
            ui.label(RichText::new(text::KIT).weak())
                .on_hover_text(text::KIT_HINT);
        }
    } else {
        notes_menu(ui, session);
        ui.add_space(12.0);
        note_length_menu(ui, session);
        ui.label(RichText::new(text::NOTE_LENGTH).weak());
        ui.add_space(12.0);
        grid_menu(ui, session);
        ui.label(RichText::new(text::SNAP).weak())
            .on_hover_text(text::SNAP_HINT);
        ui.add_space(12.0);
        if session.is_voice_track() {
            pitch_switch(ui, session, keymap);
        } else {
            draw_menu(ui, session);
            ui.label(RichText::new(text::DRAW).weak())
                .on_hover_text(text::DRAW_HINT);
        }
        ui.add_space(12.0);
        swing_menu(ui, session);
        ui.add_space(12.0);
        pattern_length(ui, session);
    }
}

/// For a plugin track, the button that opens the plugin's own window; for
/// a synth track, the synth's; for an instrument that plays recorded
/// sounds, its note shape.
fn plugin_button(ui: &mut Ui, session: &mut Session) {
    let track = session.track_id();
    // An instrument that plays recorded sounds: its note shape (decision
    // 0031).
    if session.track_shape(track).is_some() {
        crate::shape_ui::shape_button(ui, session, track, 26.0);
        return;
    }
    if session.synth_settings(track).is_some() {
        crate::synth_ui::window_button(ui, session, track, 26.0);
        return;
    }
    let Some(target) = session.track_plugin(track) else {
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
    let soundfont = session.soundfont_file(session.track_id()).is_some();
    let hint = if soundfont {
        text::OPEN_TRACK_OR_PRESET_HINT
    } else {
        text::OPEN_TRACK_HINT
    };
    let response = dropdown(ui, name, hint);
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
        if soundfont {
            soundfont_presets(ui, session);
        }
    });
}

/// In the track menu of a track playing a SoundFont the person chose: its
/// presets, the one playing ticked (decision 0023).
fn soundfont_presets(ui: &mut Ui, session: &mut Session) {
    let track = session.track_id();
    let presets = session.soundfont_presets(track);
    let current = session.soundfont_preset(track);
    ui.separator();
    ui.label(RichText::new(text::PRESET).weak())
        .on_hover_text(text::PRESET_HINT);
    egui::ScrollArea::vertical()
        .id_salt("soundfont-presets")
        .max_height(280.0)
        .show(ui, |ui| {
            for preset in &presets {
                let chosen = current == Some((preset.bank, preset.program));
                let label = format!(
                    "{}  {}",
                    text::preset_number(preset.bank, preset.program),
                    preset.name
                );
                if ui.selectable_label(chosen, label).clicked() {
                    session.set_soundfont_preset(track, preset.bank, preset.program);
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
const STEP_SIZES: [i64; 7] = [8, 16, 32, 12, 24, 20, 28];

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
    let mut name = text::grid_name(session.grid);
    if session.keeps_in_key() {
        name = format!("{name}  ·  {}", text::IN_KEY);
    }
    let response = ui
        .add(Button::new(name).min_size(vec2(56.0, 28.0)))
        .on_hover_text(text::SNAP_HINT);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let groups: [(&str, &[Grid]); 4] = [
                (text::STRAIGHT_GRIDS, &Grid::CHOICES[0..6]),
                (text::TRIPLETS, &Grid::CHOICES[6..9]),
                (text::QUINTUPLETS, &Grid::CHOICES[9..11]),
                (text::SEPTUPLETS, &Grid::CHOICES[11..13]),
            ];
            ui.horizontal_top(|ui| {
                for (title, grids) in groups {
                    ui.vertical(|ui| {
                        ui.set_min_width(118.0);
                        ui.label(RichText::new(title).small().weak());
                        for &grid in grids {
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
            });
            if ui
                .selectable_label(session.grid == Grid::Off, text::grid_name(Grid::Off))
                .clicked()
            {
                session.grid = Grid::Off;
                ui.close();
            }
            ui.separator();
            let has_key = session.pattern_key().is_some();
            let hint = if has_key {
                text::KEEP_IN_KEY_HINT
            } else {
                text::KEEP_IN_KEY_NEEDS_KEY
            };
            ui.add_enabled(
                has_key,
                egui::Checkbox::new(&mut session.keep_in_key, text::KEEP_IN_KEY),
            )
            .on_hover_text(hint)
            .on_disabled_hover_text(hint);
        });
}

/// What the piano roll of a voice track edits: the notes, or the pitch the
/// voice sings (decision 0028). Laid out from the right.
fn pitch_switch(ui: &mut Ui, session: &mut Session, keymap: &Keymap) {
    let shortcut = keymap.text(Command::PitchMode);
    let pitch = session.pitch_mode();
    for (on, name, label, hint) in [
        (
            true,
            text::PITCH_MODE,
            text::EDIT_THE_PITCH,
            text::PITCH_MODE_HINT,
        ),
        (
            false,
            text::NOTES_MODE,
            text::EDIT_THE_NOTES,
            text::NOTES_MODE_HINT,
        ),
    ] {
        let chosen = pitch == on;
        let button = ui
            .add(Button::selectable(chosen, name).min_size(vec2(52.0, 28.0)))
            .on_hover_text(with_hint(hint, &shortcut));
        button.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, chosen, label));
        if button.clicked() {
            session.set_pitch_mode(on);
        }
    }
}

/// What a click places: a note or a chord (decision 0018).
fn draw_menu(ui: &mut Ui, session: &mut Session) {
    let response = ui
        .add(
            Button::new(text::draw_button(session.draw, session.chord_inversion))
                .min_size(vec2(56.0, 28.0)),
        )
        .on_hover_text(text::DRAW_HINT);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(320.0);
            if ui
                .selectable_label(session.draw == Draw::Note, text::DRAW_NOTES)
                .clicked()
            {
                session.draw = Draw::Note;
                ui.close();
            }
            ui.separator();
            let has_key = session.pattern_key().is_some();
            for (size, name) in [(3, text::TRIADS_IN_KEY), (4, text::SEVENTHS_IN_KEY)] {
                let draw = Draw::Chord(ChordShape::InKey(size));
                let response = ui
                    .add_enabled(has_key, Button::selectable(session.draw == draw, name))
                    .on_disabled_hover_text(text::IN_KEY_NEEDS_KEY);
                if response.clicked() {
                    session.draw = draw;
                    ui.close();
                }
            }
            ui.add_space(4.0);
            ui.label(RichText::new(text::CHORD_KINDS).small().weak());
            egui::Grid::new("chord-kinds")
                .num_columns(2)
                .spacing(vec2(24.0, 2.0))
                .show(ui, |ui| {
                    for (index, kind) in ChordKind::ALL.into_iter().enumerate() {
                        let draw = Draw::Chord(ChordShape::Kind(kind));
                        if ui
                            .selectable_label(session.draw == draw, text::chord_kind_name(kind))
                            .clicked()
                        {
                            session.draw = draw;
                            ui.close();
                        }
                        if index % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            ui.separator();
            ui.label(RichText::new(text::INVERSION).small().weak());
            ui.horizontal(|ui| {
                for inversion in 0..=3 {
                    if ui
                        .selectable_label(
                            session.chord_inversion == inversion,
                            text::inversion_name(inversion),
                        )
                        .clicked()
                    {
                        session.chord_inversion = inversion;
                    }
                }
            });
        });
}

fn note_length_menu(ui: &mut Ui, session: &mut Session) {
    let current = NOTE_LENGTHS
        .iter()
        .find(|&&(n, d)| Ticks::note_value(n, d) == session.note_length)
        .map_or_else(
            || text::CUSTOM_LENGTH.to_owned(),
            |&(n, d)| text::length_name(n, d),
        );
    let response = ui
        .add(Button::new(current).min_size(vec2(56.0, 28.0)))
        .on_hover_text(text::NOTE_LENGTH_HINT);
    Popup::menu(&response).show(|ui| {
        let groups: [(&str, &[(i64, i64)]); 3] = [
            (text::STRAIGHT_GRIDS, &NOTE_LENGTHS[0..6]),
            (text::DOTTED, &NOTE_LENGTHS[6..10]),
            (text::TRIPLETS, &NOTE_LENGTHS[10..15]),
        ];
        ui.horizontal_top(|ui| {
            for (title, lengths) in groups {
                ui.vertical(|ui| {
                    ui.set_min_width(118.0);
                    ui.label(RichText::new(title).small().weak());
                    for &(n, d) in lengths {
                        let length = Ticks::note_value(n, d);
                        if ui
                            .selectable_label(
                                session.note_length == length,
                                text::length_name(n, d),
                            )
                            .clicked()
                        {
                            session.note_length = length;
                            ui.close();
                        }
                    }
                });
            }
        });
    });
}

/// The pattern's swing (decision 0018): an amount, and the note value
/// that swings.
fn swing_menu(ui: &mut Ui, session: &mut Session) {
    let swing = session.swing();
    let response = ui
        .add(Button::new(text::swing_name(swing.percent)).min_size(vec2(56.0, 28.0)))
        .on_hover_text(text::SWING_HINT);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(240.0);
            let mut percent = swing.percent;
            let slider = ui.add(
                Slider::new(&mut percent, 0..=Swing::MAX_PERCENT)
                    .suffix("%")
                    .text(text::SWING_AMOUNT),
            );
            if slider.drag_started() {
                session.begin_gesture(text::EDIT_SWING);
            }
            if slider.changed() {
                session.set_swing(Swing { percent, ..swing });
            }
            if slider.drag_stopped() {
                session.end_gesture();
            }
            ui.horizontal(|ui| {
                for (name, value) in [(text::STRAIGHT, 0), (text::TRIPLET_FEEL, 67)] {
                    if ui.selectable_label(swing.percent == value, name).clicked() {
                        session.set_swing(Swing {
                            percent: value,
                            ..swing
                        });
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                for (name, unit) in [
                    (text::SWING_EIGHTHS, Ticks::note_value(1, 8)),
                    (text::SWING_SIXTEENTHS, Ticks::note_value(1, 16)),
                ] {
                    if ui.selectable_label(swing.unit == unit, name).clicked() {
                        session.set_swing(Swing { unit, ..swing });
                    }
                }
            });
        });
}

/// Transposing and inverting the selected notes (decision 0018).
fn notes_menu(ui: &mut Ui, session: &mut Session) {
    let response = dropdown(ui, RichText::new(text::NOTES_MENU), text::NOTES_MENU_HINT);
    Popup::menu(&response).show(|ui| {
        ui.set_min_width(220.0);
        let selected = !session.selection().is_empty();
        if !selected {
            ui.label(RichText::new(text::SELECT_NOTES_FIRST).weak());
        }
        ui.add_enabled_ui(selected, |ui| {
            for up in [true, false] {
                let title = if up {
                    text::TRANSPOSE_UP
                } else {
                    text::TRANSPOSE_DOWN
                };
                ui.menu_button(title, |ui| {
                    for semitones in daw_model::INTERVALS {
                        let name = text::by_interval(up, text::interval_name(semitones));
                        if ui.button(name).clicked() {
                            let semitones = i32::from(semitones);
                            session.transpose_by(if up { semitones } else { -semitones });
                            ui.close();
                        }
                    }
                });
            }
            if session.pattern_key().is_some() {
                ui.menu_button(text::TRANSPOSE_IN_KEY, |ui| {
                    for up in [true, false] {
                        for steps in 1..=7_u8 {
                            let name = text::by_interval(up, text::steps_name(steps));
                            if ui.button(name).clicked() {
                                let steps = i32::from(steps);
                                session.transpose_in_key(if up { steps } else { -steps });
                                ui.close();
                            }
                        }
                        if up {
                            ui.separator();
                        }
                    }
                });
            }
            ui.separator();
            if ui.button(text::INVERT_UP).clicked() {
                session.invert_selection(true);
                ui.close();
            }
            if ui.button(text::INVERT_DOWN).clicked() {
                session.invert_selection(false);
                ui.close();
            }
            if session.is_voice_track() {
                // How the selected notes are sung (decision 0028).
                ui.separator();
                ui.label(RichText::new(text::SINGING).small().weak());
                for (name, hint, run) in [
                    (
                        text::ADD_VIBRATO,
                        text::ADD_VIBRATO_HINT,
                        Session::add_vibrato_to_selection as fn(&mut Session),
                    ),
                    (
                        text::REMOVE_VIBRATO,
                        "",
                        Session::remove_vibrato_from_selection,
                    ),
                    (
                        text::PLAIN_PITCH,
                        text::PLAIN_PITCH_HINT,
                        Session::plain_pitch_for_selection,
                    ),
                ] {
                    let button = ui.button(name);
                    let button = if hint.is_empty() {
                        button
                    } else {
                        button.on_hover_text(hint)
                    };
                    if button.clicked() {
                        run(session);
                        ui.close();
                    }
                }
            }
        });
    });
}

/// The pattern's length: whole bars with the buttons; a length that is not
/// whole bars (dragged at the end of the pattern) shows in steps or beats.
fn pattern_length(ui: &mut Ui, session: &mut Session) {
    let length = session.pattern().length;
    let meter = session.pattern_meter();
    let end = meter.position(length);
    let whole = end.beat == 0 && end.ticks == Ticks::ZERO;
    let bars = session.pattern_bars();
    if icon_button(ui, Icon::Plus, text::LONGER, "", Look::Normal, 26.0).clicked() {
        session.set_pattern_bars(if whole { bars + 1 } else { bars });
    }
    let shown = if whole {
        text::bars(bars)
    } else {
        let step = session.step();
        let beat = meter.initial().beat_length();
        if session.is_drum_track() && length.0 % step.0 == 0 {
            text::steps(length.0 / step.0)
        } else if length.0 % beat.0 == 0 {
            text::beats(length.0 / beat.0)
        } else {
            text::CUSTOM_LENGTH.to_owned()
        }
    };
    ui.label(RichText::new(shown).monospace())
        .on_hover_text(text::PATTERN_LENGTH_HINT);
    let fewer = if whole {
        bars - 1
    } else {
        bars.saturating_sub(1)
    };
    let shorter = ui
        .add_enabled_ui(fewer >= 1, |ui| {
            icon_button(ui, Icon::Minus, text::SHORTER, "", Look::Normal, 26.0)
        })
        .inner;
    if shorter.clicked() {
        session.set_pattern_bars(fewer);
    }
    // (No "Length" label: "− 4 bars +" says it, and the line has to fit a
    // laptop's width with the pattern tools of phase 5.)
}

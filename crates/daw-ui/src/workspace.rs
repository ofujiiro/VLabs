// SPDX-License-Identifier: GPL-3.0-or-later

//! The main window (decision 0011): the transport bar on top, the location
//! line, the piano roll as the one large focus surface, and the play
//! surface at the bottom. Routes the keyboard: note keys to the session,
//! shortcuts to commands. Shows the menus and dialogs.

use std::path::PathBuf;
use std::time::Duration;

use daw_model::{Note, Project};
use egui::{Button, Event, Key, Modal, Panel, RichText, ThemePreference, Ui, ViewportCommand};

use crate::keyboard_panel::{self, KeyboardPanel};
use crate::keys::{BindError, Command, KeyLabels, Keymap, LayoutFamily, Shortcut, note_offset};
use crate::piano_roll::PianoRoll;
use crate::session::Session;
use crate::settings::{LabelChoice, Settings, ThemeChoice};
use crate::text;
use crate::theme::{self, Palette};
use crate::transport;

/// What the window needs from the program around it.
pub trait Host {
    /// Asks the person which song to open (a file dialog).
    fn choose_file_to_open(&mut self) -> Option<PathBuf>;
    /// Asks the person where to save the song.
    fn choose_file_to_save(&mut self, suggested_name: &str) -> Option<PathBuf>;
}

/// Something the window asks the program to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The person declined to recover the work found after a crash: it can
    /// be deleted.
    DiscardRecovery,
    /// The person continued with the work found after a crash: it is the
    /// open song now (with unsaved changes).
    RecoveryAccepted,
}

/// What to do once unsaved changes have been dealt with.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pending {
    New,
    Open(Option<PathBuf>),
    Quit,
}

/// The dialog on screen, if any.
enum Dialog {
    Unsaved(Pending),
    Recovery {
        project: Box<Project>,
        path: Option<PathBuf>,
    },
    About,
    Shortcuts {
        capturing: Option<Command>,
        message: Option<String>,
    },
}

/// The whole window.
pub struct Workspace {
    session: Session,
    settings: Settings,
    keymap: Keymap,
    labels: KeyLabels,
    roll: PianoRoll,
    keyboard: KeyboardPanel,
    host: Box<dyn Host>,
    dialog: Option<Dialog>,
    allow_close: bool,
    title: String,
    requests: Vec<Request>,
    notice: Option<String>,
    applied_theme: Option<ThemeChoice>,
}

impl Workspace {
    /// Sets up the window: fonts, themes, shortcuts and preferences.
    /// `layout` is the keyboard layout family the system reports (for the
    /// key labels) and `known_labels` any labels read from the system.
    pub fn new(
        ctx: &egui::Context,
        mut session: Session,
        settings: Settings,
        host: Box<dyn Host>,
        labels: KeyLabels,
    ) -> Self {
        theme::install(ctx);
        session.apply_settings(&settings);
        let keymap = Keymap::from_settings(&settings.shortcuts);
        let labels = match settings.key_labels {
            LabelChoice::Automatic => labels,
            LabelChoice::Fixed(family) => KeyLabels::new(family),
        };
        Self {
            session,
            settings,
            keymap,
            labels,
            roll: PianoRoll::default(),
            keyboard: KeyboardPanel::default(),
            host,
            dialog: None,
            allow_close: false,
            title: String::new(),
            requests: Vec::new(),
            notice: None,
            applied_theme: None,
        }
    }

    /// The session, for the program around the window (autosave, devices).
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The session, to change it.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// The preferences as they are now, to be saved.
    pub fn settings(&self) -> Settings {
        let mut settings = self.settings.clone();
        self.session.store_settings(&mut settings);
        settings.shortcuts = self.keymap.to_settings();
        settings
    }

    /// The shortcut table.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Requests from the window since the last call.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// Shows (or clears) a lasting notice, such as "no sound device".
    pub fn set_notice(&mut self, notice: Option<String>) {
        self.notice = notice;
    }

    /// Offers to continue with work recovered after a crash.
    pub fn offer_recovery(&mut self, project: Project, path: Option<PathBuf>) {
        self.dialog = Some(Dialog::Recovery {
            project: Box::new(project),
            path,
        });
    }

    /// Opens a song (from the command line, for instance).
    pub fn open(&mut self, path: PathBuf) {
        if self.session.open(&path).is_ok() {
            self.settings.remember_file(path);
            self.roll.refit();
        }
    }

    /// Draws the window and handles its input. Call once per frame.
    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        self.session.update();
        self.handle_close(&ctx);
        let mut actions = Vec::new();
        self.handle_keys(&ctx, &mut actions);

        let p = Palette::of(&ctx);
        let bar = egui::Frame::new()
            .fill(p.surface)
            .inner_margin(egui::Margin::symmetric(12, 0));
        Panel::top("transport")
            .exact_size(60.0)
            .frame(bar)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    self.main_menu(ui, &mut actions);
                    ui.add_space(8.0);
                    transport::transport_bar(ui, &mut self.session, &self.keymap, &mut actions);
                });
            });
        let line = egui::Frame::new()
            .fill(p.background)
            .inner_margin(egui::Margin::symmetric(16, 0));
        Panel::top("location")
            .exact_size(44.0)
            .frame(line)
            .show(ui, |ui| {
                transport::location_bar(
                    ui,
                    &mut self.session,
                    self.settings.show_keyboard,
                    &mut actions,
                );
            });
        let bottom = egui::Frame::new()
            .fill(p.surface)
            .inner_margin(egui::Margin::symmetric(12, 0));
        let height = if self.settings.show_keyboard {
            36.0 + keyboard_panel::KEYS_HEIGHT + 8.0
        } else {
            36.0
        };
        Panel::bottom("keyboard")
            .exact_size(height)
            .frame(bottom)
            .show(ui, |ui| {
                ui.allocate_ui(egui::vec2(ui.available_width(), 36.0), |ui| {
                    keyboard_panel::header(
                        ui,
                        &mut self.session,
                        &self.keymap,
                        self.notice.as_deref(),
                        &mut actions,
                    );
                });
                if self.settings.show_keyboard {
                    self.keyboard.keys(ui, &mut self.session, &self.labels);
                }
            });
        if self.session.take_finished() {
            self.roll.reveal_notes();
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.grid))
            .show(ui, |ui| {
                self.roll.ui(ui, &mut self.session);
            });
        self.show_dialog(&ctx);

        for command in actions {
            self.run(&ctx, command);
        }
        self.update_title(&ctx);
        if self.session.is_playing()
            || self.session.is_recording()
            || self.roll.is_dragging()
            || self.session.pointer_note().is_some()
        {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme == Some(self.settings.theme) {
            return;
        }
        self.applied_theme = Some(self.settings.theme);
        ctx.set_theme(match self.settings.theme {
            ThemeChoice::System => ThemePreference::System,
            ThemeChoice::Light => ThemePreference::Light,
            ThemeChoice::Dark => ThemePreference::Dark,
        });
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = text::window_title(&self.session.display_name(), self.session.is_dirty());
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.allow_close {
            return;
        }
        if self.session.is_dirty() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::Unsaved(Pending::Quit));
        } else {
            self.session.release_keys();
        }
    }

    // ----- Keyboard

    #[allow(clippy::too_many_lines, reason = "one place that routes every key")]
    fn handle_keys(&mut self, ctx: &egui::Context, actions: &mut Vec<Command>) {
        let events = ctx.input(|i| i.events.clone());
        let typing = ctx.text_edit_focused();
        let mut consumed = vec![false; events.len()];
        for (index, event) in events.iter().enumerate() {
            match event {
                Event::WindowFocused(false) => self.session.release_keys(),
                Event::Key {
                    key,
                    physical_key,
                    pressed,
                    repeat,
                    modifiers,
                } => {
                    let physical = physical_key.unwrap_or(*key);
                    let plain = !modifiers.command && !modifiers.ctrl && !modifiers.alt;
                    if *pressed && plain {
                        self.labels.learn(physical, *key);
                    }
                    if let Some(Dialog::Shortcuts { capturing, message }) = &mut self.dialog
                        && let Some(command) = *capturing
                    {
                        if *pressed && !*repeat {
                            consumed[index] = true;
                            if *key == Key::Escape {
                                *capturing = None;
                            } else {
                                let shortcut = Shortcut::from_press(*key, *modifiers);
                                match self.keymap.bind(command, shortcut) {
                                    Ok(()) => {
                                        *capturing = None;
                                        *message = None;
                                    }
                                    Err(error) => *message = Some(bind_error_text(error, shortcut)),
                                }
                            }
                        }
                        continue;
                    }
                    if self.dialog.is_some() || typing {
                        if !*pressed {
                            self.session.key_up(physical);
                        }
                        continue;
                    }
                    if !*pressed {
                        self.session.key_up(physical);
                        continue;
                    }
                    if let Some(command) = self.keymap.command_for(*key, *modifiers) {
                        consumed[index] = true;
                        if !*repeat || command.repeats() {
                            actions.push(command);
                        }
                        continue;
                    }
                    if plain && note_offset(physical).is_some() {
                        consumed[index] = true;
                        if !*repeat {
                            self.session.key_down(physical);
                        }
                    }
                }
                Event::Copy if !typing && self.dialog.is_none() => {
                    consumed[index] = true;
                    actions.push(Command::Copy);
                }
                Event::Cut if !typing && self.dialog.is_none() => {
                    consumed[index] = true;
                    actions.push(Command::Cut);
                }
                Event::Paste(text) if !typing && self.dialog.is_none() => {
                    consumed[index] = true;
                    if let Some(notes) = notes_from_text(text) {
                        self.session.set_clipboard(notes);
                    }
                    actions.push(Command::Paste);
                }
                _ => {}
            }
        }
        ctx.input_mut(|i| {
            let mut index = 0;
            i.events.retain(|_| {
                let keep = !consumed.get(index).copied().unwrap_or(false);
                index += 1;
                keep
            });
        });
    }

    /// Runs a command, from a shortcut, a button or the menu.
    pub fn run(&mut self, ctx: &egui::Context, command: Command) {
        let s = &mut self.session;
        match command {
            Command::PlayStop => s.toggle_play(),
            Command::Record => s.toggle_record(),
            Command::ToStart => s.to_start(),
            Command::Loop => s.toggle_loop(),
            Command::Metronome => s.toggle_metronome(),
            Command::Undo => {
                self.roll.cancel_drag(s);
                s.undo();
            }
            Command::Redo => {
                self.roll.cancel_drag(s);
                s.redo();
            }
            Command::New => self.ask_then(ctx, Pending::New),
            Command::Open => self.ask_then(ctx, Pending::Open(None)),
            Command::Save => {
                self.save(false);
            }
            Command::SaveAs => {
                self.save(true);
            }
            Command::SelectAll => s.select_all(),
            Command::Copy => {
                if s.copy() {
                    ctx.copy_text(notes_to_text(s.clipboard()));
                }
            }
            Command::Cut => {
                s.cut();
                if !s.clipboard().is_empty() {
                    ctx.copy_text(notes_to_text(s.clipboard()));
                }
            }
            Command::Paste => s.paste(),
            Command::Duplicate => s.duplicate(),
            Command::Delete => s.delete_selected(),
            Command::Deselect => {
                if !self.roll.cancel_drag(s) {
                    s.deselect();
                }
            }
            Command::MoveEarlier => s.nudge(-1),
            Command::MoveLater => s.nudge(1),
            Command::TransposeUp => s.transpose(1),
            Command::TransposeDown => s.transpose(-1),
            Command::OctaveUp => s.transpose(12),
            Command::OctaveDown => s.transpose(-12),
            Command::KeyboardOctaveUp => s.shift_keyboard_octave(1),
            Command::KeyboardOctaveDown => s.shift_keyboard_octave(-1),
            Command::ZoomIn => self.roll.zoom(1.25),
            Command::ZoomOut => self.roll.zoom(0.8),
            Command::ToggleKeyboard => self.settings.show_keyboard = !self.settings.show_keyboard,
        }
    }

    /// Saves; asks where if the song has no file yet (or `choose` is set).
    /// Returns whether it was saved.
    fn save(&mut self, choose: bool) -> bool {
        if !choose {
            match self.session.save() {
                Ok(true) => {
                    if let Some(path) = self.session.path() {
                        self.settings.remember_file(path.to_path_buf());
                    }
                    return true;
                }
                Ok(false) => {}
                Err(_) => return false,
            }
        }
        let suggested = format!(
            "{}.{}",
            self.session.display_name(),
            daw_ident::PROJECT_FILE_EXTENSION
        );
        let Some(path) = self.host.choose_file_to_save(&suggested) else {
            return false;
        };
        if self.session.save_as(&path).is_ok() {
            if let Some(path) = self.session.path() {
                self.settings.remember_file(path.to_path_buf());
            }
            true
        } else {
            false
        }
    }

    /// Does `then` now, or first asks about unsaved changes.
    fn ask_then(&mut self, ctx: &egui::Context, then: Pending) {
        self.roll.cancel_drag(&mut self.session);
        if self.session.is_dirty() {
            self.dialog = Some(Dialog::Unsaved(then));
        } else {
            self.carry_out(ctx, then);
        }
    }

    fn carry_out(&mut self, ctx: &egui::Context, pending: Pending) {
        match pending {
            Pending::New => {
                self.session.new_song();
                self.roll.refit();
            }
            Pending::Open(path) => {
                let path = path.or_else(|| self.host.choose_file_to_open());
                if let Some(path) = path {
                    self.open(path);
                }
            }
            Pending::Quit => {
                self.session.release_keys();
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    // ----- Menus and dialogs

    fn main_menu(&mut self, ui: &mut Ui, actions: &mut Vec<Command>) {
        let response = crate::icons::icon_button(
            ui,
            crate::icons::Icon::Menu,
            text::MENU,
            "",
            crate::icons::Look::Normal,
            40.0,
        );
        egui::Popup::menu(&response).show(|ui| {
            ui.set_min_width(260.0);
            let item = |ui: &mut Ui, label: &str, command: Command, keymap: &Keymap| {
                ui.add(Button::new(label).shortcut_text(keymap.text(command)))
                    .clicked()
            };
            for (label, command) in [
                (text::NEW, Command::New),
                (text::OPEN, Command::Open),
                (text::SAVE, Command::Save),
                (text::SAVE_AS, Command::SaveAs),
            ] {
                if item(ui, label, command, &self.keymap) {
                    actions.push(command);
                    ui.close();
                }
            }
            if !self.settings.recent_files.is_empty() {
                ui.menu_button(text::OPEN_RECENT, |ui| {
                    for path in self.settings.recent_files.clone() {
                        let name = path.file_name().map_or_else(
                            || path.display().to_string(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        if ui
                            .button(name)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            let ctx = ui.ctx().clone();
                            self.ask_then(&ctx, Pending::Open(Some(path)));
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            ui.menu_button(text::THEME, |ui| {
                for (choice, label) in [
                    (ThemeChoice::System, text::THEME_SYSTEM),
                    (ThemeChoice::Light, text::THEME_LIGHT),
                    (ThemeChoice::Dark, text::THEME_DARK),
                ] {
                    if ui
                        .selectable_label(self.settings.theme == choice, label)
                        .clicked()
                    {
                        self.settings.theme = choice;
                        ui.close();
                    }
                }
            });
            ui.menu_button(text::KEY_LABELS, |ui| {
                ui.label(RichText::new(text::KEY_LABELS_HINT).small().weak());
                let choices = [
                    (LabelChoice::Automatic, text::KEY_LABELS_AUTOMATIC),
                    (LabelChoice::Fixed(LayoutFamily::Qwerty), "QWERTY"),
                    (LabelChoice::Fixed(LayoutFamily::Azerty), "AZERTY"),
                    (LabelChoice::Fixed(LayoutFamily::Qwertz), "QWERTZ"),
                ];
                for (choice, label) in choices {
                    if ui
                        .selectable_label(self.settings.key_labels == choice, label)
                        .clicked()
                    {
                        self.settings.key_labels = choice;
                        if let LabelChoice::Fixed(family) = choice {
                            self.labels.set_family(family);
                        }
                        ui.close();
                    }
                }
            });
            if ui.button(text::SHORTCUTS).clicked() {
                self.dialog = Some(Dialog::Shortcuts {
                    capturing: None,
                    message: None,
                });
                ui.close();
            }
            if ui.button(text::ABOUT).clicked() {
                self.dialog = Some(Dialog::About);
                ui.close();
            }
        });
    }

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let mut keep = true;
        let mut then: Option<Pending> = None;
        let dialog = match dialog {
            Dialog::Unsaved(pending) => {
                let mut choice = None;
                modal("unsaved", ctx).show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.heading(text::UNSAVED_QUESTION);
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(text::SAVE_CHANGES).clicked() {
                            choice = Some(1);
                        }
                        if ui.button(text::DISCARD_CHANGES).clicked() {
                            choice = Some(2);
                        }
                        if ui.button(text::CANCEL).clicked() {
                            choice = Some(0);
                        }
                    });
                });
                match choice {
                    Some(1) => {
                        keep = false;
                        if self.save(false) {
                            then = Some(pending.clone());
                        }
                    }
                    Some(2) => {
                        keep = false;
                        then = Some(pending.clone());
                    }
                    Some(_) => keep = false,
                    None => {}
                }
                Dialog::Unsaved(pending)
            }
            Dialog::Recovery { project, path } => {
                let mut choice = None;
                modal("recovery", ctx).show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.heading(text::RECOVER_TITLE);
                    ui.add_space(8.0);
                    ui.label(text::recover_question());
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(text::RECOVER).clicked() {
                            choice = Some(true);
                        }
                        if ui.button(text::DISCARD_RECOVERY).clicked() {
                            choice = Some(false);
                        }
                    });
                });
                match choice {
                    Some(true) => {
                        keep = false;
                        self.session
                            .replace((*project).clone(), path.clone(), false);
                        self.roll.refit();
                        self.requests.push(Request::RecoveryAccepted);
                    }
                    Some(false) => {
                        keep = false;
                        self.requests.push(Request::DiscardRecovery);
                    }
                    None => {}
                }
                Dialog::Recovery { project, path }
            }
            Dialog::About => {
                let response = modal("about", ctx).show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.heading(daw_ident::PRODUCT_NAME);
                    ui.add_space(8.0);
                    ui.label(text::about());
                    ui.add_space(12.0);
                    ui.button(text::CLOSE).clicked()
                });
                if response.inner || response.should_close() {
                    keep = false;
                }
                Dialog::About
            }
            Dialog::Shortcuts { capturing, message } => {
                let (capturing, message, close) = self.shortcuts_window(ctx, capturing, message);
                keep = !close;
                Dialog::Shortcuts { capturing, message }
            }
        };
        if keep {
            self.dialog = Some(dialog);
        }
        if let Some(pending) = then {
            self.carry_out(ctx, pending);
        }
    }

    fn shortcuts_window(
        &mut self,
        ctx: &egui::Context,
        mut capturing: Option<Command>,
        mut message: Option<String>,
    ) -> (Option<Command>, Option<String>, bool) {
        let mut close = false;
        let response = modal("shortcuts", ctx).show(ctx, |ui| {
            ui.set_width(680.0);
            ui.heading(text::SHORTCUTS);
            ui.label(RichText::new(text::SHORTCUTS_HINT).small().weak());
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("shortcut-table")
                        .num_columns(3)
                        .spacing([16.0, 6.0])
                        .min_col_width(150.0)
                        .striped(true)
                        .show(ui, |ui| {
                            for command in Command::ALL {
                                ui.label(text::command_name(command));
                                let shown: Vec<String> = self
                                    .keymap
                                    .shortcuts(command)
                                    .map(Shortcut::display)
                                    .collect();
                                let current = if capturing == Some(command) {
                                    text::PRESS_A_KEY.to_owned()
                                } else if shown.is_empty() {
                                    text::NO_SHORTCUT.to_owned()
                                } else {
                                    shown.join(", ")
                                };
                                ui.label(RichText::new(current).monospace());
                                ui.horizontal(|ui| {
                                    if command.rebindable() {
                                        if ui.small_button(text::CHANGE).clicked() {
                                            capturing = Some(command);
                                            message = None;
                                        }
                                        if ui.small_button(text::RESET).clicked() {
                                            self.keymap.reset(command);
                                        }
                                    } else {
                                        ui.label(
                                            RichText::new(text::FIXED_SHORTCUT).small().weak(),
                                        );
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
            if let Some(message) = &message {
                ui.add_space(6.0);
                ui.label(RichText::new(message).color(Palette::of(ui.ctx()).record));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(text::RESET_ALL).clicked() {
                    self.keymap = Keymap::default();
                    capturing = None;
                    message = None;
                }
                if ui.button(text::CLOSE).clicked() {
                    close = true;
                }
            });
        });
        if response.should_close() && capturing.is_none() {
            close = true;
        }
        (capturing, message, close)
    }
}

/// A dialog with generous margins.
fn modal(name: &str, ctx: &egui::Context) -> Modal {
    let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(20));
    Modal::new(egui::Id::unique(("dialog", name))).frame(frame)
}

fn bind_error_text(error: BindError, shortcut: Shortcut) -> String {
    match error {
        BindError::PlaysNotes => text::shortcut_plays_notes(&shortcut.display()),
        BindError::InUse(other) => {
            text::shortcut_in_use(&shortcut.display(), text::command_name(other))
        }
        BindError::Fixed => text::FIXED_SHORTCUT.to_owned(),
    }
}

/// The clipboard text for notes: a header line naming the program, then
/// the notes as JSON, so notes can be pasted into another window of it.
pub fn notes_to_text(notes: &[Note]) -> String {
    format!(
        "{}\n{}",
        clipboard_header(),
        serde_json::to_string(notes).unwrap_or_default()
    )
}

/// Notes from clipboard text written by [`notes_to_text`].
pub fn notes_from_text(text: &str) -> Option<Vec<Note>> {
    let (header, json) = text.split_once('\n')?;
    if header.trim() != clipboard_header() {
        return None;
    }
    serde_json::from_str(json).ok()
}

fn clipboard_header() -> String {
    format!("{} notes 1", daw_ident::PRODUCT_NAME)
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The main window (decisions 0011 and 0014): the transport bar on top, the
//! location line, one large focus surface (the song, the step grid or the
//! piano roll), and the bottom area (the keyboard or the mixer). Routes the
//! keyboard: note keys to the session, shortcuts to commands. Shows the
//! menus, dialogs and effect windows.

use std::path::PathBuf;
use std::time::Duration;

use daw_engine::PluginKind;
use daw_model::{EffectId, PatternId, Project, TrackId};
use egui::{Button, Event, Key, Modal, Panel, RichText, ThemePreference, Ui, ViewportCommand};

use crate::effect_ui::EffectWindows;
use crate::export::{ExportFormat, ExportTarget};
use crate::keyboard_panel::{self, KeyboardPanel};
use crate::keys::{BindError, Command, KeyLabels, Keymap, LayoutFamily, Shortcut, note_offset};
use crate::library_panel::{self, LibraryAction, LibraryPanel};
use crate::mixer::{MIXER_HEIGHT, MixerPanel, format_name};
use crate::piano_roll::PianoRoll;
use crate::plugin_manager::{PluginJobDone, PluginPlaces, PluginsWindow};
use crate::session::{CopiedNote, Session, View};
use crate::settings::{LabelChoice, Settings, ThemeChoice};
use crate::song_view::SongView;
use crate::step_grid::StepGrid;
use crate::text;
use crate::theme::{self, Palette};
use crate::transport;
use crate::voicebanks::VoicebanksWindow;

/// What the window needs from the program around it.
pub trait Host {
    /// Asks the person which song to open (a file dialog).
    fn choose_file_to_open(&mut self) -> Option<PathBuf>;
    /// Asks the person where to save the song.
    fn choose_file_to_save(&mut self, suggested_name: &str) -> Option<PathBuf>;
    /// Asks the person where to export the song (a sound file, a
    /// DAWproject or a MIDI file, as `target` says).
    fn choose_export_file(&mut self, suggested_name: &str, target: ExportTarget)
    -> Option<PathBuf>;
    /// Asks the person which sound or MIDI files to import (none if they
    /// cancel).
    fn choose_audio_files(&mut self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Shows the user guide (in the browser).
    fn open_user_guide(&mut self) {}
    /// Asks the person which plugin files, downloads or installers to add
    /// (none if they cancel).
    fn choose_plugin_files(&mut self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Asks the person which SoundFont (`.sf2`) file to play.
    fn choose_soundfont(&mut self) -> Option<PathBuf> {
        None
    }
    /// Asks the person which sounds to put in a custom kit (many at once;
    /// none if they cancel).
    fn choose_kit_sounds(&mut self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Asks the person for a folder (a dialog titled `title`).
    fn choose_folder(&mut self, title: &str) -> Option<PathBuf> {
        let _ = title;
        None
    }
    /// Shows a folder in the system's file manager.
    fn open_folder(&mut self, folder: &std::path::Path) {
        let _ = folder;
    }
    /// Asks the person for the `.zip` a voicebank came in.
    fn choose_voicebank_zip(&mut self) -> Option<PathBuf> {
        None
    }
}

/// Something a part of the window asks the window to do (open a dialog or
/// a window), collected during a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    /// Ask for a track's new name.
    RenameTrack(TrackId),
    /// Ask for a pattern's new name.
    RenamePattern(PatternId),
    /// Open an effect's window (a plugin effect's own window).
    OpenEffect(EffectId),
    /// Choose the instrument of a new track.
    ChooseInstrument,
    /// Choose sounds to add to a custom kit (decision 0025).
    AddKitSounds(TrackId),
}

/// The instruments that come with the program, in the chooser.
#[derive(Clone, Copy)]
enum Included {
    Piano,
    Drums,
    Kit,
    Synth,
    SoundFont,
}

/// What a rename dialog renames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Renaming {
    Track(TrackId),
    Pattern(PatternId),
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
    /// Look for plugins again (some may have been installed since).
    FindPlugins,
    /// Add plugins from these files, folders, downloads or installers
    /// (decision 0023).
    AddPlugins(Vec<PathBuf>),
    /// Remove a plugin the program added.
    RemovePlugin(PathBuf),
    /// Look for Wine and yabridge again, and set up the Windows plugins
    /// added here.
    SetUpWindowsPlugins,
}

/// What the person is told about work found after a crash (decision
/// 0021).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecoveryDetails {
    /// How long ago the copy was written, in seconds.
    pub age_seconds: u64,
    /// Whether the song's own file was saved after the copy.
    pub newer_file: bool,
    /// Whether a take being recorded was brought back.
    pub take: bool,
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
        details: RecoveryDetails,
    },
    About,
    Shortcuts {
        capturing: Option<Command>,
        message: Option<String>,
    },
    Rename {
        what: Renaming,
        name: String,
    },
    Export {
        format: ExportFormat,
        target: ExportTarget,
    },
    Exporting,
    Instruments {
        search: String,
    },
    Plugins {
        window: PluginsWindow,
        /// Opened from the instrument chooser, which comes back after.
        from_chooser: bool,
    },
    Voicebanks {
        window: VoicebanksWindow,
        /// Opened from the instrument chooser, which comes back after.
        from_chooser: bool,
    },
    /// The MIDI controllers: which are connected, what their controls do
    /// (decision 0033).
    Midi,
}

/// The whole window.
pub struct Workspace {
    session: Session,
    settings: Settings,
    keymap: Keymap,
    labels: KeyLabels,
    roll: PianoRoll,
    steps: StepGrid,
    song: SongView,
    mixer: MixerPanel,
    library: LibraryPanel,
    effects: EffectWindows,
    keyboard: KeyboardPanel,
    host: Box<dyn Host>,
    dialog: Option<Dialog>,
    allow_close: bool,
    title: String,
    requests: Vec<Request>,
    notice: Option<String>,
    applied_theme: Option<ThemeChoice>,
    /// Where plugins come from, as the program says (decision 0023).
    plugin_places: PluginPlaces,
    /// What the last change to the plugins did, for the Plugins window.
    plugin_messages: Vec<String>,
    /// The program's own voicebanks folder, and what it holds (decision
    /// 0027).
    voicebanks_root: Option<PathBuf>,
    voicebank_list: Vec<daw_formats::utau::VoicebankInfo>,
    /// Where messages from MIDI controllers go, as the MIDI thread asks it
    /// (decision 0033).
    midi_table: std::sync::Arc<std::sync::Mutex<crate::midi::RouteTable>>,
    /// What MIDI controllers sent the window.
    midi_inputs: Option<std::sync::mpsc::Receiver<crate::midi::ControllerInput>>,
    /// The MIDI controllers' buttons and pads that run commands (kept in
    /// the settings).
    midi_buttons: std::collections::BTreeMap<Command, daw_model::MidiControl>,
    /// The command waiting for a button or pad, if one is.
    button_learning: Option<Command>,
    /// The buttons held down (a command runs once per press).
    buttons_down: std::collections::HashSet<daw_model::MidiControl>,
    /// Every MIDI controller found, as the program says, and why MIDI is
    /// unavailable, if it is.
    midi_devices: Vec<String>,
    midi_problem: Option<String>,
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
        let midi_buttons = settings
            .midi_buttons
            .iter()
            .filter_map(|(id, control)| Some((Command::from_id(id)?, *control)))
            .collect();
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
            steps: StepGrid::default(),
            song: SongView::default(),
            mixer: MixerPanel::default(),
            library: LibraryPanel::default(),
            effects: EffectWindows::default(),
            keyboard: KeyboardPanel::default(),
            host,
            dialog: None,
            allow_close: false,
            title: String::new(),
            requests: Vec::new(),
            notice: None,
            applied_theme: None,
            plugin_places: PluginPlaces::default(),
            plugin_messages: Vec::new(),
            voicebanks_root: None,
            voicebank_list: Vec::new(),
            midi_table: std::sync::Arc::default(),
            midi_inputs: None,
            midi_buttons,
            button_learning: None,
            buttons_down: std::collections::HashSet::new(),
            midi_devices: Vec::new(),
            midi_problem: None,
        }
    }

    /// The MIDI controllers found (connected or switched off), and why
    /// MIDI is unavailable if it is, for the MIDI window.
    pub fn set_midi_devices(&mut self, devices: Vec<String>, problem: Option<String>) {
        self.midi_devices = devices;
        self.midi_problem = problem;
    }

    /// Opens the MIDI controllers window (decision 0033).
    pub fn open_midi_window(&mut self) {
        self.dialog = Some(Dialog::Midi);
    }

    /// The command a MIDI button or pad runs, if it runs one.
    fn button_command(&self, control: daw_model::MidiControl) -> Option<Command> {
        self.midi_buttons
            .iter()
            .find(|(_, c)| **c == control)
            .map(|(command, _)| *command)
    }

    /// Gives a MIDI button or pad a command (one command per button: a
    /// command it ran before is let go).
    fn assign_button(&mut self, command: Command, control: daw_model::MidiControl) {
        self.midi_buttons.retain(|_, c| *c != control);
        self.midi_buttons.insert(command, control);
        self.button_learning = None;
        self.session
            .set_status(text::button_learned(control, text::command_name(command)));
    }

    /// Tells the MIDI thread where messages go now (decision 0033): what
    /// the window waits for, and the controls with a job.
    fn update_midi_table(&self) {
        use crate::midi::{Learning, RouteTable};
        let learning = if self.button_learning.is_some() {
            Some(Learning::Button)
        } else if self.session.learning_midi().is_some() {
            Some(Learning::Knob)
        } else {
            None
        };
        let taken = self
            .session
            .midi_links()
            .iter()
            .map(|l| l.control)
            .chain(self.midi_buttons.values().copied())
            .collect();
        let table = RouteTable { learning, taken };
        let mut shared = self
            .midi_table
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *shared != table {
            *shared = table;
        }
    }

    /// The MIDI thread's end (decision 0033): messages from controllers go
    /// through it to the instrument in focus or to this window. Call once;
    /// a second call replaces the first router's way to the window.
    pub fn midi_router(&mut self) -> crate::midi::MidiRouter {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.midi_inputs = Some(receiver);
        crate::midi::MidiRouter::new(
            std::sync::Arc::clone(&self.midi_table),
            sender,
            self.session.engine().midi_sender(),
        )
    }

    /// Takes what MIDI controllers sent the window since the last frame.
    fn follow_midi(&mut self, ctx: &egui::Context) {
        let Some(inputs) = &self.midi_inputs else {
            return;
        };
        let inputs: Vec<crate::midi::ControllerInput> = inputs.try_iter().collect();
        for input in inputs {
            self.midi_input(ctx, input);
        }
    }

    /// One message from a MIDI controller, for the window: a control being
    /// waited for gets its job, a button runs its command, a knob moves its
    /// setting, a key lights up, a transport button plays or stops.
    fn midi_input(&mut self, ctx: &egui::Context, input: crate::midi::ControllerInput) {
        use crate::midi::{ControllerInput, Learning, TransportButton, can_learn};
        let control = input.control();
        if let (Some(command), Some(control)) = (self.button_learning, control)
            && input.is_press()
        {
            self.assign_button(command, control);
            return;
        }
        if let (ControllerInput::Controller { number, .. }, Some(control)) = (input, control)
            && self.session.learning_midi().is_some()
            && can_learn(number, Learning::Knob)
        {
            self.session.midi_learned(control);
            return;
        }
        if let Some(control) = control
            && let Some(command) = self.button_command(control)
        {
            if input.is_press() {
                if self.buttons_down.insert(control) {
                    self.run(ctx, command);
                }
            } else {
                self.buttons_down.remove(&control);
            }
            return;
        }
        if let (ControllerInput::Controller { value, .. }, Some(control)) = (input, control)
            && self.session.midi_moved(control, value)
        {
            return;
        }
        let s = &mut self.session;
        match input {
            ControllerInput::NoteOn { key, velocity, .. } => {
                s.show_outside_note(key, velocity > 0);
            }
            ControllerInput::NoteOff { key, .. } => s.show_outside_note(key, false),
            ControllerInput::Transport(button) => match button {
                TransportButton::Play => {
                    if !s.is_playing() && !s.is_recording() {
                        s.play();
                    }
                }
                TransportButton::Stop => s.stop(),
                TransportButton::Record => s.toggle_record(),
                TransportButton::Rewind => s.to_start(),
            },
            ControllerInput::Controller { .. }
            | ControllerInput::PitchBend { .. }
            | ControllerInput::Pressure { .. } => {}
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
        settings.midi_buttons = self
            .midi_buttons
            .iter()
            .map(|(command, control)| (command.id().to_owned(), *control))
            .collect();
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

    /// Where plugins come from, for the Plugins window.
    pub fn set_plugin_places(&mut self, places: PluginPlaces) {
        self.plugin_places = places;
    }

    /// Tells the person what adding or removing plugins did.
    pub fn plugin_job_done(&mut self, done: &PluginJobDone) {
        self.plugin_messages = text::plugin_job_lines(done);
        if let Some(first) = self.plugin_messages.first() {
            self.session.set_status(first);
        }
    }

    /// Where voicebanks are kept (the program's own folder), and what is
    /// there.
    pub fn set_voicebanks_folder(&mut self, root: Option<PathBuf>) {
        self.voicebank_list = root
            .as_deref()
            .map_or_else(Vec::new, daw_formats::utau::installed);
        self.session.set_voicebanks_folder(root.as_deref());
        self.voicebanks_root = root;
    }

    /// Opens the Voicebanks window.
    pub fn open_voicebanks_window(&mut self) {
        let from_chooser = matches!(self.dialog, Some(Dialog::Instruments { .. }));
        if !matches!(self.dialog, Some(Dialog::Voicebanks { .. })) {
            self.dialog = Some(Dialog::Voicebanks {
                window: VoicebanksWindow::default(),
                from_chooser,
            });
        }
    }

    /// Opens the Plugins window.
    pub fn open_plugins_window(&mut self) {
        let from_chooser = matches!(self.dialog, Some(Dialog::Instruments { .. }));
        if !matches!(self.dialog, Some(Dialog::Plugins { .. })) {
            self.plugin_messages.clear();
            self.dialog = Some(Dialog::Plugins {
                window: PluginsWindow::default(),
                from_chooser,
            });
        }
    }

    /// Shows (or clears) a lasting notice, such as "no sound device".
    pub fn set_notice(&mut self, notice: Option<String>) {
        self.notice = notice;
    }

    /// Offers to continue with work recovered after a crash.
    pub fn offer_recovery(
        &mut self,
        project: Project,
        path: Option<PathBuf>,
        details: RecoveryDetails,
    ) {
        self.dialog = Some(Dialog::Recovery {
            project: Box::new(project),
            path,
            details,
        });
    }

    /// Opens a song (from the command line, for instance): a song of this program,
    /// or another program's (DAWproject, MIDI) as a new unsaved song.
    pub fn open(&mut self, path: PathBuf) {
        if self.session.open_any(&path).is_ok() {
            if crate::session::SongFile::of(&path) != Some(crate::session::SongFile::Midi)
                && crate::session::SongFile::of(&path) != Some(crate::session::SongFile::DawProject)
            {
                self.settings.remember_file(path);
            }
            self.refit_views();
        }
    }

    /// Fits every view to the song that has just opened.
    fn refit_views(&mut self) {
        self.roll.refit();
        self.song.refit();
        self.effects.close_all();
    }

    /// Whether the person is dragging something in the view on screen.
    fn is_dragging(&self) -> bool {
        self.roll.is_dragging() || self.steps.is_dragging() || self.song.is_dragging()
    }

    /// Abandons a drag in the view on screen. Returns whether there was one.
    fn cancel_drag(&mut self) -> bool {
        let s = &mut self.session;
        self.roll.cancel_drag(s) | self.steps.cancel_drag(s) | self.song.cancel_drag(s)
    }

    /// Draws the window and handles its input. Call once per frame.
    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        self.follow_midi(&ctx);
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
        let mut intents = Vec::new();
        let mut library_actions = Vec::new();
        let library_shown = self.settings.show_library && !self.settings.show_mixer;
        let keyboard_shown =
            self.settings.show_keyboard && !self.settings.show_mixer && !library_shown;
        Panel::top("location")
            .exact_size(44.0)
            .frame(line)
            .show(ui, |ui| {
                transport::location_bar(
                    ui,
                    &mut self.session,
                    &self.keymap,
                    (keyboard_shown, self.settings.show_mixer, library_shown),
                    &mut actions,
                    &mut intents,
                );
            });
        self.bottom_panel(
            ui,
            (keyboard_shown, library_shown),
            &mut actions,
            &mut intents,
            &mut library_actions,
        );
        if self.session.take_finished() {
            self.roll.reveal_notes();
        }
        self.take_dropped_files(&ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.grid))
            .show(ui, |ui| match self.session.view() {
                View::Song => self.song.ui(ui, &mut self.session, &mut intents),
                View::Pattern if self.session.is_drum_track() => {
                    self.steps.ui(ui, &mut self.session, &self.labels);
                }
                View::Pattern => self.roll.ui(ui, &mut self.session),
            });
        self.effects.ui(&ctx, &mut self.session);
        crate::synth_ui::show_windows(&ctx, &mut self.session);
        crate::sound_editor::show(&ctx, &mut self.session);
        self.show_dialog(&ctx);

        for intent in intents {
            self.handle_intent(intent);
        }
        for LibraryAction::Add(path) in library_actions {
            self.import(vec![path]);
        }
        for command in actions {
            self.run(&ctx, command);
        }
        self.update_midi_table();
        self.update_title(&ctx);
        self.schedule_repaint(&ctx);
    }

    /// The panel at the bottom: its header, then the mixer, the library or
    /// the keyboard (or nothing).
    fn bottom_panel(
        &mut self,
        ui: &mut Ui,
        (keyboard_shown, library_shown): (bool, bool),
        actions: &mut Vec<Command>,
        intents: &mut Vec<Intent>,
        library_actions: &mut Vec<LibraryAction>,
    ) {
        let p = Palette::of(ui.ctx());
        let bottom = egui::Frame::new()
            .fill(p.surface)
            .inner_margin(egui::Margin::symmetric(12, 0));
        let height = if self.settings.show_mixer {
            36.0 + MIXER_HEIGHT
        } else if library_shown {
            36.0 + library_panel::LIBRARY_HEIGHT
        } else if keyboard_shown {
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
                        actions,
                    );
                });
                if self.settings.show_mixer {
                    self.mixer.ui(ui, &mut self.session, intents);
                } else if library_shown {
                    self.library.ui(
                        ui,
                        &mut self.session,
                        &mut self.settings,
                        self.host.as_mut(),
                        library_actions,
                    );
                } else if keyboard_shown {
                    self.keyboard.keys(ui, &mut self.session, &self.labels);
                }
            });
    }

    /// Files dropped on the window: a song file opens (after asking about
    /// unsaved changes); sound and MIDI files dropped while a pattern is
    /// open bring the song on screen with them (the song view takes them
    /// itself otherwise).
    fn take_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        // Plugins, downloads and installers go to the Plugins window.
        let plugins: Vec<PathBuf> = dropped
            .iter()
            .filter(|p| crate::plugin_manager::is_for_plugins(p))
            .cloned()
            .collect();
        if !plugins.is_empty() {
            ctx.input_mut(|i| {
                i.raw
                    .dropped_files
                    .retain(|f| !plugins.iter().any(|p| p.as_path() == f.path()));
            });
            if matches!(self.dialog, None | Some(Dialog::Instruments { .. })) {
                self.open_plugins_window();
            }
            self.plugin_messages.clear();
            self.requests.push(Request::AddPlugins(plugins.clone()));
        }
        let dropped: Vec<PathBuf> = dropped
            .into_iter()
            .filter(|p| !plugins.contains(p))
            .collect();
        // Other folders join the sound library (decision 0023).
        let folders: Vec<PathBuf> = dropped.iter().filter(|p| p.is_dir()).cloned().collect();
        if !folders.is_empty() {
            ctx.input_mut(|i| {
                i.raw
                    .dropped_files
                    .retain(|f| !folders.iter().any(|p| p.as_path() == f.path()));
            });
            for folder in &folders {
                library_panel::add_folder(&mut self.settings, folder);
            }
            self.settings.show_library = true;
            self.settings.show_mixer = false;
        }
        let dropped: Vec<PathBuf> = dropped
            .into_iter()
            .filter(|p| !folders.contains(p))
            .collect();
        if dropped.is_empty() {
            return;
        }
        let song = dropped.iter().find(|p| {
            matches!(
                crate::session::SongFile::of(p),
                Some(crate::session::SongFile::Native | crate::session::SongFile::DawProject)
            )
        });
        if let Some(song) = song.cloned() {
            ctx.input_mut(|i| i.raw.dropped_files.clear());
            self.ask_then(ctx, Pending::Open(Some(song)));
            return;
        }
        if self.session.view() == View::Pattern {
            ctx.input_mut(|i| i.raw.dropped_files.clear());
            // Sounds dropped on a custom kit's beat join the kit (decision
            // 0025); anything else is imported.
            let kit = self.session.focused_track();
            let (sounds, others): (Vec<PathBuf>, Vec<PathBuf>) = if self.session.is_custom_kit(kit)
            {
                dropped.into_iter().partition(|f| {
                    daw_formats::library::ItemKind::of(f)
                        == Some(daw_formats::library::ItemKind::Sound)
                })
            } else {
                (Vec::new(), dropped)
            };
            if !sounds.is_empty() {
                self.session.add_kit_sounds(kit, sounds);
            }
            // A MIDI file dropped on an open pattern goes into its track,
            // which opens the new pattern (decision 0034).
            let (midi_files, others): (Vec<PathBuf>, Vec<PathBuf>) =
                others.into_iter().partition(|f| {
                    crate::session::SongFile::of(f) == Some(crate::session::SongFile::Midi)
                });
            let track = self.session.focused_track();
            let at = self.session.song_position();
            let mut opened = None;
            for file in &midi_files {
                if let Some(pattern) = self.session.import_midi_into(file, track, at) {
                    opened = Some(pattern);
                }
            }
            if let Some(pattern) = opened {
                self.session.open_pattern(track, pattern);
                self.roll.refit();
            }
            if !others.is_empty() {
                self.import(others);
            }
        }
    }

    /// Draws again soon while something moves; rarely otherwise.
    fn schedule_repaint(&self, ctx: &egui::Context) {
        if self.session.is_playing()
            || self.session.is_recording()
            || self.session.is_loading_audio()
            || self.session.is_recording_audio()
            || self.is_dragging()
            || self.session.pointer_note().is_some()
            || self.session.export_progress().is_some()
            || (self.settings.show_mixer && self.mixer.is_moving())
        {
            ctx.request_repaint();
        } else if self.session.is_previewing()
            || self.library.is_listing()
            || self.session.is_reading_instruments()
            || self.session.is_rendering_voices()
        {
            // The library's playing mark and listings; instruments read;
            // singing rendered.
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.session.any_plugin_window_open() || !self.session.plugin_search().done {
            // Plugin windows are looked after between frames.
            ctx.request_repaint_after(Duration::from_millis(15));
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

    /// Something a part of the window asked for.
    fn handle_intent(&mut self, intent: Intent) {
        let project = self.session.project();
        match intent {
            Intent::RenameTrack(track) => {
                if let Some(t) = project.track(track) {
                    self.dialog = Some(Dialog::Rename {
                        what: Renaming::Track(track),
                        name: t.name.clone(),
                    });
                }
            }
            Intent::RenamePattern(pattern) => {
                if let Some(p) = project.pattern(pattern) {
                    self.dialog = Some(Dialog::Rename {
                        what: Renaming::Pattern(pattern),
                        name: p.name.clone(),
                    });
                }
            }
            Intent::OpenEffect(id) => match self.session.effect_plugin(id) {
                Some(target)
                    if !self.session.is_plugin_missing(target)
                        && self.session.has_plugin_window(target) =>
                {
                    self.session.show_plugin_window(target);
                }
                _ => self.effects.open(id),
            },
            Intent::ChooseInstrument => {
                self.dialog = Some(Dialog::Instruments {
                    search: String::new(),
                });
            }
            Intent::AddKitSounds(track) => {
                let files = self.host.choose_kit_sounds();
                self.session.add_kit_sounds(track, files);
            }
        }
    }

    /// Runs a command, from a shortcut, a button or the menu.
    pub fn run(&mut self, ctx: &egui::Context, command: Command) {
        // Escape first stops waiting for a MIDI control (decision 0033).
        if command == Command::Deselect
            && (self.button_learning.take().is_some() || self.session.stop_learning_midi())
        {
            return;
        }
        if self.session.view() == View::Song && self.run_in_song(command) {
            return;
        }
        let s = &mut self.session;
        match command {
            Command::PlayStop => s.toggle_play(),
            Command::Record => s.toggle_record(),
            Command::ToStart => s.to_start(),
            Command::Loop => s.toggle_loop(),
            Command::Metronome => s.toggle_metronome(),
            Command::Undo => {
                self.cancel_drag();
                self.session.undo();
            }
            Command::Redo => {
                self.cancel_drag();
                self.session.redo();
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
                if !self.roll.close_popups() && !self.cancel_drag() {
                    self.session.deselect();
                }
            }
            Command::MoveEarlier => s.nudge(-1),
            Command::MoveLater => s.nudge(1),
            Command::TransposeUp => s.step_selection(true),
            Command::TransposeDown => s.step_selection(false),
            Command::InvertUp => s.invert_selection(true),
            Command::InvertDown => s.invert_selection(false),
            Command::PitchMode => {
                self.cancel_drag();
                self.session.toggle_pitch_mode();
            }
            Command::OctaveUp => s.transpose(12),
            Command::OctaveDown => s.transpose(-12),
            Command::KeyboardOctaveUp => s.shift_keyboard_octave(1),
            Command::KeyboardOctaveDown => s.shift_keyboard_octave(-1),
            Command::ZoomIn => self.roll.zoom(1.25),
            Command::ZoomOut => self.roll.zoom(0.8),
            Command::ToggleKeyboard | Command::ToggleMixer | Command::ToggleLibrary => {
                self.toggle_panel(command);
            }
            Command::SwitchView => {
                self.cancel_drag();
                match self.session.view() {
                    View::Song => {
                        let track = self.session.track_id();
                        self.session.open_track(track);
                    }
                    View::Pattern => self.session.show_song(),
                }
            }
            Command::Export => {
                if self.session.export_progress().is_some() {
                    self.dialog = Some(Dialog::Exporting);
                } else {
                    self.dialog = Some(Dialog::Export {
                        format: ExportFormat::default(),
                        target: ExportTarget::default(),
                    });
                }
            }
            Command::ImportAudio => {
                self.cancel_drag();
                let files = self.host.choose_audio_files();
                if !files.is_empty() {
                    self.import(files);
                }
            }
            // Splitting acts on clips, in the song view.
            Command::Split => {}
            Command::UserGuide => self.host.open_user_guide(),
        }
    }

    /// Shows or hides the keyboard, the mixer or the library: the bottom
    /// panel shows one of them at a time.
    fn toggle_panel(&mut self, command: Command) {
        let s = &mut self.settings;
        match command {
            Command::ToggleKeyboard => {
                if s.show_mixer || s.show_library {
                    s.show_mixer = false;
                    s.show_library = false;
                    s.show_keyboard = true;
                } else {
                    s.show_keyboard = !s.show_keyboard;
                }
            }
            Command::ToggleMixer => {
                s.show_mixer = !s.show_mixer;
                if s.show_mixer {
                    s.show_library = false;
                }
            }
            Command::ToggleLibrary => {
                let show = !s.show_library || s.show_mixer;
                s.show_library = show;
                if show {
                    s.show_mixer = false;
                } else {
                    self.session.stop_preview();
                }
            }
            _ => {}
        }
    }

    /// Imports sound files at the play position: on the audio track in
    /// focus, or on new tracks. The song comes on screen.
    fn import(&mut self, files: Vec<PathBuf>) {
        self.session.show_song();
        let track = self
            .session
            .audio_track_in_focus()
            .then(|| self.session.focused_track());
        let at = self.session.song_position();
        self.session.import_files(files, track, at);
    }

    /// Commands that mean something else in the song view: they act on the
    /// selected clips (decision 0032). Returns whether the command was
    /// handled here.
    fn run_in_song(&mut self, command: Command) -> bool {
        let s = &mut self.session;
        match command {
            Command::Delete => s.remove_selected_clips(),
            Command::Duplicate => s.duplicate_selected_clips(),
            Command::MoveEarlier => s.nudge_selected_clips(-1),
            Command::MoveLater => s.nudge_selected_clips(1),
            Command::SelectAll => s.select_all_clips(),
            Command::Split => {
                s.split_at_play_position();
            }
            Command::Deselect => {
                if !self.song.close_popups() && !self.song.cancel_drag(s) {
                    s.select_clip(None);
                }
            }
            Command::ZoomIn => self.song.zoom(1.25),
            Command::ZoomOut => self.song.zoom(0.8),
            Command::Copy
            | Command::Cut
            | Command::Paste
            | Command::TransposeUp
            | Command::TransposeDown
            | Command::OctaveUp
            | Command::OctaveDown
            | Command::InvertUp
            | Command::InvertDown
            | Command::PitchMode => {}
            _ => return false,
        }
        true
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
        self.cancel_drag();
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
                self.refit_views();
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
                (text::IMPORT_AUDIO, Command::ImportAudio),
                (text::EXPORT, Command::Export),
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
            if ui
                .button(text::MANAGE_PLUGINS)
                .on_hover_text(text::MANAGE_PLUGINS_HINT)
                .clicked()
            {
                self.open_plugins_window();
                ui.close();
            }
            if ui
                .button(text::MANAGE_VOICEBANKS)
                .on_hover_text(text::MANAGE_VOICEBANKS_HINT)
                .clicked()
            {
                self.open_voicebanks_window();
                ui.close();
            }
            ui.separator();
            ui.menu_button(text::THEME, |ui| self.theme_menu(ui));
            ui.menu_button(text::KEY_LABELS, |ui| self.labels_menu(ui));
            if ui.button(text::SHORTCUTS).clicked() {
                self.dialog = Some(Dialog::Shortcuts {
                    capturing: None,
                    message: None,
                });
                ui.close();
            }
            if ui
                .button(text::MIDI_CONTROLLERS_MENU)
                .on_hover_text(text::MIDI_CONTROLLERS_HINT)
                .clicked()
            {
                self.open_midi_window();
                ui.close();
            }
            if item(ui, text::USER_GUIDE, Command::UserGuide, &self.keymap) {
                actions.push(Command::UserGuide);
                ui.close();
            }
            if ui.button(text::ABOUT).clicked() {
                self.dialog = Some(Dialog::About);
                ui.close();
            }
        });
    }

    fn theme_menu(&mut self, ui: &mut Ui) {
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
    }

    fn labels_menu(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(text::KEY_LABELS_HINT).small().weak());
        let families = [
            LayoutFamily::Qwerty,
            LayoutFamily::Azerty,
            LayoutFamily::Qwertz,
        ];
        let choices = std::iter::once((LabelChoice::Automatic, text::KEY_LABELS_AUTOMATIC)).chain(
            families
                .into_iter()
                .map(|family| (LabelChoice::Fixed(family), text::layout_name(family))),
        );
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
    }

    #[allow(clippy::too_many_lines, reason = "one place for every dialog")]
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
            Dialog::Recovery {
                project,
                path,
                details,
            } => {
                let mut choice = None;
                modal("recovery", ctx).show(ctx, |ui| {
                    ui.set_width(440.0);
                    ui.heading(text::RECOVER_TITLE);
                    ui.add_space(8.0);
                    ui.label(text::recover_question());
                    ui.add_space(8.0);
                    let name = path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .or_else(|| {
                            (!project.title.trim().is_empty()).then(|| project.title.clone())
                        })
                        .unwrap_or_else(|| text::UNTITLED.to_owned());
                    ui.label(
                        RichText::new(text::recovered_song(&name, details.age_seconds)).strong(),
                    );
                    if details.take {
                        ui.label(text::RECOVERED_TAKE);
                    }
                    if details.newer_file {
                        let p = Palette::of(ui.ctx());
                        ui.label(RichText::new(text::RECOVERY_OLDER).color(p.record));
                    }
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
                        self.refit_views();
                        self.requests.push(Request::RecoveryAccepted);
                    }
                    Some(false) => {
                        keep = false;
                        self.requests.push(Request::DiscardRecovery);
                    }
                    None => {}
                }
                Dialog::Recovery {
                    project,
                    path,
                    details,
                }
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
                if close {
                    self.button_learning = None;
                }
                Dialog::Shortcuts { capturing, message }
            }
            Dialog::Midi => {
                let (close, shortcuts) = self.midi_window(ctx);
                keep = !close;
                if shortcuts {
                    // Buttons are given commands beside the commands' keys.
                    Dialog::Shortcuts {
                        capturing: None,
                        message: None,
                    }
                } else {
                    Dialog::Midi
                }
            }
            Dialog::Rename { what, mut name } => {
                let mut done = None;
                modal("rename", ctx).show(ctx, |ui| {
                    ui.set_width(340.0);
                    ui.heading(text::RENAME);
                    ui.add_space(8.0);
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .desired_width(300.0)
                            .hint_text(text::NAME),
                    );
                    if !field.has_focus() && !field.lost_focus() {
                        field.request_focus();
                    }
                    if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        done = Some(true);
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(text::OK).clicked() {
                            done = Some(true);
                        }
                        if ui.button(text::CANCEL).clicked() {
                            done = Some(false);
                        }
                    });
                });
                if let Some(accepted) = done {
                    keep = false;
                    if accepted {
                        match what {
                            Renaming::Track(track) => self.session.rename_track(track, &name),
                            Renaming::Pattern(pattern) => {
                                self.session.rename_pattern(pattern, &name);
                            }
                        }
                    }
                }
                Dialog::Rename { what, name }
            }
            Dialog::Export {
                mut format,
                mut target,
            } => {
                let mut choice = None;
                modal("export", ctx).show(ctx, |ui| {
                    ui.set_width(460.0);
                    ui.heading(text::EXPORT_TITLE);
                    ui.add_space(6.0);
                    ui.label(RichText::new(text::EXPORT_HINT).weak());
                    ui.add_space(10.0);
                    let mut sound = target.is_sound();
                    if ui.radio(sound, text::EXPORT_SOUND).clicked() && !sound {
                        target = ExportTarget::Wav;
                        sound = true;
                    }
                    ui.indent("export-sound", |ui| {
                        ui.label(RichText::new(text::EXPORT_SOUND_HINT).small().weak());
                        ui.add_enabled_ui(sound, |ui| {
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut target, ExportTarget::Wav, text::EXPORT_WAV)
                                    .on_hover_text(text::EXPORT_WAV_HINT);
                                ui.add_space(16.0);
                                ui.radio_value(&mut target, ExportTarget::Flac, text::EXPORT_FLAC)
                                    .on_hover_text(text::EXPORT_FLAC_HINT);
                            });
                            for (value, label, hint) in [
                                (
                                    ExportFormat::Studio,
                                    text::EXPORT_STUDIO,
                                    text::EXPORT_STUDIO_HINT,
                                ),
                                (ExportFormat::Cd, text::EXPORT_CD, text::EXPORT_CD_HINT),
                            ] {
                                ui.radio_value(&mut format, value, label);
                                ui.label(RichText::new(hint).small().weak());
                            }
                        });
                    });
                    ui.add_space(6.0);
                    for (value, label, hint) in [
                        (
                            ExportTarget::DawProject,
                            text::EXPORT_DAWPROJECT,
                            text::EXPORT_DAWPROJECT_HINT,
                        ),
                        (
                            ExportTarget::Midi,
                            text::EXPORT_MIDI,
                            text::EXPORT_MIDI_HINT,
                        ),
                    ] {
                        ui.radio_value(&mut target, value, label);
                        ui.indent(label, |ui| {
                            ui.label(RichText::new(hint).small().weak());
                        });
                        ui.add_space(6.0);
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(text::EXPORT_BUTTON).clicked() {
                            choice = Some(true);
                        }
                        if ui.button(text::CANCEL).clicked() {
                            choice = Some(false);
                        }
                    });
                });
                match choice {
                    Some(true) => {
                        keep = false;
                        let suggested =
                            format!("{}.{}", self.session.display_name(), target.extension());
                        if let Some(path) = self.host.choose_export_file(&suggested, target) {
                            if target.is_sound() {
                                self.session.start_sound_export(&path, format, target);
                                self.dialog = Some(Dialog::Exporting);
                                return;
                            }
                            let _ = match target {
                                ExportTarget::DawProject => self.session.export_dawproject(&path),
                                _ => self.session.export_midi(&path),
                            };
                        }
                    }
                    Some(false) => keep = false,
                    None => {}
                }
                Dialog::Export { format, target }
            }
            Dialog::Instruments { search } => {
                let (search, close) = self.instrument_chooser(ctx, search);
                keep = !close;
                if matches!(
                    self.dialog,
                    Some(Dialog::Plugins { .. } | Dialog::Voicebanks { .. })
                ) {
                    // "Plugins…" or "Voicebanks…" in the chooser opened its
                    // window.
                    return;
                }
                Dialog::Instruments { search }
            }
            Dialog::Voicebanks {
                mut window,
                from_chooser,
            } => {
                let mut close = false;
                let busy = window.is_busy();
                let response = modal("voicebanks", ctx).show(ctx, |ui| {
                    let mut parts = crate::voicebanks::Parts {
                        root: self.voicebanks_root.as_deref(),
                        list: &mut self.voicebank_list,
                        host: self.host.as_mut(),
                        session: &mut self.session,
                    };
                    close = window.contents(ui, &mut parts);
                });
                if response.should_close() && !busy {
                    close = true;
                }
                if close {
                    keep = from_chooser;
                    Dialog::Instruments {
                        search: String::new(),
                    }
                } else {
                    Dialog::Voicebanks {
                        window,
                        from_chooser,
                    }
                }
            }
            Dialog::Plugins {
                mut window,
                from_chooser,
            } => {
                let mut close = false;
                let response = modal("plugins", ctx).show(ctx, |ui| {
                    let mut parts = crate::plugin_manager::Parts {
                        session: &self.session,
                        settings: &mut self.settings,
                        host: self.host.as_mut(),
                        requests: &mut self.requests,
                        places: &self.plugin_places,
                        messages: &self.plugin_messages,
                    };
                    close = window.contents(ui, &mut parts);
                });
                if response.should_close() {
                    close = true;
                }
                if close {
                    keep = from_chooser;
                    Dialog::Instruments {
                        search: String::new(),
                    }
                } else {
                    Dialog::Plugins {
                        window,
                        from_chooser,
                    }
                }
            }
            Dialog::Exporting => {
                let progress = self.session.export_progress();
                if progress.is_none() {
                    keep = false;
                }
                modal("exporting", ctx).show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.heading(text::EXPORTING);
                    ui.add_space(10.0);
                    ui.add(egui::ProgressBar::new(progress.unwrap_or(1.0)).show_percentage());
                    ui.add_space(10.0);
                    if ui.button(text::CANCEL).clicked() {
                        self.session.cancel_export();
                    }
                });
                Dialog::Exporting
            }
        };
        if keep {
            self.dialog = Some(dialog);
        }
        if let Some(pending) = then {
            self.carry_out(ctx, pending);
        }
    }

    /// Adds a track with an included instrument (a SoundFont asks for its
    /// file first). Returns whether a track was added.
    fn add_included(&mut self, which: Included) -> bool {
        match which {
            Included::Piano => {
                self.session.add_piano_track();
            }
            Included::Drums => {
                self.session.add_drum_track();
            }
            Included::Kit => {
                // A custom kit, then its sounds (decision 0025).
                let track = self.session.add_kit_track(None);
                let files = self.host.choose_kit_sounds();
                self.session.add_kit_sounds(track, files);
            }
            Included::Synth => {
                self.session.add_synth_track();
            }
            Included::SoundFont => {
                let Some(file) = self.host.choose_soundfont() else {
                    return false;
                };
                self.session.add_instrument_file(&file);
            }
        }
        true
    }

    /// The buttons under the instrument chooser: Cancel, Look again (when
    /// the search is done), Plugins… and Voicebanks….
    fn chooser_buttons(&mut self, ui: &mut Ui, search_done: bool) -> ChooserButtons {
        let mut pressed = ChooserButtons::default();
        ui.horizontal(|ui| {
            pressed.cancel = ui.button(text::CANCEL).clicked();
            if ui
                .add_enabled(search_done, Button::new(text::LOOK_AGAIN))
                .on_hover_text(text::LOOK_AGAIN_HINT)
                .clicked()
            {
                self.requests.push(Request::FindPlugins);
            }
            pressed.plugins = ui
                .button(text::MANAGE_PLUGINS)
                .on_hover_text(text::MANAGE_PLUGINS_HINT)
                .clicked();
            pressed.voicebanks = ui
                .button(text::MANAGE_VOICEBANKS)
                .on_hover_text(text::MANAGE_VOICEBANKS_HINT)
                .clicked();
        });
        pressed
    }

    /// The chooser's voices (decision 0027): the installed voicebanks that
    /// match the search (none installed: "Voicebanks…" below is the way
    /// in). Returns whether a voice track was added.
    fn voice_choices(&mut self, ui: &mut Ui, needle: &str) -> bool {
        let wanted = |text: &str| needle.is_empty() || text.to_lowercase().contains(needle);
        let voices: Vec<_> = self
            .voicebank_list
            .iter()
            .filter(|info| {
                wanted(&info.name)
                    || wanted(&text::voicebank_detail(
                        info.author.as_deref(),
                        info.syllables,
                    ))
                    || wanted(text::VOICES)
            })
            .cloned()
            .collect();
        if voices.is_empty() {
            return false;
        }
        ui.label(RichText::new(text::VOICES).weak());
        let mut added = false;
        for info in voices {
            let detail = text::voicebank_detail(info.author.as_deref(), info.syllables);
            let folder = crate::voicebanks::folder_name(&info);
            let picture = self.session.voice_icon(ui.ctx(), &folder);
            if voice_row(ui, &info.name, &detail, picture) {
                self.session.add_voice_track(&folder, &info.name);
                added = true;
            }
        }
        ui.add_space(8.0);
        added
    }

    /// The instrument chooser: the included instruments, then the plugins
    /// found on this computer, with a search field. Returns the search text
    /// and whether the chooser closes.
    fn instrument_chooser(&mut self, ctx: &egui::Context, mut search: String) -> (String, bool) {
        let mut close = false;
        let mut manage = false;
        let mut manage_voices = false;
        let plugins = self.session.plugins_of(PluginKind::Instrument);
        let found = self.session.plugin_search().clone();
        let response = modal("instruments", ctx).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading(text::INSTRUMENTS_TITLE);
            ui.add_space(8.0);
            let field = ui.add(
                egui::TextEdit::singleline(&mut search)
                    .desired_width(f32::INFINITY)
                    .hint_text(text::SEARCH),
            );
            if !field.has_focus() && !field.lost_focus() {
                field.request_focus();
            }
            ui.add_space(8.0);
            let needle = search.trim().to_lowercase();
            let wanted = |name: &str, detail: &str| {
                needle.is_empty()
                    || name.to_lowercase().contains(&needle)
                    || detail.to_lowercase().contains(&needle)
            };
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let included = [
                        (text::ADD_PIANO, text::ADD_PIANO_HINT, Included::Piano),
                        (text::ADD_DRUMS, text::ADD_DRUMS_HINT, Included::Drums),
                        (text::ADD_KIT, text::ADD_KIT_HINT, Included::Kit),
                        (text::ADD_SYNTH, text::ADD_SYNTH_HINT, Included::Synth),
                        (
                            text::ADD_SOUNDFONT,
                            text::ADD_SOUNDFONT_HINT,
                            Included::SoundFont,
                        ),
                    ];
                    let included: Vec<_> = included
                        .into_iter()
                        .filter(|(name, hint, _)| wanted(name, hint))
                        .collect();
                    if !included.is_empty() {
                        ui.label(RichText::new(text::INCLUDED).weak());
                        for (name, hint, which) in included {
                            if choice_row(ui, name, hint) && self.add_included(which) {
                                close = true;
                            }
                        }
                        ui.add_space(8.0);
                    }
                    if self.voice_choices(ui, &needle) {
                        close = true;
                    }
                    ui.label(RichText::new(text::PLUGINS).weak());
                    let mut shown = 0;
                    for info in &plugins {
                        let detail = text::plugin_detail(&info.vendor, format_name(info));
                        if !wanted(&info.name, &detail) {
                            continue;
                        }
                        shown += 1;
                        if choice_row(ui, &info.name, &detail) {
                            self.session.add_plugin_track(info);
                            close = true;
                        }
                    }
                    search_state(ui, &found, plugins.is_empty(), shown);
                });
            ui.add_space(10.0);
            let pressed = self.chooser_buttons(ui, found.done);
            close |= pressed.cancel;
            manage |= pressed.plugins;
            manage_voices |= pressed.voicebanks;
        });
        if manage_voices {
            // The chooser comes back once the Voicebanks window closes.
            self.dialog = Some(Dialog::Voicebanks {
                window: VoicebanksWindow::default(),
                from_chooser: true,
            });
            return (search, false);
        }
        if manage {
            // The chooser comes back once the Plugins window closes.
            self.plugin_messages.clear();
            self.dialog = Some(Dialog::Plugins {
                window: PluginsWindow::default(),
                from_chooser: true,
            });
            return (search, false);
        }
        if response.should_close() {
            close = true;
        }
        (search, close)
    }

    /// The MIDI controllers window (decision 0033): the controllers found,
    /// each with a switch; the song's knobs and faders and what they move;
    /// the buttons and their commands. Returns whether it closes, and
    /// whether the shortcuts window is wanted in its place.
    fn midi_window(&mut self, ctx: &egui::Context) -> (bool, bool) {
        let mut close = false;
        let mut shortcuts = false;
        let links = self.session.midi_links();
        let mut named: Vec<(daw_model::MidiLink, String)> = links
            .into_iter()
            .map(|link| {
                let name = self.session.link_name(link.target);
                (link, name)
            })
            .collect();
        named.sort_by_key(|(link, _)| link.control);
        let response = modal("midi", ctx).show(ctx, |ui| {
            ui.set_width(560.0);
            let p = Palette::of(ui.ctx());
            ui.heading(text::MIDI_CONTROLLERS);
            ui.label(RichText::new(text::MIDI_WINDOW_INTRO).color(p.text_weak));
            ui.add_space(10.0);
            ui.label(RichText::new(text::MIDI_DEVICES).strong());
            if let Some(problem) = &self.midi_problem {
                ui.label(RichText::new(problem).small().color(p.record));
            } else if self.midi_devices.is_empty() {
                ui.label(RichText::new(text::NO_MIDI_DEVICES).small().weak());
            }
            for device in self.midi_devices.clone() {
                let mut on = !self.settings.midi_off.contains(&device);
                if ui
                    .checkbox(&mut on, &device)
                    .on_hover_text(text::MIDI_DEVICE_HINT)
                    .changed()
                {
                    self.settings.midi_off.retain(|d| *d != device);
                    if !on {
                        self.settings.midi_off.push(device.clone());
                    }
                }
            }
            ui.add_space(10.0);
            ui.label(RichText::new(text::MIDI_KNOBS).strong());
            if named.is_empty() {
                ui.label(RichText::new(text::NO_MIDI_KNOBS).small().weak());
            }
            egui::Grid::new("midi-knobs")
                .num_columns(3)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (link, name) in &named {
                        ui.label(text::midi_control_title(link.control));
                        ui.label(name);
                        let forget = ui.small_button(text::FORGET);
                        forget.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                text::forget_knob(name),
                            )
                        });
                        if forget.clicked() {
                            self.session.forget_midi(link.target);
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(10.0);
            ui.label(RichText::new(text::MIDI_BUTTONS).strong());
            if self.midi_buttons.is_empty() {
                ui.label(RichText::new(text::NO_MIDI_BUTTONS).small().weak());
            }
            let mut forgotten = None;
            egui::Grid::new("midi-buttons")
                .num_columns(3)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (command, control) in &self.midi_buttons {
                        ui.label(text::midi_control_title(*control));
                        ui.label(text::command_name(*command));
                        if ui.small_button(text::FORGET).clicked() {
                            forgotten = Some(*command);
                        }
                        ui.end_row();
                    }
                });
            if let Some(command) = forgotten {
                self.midi_buttons.remove(&command);
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(text::GIVE_BUTTONS_COMMANDS).clicked() {
                    shortcuts = true;
                }
                if ui.button(text::CLOSE).clicked() {
                    close = true;
                }
            });
        });
        if response.should_close() {
            close = true;
        }
        (close, shortcuts)
    }

    /// A command's MIDI button in the shortcuts window (decision 0033): the
    /// button or pad that runs it, Learn and Clear.
    fn midi_button_cells(&mut self, ui: &mut Ui, command: Command) {
        let button = self.midi_buttons.get(&command).copied();
        let shown = if self.button_learning == Some(command) {
            text::PRESS_A_BUTTON.to_owned()
        } else {
            button.map_or_else(|| text::NO_MIDI_BUTTON.to_owned(), text::midi_control_title)
        };
        ui.label(RichText::new(shown).small());
        ui.horizontal(|ui| {
            let learn = ui
                .small_button(text::LEARN_BUTTON)
                .on_hover_text(text::LEARN_BUTTON_HINT);
            learn.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    text::learn_button_for(text::command_name(command)),
                )
            });
            if learn.clicked() {
                self.button_learning = Some(command);
                self.session.stop_learning_midi();
            }
            if button.is_some() && ui.small_button(text::CLEAR_BUTTON).clicked() {
                self.midi_buttons.remove(&command);
            }
        });
    }

    fn shortcuts_window(
        &mut self,
        ctx: &egui::Context,
        mut capturing: Option<Command>,
        mut message: Option<String>,
    ) -> (Option<Command>, Option<String>, bool) {
        let mut close = false;
        let response = modal("shortcuts", ctx).show(ctx, |ui| {
            ui.set_width(960.0);
            ui.heading(text::SHORTCUTS);
            ui.label(RichText::new(text::SHORTCUTS_HINT).small().weak());
            ui.label(RichText::new(text::MIDI_BUTTONS_HINT).small().weak());
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("shortcut-table")
                        .num_columns(5)
                        .spacing([16.0, 6.0])
                        .min_col_width(110.0)
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
                                self.midi_button_cells(ui, command);
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

/// A row of the instrument chooser: a name, and in weak text on the right
/// who made it or what it is.
fn choice_row(ui: &mut Ui, name: &str, detail: &str) -> bool {
    ui.add(
        Button::new(RichText::new(name).strong())
            .shortcut_text(detail)
            .min_size(egui::vec2(ui.available_width(), 30.0)),
    )
    .clicked()
}

/// A voice in the instrument chooser: its name, with its voicebank's
/// picture small beside it when it has one (decision 0029).
fn voice_row(
    ui: &mut Ui,
    name: &str,
    detail: &str,
    picture: Option<crate::session::VoicePicture>,
) -> bool {
    let label = RichText::new(name).strong();
    let button = match picture {
        Some(picture) => {
            let side = picture.size.max_elem().max(1.0);
            let image = egui::Image::from_texture((picture.texture, picture.size))
                .fit_to_exact_size(picture.size * (22.0 / side));
            Button::image_and_text(image, label)
        }
        None => Button::new(label),
    };
    ui.add(
        button
            .shortcut_text(detail)
            .min_size(egui::vec2(ui.available_width(), 30.0)),
    )
    .clicked()
}

/// Which of the buttons under the instrument chooser were pressed.
#[derive(Clone, Copy, Debug, Default)]
struct ChooserButtons {
    cancel: bool,
    plugins: bool,
    voicebanks: bool,
}

/// How many plugin files that cannot be used the chooser explains in full.
const PROBLEMS_SHOWN: usize = 3;

/// Under the plugin list: the search still running, nothing found, nothing
/// matching, and files that could not be opened.
pub(crate) fn search_state(
    ui: &mut Ui,
    found: &crate::session::PluginSearch,
    none: bool,
    shown: usize,
) {
    if !found.done {
        ui.label(text::looking_for_plugins(found.checked, found.total));
    } else if none {
        ui.label(RichText::new(text::NO_PLUGINS_FOUND).weak());
    } else if shown == 0 {
        ui.label(RichText::new(text::NOTHING_MATCHES).weak());
    }
    if found.problems.is_empty() {
        return;
    }
    // Each file that cannot be used, and why (a plugin made for another
    // system is the usual reason): the first few in full.
    ui.add_space(4.0);
    ui.label(RichText::new(text::unreadable_plugins(found.problems.len())).small());
    let line = |(path, problem): &(PathBuf, daw_engine::ScanProblem)| {
        let file = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        (
            text::scan_problem(&file, problem),
            path.display().to_string(),
        )
    };
    for problem in found.problems.iter().take(PROBLEMS_SHOWN) {
        let (words, place) = line(problem);
        ui.label(RichText::new(words).small().weak())
            .on_hover_text(place);
    }
    if found.problems.len() > PROBLEMS_SHOWN {
        let rest: Vec<String> = found.problems[PROBLEMS_SHOWN..]
            .iter()
            .map(|p| line(p).0)
            .collect();
        ui.label(
            RichText::new(text::and_more(found.problems.len() - PROBLEMS_SHOWN))
                .small()
                .weak(),
        )
        .on_hover_text(rest.join("\n"));
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
/// the notes as JSON (with their lyrics and expression), so notes can be
/// pasted into another window of it.
pub fn notes_to_text(notes: &[CopiedNote]) -> String {
    format!(
        "{}\n{}",
        clipboard_header(),
        serde_json::to_string(notes).unwrap_or_default()
    )
}

/// Notes from clipboard text written by [`notes_to_text`] (or by an older
/// copy of the program, whose notes have no lyrics).
pub fn notes_from_text(text: &str) -> Option<Vec<CopiedNote>> {
    let (header, json) = text.split_once('\n')?;
    if header.trim() != clipboard_header() {
        return None;
    }
    serde_json::from_str(json).ok()
}

fn clipboard_header() -> String {
    format!("{} notes 1", daw_ident::PRODUCT_NAME)
}

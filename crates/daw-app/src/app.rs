// SPDX-License-Identifier: GPL-3.0-or-later

//! The running program: the window, connected to the sound output, the MIDI
//! keyboards, autosave and the settings file.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use daw_devices::{AudioOutput, MidiInputs, MidiMessage, OutputMode};
use daw_engine::MidiSender;
use daw_ui::{Request, Settings, Workspace, text};

use crate::paths::Paths;
use crate::recovery::Autosaver;

/// How often devices are checked.
const DEVICE_CHECK: Duration = Duration::from_secs(1);
/// How long changed settings wait before they are written.
const SETTINGS_DELAY: Duration = Duration::from_secs(2);

/// Everything the window runs with.
pub struct App {
    workspace: Workspace,
    audio: AudioOutput,
    midi: Option<MidiInputs>,
    midi_notes: mpsc::Receiver<(u8, bool)>,
    known_midi: Vec<String>,
    paths: Option<Paths>,
    autosaver: Option<Autosaver>,
    last_device_check: Option<Instant>,
    saved_settings: Settings,
    settings_changed_at: Option<Instant>,
}

/// What [`App::new`] needs.
pub struct Parts {
    /// The window.
    pub workspace: Workspace,
    /// The sound output (kept running for as long as the window is open).
    pub audio: AudioOutput,
    /// The program's folders, if the system has them.
    pub paths: Option<Paths>,
    /// Autosave, if the recovery folder could be set up.
    pub autosaver: Option<Autosaver>,
    /// The settings as loaded.
    pub settings: Settings,
    /// Sends MIDI keyboard notes to the engine.
    pub midi_sender: std::sync::Arc<MidiSender>,
}

impl App {
    /// Connects the window to everything around it.
    pub fn new(ctx: &egui::Context, parts: Parts) -> Self {
        let Parts {
            mut workspace,
            audio,
            paths,
            mut autosaver,
            settings,
            midi_sender,
        } = parts;
        if let Some((project, path)) = autosaver.as_mut().and_then(Autosaver::take_orphan) {
            workspace.offer_recovery(project, path);
        }

        // MIDI keyboards play straight into the engine; the window is told
        // too, only to light up the keys.
        let (notes_tx, midi_notes) = mpsc::channel();
        let repaint = ctx.clone();
        let midi = MidiInputs::start(daw_ident::PRODUCT_NAME, move |message| {
            let note = match message {
                MidiMessage::NoteOn { key, velocity, .. } => {
                    midi_sender.note_on(key, f32::from(velocity) / 127.0);
                    Some((key, true))
                }
                MidiMessage::NoteOff { key, .. } => {
                    midi_sender.note_off(key);
                    Some((key, false))
                }
                // The sustain pedal (controller 64): down from half way.
                MidiMessage::ControlChange {
                    controller: 64,
                    value,
                    ..
                } => {
                    midi_sender.sustain(value >= 64);
                    None
                }
                MidiMessage::ControlChange { .. } | MidiMessage::PitchBend { .. } => None,
            };
            if let Some(note) = note {
                let _ = notes_tx.send(note);
                repaint.request_repaint();
            }
        });

        Self {
            workspace,
            audio,
            midi: Some(midi),
            midi_notes,
            known_midi: Vec::new(),
            paths,
            autosaver,
            last_device_check: None,
            saved_settings: settings,
            settings_changed_at: None,
        }
    }

    /// Opens a file given on the command line.
    pub fn open(&mut self, path: PathBuf) {
        self.workspace.open(path);
    }

    fn check_devices(&mut self) {
        if self
            .last_device_check
            .is_some_and(|at| at.elapsed() < DEVICE_CHECK)
        {
            return;
        }
        self.last_device_check = Some(Instant::now());
        let status = self.audio.status();
        let notice = match status.mode {
            OutputMode::Silent => Some(text::NO_SOUND_DEVICE.to_owned()),
            OutputMode::Starting | OutputMode::Device { .. } => None,
        };
        self.workspace.set_notice(notice);

        if let Some(midi) = &self.midi {
            let connected = midi.connected();
            for name in connected.iter().filter(|n| !self.known_midi.contains(n)) {
                self.workspace
                    .session_mut()
                    .set_status(text::midi_connected(name));
            }
            self.known_midi = connected;
        }
    }

    fn keep_settings(&mut self) {
        let settings = self.workspace.settings();
        if settings == self.saved_settings {
            self.settings_changed_at = None;
            return;
        }
        let changed_at = *self.settings_changed_at.get_or_insert_with(Instant::now);
        if changed_at.elapsed() >= SETTINGS_DELAY {
            self.write_settings(&settings);
        }
    }

    fn write_settings(&mut self, settings: &Settings) {
        if let Some(paths) = &self.paths {
            let _ = std::fs::create_dir_all(&paths.config);
            let _ = daw_formats::write_atomically(
                &paths.settings_file(),
                settings.to_json().as_bytes(),
            );
        }
        self.saved_settings = settings.clone();
        self.settings_changed_at = None;
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok((key, down)) = self.midi_notes.try_recv() {
            self.workspace.session_mut().show_outside_note(key, down);
        }
        self.check_devices();
        self.workspace.ui(ui);
        let requests = self.workspace.take_requests();
        if let Some(autosaver) = &mut self.autosaver {
            let session = self.workspace.session();
            autosaver.tick(
                session.project(),
                session.path(),
                session.document().revision(),
                session.is_dirty(),
            );
            // Once decided, the work found after a crash is deleted: either
            // the person declined it, or it is now the open song and has
            // just been autosaved here.
            if requests
                .iter()
                .any(|r| matches!(r, Request::DiscardRecovery | Request::RecoveryAccepted))
            {
                autosaver.forget_orphan();
            }
        }
        self.keep_settings();
    }

    fn on_exit(&mut self) {
        let settings = self.workspace.settings();
        self.write_settings(&settings);
        self.midi = None;
        if let Some(autosaver) = &mut self.autosaver {
            autosaver.finish();
        }
    }
}

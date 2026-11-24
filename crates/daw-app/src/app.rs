// SPDX-License-Identifier: GPL-3.0-or-later

//! The running program: the window, connected to the sound output and
//! input, the MIDI keyboards, autosave and the settings file.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use daw_devices::{AudioInput, AudioOutput, MidiInputs, MidiMessage, OutputMode, TransportMessage};
use daw_plugin_host::install;
use daw_plugin_host::scan::{self, Scanner};
use daw_ui::midi::{ControllerInput, TransportButton};
use daw_ui::{PluginPlaces, PluginSearch, Request, Settings, Workspace, text};

use crate::paths::Paths;
use crate::plugin_jobs::{Job, PluginJobs, bridge_state};
use crate::recovery::Autosaver;

/// How often devices are checked.
const DEVICE_CHECK: Duration = Duration::from_secs(1);
/// How long changed settings wait before they are written.
const SETTINGS_DELAY: Duration = Duration::from_secs(2);
/// How long an input device may take to open before the person is told
/// there is none.
const INPUT_GRACE: Duration = Duration::from_secs(2);

/// Everything the window runs with.
pub struct App {
    // The sound output stops first: plugins' audio halves are dropped before
    // their main halves (in the workspace's engine).
    audio: AudioOutput,
    /// The microphone or interface (none when started silent).
    input: Option<AudioInput>,
    /// Since when the input has been wanted, and whether the person was
    /// told it did not open.
    input_wanted: Option<(Instant, bool)>,
    workspace: Workspace,
    midi: Option<MidiInputs>,
    known_midi: Vec<String>,
    paths: Option<Paths>,
    autosaver: Option<Autosaver>,
    last_device_check: Option<Instant>,
    saved_settings: Settings,
    settings_changed_at: Option<Instant>,
    /// Plugins being looked for, in the background.
    plugin_scan: Option<Scanner>,
    /// Another search for plugins is wanted once this one ends.
    search_again: bool,
    /// Adding, removing and installing plugins (decision 0023).
    plugin_jobs: Option<PluginJobs>,
    /// Where plugins come from, as the Plugins window shows it.
    plugin_places: PluginPlaces,
}

/// What [`App::new`] needs.
pub struct Parts {
    /// The window.
    pub workspace: Workspace,
    /// The sound output (kept running for as long as the window is open).
    pub audio: AudioOutput,
    /// The sound input, if there is to be one.
    pub input: Option<AudioInput>,
    /// The program's folders, if the system has them.
    pub paths: Option<Paths>,
    /// Autosave, if the recovery folder could be set up.
    pub autosaver: Option<Autosaver>,
    /// The settings as loaded.
    pub settings: Settings,
}

impl App {
    /// Connects the window to everything around it.
    pub fn new(ctx: &egui::Context, parts: Parts) -> Self {
        let Parts {
            mut workspace,
            audio,
            input,
            paths,
            mut autosaver,
            settings,
        } = parts;
        // The song's audio files live in the recovery folder, so a crash
        // keeps them.
        if let Some(autosaver) = &autosaver {
            workspace
                .session_mut()
                .set_audio_folder(autosaver.audio_folder());
        }
        if let Some(writer) = autosaver.as_ref().and_then(Autosaver::safety_writer) {
            workspace.session_mut().set_safety_copy(Box::new(writer));
        }
        if let Some((project, path, found)) = autosaver.as_mut().and_then(Autosaver::take_orphan) {
            let details = daw_ui::RecoveryDetails {
                age_seconds: found.age_seconds,
                newer_file: found.newer_file,
                take: found.take,
            };
            workspace.offer_recovery(project, path, details);
        }

        // MIDI controllers (decision 0033): keys, pads, wheels and pedals
        // play straight into the engine; transport buttons and controls
        // given a job go to the window, which is woken for them.
        let router = workspace.midi_router();
        let repaint = ctx.clone();
        let off = settings.midi_off.clone();
        let midi = MidiInputs::start(daw_ident::PRODUCT_NAME, off, move |message| {
            if router.handle(controller_input(message)) {
                repaint.request_repaint();
            }
        });

        // Plugins added in the program live in its own folder (decision
        // 0023), which the search always looks in.
        let own = paths.as_ref().map(|paths| install::own_folder(&paths.data));
        let plugin_jobs = own.clone().map(|own| {
            let repaint = ctx.clone();
            PluginJobs::start(own, move || repaint.request_repaint())
        });
        let bridge = install::find_bridge();
        let plugin_places = PluginPlaces {
            usual: scan::default_folders(),
            added: own.as_deref().map_or_else(Vec::new, |own| {
                install::own_plugins(own).into_iter().collect()
            }),
            own,
            bridge: bridge_state(&bridge),
            busy: false,
        };
        workspace.set_plugin_places(plugin_places.clone());
        workspace.set_voicebanks_folder(paths.as_ref().map(crate::paths::Paths::voicebanks));

        let mut app = Self {
            workspace,
            audio,
            input,
            input_wanted: None,
            midi: Some(midi),
            known_midi: Vec::new(),
            paths,
            autosaver,
            last_device_check: None,
            saved_settings: settings,
            settings_changed_at: None,
            plugin_scan: None,
            search_again: false,
            plugin_jobs,
            plugin_places,
        };
        app.find_plugins();
        app
    }

    /// Starts looking for plugins in the usual folders (in the background;
    /// what was learnt before is remembered).
    fn find_plugins(&mut self) {
        let Ok(program) = std::env::current_exe() else {
            let done = PluginSearch {
                done: true,
                ..PluginSearch::default()
            };
            self.workspace.session_mut().set_plugins(Vec::new(), done);
            return;
        };
        let cache = self.paths.as_ref().map(|paths| {
            let _ = std::fs::create_dir_all(&paths.data);
            paths.plugin_cache()
        });
        let mut folders = scan::default_folders();
        if let Some(own) = &self.plugin_places.own {
            folders.extend(install::own_search_folders(own));
        }
        folders.extend(self.workspace.settings().plugin_folders);
        self.plugin_scan = Some(Scanner::start(program, cache, folders));
    }

    /// Looks for plugins again now, or once the search under way ends.
    fn search_again(&mut self) {
        if self.plugin_scan.is_none() {
            self.find_plugins();
        } else {
            self.search_again = true;
        }
    }

    /// Passes the Plugins window's requests to the worker, and what it
    /// finished back to the window.
    fn follow_plugin_jobs(&mut self, requests: &[Request]) {
        let Some(jobs) = &mut self.plugin_jobs else {
            return;
        };
        for request in requests {
            match request {
                Request::AddPlugins(files) => jobs.send(Job::Add(files.clone())),
                Request::RemovePlugin(plugin) => jobs.send(Job::Remove(plugin.clone())),
                Request::SetUpWindowsPlugins => jobs.send(Job::SetUpWindowsPlugins),
                _ => {}
            }
        }
        let finished = jobs.finished();
        let busy = jobs.busy();
        let changed = !finished.is_empty() || busy != self.plugin_places.busy;
        for job in &finished {
            self.workspace.plugin_job_done(&job.done);
            self.plugin_places.added.clone_from(&job.added);
            self.plugin_places.bridge = bridge_state(&job.bridge);
        }
        self.plugin_places.busy = busy;
        if changed {
            self.workspace.set_plugin_places(self.plugin_places.clone());
        }
        if !finished.is_empty() {
            self.search_again();
        }
    }

    /// Passes what the plugin search found to the song.
    fn follow_plugin_search(&mut self) {
        let Some(scanner) = &self.plugin_scan else {
            return;
        };
        let (checked, total) = scanner.progress();
        let done = scanner.is_done();
        let findings = scanner.findings();
        let search = PluginSearch {
            done,
            checked,
            total,
            problems: findings.problems,
        };
        self.workspace
            .session_mut()
            .set_plugins(findings.plugins, search);
        if done {
            self.plugin_scan = None;
            if std::mem::take(&mut self.search_again) {
                self.find_plugins();
            }
        }
    }

    /// Opens a file given on the command line.
    pub fn open(&mut self, path: PathBuf) {
        self.workspace.open(path);
    }

    /// Opens the input while the song wants it, and tells the engine how
    /// long sound takes to go out and come back in.
    fn follow_input(&mut self) {
        let Some(input) = &self.input else {
            return;
        };
        let wanted = self.workspace.session().wants_input();
        input.set_active(wanted);
        if !wanted {
            self.input_wanted = None;
            return;
        }
        let (since, told) = *self.input_wanted.get_or_insert((Instant::now(), false));
        if !told && since.elapsed() > INPUT_GRACE && input.status().device.is_none() {
            self.workspace
                .session_mut()
                .set_status(text::NO_INPUT_DEVICE);
            self.input_wanted = Some((since, true));
        }
    }

    /// The delay out (the output's report, or its buffer) plus the delay
    /// in, for lining takes up with what was heard.
    fn record_latency(&self) -> Duration {
        let status = self.audio.status();
        let out = self.audio.latency().or_else(|| {
            status.buffer_frames.map(|frames| {
                Duration::from_secs_f64(f64::from(frames) / f64::from(status.sample_rate))
            })
        });
        let input = self.input.as_ref().and_then(AudioInput::latency);
        out.unwrap_or_default() + input.unwrap_or_default()
    }

    fn check_devices(&mut self) {
        if self
            .last_device_check
            .is_some_and(|at| at.elapsed() < DEVICE_CHECK)
        {
            return;
        }
        self.last_device_check = Some(Instant::now());
        let latency = self.record_latency();
        self.workspace
            .session_mut()
            .engine_mut()
            .set_record_latency(latency.as_secs_f32());
        let status = self.audio.status();
        let notice = match status.mode {
            OutputMode::Silent => Some(text::NO_SOUND_DEVICE.to_owned()),
            OutputMode::Starting | OutputMode::Device { .. } => None,
        };
        self.workspace.set_notice(notice);

        if let Some(midi) = &self.midi {
            // Controllers switched off in the MIDI window are let go.
            midi.set_off(&self.workspace.settings().midi_off);
            self.workspace.set_midi_devices(midi.seen());
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
        self.check_devices();
        self.follow_input();
        self.follow_plugin_search();
        self.workspace.ui(ui);
        let requests = self.workspace.take_requests();
        if requests.contains(&Request::FindPlugins) {
            self.search_again();
        }
        self.follow_plugin_jobs(&requests);
        if let Some(autosaver) = &mut self.autosaver {
            let session = self.workspace.session_mut();
            autosaver.note_take(session.take_in_progress());
            let version = session.document().version();
            let dirty = session.is_dirty();
            autosaver.tick(version, dirty, || {
                // Plugins' settings live in the plugins until copied.
                session.capture_plugin_states();
                (
                    session.project().clone(),
                    session.path().map(std::path::Path::to_path_buf),
                )
            });
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
        self.input = None;
        if let Some(autosaver) = &mut self.autosaver {
            autosaver.finish();
        }
    }
}

/// A device's MIDI message as the window's router takes it.
fn controller_input(message: MidiMessage) -> ControllerInput {
    match message {
        MidiMessage::NoteOn {
            channel,
            key,
            velocity,
        } => ControllerInput::NoteOn {
            channel,
            key,
            velocity,
        },
        MidiMessage::NoteOff { channel, key } => ControllerInput::NoteOff { channel, key },
        MidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => ControllerInput::Controller {
            channel,
            number: controller,
            value,
        },
        MidiMessage::PitchBend { channel, value } => ControllerInput::PitchBend { channel, value },
        MidiMessage::Pressure { channel, value } => ControllerInput::Pressure { channel, value },
        MidiMessage::Transport(transport) => ControllerInput::Transport(match transport {
            TransportMessage::Play => TransportButton::Play,
            TransportMessage::Stop => TransportButton::Stop,
            TransportMessage::Record => TransportButton::Record,
            TransportMessage::Rewind => TransportButton::Rewind,
        }),
    }
}

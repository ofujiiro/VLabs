// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod library;
mod messages;
mod plugins;
mod processor;
mod program;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use daw_dsp::effects::Effect as EffectInstance;
use daw_model::{
    EffectId, EffectKind, Instrument as InstrumentSpec, PluginSetup, PluginState, Project, Ticks,
    TrackId,
};
use daw_plugin_host::{AudioConfig, LoadedPlugin};

pub use daw_plugin_host::{Format as PluginFileFormat, Kind as PluginKind, ParamInfo, PluginInfo};
pub use library::SoundLibrary;
pub use messages::EngineEvent;
pub use plugins::PluginTarget;
pub use processor::{EngineProcessor, MAX_EFFECTS, MAX_SLOTS};
pub use program::PlaybackScope;

use messages::{
    Command, Garbage, LiveNote, NO_TARGET, Outgoing, RECORD_ACTIVE, RECORD_COUNT_IN, Shared,
};
use plugins::{EffectSlot, PluginEffect, PluginInstrument, Plugins, Silence};

/// Something the engine could not do, for the interface to put into words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// A sound bundled with the program is missing (a plain sound plays
    /// instead).
    MissingSound(String),
    /// A sound could not be read (a plain sound plays instead): which, and
    /// why.
    UnreadableSound {
        /// Which sound.
        sound: String,
        /// Why, as the system or the file reader put it.
        reason: String,
    },
    /// More tracks than can play at once: at most this many.
    TooManyTracks(usize),
    /// More effects than can run at once: at most this many.
    TooManyEffects(usize),
    /// A plugin of the song is not installed on this computer.
    PluginMissing(String),
    /// A plugin could not be started: which, and why.
    PluginFailed {
        /// Which plugin.
        plugin: String,
        /// Why, as the plugin or the system put it.
        reason: String,
    },
    /// A plugin's settings could not be given back to it, or taken from it.
    PluginSettings {
        /// Which plugin.
        plugin: String,
        /// Why, as the plugin put it.
        reason: String,
    },
}

/// How a song refers to a plugin found on this computer.
pub fn plugin_ref(info: &PluginInfo) -> daw_model::PluginRef {
    daw_model::PluginRef {
        format: match info.format {
            daw_plugin_host::Format::Clap => daw_model::PluginFormat::Clap,
            daw_plugin_host::Format::Vst3 => daw_model::PluginFormat::Vst3,
        },
        id: info.id.clone(),
        name: info.name.clone(),
        vendor: info.vendor.clone(),
        path: info.path.to_string_lossy().into_owned(),
    }
}

/// How long after loading a plugin the changes it reports are taken as part
/// of loading it.
const PLUGIN_SETTLING: Duration = Duration::from_secs(2);

/// What the song's plugins reported since the last look.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginNews {
    /// A plugin's settings changed in its editor: the song has unsaved
    /// changes.
    pub changed: bool,
    /// Parameters the person moved in a plugin's editor.
    pub touched: Vec<(PluginTarget, u32)>,
}

/// Room in the queue from the interface to the audio thread.
const COMMAND_QUEUE: usize = 4096;
/// Room in the queue from the audio thread back.
const OUTGOING_QUEUE: usize = 8192;

/// How the engine is set up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    /// Frames per second.
    pub sample_rate: u32,
    /// The largest block the processor renders at once; bigger requests are
    /// split. Memory for this many frames is reserved up front.
    pub max_block: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            max_block: 1024,
        }
    }
}

/// Room in the queue from MIDI keyboards to the audio thread.
const LIVE_QUEUE: usize = 2048;

/// Sends notes from MIDI keyboards straight to the audio thread, to the
/// track chosen with [`Engine::set_live_target`]. Cheap to share between
/// MIDI threads; the audio thread never waits for it.
pub struct MidiSender {
    producer: Mutex<rtrb::Producer<LiveNote>>,
}

impl MidiSender {
    /// A key went down (`velocity` from 0 to 1).
    pub fn note_on(&self, key: u8, velocity: f32) {
        self.push(LiveNote::Note {
            key,
            velocity: velocity.max(f32::MIN_POSITIVE),
        });
    }

    /// A key came up.
    pub fn note_off(&self, key: u8) {
        self.push(LiveNote::Note { key, velocity: 0.0 });
    }

    /// The sustain pedal went down (`true`) or came up. While it is down,
    /// released keys keep sounding, and recorded notes last until the pedal
    /// comes up, so a recording sounds the way it was played.
    pub fn sustain(&self, down: bool) {
        self.push(LiveNote::Sustain(down));
    }

    fn push(&self, note: LiveNote) {
        // Only MIDI threads take this lock; a full queue drops the note.
        let _ = self
            .producer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(note);
    }
}

/// What the recorder is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordState {
    /// Not recording.
    Idle,
    /// Counting in; the value is the ticks left before recording starts.
    CountIn(Ticks),
    /// Recording.
    Recording,
}

/// The interface's side of the engine: send it the project and transport
/// commands, read back the play position and recorded notes.
pub struct Engine {
    config: EngineConfig,
    commands: rtrb::Producer<Command>,
    outgoing: rtrb::Consumer<Outgoing>,
    shared: Arc<Shared>,
    library: SoundLibrary,
    /// Which track and instrument each slot holds.
    slots: Vec<Option<(TrackId, InstrumentSpec)>>,
    /// Which effect (and of which kind) each effect table entry holds.
    effects: Vec<Option<(EffectId, EffectKind)>>,
    compiled: Option<(u64, PlaybackScope)>,
    problems: Vec<Problem>,
    midi: Arc<MidiSender>,
    /// The main-thread halves of the song's plugins.
    plugins: Plugins,
    /// Plugins of the song that could not be loaded (not installed here).
    missing: HashSet<PluginTarget>,
    /// Try loading the missing plugins again at the next sync (more
    /// plugins were found).
    retry_missing: bool,
    /// When each plugin was loaded: just after, the changes it reports are
    /// about taking its settings, not changes made by the person.
    loaded_at: HashMap<PluginTarget, Instant>,
}

impl Engine {
    /// Creates the engine: this half for the interface, and the processor
    /// to hand to the audio thread.
    pub fn new(config: EngineConfig, library: SoundLibrary) -> (Self, EngineProcessor) {
        let (command_tx, command_rx) = rtrb::RingBuffer::new(COMMAND_QUEUE);
        let (outgoing_tx, outgoing_rx) = rtrb::RingBuffer::new(OUTGOING_QUEUE);
        let (live_tx, live_rx) = rtrb::RingBuffer::new(LIVE_QUEUE);
        let shared = Shared::new();
        let processor = EngineProcessor::new(
            config.sample_rate,
            config.max_block,
            command_rx,
            live_rx,
            outgoing_tx,
            Arc::clone(&shared),
        );
        let engine = Self {
            config,
            commands: command_tx,
            outgoing: outgoing_rx,
            shared,
            library,
            slots: vec![None; MAX_SLOTS],
            effects: vec![None; MAX_EFFECTS],
            compiled: None,
            problems: Vec::new(),
            midi: Arc::new(MidiSender {
                producer: Mutex::new(live_tx),
            }),
            plugins: Plugins::default(),
            missing: HashSet::new(),
            retry_missing: false,
            loaded_at: HashMap::new(),
        };
        (engine, processor)
    }

    /// The sample rate the engine runs at.
    pub fn config(&self) -> EngineConfig {
        self.config
    }

    fn send(&mut self, command: Command) {
        // The queue only fills if the audio thread has stopped; the command
        // is then dropped (and its memory freed here, safely).
        let _ = self.commands.push(command);
    }

    /// The sounds the engine plays from (clone it to render offline).
    pub fn library(&self) -> &SoundLibrary {
        &self.library
    }

    /// Brings the engine up to date with the project: builds instruments
    /// for new tracks, drops those of deleted tracks, creates and removes
    /// effects, and installs a freshly compiled program for what `scope`
    /// plays. Cheap when neither `revision` nor `scope` has changed.
    pub fn sync(&mut self, project: &Project, revision: u64, scope: PlaybackScope) {
        if self.compiled == Some((revision, scope)) {
            return;
        }
        self.compiled = Some((revision, scope));

        // Free slots whose track is gone.
        for slot in 0..MAX_SLOTS {
            if let Some((track, _)) = &self.slots[slot]
                && project.track(*track).is_none()
            {
                let track = *track;
                self.slots[slot] = None;
                self.plugins.retire(PluginTarget::Instrument(track));
                self.send(Command::Instrument {
                    slot,
                    instrument: None,
                });
            }
        }
        // Build instruments for new tracks, or tracks whose instrument changed.
        for track in &project.tracks {
            let current = self
                .slots
                .iter()
                .position(|s| s.as_ref().is_some_and(|(id, _)| *id == track.id));
            let retry =
                self.retry_missing && self.missing.contains(&PluginTarget::Instrument(track.id));
            let up_to_date = !retry
                && current.is_some_and(|slot| {
                    self.slots[slot]
                        .as_ref()
                        .is_some_and(|(_, spec)| *spec == track.instrument)
                });
            if up_to_date {
                continue;
            }
            // Same sound source, other settings (a drum pad's tune, say):
            // the playing instrument takes them without being rebuilt. A
            // plugin keeps playing as it is (its settings were just captured
            // from it).
            if let Some(slot) = current
                && !retry
                && self.slots[slot]
                    .as_ref()
                    .is_some_and(|(_, spec)| spec.same_source(&track.instrument))
            {
                let settings = library::instrument_settings(&track.instrument);
                if settings.is_some() || track.instrument.plugin().is_some() {
                    self.slots[slot] = Some((track.id, track.instrument.clone()));
                    if let Some(settings) = settings {
                        self.send(Command::InstrumentSettings {
                            slot,
                            settings: Box::new(settings),
                        });
                    }
                    continue;
                }
            }
            let Some(slot) = current.or_else(|| self.slots.iter().position(Option::is_none)) else {
                self.problems.push(Problem::TooManyTracks(MAX_SLOTS));
                continue;
            };
            let instrument = self.build_instrument(track);
            self.slots[slot] = Some((track.id, track.instrument.clone()));
            self.send(Command::Instrument {
                slot,
                instrument: Some(instrument),
            });
        }

        self.sync_effects(project);
        self.retry_missing = false;
        self.missing.retain(|target| match *target {
            PluginTarget::Instrument(track) => project
                .track(track)
                .is_some_and(|t| t.instrument.plugin().is_some()),
            PluginTarget::Effect(id) => project
                .effect(id)
                .is_some_and(|e| matches!(e.kind, EffectKind::Plugin(_))),
        });

        let view = program::scoped(project, scope);
        let slots = &self.slots;
        let effects = &self.effects;
        let program = program::compile(
            &view,
            |id| {
                slots
                    .iter()
                    .position(|s| s.as_ref().is_some_and(|(t, _)| *t == id))
            },
            |id| {
                effects
                    .iter()
                    .position(|e| e.as_ref().is_some_and(|(e, _)| *e == id))
            },
        );
        self.send(Command::Program(Box::new(program)));
    }

    /// A track's instrument, built (or loaded) anew. Whatever played for the
    /// track before goes: a plugin is retired.
    fn build_instrument(&mut self, track: &daw_model::Track) -> Box<dyn daw_dsp::Instrument> {
        let target = PluginTarget::Instrument(track.id);
        self.plugins.retire(target);
        self.missing.remove(&target);
        match &track.instrument {
            InstrumentSpec::Plugin(setup) => match self.load_plugin(target, setup) {
                Ok((instance, processor)) => Box::new(PluginInstrument::new(processor, instance)),
                Err(problem) => {
                    self.problems.push(problem);
                    self.missing.insert(target);
                    Box::new(Silence)
                }
            },
            spec => {
                let (instrument, problem) = self.library.build(spec, self.config.sample_rate);
                if let Some(problem) = problem {
                    self.problems.push(problem);
                }
                instrument
            }
        }
    }

    /// Creates the effects new in the project and removes the ones gone
    /// from it. (Their settings travel with the program.)
    fn sync_effects(&mut self, project: &Project) {
        let all: Vec<(EffectId, &EffectKind)> = project
            .tracks
            .iter()
            .flat_map(|t| t.effects.iter())
            .chain(project.master.effects.iter())
            .map(|e| (e.id, &e.kind))
            .collect();
        for index in 0..MAX_EFFECTS {
            let gone = self.effects[index].as_ref().is_some_and(|(id, kind)| {
                (self.retry_missing && self.missing.contains(&PluginTarget::Effect(*id)))
                    || !all
                        .iter()
                        .any(|(other, other_kind)| other == id && kind.same_kind(other_kind))
            });
            if gone {
                if let Some((id, _)) = self.effects[index].take() {
                    self.plugins.retire(PluginTarget::Effect(id));
                }
                self.send(Command::Effect {
                    index,
                    effect: None,
                });
            }
        }
        let bpm = project.tempo.initial_bpm();
        for (id, kind) in all {
            if self.effects.iter().flatten().any(|(e, _)| *e == id) {
                continue;
            }
            let Some(index) = self.effects.iter().position(Option::is_none) else {
                self.problems.push(Problem::TooManyEffects(MAX_EFFECTS));
                break;
            };
            let target = PluginTarget::Effect(id);
            self.missing.remove(&target);
            let slot = match kind {
                EffectKind::Plugin(setup) => match self.load_plugin(target, setup) {
                    Ok((instance, processor)) => Some(EffectSlot::Plugin(Box::new(
                        PluginEffect::new(processor, instance),
                    ))),
                    Err(problem) => {
                        // Left out of its chain (the sound passes by it).
                        self.problems.push(problem);
                        self.missing.insert(target);
                        None
                    }
                },
                builtin => program::effect_params(builtin, bpm).map(|params| {
                    EffectSlot::Builtin(Box::new(EffectInstance::new(
                        &params,
                        self.config.sample_rate,
                    )))
                }),
            };
            self.effects[index] = Some((id, kind.clone()));
            self.send(Command::Effect {
                index,
                effect: slot,
            });
        }
    }

    /// Forces the next [`Engine::sync`] to rebuild, even at the same revision.
    pub fn invalidate(&mut self) {
        self.compiled = None;
    }

    // ----- Plugins

    /// Makes the plugins found on this computer available to songs. Plugins
    /// a song needs that were missing are looked for again at the next
    /// [`Engine::sync`].
    pub fn set_plugins(&mut self, plugins: Vec<PluginInfo>) {
        self.library.register_plugins(plugins);
        if !self.missing.is_empty() {
            self.retry_missing = true;
            self.compiled = None;
        }
    }

    /// The plugins found on this computer.
    pub fn plugins(&self) -> &[PluginInfo] {
        self.library.plugins()
    }

    /// Whether a plugin of the song could not be loaded (it is not
    /// installed on this computer, or failed to start).
    pub fn is_plugin_missing(&self, target: PluginTarget) -> bool {
        self.missing.contains(&target)
    }

    /// Another song is about to be played: its plugins are loaded afresh,
    /// with their own settings (a track of the new song may have the same
    /// identity and plugin as one of the old).
    pub fn unload_plugins(&mut self) {
        for slot in 0..MAX_SLOTS {
            if let Some((track, spec)) = &self.slots[slot]
                && spec.plugin().is_some()
            {
                let track = *track;
                self.slots[slot] = None;
                self.plugins.retire(PluginTarget::Instrument(track));
                self.send(Command::Instrument {
                    slot,
                    instrument: None,
                });
            }
        }
        for index in 0..MAX_EFFECTS {
            if let Some((id, EffectKind::Plugin(_))) = &self.effects[index] {
                let id = *id;
                self.effects[index] = None;
                self.plugins.retire(PluginTarget::Effect(id));
                self.send(Command::Effect {
                    index,
                    effect: None,
                });
            }
        }
        self.missing.clear();
        self.compiled = None;
    }

    /// Loads a plugin for a place in the song, from its saved settings.
    fn load_plugin(
        &mut self,
        target: PluginTarget,
        setup: &PluginSetup,
    ) -> Result<(u64, daw_plugin_host::PluginProcessor), Problem> {
        let name = &setup.plugin.name;
        let info = self
            .library
            .find_plugin(&setup.plugin)
            .ok_or_else(|| Problem::PluginMissing(name.clone()))?;
        let LoadedPlugin {
            mut handle,
            processor,
        } = daw_plugin_host::load(
            &info,
            AudioConfig {
                sample_rate: self.config.sample_rate,
                max_block: self.config.max_block,
            },
        )
        .map_err(|reason| Problem::PluginFailed {
            plugin: name.clone(),
            reason,
        })?;
        if let Some(state) = &setup.state
            && let Err(reason) = handle.load_state(state.bytes())
        {
            self.problems.push(Problem::PluginSettings {
                plugin: name.clone(),
                reason,
            });
        }
        let instance = self.plugins.next_instance();
        self.plugins.live.insert(target, (instance, handle));
        self.loaded_at.insert(target, Instant::now());
        Ok((instance, processor))
    }

    /// The plugins' housekeeping (callbacks, timers, their editors' windows),
    /// and what they reported. Call every frame.
    pub fn plugin_idle(&mut self) -> PluginNews {
        let mut news = PluginNews::default();
        self.loaded_at
            .retain(|_, at| at.elapsed() < PLUGIN_SETTLING);
        for (target, (_, handle)) in &mut self.plugins.live {
            handle.idle();
            // Many plugins report a change just after taking their saved
            // settings; nobody can have changed anything in them yet.
            let settling = self.loaded_at.contains_key(target);
            news.changed |= handle.take_changed() && !settling;
            if let Some(param) = handle.take_touched_param() {
                news.touched.push((*target, param));
            }
        }
        news
    }

    /// Whether a place in the song has a loaded plugin.
    pub fn has_plugin(&self, target: PluginTarget) -> bool {
        self.plugins.live.contains_key(&target)
    }

    /// Whether a plugin has an editor of its own.
    pub fn plugin_has_editor(&mut self, target: PluginTarget) -> bool {
        self.plugins
            .live
            .get_mut(&target)
            .is_some_and(|(_, handle)| handle.has_editor())
    }

    /// Opens a plugin's editor in its own window titled `title`.
    pub fn open_plugin_editor(&mut self, target: PluginTarget, title: &str) -> Result<(), String> {
        match self.plugins.live.get_mut(&target) {
            Some((_, handle)) => handle.open_editor(title),
            None => Err("this plugin is not loaded".to_owned()),
        }
    }

    /// Closes a plugin's editor.
    pub fn close_plugin_editor(&mut self, target: PluginTarget) {
        if let Some((_, handle)) = self.plugins.live.get_mut(&target) {
            handle.close_editor();
        }
    }

    /// Whether a plugin's editor is open.
    pub fn is_plugin_editor_open(&self, target: PluginTarget) -> bool {
        self.plugins
            .live
            .get(&target)
            .is_some_and(|(_, handle)| handle.is_editor_open())
    }

    /// Whether any plugin's editor is open (the window then keeps drawing,
    /// so the editors are serviced).
    pub fn any_plugin_editor_open(&self) -> bool {
        self.plugins
            .live
            .values()
            .any(|(_, handle)| handle.is_editor_open())
    }

    /// A plugin's parameters.
    pub fn plugin_params(&mut self, target: PluginTarget) -> Vec<ParamInfo> {
        self.plugins
            .live
            .get_mut(&target)
            .map_or_else(Vec::new, |(_, handle)| handle.params())
    }

    /// A plugin parameter's current value, as a fraction of its range.
    pub fn plugin_param_value(&mut self, target: PluginTarget, id: u32) -> Option<f64> {
        self.plugins
            .live
            .get_mut(&target)
            .and_then(|(_, handle)| handle.param_value(id))
    }

    /// How a plugin writes a value of one of its parameters.
    pub fn plugin_param_text(
        &mut self,
        target: PluginTarget,
        id: u32,
        value: f64,
    ) -> Option<String> {
        self.plugins
            .live
            .get_mut(&target)
            .and_then(|(_, handle)| handle.param_text(id, value))
    }

    /// Whether any loaded plugin's settings differ from those `project`
    /// keeps for it. (Many plugins report a change just after taking their
    /// saved settings back; comparing tells a real change from that.)
    pub fn plugins_differ_from(&mut self, project: &Project) -> bool {
        self.plugins.live.iter_mut().any(|(target, (_, handle))| {
            let stored = match *target {
                PluginTarget::Instrument(track) => project
                    .track(track)
                    .and_then(|t| t.instrument.plugin())
                    .and_then(|setup| setup.state.as_ref()),
                PluginTarget::Effect(id) => match project.effect(id).map(|e| &e.kind) {
                    Some(EffectKind::Plugin(setup)) => setup.state.as_ref(),
                    _ => None,
                },
            };
            match (stored, handle.save_state()) {
                (Some(stored), Ok(now)) => stored.bytes() != now.as_slice(),
                _ => true,
            }
        })
    }

    /// Writes every loaded plugin's current settings into `project` (before
    /// saving or exporting it). Returns problems met.
    pub fn capture_plugin_states(&mut self, project: &mut Project) -> Vec<Problem> {
        let mut problems = Vec::new();
        for (target, (_, handle)) in &mut self.plugins.live {
            let bytes = match handle.save_state() {
                Ok(bytes) => bytes,
                Err(reason) => {
                    problems.push(Problem::PluginSettings {
                        plugin: handle.info().name.clone(),
                        reason,
                    });
                    continue;
                }
            };
            let state = PluginState::new(bytes);
            match *target {
                PluginTarget::Instrument(track) => {
                    if let Some(t) = project.track_mut(track)
                        && let InstrumentSpec::Plugin(setup) = &mut t.instrument
                    {
                        setup.state = Some(state);
                    }
                }
                PluginTarget::Effect(id) => project.update_effect(id, |effect| {
                    if let EffectKind::Plugin(setup) = &mut effect.kind {
                        setup.state = Some(state);
                    }
                }),
            }
        }
        problems
    }

    fn slot_of(&self, track: TrackId) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.as_ref().is_some_and(|(id, _)| *id == track))
    }

    /// Starts playback from the current position.
    pub fn play(&mut self) {
        self.send(Command::Play);
    }

    /// Stops playback (and recording). The position goes back to where
    /// playback started; stopping again goes back to the start of the song.
    pub fn stop(&mut self) {
        self.send(Command::Stop);
    }

    /// Moves the play position.
    pub fn locate(&mut self, at: Ticks) {
        self.send(Command::Locate(at));
    }

    /// Metronome settings.
    pub fn set_metronome(&mut self, during_playback: bool, during_recording: bool, volume: f32) {
        self.send(Command::Metronome {
            playback: during_playback,
            recording: during_recording,
            volume,
        });
    }

    /// Starts recording notes into `track` after `count_in_bars` bars of
    /// metronome.
    pub fn record(&mut self, track: TrackId, count_in_bars: u32) {
        if let Some(slot) = self.slot_of(track) {
            self.send(Command::Record {
                slot,
                count_in_bars,
            });
        }
    }

    /// Plays a note on a track now (from the computer keyboard, a MIDI
    /// keyboard, or a click in the editor).
    pub fn note_on(&mut self, track: TrackId, key: u8, velocity: f32) {
        if let Some(slot) = self.slot_of(track) {
            self.send(Command::NoteOn {
                slot,
                key,
                velocity,
            });
        }
    }

    /// Releases a note started with [`Engine::note_on`].
    pub fn note_off(&mut self, track: TrackId, key: u8) {
        if let Some(slot) = self.slot_of(track) {
            self.send(Command::NoteOff { slot, key });
        }
    }

    /// The sender MIDI keyboards use to play the live-input track.
    pub fn midi_sender(&self) -> Arc<MidiSender> {
        Arc::clone(&self.midi)
    }

    /// Chooses the track that MIDI keyboards play (and record into).
    pub fn set_live_target(&mut self, track: Option<TrackId>) {
        let slot = track.and_then(|t| self.slot_of(t)).unwrap_or(NO_TARGET);
        self.shared.live_target.store(slot, Ordering::Relaxed);
    }

    /// Silences everything immediately.
    pub fn panic(&mut self) {
        self.send(Command::Panic);
    }

    /// Collects what the audio thread sent since the last call, and frees
    /// memory it handed back.
    pub fn poll(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        while let Ok(message) = self.outgoing.pop() {
            match message {
                Outgoing::Event(event) => events.push(event),
                Outgoing::Garbage(garbage) => {
                    // A plugin's audio half came back: drop it, then its
                    // main half.
                    let instance = match &garbage {
                        Garbage::Instrument(instrument) => instrument.plugin_instance(),
                        Garbage::Effect(effect) => effect.plugin_instance(),
                        _ => None,
                    };
                    drop(garbage);
                    if let Some(instance) = instance {
                        self.plugins.returned(instance);
                    }
                }
            }
        }
        events
    }

    /// Problems found while preparing instruments (missing sounds, for
    /// instance) since the last call.
    pub fn take_problems(&mut self) -> Vec<Problem> {
        std::mem::take(&mut self.problems)
    }

    /// The play position, as of the last audio block.
    pub fn position(&self) -> Ticks {
        Ticks(self.shared.position.load(Ordering::Relaxed))
    }

    /// Whether the transport is playing.
    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// What the recorder is doing.
    pub fn record_state(&self) -> RecordState {
        match self.shared.recording.load(Ordering::Relaxed) {
            RECORD_COUNT_IN => {
                RecordState::CountIn(Ticks(self.shared.count_in_left.load(Ordering::Relaxed)))
            }
            RECORD_ACTIVE => RecordState::Recording,
            _ => RecordState::Idle,
        }
    }

    /// How busy the audio thread is: the share of each block's time spent
    /// computing it (1.0 means it barely kept up).
    pub fn load(&self) -> f32 {
        Shared::load_f32(&self.shared.load)
    }

    /// The loudest sample of the last block (above 1.0 means clipping).
    pub fn peak(&self) -> f32 {
        Shared::load_f32(&self.shared.peak)
    }

    /// The loudest sample of a track's output in the last block, after its
    /// effects, volume and pan.
    pub fn track_peak(&self, track: TrackId) -> f32 {
        self.slot_of(track)
            .map_or(0.0, |slot| Shared::load_f32(&self.shared.meters[slot]))
    }
}

/// Renders a project offline, block by block, through exactly the same
/// processing as live playback (for exporting).
pub struct OfflineRender {
    // The audio half goes first: a plugin's audio half is dropped before
    // its main half.
    processor: EngineProcessor,
    engine: Engine,
    buffer: Vec<f32>,
    frames: u64,
}

impl OfflineRender {
    /// Prepares to play the whole song from the start.
    pub fn new(project: &Project, library: SoundLibrary, config: EngineConfig) -> Self {
        let (mut engine, processor) = Engine::new(config, library);
        engine.set_metronome(false, false, 0.0);
        engine.sync(project, 0, PlaybackScope::Song);
        engine.play();
        Self {
            engine,
            processor,
            buffer: vec![0.0; config.max_block * 2],
            frames: 0,
        }
    }

    /// Problems met while preparing the instruments (a missing sound).
    pub fn take_problems(&mut self) -> Vec<Problem> {
        self.engine.take_problems()
    }

    /// Renders the next `left.len()` frames (both slices the same length).
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let block = self.buffer.len() / 2;
        let mut done = 0;
        while done < left.len() {
            let n = block.min(left.len() - done);
            self.processor.process(&mut self.buffer[..n * 2], 2);
            for (i, frame) in self.buffer[..n * 2].as_chunks::<2>().0.iter().enumerate() {
                left[done + i] = frame[0];
                right[done + i] = frame[1];
            }
            done += n;
            // (No plugin housekeeping here: an export has no plugin windows,
            // and some plugins' timers wait for the program's main thread,
            // which may be waiting for the export.)
            self.engine.poll();
        }
        self.frames += left.len() as u64;
    }

    /// Frames rendered so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }
}

/// Renders a project offline, through exactly the same processing as live
/// playback: plays it from the start for `seconds` and returns the stereo
/// result as (left, right).
pub fn render_offline(
    project: &Project,
    library: SoundLibrary,
    config: EngineConfig,
    seconds: f64,
) -> (Vec<f32>, Vec<f32>) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive duration"
    )]
    let frames = (seconds * f64::from(config.sample_rate)).ceil() as usize;
    let mut render = OfflineRender::new(
        project,
        library,
        EngineConfig {
            max_block: 256.min(config.max_block),
            ..config
        },
    );
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    render.render(&mut left, &mut right);
    (left, right)
}

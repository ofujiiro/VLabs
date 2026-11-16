// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod library;
mod messages;
mod processor;
mod program;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use daw_dsp::effects::Effect as EffectInstance;
use daw_model::{EffectId, EffectKind, Instrument as InstrumentSpec, Project, Ticks, TrackId};

pub use library::SoundLibrary;
pub use messages::EngineEvent;
pub use processor::{EngineProcessor, MAX_EFFECTS, MAX_SLOTS};
pub use program::PlaybackScope;

use messages::{Command, LiveNote, NO_TARGET, Outgoing, RECORD_ACTIVE, RECORD_COUNT_IN, Shared};

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
    problems: Vec<String>,
    midi: Arc<MidiSender>,
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
                self.slots[slot] = None;
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
            let up_to_date = current.is_some_and(|slot| {
                self.slots[slot]
                    .as_ref()
                    .is_some_and(|(_, spec)| *spec == track.instrument)
            });
            if up_to_date {
                continue;
            }
            // Same sound source, other settings (a drum pad's tune, say):
            // the playing instrument takes them without being rebuilt.
            if let Some(slot) = current
                && self.slots[slot]
                    .as_ref()
                    .is_some_and(|(_, spec)| spec.same_source(&track.instrument))
                && let Some(settings) = library::instrument_settings(&track.instrument)
            {
                self.slots[slot] = Some((track.id, track.instrument.clone()));
                self.send(Command::InstrumentSettings {
                    slot,
                    settings: Box::new(settings),
                });
                continue;
            }
            let Some(slot) = current.or_else(|| self.slots.iter().position(Option::is_none)) else {
                self.problems
                    .push(format!("too many tracks: at most {MAX_SLOTS} can play"));
                continue;
            };
            let (instrument, problem) = self
                .library
                .build(&track.instrument, self.config.sample_rate);
            if let Some(problem) = problem {
                self.problems.push(problem);
            }
            self.slots[slot] = Some((track.id, track.instrument.clone()));
            self.send(Command::Instrument {
                slot,
                instrument: Some(instrument),
            });
        }

        self.sync_effects(project);

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
                !all.iter()
                    .any(|(other, other_kind)| other == id && kind.same_kind(other_kind))
            });
            if gone {
                self.effects[index] = None;
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
                self.problems
                    .push(format!("too many effects: at most {MAX_EFFECTS} can run"));
                break;
            };
            let params = program::effect_params(kind, bpm);
            let instance = EffectInstance::new(&params, self.config.sample_rate);
            self.effects[index] = Some((id, kind.clone()));
            self.send(Command::Effect {
                index,
                effect: Some(Box::new(instance)),
            });
        }
    }

    /// Forces the next [`Engine::sync`] to rebuild, even at the same revision.
    pub fn invalidate(&mut self) {
        self.compiled = None;
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
                Outgoing::Garbage(garbage) => drop(garbage),
            }
        }
        events
    }

    /// Problems found while preparing instruments (missing sounds, for
    /// instance) since the last call.
    pub fn take_problems(&mut self) -> Vec<String> {
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
    engine: Engine,
    processor: EngineProcessor,
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
    pub fn take_problems(&mut self) -> Vec<String> {
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

// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod library;
mod messages;
mod processor;
mod program;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use daw_model::{Instrument as InstrumentSpec, Project, Ticks, TrackId};

pub use library::SoundLibrary;
pub use messages::EngineEvent;
pub use processor::{EngineProcessor, MAX_SLOTS};

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
    compiled_revision: Option<u64>,
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
            compiled_revision: None,
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

    /// Brings the engine up to date with the project: builds instruments
    /// for new tracks, drops those of deleted tracks, and installs a freshly
    /// compiled program. Cheap when `revision` has not changed.
    pub fn sync(&mut self, project: &Project, revision: u64) {
        if self.compiled_revision == Some(revision) {
            return;
        }
        self.compiled_revision = Some(revision);

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

        let slots = &self.slots;
        let program = program::compile(project, |id| {
            slots
                .iter()
                .position(|s| s.as_ref().is_some_and(|(t, _)| *t == id))
        });
        self.send(Command::Program(Box::new(program)));
    }

    /// Forces the next [`Engine::sync`] to rebuild, even at the same revision.
    pub fn invalidate(&mut self) {
        self.compiled_revision = None;
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
    let (mut engine, mut processor) = Engine::new(config, library);
    engine.sync(project, 0);
    engine.play();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive duration"
    )]
    let frames = (seconds * f64::from(config.sample_rate)).ceil() as usize;
    let block = 256.min(config.max_block);
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    let mut buffer = vec![0.0_f32; block * 2];
    let mut done = 0;
    while done < frames {
        let n = block.min(frames - done);
        processor.process(&mut buffer[..n * 2], 2);
        for frame in buffer[..n * 2].as_chunks::<2>().0 {
            left.push(frame[0]);
            right.push(frame[1]);
        }
        done += n;
        engine.poll();
    }
    (left, right)
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! What the interface and the audio thread tell each other.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicU32, AtomicUsize, Ordering};

use daw_dsp::{Instrument, InstrumentSettings};
use daw_model::Ticks;

use crate::plugins::EffectSlot;
use crate::program::Program;

/// From the interface to the audio thread.
pub(crate) enum Command {
    /// A newly compiled program replaces the current one.
    Program(Box<Program>),
    /// Puts an instrument into a slot (or empties it).
    Instrument {
        slot: usize,
        instrument: Option<Box<dyn Instrument>>,
    },
    /// New settings for the instrument in a slot (it keeps playing).
    InstrumentSettings {
        slot: usize,
        settings: Box<InstrumentSettings>,
    },
    /// Puts an effect into the effect table (or empties the entry).
    Effect {
        index: usize,
        effect: Option<EffectSlot>,
    },
    Play,
    Stop,
    Locate(Ticks),
    Metronome {
        playback: bool,
        recording: bool,
        volume: f32,
    },
    /// Starts recording into `slot` after a count-in.
    Record {
        slot: usize,
        count_in_bars: u32,
    },
    NoteOn {
        slot: usize,
        key: u8,
        velocity: f32,
    },
    NoteOff {
        slot: usize,
        key: u8,
    },
    /// Silences everything at once.
    Panic,
}

/// From the audio thread to the interface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineEvent {
    /// The count-in is over: recording has started.
    RecordingStarted {
        /// Where recording started.
        at: Ticks,
    },
    /// A key went down while recording (shown at once, before it ends).
    RecordingNoteStarted {
        /// The key.
        key: u8,
        /// How hard, 0 to 1.
        velocity: f32,
        /// Where it started.
        start: Ticks,
    },
    /// A complete recorded note.
    RecordedNote {
        /// The key.
        key: u8,
        /// How hard, 0 to 1.
        velocity: f32,
        /// Where it started.
        start: Ticks,
        /// Where it ended.
        end: Ticks,
    },
    /// Recording ended (the transport stopped).
    RecordingStopped,
    /// The transport stopped.
    Stopped,
}

/// What a MIDI keyboard sends to the track that receives live input.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LiveNote {
    /// A key went down, or up when `velocity` is 0.
    Note { key: u8, velocity: f32 },
    /// The sustain pedal went down (`true`) or came up.
    Sustain(bool),
}

/// No track receives live input.
pub(crate) const NO_TARGET: usize = usize::MAX;

/// Everything the audio thread sends back.
pub(crate) enum Outgoing {
    Event(EngineEvent),
    /// Memory the audio thread no longer needs, to be freed elsewhere.
    Garbage(Garbage),
}

pub(crate) enum Garbage {
    // Never read: these exist only to be dropped off the audio thread.
    #[allow(
        dead_code,
        reason = "held only so it is dropped on the interface thread"
    )]
    Program(Box<Program>),
    /// An instrument (a plugin's audio half lets its main half go too).
    Instrument(Box<dyn Instrument>),
    #[allow(
        dead_code,
        reason = "held only so it is dropped on the interface thread"
    )]
    Settings(Box<InstrumentSettings>),
    /// An effect (a plugin's audio half lets its main half go too).
    Effect(EffectSlot),
}

/// Recording states, as stored in [`Shared::recording`].
pub(crate) const RECORD_IDLE: u8 = 0;
pub(crate) const RECORD_COUNT_IN: u8 = 1;
pub(crate) const RECORD_ACTIVE: u8 = 2;

/// Values the audio thread publishes for the interface to read at any time.
pub(crate) struct Shared {
    pub position: AtomicI64,
    pub playing: AtomicBool,
    pub recording: AtomicU8,
    /// Ticks left in the count-in.
    pub count_in_left: AtomicI64,
    /// Share of the available time the last block took, as f32 bits.
    pub load: AtomicU32,
    /// Peak output level of the last block, as f32 bits.
    pub peak: AtomicU32,
    /// The instrument slot that MIDI keyboards play ([`NO_TARGET`] for none).
    pub live_target: AtomicUsize,
    /// Peak output level of each slot's track in the last block (after its
    /// effects, volume and pan), as f32 bits.
    pub meters: Box<[AtomicU32]>,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            position: AtomicI64::new(0),
            playing: AtomicBool::new(false),
            recording: AtomicU8::new(RECORD_IDLE),
            count_in_left: AtomicI64::new(0),
            load: AtomicU32::new(0),
            peak: AtomicU32::new(0),
            live_target: AtomicUsize::new(NO_TARGET),
            meters: (0..crate::processor::MAX_SLOTS)
                .map(|_| AtomicU32::new(0))
                .collect(),
        })
    }

    pub fn store_f32(cell: &AtomicU32, value: f32) {
        cell.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn load_f32(cell: &AtomicU32) -> f32 {
        f32::from_bits(cell.load(Ordering::Relaxed))
    }
}

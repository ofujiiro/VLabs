// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugins in the engine (decision 0015): the audio-thread parts that play a
//! plugin as a track's instrument or as an effect in a chain, and the
//! main-thread record of every loaded plugin's other half.
//!
//! Live notes and parameter changes arriving between blocks wait in a list
//! reserved up front and go to the plugin with the next block, together with
//! the song's notes at their exact frames.

use std::collections::HashMap;

use daw_dsp::effects::{Effect, EffectParams};
use daw_dsp::{BlockEvent, BlockEventKind, Control, Instrument, SongPosition};
use daw_model::{EffectId, TrackId};
use daw_plugin_host::{Event, MAX_EVENTS, PluginHandle, PluginProcessor, Transport};

/// Which plugin, by where it sits in the song.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PluginTarget {
    /// A track's instrument.
    Instrument(TrackId),
    /// An effect.
    Effect(EffectId),
}

/// The song's position as plugins take it.
fn transport(position: &SongPosition) -> Transport {
    Transport {
        playing: position.playing,
        tempo: position.tempo,
        beats: position.beats,
        bar_start: position.bar_start,
        bar: position.bar,
        numerator: position.numerator,
        denominator: position.denominator,
        frames: position.frames,
    }
}

/// Events waiting for the next block, and the block's full list.
struct Queues {
    pending: Vec<Event>,
    block: Vec<Event>,
    held: [bool; 128],
}

impl Queues {
    fn new() -> Self {
        Self {
            pending: Vec::with_capacity(MAX_EVENTS),
            block: Vec::with_capacity(MAX_EVENTS),
            held: [false; 128],
        }
    }

    /// Queues an event for the next block (dropped if the list is full).
    fn push(&mut self, event: Event) {
        if self.pending.len() < self.pending.capacity() {
            self.pending.push(event);
        }
        match event {
            Event::NoteOn { key, .. } => self.held[usize::from(key & 0x7F)] = true,
            Event::NoteOff { key, .. } => self.held[usize::from(key & 0x7F)] = false,
            Event::Param { .. }
            | Event::PitchBend { .. }
            | Event::Controller { .. }
            | Event::Pressure { .. } => {}
        }
    }

    /// The block's events: what waited (at its start), then the block's
    /// own `events` at their frames.
    fn collect(&mut self, events: &[BlockEvent]) -> &[Event] {
        self.block.clear();
        for event in self.pending.drain(..) {
            if self.block.len() < self.block.capacity() {
                self.block.push(event);
            }
        }
        for event in events {
            if self.block.len() == self.block.capacity() {
                break;
            }
            #[allow(clippy::cast_possible_truncation, reason = "a frame within one block")]
            let frame = event.offset as u32;
            self.block.push(match event.kind {
                BlockEventKind::NoteOn { key, velocity } => {
                    self.held[usize::from(key & 0x7F)] = true;
                    Event::NoteOn {
                        frame,
                        key,
                        velocity,
                    }
                }
                BlockEventKind::NoteOff { key } => {
                    self.held[usize::from(key & 0x7F)] = false;
                    Event::NoteOff { frame, key }
                }
                BlockEventKind::Control(control) => control_event(frame, control),
            });
        }
        &self.block
    }
}

/// A controller's move as a plugin takes it (decision 0033).
fn control_event(frame: u32, control: Control) -> Event {
    match control.sane() {
        Control::PitchBend(value) => Event::PitchBend { frame, value },
        Control::Controller { number, value } => Event::Controller {
            frame,
            number,
            value,
        },
        Control::Pressure(value) => Event::Pressure { frame, value },
    }
}

/// A plugin playing a track's instrument.
pub(crate) struct PluginInstrument {
    processor: PluginProcessor,
    instance: u64,
    queues: Queues,
    transport: Transport,
}

impl PluginInstrument {
    pub fn new(processor: PluginProcessor, instance: u64) -> Self {
        Self {
            processor,
            instance,
            queues: Queues::new(),
            transport: Transport::default(),
        }
    }
}

impl Instrument for PluginInstrument {
    fn note_on(&mut self, key: u8, velocity: f32) {
        self.queues.push(Event::NoteOn {
            frame: 0,
            key,
            velocity,
        });
    }

    fn note_off(&mut self, key: u8) {
        self.queues.push(Event::NoteOff { frame: 0, key });
    }

    fn all_notes_off(&mut self, _immediate: bool) {
        for key in 0..128_u8 {
            if self.queues.held[usize::from(key)] {
                self.queues.push(Event::NoteOff { frame: 0, key });
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.render_block(&[], left, right);
    }

    fn render_block(&mut self, events: &[BlockEvent], left: &mut [f32], right: &mut [f32]) {
        // An instrument's input is silence.
        left.fill(0.0);
        right.fill(0.0);
        let events = self.queues.collect(events);
        self.processor.process(&self.transport, events, left, right);
    }

    /// Plugins take every controller (decision 0033): their own MIDI learn
    /// gives knobs their jobs.
    fn control(&mut self, control: Control) {
        self.queues.push(control_event(0, control));
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        self.queues.push(Event::Param {
            frame: 0,
            id,
            value,
        });
    }

    fn set_position(&mut self, position: &SongPosition) {
        self.transport = transport(position);
    }

    fn retire(&mut self) {
        self.processor.stop();
    }

    fn plugin_instance(&self) -> Option<u64> {
        Some(self.instance)
    }
}

/// A plugin as an effect in a chain.
pub(crate) struct PluginEffect {
    processor: PluginProcessor,
    instance: u64,
    queues: Queues,
    transport: Transport,
}

impl PluginEffect {
    pub fn new(processor: PluginProcessor, instance: u64) -> Self {
        Self {
            processor,
            instance,
            queues: Queues::new(),
            transport: Transport::default(),
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let events = self.queues.collect(&[]);
        self.processor.process(&self.transport, events, left, right);
    }
}

/// An entry of the engine's effect table: a built-in effect or a plugin.
pub(crate) enum EffectSlot {
    Builtin(Box<Effect>),
    Plugin(Box<PluginEffect>),
}

impl EffectSlot {
    /// Processes a block in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        match self {
            Self::Builtin(effect) => effect.process(left, right),
            Self::Plugin(effect) => effect.process(left, right),
        }
    }

    /// New settings (built-in effects only).
    pub fn set_params(&mut self, params: &EffectParams) {
        if let Self::Builtin(effect) = self {
            effect.set_params(params);
        }
    }

    /// A plugin parameter changes, from the next block.
    pub fn set_parameter(&mut self, id: u32, value: f64) {
        if let Self::Plugin(effect) = self {
            effect.queues.push(Event::Param {
                frame: 0,
                id,
                value,
            });
        }
    }

    /// Where the song is (plugins follow the tempo).
    pub fn set_position(&mut self, position: &SongPosition) {
        if let Self::Plugin(effect) = self {
            effect.transport = transport(position);
        }
    }

    /// Before being handed back to be dropped (on the audio thread).
    pub fn retire(&mut self) {
        if let Self::Plugin(effect) = self {
            effect.processor.stop();
        }
    }

    /// For a plugin: which loaded plugin this is.
    pub fn plugin_instance(&self) -> Option<u64> {
        match self {
            Self::Builtin(_) => None,
            Self::Plugin(effect) => Some(effect.instance),
        }
    }
}

/// An instrument that makes no sound: stands in for a plugin that cannot be
/// found on this computer, so its notes are not played by something else.
pub(crate) struct Silence;

impl Instrument for Silence {
    fn note_on(&mut self, _key: u8, _velocity: f32) {}
    fn note_off(&mut self, _key: u8) {}
    fn all_notes_off(&mut self, _immediate: bool) {}
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
    }
    fn is_sounding(&self) -> bool {
        false
    }
}

/// The main-thread halves of every loaded plugin.
#[derive(Default)]
pub(crate) struct Plugins {
    /// Plugins in the song now, by where they sit.
    pub live: HashMap<PluginTarget, (u64, PluginHandle)>,
    /// Plugins taken out of the song, waiting for their audio half to come
    /// back from the audio thread before both are dropped.
    pub retiring: HashMap<u64, PluginHandle>,
    next: u64,
}

impl Plugins {
    /// A new instance number.
    pub fn next_instance(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// The plugin at a place is taken out of the song.
    pub fn retire(&mut self, target: PluginTarget) {
        if let Some((instance, handle)) = self.live.remove(&target) {
            self.retiring.insert(instance, handle);
        }
    }

    /// An audio half came back: its main half can go too.
    pub fn returned(&mut self, instance: u64) {
        self.retiring.remove(&instance);
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The audio thread's side of the engine.
//!
//! Everything here runs inside a real-time region: no allocation, no locks,
//! no files, no logging. All memory is reserved when the processor is
//! created; programs and instruments arrive ready-made through the command
//! queue, and old ones leave through the outgoing queue to be freed on the
//! interface thread.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use daw_dsp::{BlockEvent, BlockEventKind, Click, Control, Instrument, SongPosition};
use daw_model::{ControlKind, Region, TICKS_PER_QUARTER, Ticks};

use crate::messages::{
    Command, EngineEvent, Garbage, LiveNote, NO_TARGET, Outgoing, RECORD_ACTIVE, RECORD_COUNT_IN,
    RECORD_IDLE, RecordedChunk, Shared,
};
use crate::plugins::EffectSlot;
use crate::program::{LaneTarget, Program, pan_gains};

/// How many instrument slots (tracks) the engine can play.
pub const MAX_SLOTS: usize = 256;

/// How many effects the engine can run at once, over all chains.
pub const MAX_EFFECTS: usize = 1024;

/// Events handled per instrument per stretch of audio; more are applied at
/// the end of the stretch (never reached in practice).
const MAX_EVENTS: usize = 512;

/// The most commands handled per audio block, so a flood of messages cannot
/// make one block late.
const MAX_COMMANDS_PER_BLOCK: usize = 256;

/// Something an instrument is to do at a frame of the stretch being
/// rendered.
#[derive(Clone, Copy)]
struct Scheduled {
    offset: usize,
    kind: BlockEventKind,
}

impl Scheduled {
    const fn on(offset: usize, key: u8, velocity: f32) -> Self {
        Self {
            offset,
            kind: BlockEventKind::NoteOn { key, velocity },
        }
    }

    const fn off(offset: usize, key: u8) -> Self {
        Self {
            offset,
            kind: BlockEventKind::NoteOff { key },
        }
    }

    /// The order of events at the same frame: ends first, then controllers
    /// (so a note starting with a bend starts bent), then starts.
    fn order(&self) -> (usize, u8) {
        let rank = match self.kind {
            BlockEventKind::NoteOff { .. } => 0,
            BlockEventKind::Control(_) => 1,
            BlockEventKind::NoteOn { .. } => 2,
        };
        (self.offset, rank)
    }
}

const NO_EVENT: Scheduled = Scheduled::off(0, 0);

/// No key held from a MIDI keyboard (see `EngineProcessor::live_keys`).
const NO_SLOT: u16 = u16::MAX;

/// How often a pattern's curves are looked at within a block, in frames
/// (decision 0033): a bend moves in steps of 1.3 ms at 48 kHz.
const CONTROL_STEP: usize = 64;

/// The controllers, numbered: 0 to 127 the numbered controllers, then
/// pressure, then the pitch wheel.
const CONTROLS: usize = 130;
const PRESSURE: usize = 128;
const PITCH_BEND: usize = 129;

/// A controller's number in the processor's tables.
fn control_index(control: ControlKind) -> usize {
    match control {
        ControlKind::Controller(number) => usize::from(number & 0x7F),
        ControlKind::Pressure => PRESSURE,
        ControlKind::PitchBend => PITCH_BEND,
    }
}

/// The controller with a number of the processor's tables.
fn control_kind(index: usize) -> ControlKind {
    match index {
        PITCH_BEND => ControlKind::PitchBend,
        PRESSURE => ControlKind::Pressure,
        #[allow(clippy::cast_possible_truncation, reason = "below 128")]
        number => ControlKind::Controller((number & 0x7F) as u8),
    }
}

/// A curve's value as an instrument takes it.
fn dsp_control(control: ControlKind, value: f32) -> Control {
    match control {
        ControlKind::PitchBend => Control::PitchBend(value),
        ControlKind::Controller(number) => Control::Controller { number, value },
        ControlKind::Pressure => Control::Pressure(value),
    }
}

/// An instrument's controller as a curve's.
fn model_control(control: Control) -> (ControlKind, f32) {
    match control {
        Control::PitchBend(value) => (ControlKind::PitchBend, value),
        Control::Controller { number, value } => (ControlKind::Controller(number), value),
        Control::Pressure(value) => (ControlKind::Pressure, value),
    }
}

/// Per-slot playback state.
struct SlotState {
    /// For each key sounding because of the song: where its note ends.
    held: [Option<Ticks>; 128],
    /// Whether the sustain pedal is down.
    sustain: bool,
    /// Keys played live and released while the pedal was down: they keep
    /// sounding (and recording) until the pedal comes up.
    sustained: [bool; 128],
    /// Whether a MIDI keyboard's wheels or pedals moved this instrument
    /// since it was last put back to rest (decision 0033).
    live_moved: bool,
}

impl SlotState {
    fn new() -> Self {
        Self {
            held: [None; 128],
            sustain: false,
            sustained: [false; 128],
            live_moved: false,
        }
    }
}

#[derive(Clone, Copy)]
enum Recording {
    Idle,
    /// Counting in, before recording notes into a slot, or sound (`None`).
    CountIn {
        slot: Option<usize>,
        /// Count-in position, in ticks from its start.
        position: f64,
        length: f64,
        bpm_ticks_per_second: f64,
        bar: f64,
        beat: f64,
    },
    /// Recording notes into a slot.
    Active {
        slot: usize,
    },
    /// Recording sound from the input.
    Audio,
}

/// The audio thread's side of the engine. Call [`EngineProcessor::process`]
/// from the sound card's callback (or in a loop, for offline rendering).
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches: playing, the two metronome settings, settling gains"
)]
pub struct EngineProcessor {
    sample_rate: f64,
    max_block: usize,
    commands: rtrb::Consumer<Command>,
    live: rtrb::Consumer<LiveNote>,
    /// The slot MIDI keyboards played at the last block ([`NO_TARGET`] for
    /// none): when the focus moves, the slot left behind goes back to rest.
    live_slot: usize,
    /// For each key held down on a MIDI keyboard, the slot it went down on
    /// ([`NO_SLOT`] for none): it comes up there, wherever the focus is by
    /// then.
    live_keys: [u16; 128],
    /// Where each controller of the MIDI keyboards was last moved to (NaN
    /// if never), so a take starts with them where they are.
    live_values: [f32; CONTROLS],
    /// The controllers the hand has moved during the take being recorded:
    /// the pattern's old curves for them are not played meanwhile.
    live_override: [bool; CONTROLS],
    /// The controllers' moves to record at the end of this block's input
    /// (NaN for none): one point per controller per block at most.
    recorded_controls: [f32; CONTROLS],
    outgoing: rtrb::Producer<Outgoing>,
    shared: Arc<Shared>,
    program: Option<Box<Program>>,
    /// Next note to start, per program track.
    cursors: Vec<usize>,
    instruments: Vec<Option<Box<dyn Instrument>>>,
    slots: Vec<SlotState>,
    playing: bool,
    position: f64,
    play_started_at: f64,
    /// Volume and pan take their values at once in the next block that plays
    /// the song (after starting or moving), instead of gliding to them.
    settle_gains: bool,
    recording: Recording,
    recorded_held: [Option<(Ticks, f32)>; 128],
    metronome_playback: bool,
    metronome_recording: bool,
    click: Click,
    scratch_left: Vec<f32>,
    scratch_right: Vec<f32>,
    mix_left: Vec<f32>,
    mix_right: Vec<f32>,
    /// The metronome, kept apart so the master effects never touch it.
    click_left: Vec<f32>,
    click_right: Vec<f32>,
    events: Vec<Scheduled>,
    /// The song's notes and controllers of one stretch, as instruments
    /// take them.
    notes: Vec<BlockEvent>,
    /// Every effect, by its table entry (chains refer to entries).
    effects: Vec<Option<EffectSlot>>,
    /// The loudest output of each slot's track in this block.
    meter_peaks: Vec<f32>,
    /// Garbage that could not be sent yet because the queue was full.
    pending: Option<Garbage>,
    /// Sound from the input device, as stereo frames.
    input: rtrb::Consumer<f32>,
    /// The take of sound being recorded.
    take: Take,
    /// The sound library's preview, playing over the song.
    preview: Option<Preview>,
}

/// How loud the library's preview plays (about -3 dB).
const PREVIEW_GAIN: f32 = 0.7;

/// A sound from the library playing once, over the song (decision 0023).
struct Preview {
    sound: Arc<daw_dsp::audio::AudioBuffer>,
    /// The next frame to play.
    at: usize,
}

/// A take of sound on its way to the recorder.
struct Take {
    /// Where the sound of takes goes, chunk by chunk.
    recorded: rtrb::Producer<RecordedChunk>,
    /// The chunk being filled.
    chunk: RecordedChunk,
    /// The take being recorded (or the last one).
    number: u32,
    /// Its frames so far: passed on, and lost because the recorder fell
    /// behind.
    frames: u64,
    dropped: u64,
}

impl Take {
    /// Starts the next take.
    fn begin(&mut self) {
        self.number = self.number.wrapping_add(1);
        self.frames = 0;
        self.dropped = 0;
        self.chunk.clear();
    }

    fn push(&mut self, left: f32, right: f32) {
        if self.chunk.push(left, right) {
            self.flush();
        }
    }

    /// Passes the chunk being filled on to the recorder.
    fn flush(&mut self) {
        let frames = self.chunk.frames() as u64;
        if frames == 0 {
            return;
        }
        self.chunk.take = self.number;
        self.chunk.first = self.frames + self.dropped;
        if self.recorded.push(self.chunk).is_ok() {
            self.frames += frames;
        } else {
            self.dropped += frames;
        }
        self.chunk.clear();
    }
}

impl EngineProcessor {
    #[allow(
        clippy::too_many_arguments,
        reason = "the queues the processor is connected by"
    )]
    pub(crate) fn new(
        sample_rate: u32,
        max_block: usize,
        commands: rtrb::Consumer<Command>,
        live: rtrb::Consumer<LiveNote>,
        outgoing: rtrb::Producer<Outgoing>,
        shared: Arc<Shared>,
        input: rtrb::Consumer<f32>,
        recorded: rtrb::Producer<RecordedChunk>,
    ) -> Self {
        let max_block = max_block.max(1);
        Self {
            sample_rate: f64::from(sample_rate),
            max_block,
            commands,
            live,
            live_slot: NO_TARGET,
            live_keys: [NO_SLOT; 128],
            live_values: [f32::NAN; CONTROLS],
            live_override: [false; CONTROLS],
            recorded_controls: [f32::NAN; CONTROLS],
            outgoing,
            shared,
            program: None,
            cursors: Vec::with_capacity(MAX_SLOTS),
            instruments: (0..MAX_SLOTS).map(|_| None).collect(),
            slots: (0..MAX_SLOTS).map(|_| SlotState::new()).collect(),
            playing: false,
            position: 0.0,
            play_started_at: 0.0,
            settle_gains: true,
            recording: Recording::Idle,
            recorded_held: [None; 128],
            metronome_playback: false,
            metronome_recording: true,
            click: Click::new(sample_rate),
            scratch_left: vec![0.0; max_block],
            scratch_right: vec![0.0; max_block],
            mix_left: vec![0.0; max_block],
            mix_right: vec![0.0; max_block],
            click_left: vec![0.0; max_block],
            click_right: vec![0.0; max_block],
            events: vec![NO_EVENT; MAX_EVENTS],
            notes: Vec::with_capacity(MAX_EVENTS),
            effects: (0..MAX_EFFECTS).map(|_| None).collect(),
            meter_peaks: vec![0.0; MAX_SLOTS],
            pending: None,
            preview: None,
            input,
            take: Take {
                recorded,
                chunk: RecordedChunk::EMPTY,
                number: 0,
                frames: 0,
                dropped: 0,
            },
        }
    }

    /// Produces the next block of sound into `output`: interleaved frames of
    /// `channels` samples. Stereo goes to the first two channels; a mono
    /// output gets the average.
    pub fn process(&mut self, output: &mut [f32], channels: usize) {
        let _real_time = daw_rt::RtGuard::enter();
        let started = Instant::now();
        let channels = channels.max(1);
        self.handle_commands();
        self.drain_input();

        let frames = output.len() / channels;
        let mut peak = 0.0_f32;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(self.max_block);
            self.mix_left[..n].fill(0.0);
            self.mix_right[..n].fill(0.0);
            self.click_left[..n].fill(0.0);
            self.click_right[..n].fill(0.0);
            self.run(n);
            self.master(n);
            for i in 0..n {
                self.mix_left[i] += self.click_left[i];
                self.mix_right[i] += self.click_right[i];
            }
            self.mix_preview(n);
            for i in 0..n {
                let left = self.mix_left[i].clamp(-1.0, 1.0);
                let right = self.mix_right[i].clamp(-1.0, 1.0);
                peak = peak
                    .max(self.mix_left[i].abs())
                    .max(self.mix_right[i].abs());
                let frame = &mut output[(done + i) * channels..(done + i + 1) * channels];
                if channels == 1 {
                    frame[0] = f32::midpoint(left, right);
                } else {
                    frame[0] = left;
                    frame[1] = right;
                    frame[2..].fill(0.0);
                }
            }
            done += n;
        }
        output[frames * channels..].fill(0.0);

        #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
        self.shared
            .position
            .store(self.position.floor() as i64, Ordering::Relaxed);
        self.shared.playing.store(self.playing, Ordering::Relaxed);
        self.finish_preview();
        Shared::store_f32(&self.shared.peak, peak);
        for (meter, peak) in self.shared.meters.iter().zip(self.meter_peaks.iter_mut()) {
            Shared::store_f32(meter, *peak);
            *peak = 0.0;
        }
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            reason = "a ratio for display"
        )]
        let load =
            (started.elapsed().as_secs_f64() * self.sample_rate / frames.max(1) as f64) as f32;
        Shared::store_f32(&self.shared.load, load);
    }

    /// Takes the sound the input delivered since the last block: passed on
    /// while recording sound, let go otherwise (so it never piles up and
    /// delays what follows).
    fn drain_input(&mut self) {
        let available = self.input.slots() & !1;
        if available == 0 {
            return;
        }
        let Ok(chunk) = self.input.read_chunk(available) else {
            return;
        };
        if matches!(self.recording, Recording::Audio) {
            self.take.chunk.sample_rate = self.shared.input_rate.load(Ordering::Relaxed);
            self.take.chunk.channels = self.shared.input_channels.load(Ordering::Relaxed);
            let (first, second) = chunk.as_slices();
            for part in [first, second] {
                for frame in part.as_chunks::<2>().0 {
                    self.take.push(frame[0], frame[1]);
                }
            }
        }
        chunk.commit_all();
    }

    fn send(&mut self, message: Outgoing) {
        // A full queue loses only an event the interface would have shown;
        // garbage is kept and retried, never freed here.
        if let Err(rtrb::PushError::Full(Outgoing::Garbage(garbage))) = self.outgoing.push(message)
        {
            self.pending = Some(garbage);
        }
    }

    fn discard(&mut self, garbage: Garbage) {
        self.send(Outgoing::Garbage(garbage));
    }

    /// Adds the library's preview to the mix, after the master effects (so
    /// they never colour it) like the metronome.
    fn mix_preview(&mut self, n: usize) {
        let Some(preview) = &mut self.preview else {
            return;
        };
        let (left, right) = (preview.sound.left(), preview.sound.right());
        let end = (preview.at + n).min(left.len()).min(right.len());
        for (i, frame) in (preview.at..end).enumerate() {
            self.mix_left[i] += left[frame] * PREVIEW_GAIN;
            self.mix_right[i] += right[frame] * PREVIEW_GAIN;
        }
        preview.at = end.max(preview.at);
    }

    /// Lets a finished preview go (to be freed elsewhere) and says whether
    /// one is playing.
    fn finish_preview(&mut self) {
        let finished = self
            .preview
            .as_ref()
            .is_some_and(|p| p.at >= p.sound.frames());
        // Garbage waiting for room in the queue goes first.
        if finished
            && self.pending.is_none()
            && let Some(done) = self.preview.take()
        {
            self.discard(Garbage::Sound(done.sound));
        }
        let playing = self
            .preview
            .as_ref()
            .is_some_and(|p| p.at < p.sound.frames());
        self.shared.previewing.store(playing, Ordering::Relaxed);
    }

    fn handle_commands(&mut self) {
        if let Some(garbage) = self.pending.take() {
            self.discard(garbage);
            if self.pending.is_some() {
                return; // Still full: handle commands (which may create garbage) later.
            }
        }
        for _ in 0..MAX_COMMANDS_PER_BLOCK {
            let Ok(command) = self.commands.pop() else {
                break;
            };
            self.handle(command);
            if self.pending.is_some() {
                return;
            }
        }
        // Notes from MIDI keyboards go to the track that receives live input.
        let target = self.shared.live_target.load(Ordering::Relaxed);
        let target = if target < MAX_SLOTS {
            target
        } else {
            NO_TARGET
        };
        if target != self.live_slot {
            // The focus moved: the instrument left behind goes back to rest
            // (decision 0033).
            if self.live_slot != NO_TARGET {
                self.rest_live(self.live_slot);
            }
            self.live_slot = target;
        }
        for _ in 0..MAX_COMMANDS_PER_BLOCK {
            let Ok(note) = self.live.pop() else { break };
            match note {
                LiveNote::Note { key, velocity } if velocity > 0.0 => {
                    if target == NO_TARGET {
                        continue;
                    }
                    #[allow(clippy::cast_possible_truncation, reason = "slots < MAX_SLOTS")]
                    {
                        self.live_keys[usize::from(key & 0x7F)] = target as u16;
                    }
                    self.note_on(target, key, velocity);
                }
                LiveNote::Note { key, .. } => {
                    // A key comes up on the instrument it went down on.
                    let held =
                        std::mem::replace(&mut self.live_keys[usize::from(key & 0x7F)], NO_SLOT);
                    let slot = if held == NO_SLOT {
                        target
                    } else {
                        usize::from(held)
                    };
                    if slot != NO_TARGET {
                        self.note_off(slot, key);
                    }
                }
                LiveNote::Sustain(true) => {
                    if target != NO_TARGET {
                        self.sustain(target, true);
                    }
                }
                // The pedal comes up wherever it was holding notes.
                LiveNote::Sustain(false) => {
                    for slot in 0..MAX_SLOTS {
                        if self.slots[slot].sustain {
                            self.sustain(slot, false);
                        }
                    }
                }
                LiveNote::Control(control) => {
                    let (kind, value) = model_control(control);
                    self.live_values[control_index(kind)] = value;
                    if target != NO_TARGET {
                        self.live_control(target, control);
                    }
                }
            }
        }
        self.flush_recorded_controls();
    }

    /// A wheel, pedal or knob of a MIDI keyboard moved (decision 0033).
    /// While notes are recorded into that slot, the move is recorded too.
    fn live_control(&mut self, slot: usize, control: Control) {
        let Some(Some(instrument)) = self.instruments.get_mut(slot) else {
            return;
        };
        instrument.control(control);
        let plugin = instrument.plugin_instance().is_some();
        self.slots[slot].live_moved = true;
        if let Recording::Active { slot: recording } = self.recording
            && recording == slot
        {
            let (kind, value) = model_control(control);
            if records(kind, plugin) {
                let index = control_index(kind);
                self.live_override[index] = true;
                self.recorded_controls[index] = value;
            }
        }
    }

    /// Sends the controllers' moves of this block to be recorded.
    fn flush_recorded_controls(&mut self) {
        if !matches!(self.recording, Recording::Active { .. }) {
            return;
        }
        let at = self.position_ticks();
        for index in 0..CONTROLS {
            let value = std::mem::replace(&mut self.recorded_controls[index], f32::NAN);
            if value.is_finite() {
                self.send(Outgoing::Event(EngineEvent::RecordedControl {
                    control: control_kind(index),
                    value,
                    at,
                }));
            }
        }
    }

    /// Puts the controllers the song's curves moved back to rest (where they
    /// have a resting place), when playback stops or moves: a bend in a
    /// pattern does not stay bent.
    fn rest_song_controls(&mut self) {
        let Some(program) = self.program.as_deref_mut() else {
            return;
        };
        for track in &mut program.tracks {
            for control in &mut track.controls {
                if !control.last.is_finite() {
                    continue;
                }
                if let Some(rest) = control.control.rest()
                    && (control.last - rest).abs() > 1e-6
                    && let Some(Some(instrument)) = self.instruments.get_mut(track.slot)
                {
                    instrument.control(dsp_control(control.control, rest));
                }
                control.last = f32::NAN;
            }
        }
    }

    /// Puts what a MIDI keyboard moved on a slot back to rest: the wheel
    /// centred, no modulation or pressure, full expression.
    fn rest_live(&mut self, slot: usize) {
        let Some(state) = self.slots.get_mut(slot) else {
            return;
        };
        if !std::mem::take(&mut state.live_moved) {
            return;
        }
        if let Some(Some(instrument)) = self.instruments.get_mut(slot) {
            for control in Control::REST {
                instrument.control(control);
            }
        }
    }

    fn note_on(&mut self, slot: usize, key: u8, velocity: f32) {
        if let Some(state) = self.slots.get_mut(slot) {
            state.sustained[usize::from(key & 0x7F)] = false;
        }
        if let Some(Some(instrument)) = self.instruments.get_mut(slot) {
            instrument.note_on(key, velocity);
        }
        if let Recording::Active { slot: target } = self.recording
            && target == slot
        {
            // Struck again before its last note ended (held by the pedal, or
            // a keyboard that never sent the release): that note ends here.
            self.close_recorded_note(key, self.position_ticks());
            let start = self.position_ticks();
            self.recorded_held[usize::from(key & 0x7F)] = Some((start, velocity));
            self.send(Outgoing::Event(EngineEvent::RecordingNoteStarted {
                key,
                velocity,
                start,
            }));
        }
    }

    fn note_off(&mut self, slot: usize, key: u8) {
        if let Some(state) = self.slots.get_mut(slot)
            && state.sustain
        {
            state.sustained[usize::from(key & 0x7F)] = true;
            return;
        }
        if let Some(Some(instrument)) = self.instruments.get_mut(slot) {
            instrument.note_off(key);
        }
        if let Recording::Active { slot: target } = self.recording
            && target == slot
        {
            self.close_recorded_note(key, self.position_ticks());
        }
    }

    /// The sustain pedal went down or came up. Coming up releases every key
    /// it was holding.
    fn sustain(&mut self, slot: usize, down: bool) {
        let Some(state) = self.slots.get_mut(slot) else {
            return;
        };
        state.sustain = down;
        if down {
            return;
        }
        for key in 0..128_u8 {
            if std::mem::take(&mut self.slots[slot].sustained[usize::from(key)]) {
                self.note_off(slot, key);
            }
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Program(program) => self.install_program(program),
            Command::Instrument { slot, instrument } => {
                if slot < MAX_SLOTS {
                    self.slots[slot] = SlotState::new();
                    let old = std::mem::replace(&mut self.instruments[slot], instrument);
                    if let Some(mut old) = old {
                        old.retire();
                        self.discard(Garbage::Instrument(old));
                    }
                }
            }
            Command::InstrumentSettings { slot, settings } => {
                if let Some(Some(instrument)) = self.instruments.get_mut(slot) {
                    instrument.apply_settings(&settings);
                }
                self.discard(Garbage::Settings(settings));
            }
            Command::Effect { index, effect } => {
                if index < MAX_EFFECTS {
                    let old = std::mem::replace(&mut self.effects[index], effect);
                    if let Some(mut old) = old {
                        old.retire();
                        self.discard(Garbage::Effect(old));
                    }
                }
            }
            Command::Play => {
                if !self.playing {
                    self.playing = true;
                    self.play_started_at = self.position;
                    self.settle_gains = true;
                    self.reset_cursors();
                }
            }
            Command::Stop => self.stop(),
            Command::Locate(at) => {
                #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
                let at = at.0.max(0) as f64;
                self.release_song_notes();
                self.rest_song_controls();
                self.position = at;
                self.play_started_at = at;
                self.settle_gains = true;
                self.reset_cursors();
            }
            Command::Metronome {
                playback,
                recording,
                volume,
            } => {
                self.metronome_playback = playback;
                self.metronome_recording = recording;
                self.click.volume = volume.clamp(0.0, 2.0);
            }
            Command::Record {
                slot,
                count_in_bars,
            } => self.start_recording(Some(slot), count_in_bars),
            Command::RecordAudio { count_in_bars } => self.start_recording(None, count_in_bars),
            Command::NoteOn {
                slot,
                key,
                velocity,
            } => self.note_on(slot, key, velocity),
            Command::NoteOff { slot, key } => self.note_off(slot, key),
            Command::Panic => self.panic(),
            Command::Preview(sound) => {
                if let Some(old) = self.preview.take() {
                    self.discard(Garbage::Sound(old.sound));
                }
                if let Some((sound, number)) = sound {
                    self.shared.previewing.store(true, Ordering::Relaxed);
                    self.shared.preview_taken.store(number, Ordering::Release);
                    self.preview = Some(Preview { sound, at: 0 });
                }
            }
        }
    }

    /// Silences everything at once: every note, every controller back to
    /// rest, the preview.
    fn panic(&mut self) {
        for instrument in self.instruments.iter_mut().flatten() {
            instrument.all_notes_off(true);
            for control in Control::REST {
                instrument.control(control);
            }
        }
        self.live_keys = [NO_SLOT; 128];
        if let Some(program) = self.program.as_deref_mut() {
            for control in program
                .tracks
                .iter_mut()
                .flat_map(|t| t.controls.iter_mut())
            {
                control.last = f32::NAN;
            }
        }
        for slot in &mut self.slots {
            *slot = SlotState::new();
        }
        if let Some(old) = self.preview.take() {
            self.discard(Garbage::Sound(old.sound));
        }
    }

    #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
    fn position_ticks(&self) -> Ticks {
        Ticks(self.position.floor() as i64)
    }

    fn close_recorded_note(&mut self, key: u8, end: Ticks) {
        if let Some((start, velocity)) = self.recorded_held[usize::from(key & 0x7F)].take() {
            let end = end.max(start + Ticks(1));
            self.send(Outgoing::Event(EngineEvent::RecordedNote {
                key,
                velocity,
                start,
                end,
            }));
        }
    }

    fn close_all_recorded_notes(&mut self, end: Ticks) {
        for key in 0..128_u8 {
            self.close_recorded_note(key, end);
        }
    }

    /// Starts recording notes into a slot, or sound (`None`), after a
    /// count-in.
    fn start_recording(&mut self, slot: Option<usize>, count_in_bars: u32) {
        if count_in_bars == 0 {
            self.begin_recording(slot);
            return;
        }
        let at = self.position_ticks();
        let (bar, beat, bpm) = match &self.program {
            Some(program) => {
                let signature = program.meter.signature_at(at);
                #[allow(clippy::cast_precision_loss, reason = "tick counts fit easily")]
                let bpm_ticks = 1.0
                    / (program.tempo.seconds_at(self.position + 1.0)
                        - program.tempo.seconds_at(self.position));
                #[allow(clippy::cast_precision_loss, reason = "tick counts fit easily")]
                let values = (
                    signature.bar_length().0 as f64,
                    signature.beat_length().0 as f64,
                    bpm_ticks,
                );
                values
            }
            None => return,
        };
        self.release_song_notes();
        self.playing = false;
        self.recording = Recording::CountIn {
            slot,
            position: 0.0,
            length: bar * f64::from(count_in_bars),
            bpm_ticks_per_second: bpm,
            bar,
            beat,
        };
        self.shared
            .recording
            .store(RECORD_COUNT_IN, Ordering::Relaxed);
    }

    fn begin_recording(&mut self, slot: Option<usize>) {
        self.recording = match slot {
            Some(slot) => Recording::Active { slot },
            None => Recording::Audio,
        };
        self.recorded_held = [None; 128];
        self.shared
            .recording
            .store(RECORD_ACTIVE, Ordering::Relaxed);
        self.shared.count_in_left.store(0, Ordering::Relaxed);
        if !self.playing {
            self.playing = true;
            self.play_started_at = self.position;
            self.reset_cursors();
        }
        let at = self.position_ticks();
        if let Some(slot) = slot {
            self.send(Outgoing::Event(EngineEvent::RecordingStarted { at }));
            // The take starts with the controllers where they are.
            self.live_override = [false; CONTROLS];
            self.recorded_controls = [f32::NAN; CONTROLS];
            let plugin = self
                .instruments
                .get(slot)
                .and_then(Option::as_ref)
                .is_some_and(|i| i.plugin_instance().is_some());
            for index in 0..CONTROLS {
                let value = self.live_values[index];
                let kind = control_kind(index);
                if value.is_finite() && kind.rest() != Some(value) && records(kind, plugin) {
                    self.send(Outgoing::Event(EngineEvent::RecordedControl {
                        control: kind,
                        value,
                        at,
                    }));
                }
            }
        } else {
            self.take.begin();
            self.send(Outgoing::Event(EngineEvent::AudioRecordingStarted {
                take: self.take.number,
                at,
                sample_rate: self.shared.input_rate.load(Ordering::Relaxed),
                channels: self.shared.input_channels.load(Ordering::Relaxed),
                latency: Shared::load_f32(&self.shared.record_latency),
            }));
        }
    }

    fn stop(&mut self) {
        let was_recording = !matches!(self.recording, Recording::Idle);
        if matches!(self.recording, Recording::Active { .. }) {
            self.close_all_recorded_notes(self.position_ticks());
        }
        if matches!(self.recording, Recording::Audio) {
            self.take.flush();
            self.send(Outgoing::Event(EngineEvent::AudioRecordingStopped {
                take: self.take.number,
                frames: self.take.frames,
                dropped: self.take.dropped,
            }));
        }
        self.recording = Recording::Idle;
        self.live_override = [false; CONTROLS];
        self.shared.recording.store(RECORD_IDLE, Ordering::Relaxed);
        self.shared.count_in_left.store(0, Ordering::Relaxed);
        self.release_song_notes();
        self.rest_song_controls();
        if self.playing {
            self.playing = false;
            // Back to where playback started; a second stop goes to the start.
            self.position = self.play_started_at;
        } else {
            self.position = 0.0;
            self.play_started_at = 0.0;
        }
        if was_recording {
            self.send(Outgoing::Event(EngineEvent::RecordingStopped));
        }
        self.send(Outgoing::Event(EngineEvent::Stopped));
    }

    /// Releases every note that is sounding because of the song.
    fn release_song_notes(&mut self) {
        for (slot, state) in self.slots.iter_mut().enumerate() {
            for (key, held) in state.held.iter_mut().enumerate() {
                if held.take().is_some()
                    && let Some(instrument) = &mut self.instruments[slot]
                {
                    #[allow(clippy::cast_possible_truncation, reason = "key < 128")]
                    instrument.note_off(key as u8);
                }
            }
        }
    }

    fn reset_cursors(&mut self) {
        let position = self.position_ticks();
        self.cursors.clear();
        if let Some(program) = &self.program {
            for track in &program.tracks {
                self.cursors
                    .push(track.notes.partition_point(|n| n.start < position));
            }
        }
    }

    fn install_program(&mut self, program: Box<Program>) {
        let position = self.position_ticks();
        // Notes held because of the old program: keep the ones the new
        // program still plays at this moment, release the others.
        for (slot, state) in self.slots.iter_mut().enumerate() {
            let track = program.tracks.iter().find(|t| t.slot == slot);
            for key in 0..128_u8 {
                let held = &mut state.held[usize::from(key)];
                if held.is_none() {
                    continue;
                }
                let still = track.and_then(|t| {
                    let upto = t.notes.partition_point(|n| n.start <= position);
                    t.notes[..upto]
                        .iter()
                        .rev()
                        .take(4096)
                        .find(|n| n.key == key && n.end > position)
                        .map(|n| n.end)
                });
                *held = still;
                if still.is_none()
                    && let Some(instrument) = &mut self.instruments[slot]
                {
                    instrument.note_off(key);
                }
            }
        }
        // New settings for every built-in effect in the program's chains.
        for effect in program
            .tracks
            .iter()
            .flat_map(|t| t.effects.iter())
            .chain(program.master_effects.iter())
        {
            if let (Some(Some(instance)), Some(params)) =
                (self.effects.get_mut(effect.index), &effect.params)
            {
                instance.set_params(params);
            }
        }
        // Gains glide on from where the old program left them, and curves
        // go on from what they last sent; a curve gone from a track puts its
        // controller back to rest.
        let mut program = program;
        if let Some(old) = &self.program {
            for track in &mut program.tracks {
                if let Some(previous) = old.tracks.iter().find(|t| t.slot == track.slot) {
                    track.current_left = previous.current_left;
                    track.current_right = previous.current_right;
                    for control in &mut track.controls {
                        if let Some(before) = previous
                            .controls
                            .iter()
                            .find(|c| c.control == control.control)
                        {
                            control.last = before.last;
                        }
                    }
                }
            }
            for previous in &old.tracks {
                let kept = program.tracks.iter().find(|t| t.slot == previous.slot);
                for control in &previous.controls {
                    let still = kept
                        .is_some_and(|t| t.controls.iter().any(|c| c.control == control.control));
                    if let (false, true, Some(rest)) =
                        (still, control.last.is_finite(), control.control.rest())
                        && (control.last - rest).abs() > 1e-6
                        && let Some(Some(instrument)) = self.instruments.get_mut(previous.slot)
                    {
                        instrument.control(dsp_control(control.control, rest));
                    }
                }
            }
        }
        let tracks = program.tracks.len();
        if let Some(old) = self.program.replace(program) {
            self.discard(Garbage::Program(old));
        }
        // `cursors` was given capacity for MAX_SLOTS tracks up front.
        if tracks <= self.cursors.capacity() {
            self.reset_cursors();
        }
    }

    /// Fills the mix buffers with `n` frames, advancing the transport.
    fn run(&mut self, n: usize) {
        let mut offset = 0;
        while offset < n {
            let remaining = n - offset;
            match self.recording {
                Recording::CountIn { .. } => offset += self.run_count_in(offset, remaining),
                _ if self.playing && self.program.is_some() => {
                    offset += self.run_playing(offset, remaining);
                }
                _ => {
                    self.render(offset, remaining, None);
                    offset = n;
                }
            }
        }
    }

    fn run_count_in(&mut self, offset: usize, remaining: usize) -> usize {
        let Recording::CountIn {
            slot,
            position,
            length,
            bpm_ticks_per_second,
            bar,
            beat,
        } = self.recording
        else {
            return remaining;
        };
        let ticks_per_frame = bpm_ticks_per_second / self.sample_rate;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bounded by `remaining`"
        )]
        let frames_left = ((length - position) / ticks_per_frame).ceil().max(0.0) as usize;
        let len = frames_left.min(remaining);
        // Clicks on every beat of the count-in.
        let mut click_at = (position / beat).ceil() * beat;
        #[allow(clippy::cast_precision_loss, reason = "block sizes are small")]
        let end = position + ticks_per_frame * len as f64;
        let mut cursor = 0;
        while click_at < end && click_at < length {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "within this stretch"
            )]
            let at = (((click_at - position) / ticks_per_frame).round() as usize)
                .min(len.saturating_sub(1));
            self.click.add_to(
                &mut self.click_left[offset + cursor..offset + at],
                &mut self.click_right[offset + cursor..offset + at],
            );
            cursor = at;
            self.click.trigger((click_at / bar).fract() < 1e-9);
            click_at += beat;
        }
        self.click.add_to(
            &mut self.click_left[offset + cursor..offset + len],
            &mut self.click_right[offset + cursor..offset + len],
        );
        self.render(offset, len, None);

        let position = end;
        if position >= length - 1e-6 {
            self.begin_recording(slot);
        } else {
            self.recording = Recording::CountIn {
                slot,
                position,
                length,
                bpm_ticks_per_second,
                bar,
                beat,
            };
            #[allow(clippy::cast_possible_truncation, reason = "tick counts fit easily")]
            self.shared
                .count_in_left
                .store((length - position) as i64, Ordering::Relaxed);
        }
        len.max(1).min(remaining)
    }

    fn run_playing(&mut self, offset: usize, remaining: usize) -> usize {
        let Some(program) = &self.program else {
            return remaining;
        };
        let start_tick = self.position;
        let start_seconds = program.tempo.seconds_at(start_tick);
        #[allow(clippy::cast_precision_loss, reason = "block sizes are small")]
        let full_end = program
            .tempo
            .ticks_at(start_seconds + remaining as f64 / self.sample_rate);
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        let loop_region = program
            .loop_region
            .filter(|r| (r.end.0 as f64) > start_tick);
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        let (len, end_tick, wrap) = match loop_region {
            Some(region) if (region.end.0 as f64) <= full_end => {
                let end = region.end.0 as f64;
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "bounded by `remaining`"
                )]
                let frames = (((program.tempo.seconds_at(end) - start_seconds) * self.sample_rate)
                    .round()
                    .max(0.0) as usize)
                    .min(remaining);
                (frames, end, Some(region))
            }
            _ => (remaining, full_end, None),
        };
        if len > 0 {
            self.render(offset, len, Some((start_tick, end_tick)));
        }
        self.position = end_tick;
        if let Some(region) = wrap {
            self.wrap_loop(region);
        }
        if len == 0 && wrap.is_none() {
            remaining
        } else {
            len
        }
    }

    fn wrap_loop(&mut self, region: Region) {
        let end = region.end;
        self.release_song_notes();
        if let Recording::Active { .. } = self.recording {
            // Held recorded notes end at the loop end and carry on from the
            // loop start.
            for key in 0..128_u8 {
                if let Some((start, velocity)) = self.recorded_held[usize::from(key)] {
                    self.close_recorded_note(key, end);
                    if start < end {
                        self.recorded_held[usize::from(key)] = Some((region.start, velocity));
                    }
                }
            }
        }
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        let start = region.start.0 as f64;
        self.position = start;
        self.reset_cursors();
    }

    /// The master bus over the mix: its gain, then its effect chain. The
    /// gain comes first so a limiter at the end of the chain always has the
    /// last word, however far the master volume is turned up.
    fn master(&mut self, n: usize) {
        let Some(program) = &self.program else {
            return;
        };
        let (left, right) = (&mut self.mix_left[..n], &mut self.mix_right[..n]);
        let gain = program.master_gain;
        if (gain - 1.0).abs() > f32::EPSILON {
            for sample in left.iter_mut().chain(right.iter_mut()) {
                *sample *= gain;
            }
        }
        for effect in program.master_effects.iter().filter(|e| e.enabled) {
            if let Some(Some(instance)) = self.effects.get_mut(effect.index) {
                instance.process(left, right);
            }
        }
    }

    /// Where the song is at `tick`, for plugins that follow it.
    fn song_position(&self, tick: f64, playing: bool) -> SongPosition {
        let Some(program) = &self.program else {
            return SongPosition::default();
        };
        #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
        let at = Ticks(tick.max(0.0) as i64);
        let signature = program.meter.signature_at(at);
        let bar = program.meter.position(at).bar;
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        let quarter = TICKS_PER_QUARTER as f64;
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        let bar_start = program.meter.bar_start(bar).0 as f64 / quarter;
        let seconds_per_tick =
            program.tempo.seconds_at(tick + 1.0) - program.tempo.seconds_at(tick);
        #[allow(clippy::cast_possible_truncation, reason = "a frame count")]
        let frames = (program.tempo.seconds_at(tick) * self.sample_rate) as i64;
        SongPosition {
            playing,
            tempo: if seconds_per_tick > 0.0 {
                60.0 / (seconds_per_tick * quarter)
            } else {
                120.0
            },
            beats: tick / quarter,
            bar_start,
            bar,
            numerator: signature.numerator,
            denominator: signature.denominator,
            frames,
        }
    }

    /// Applies a track's automation at `tick`: plugin parameters, effect
    /// settings. Returns the gains its volume and pan call for.
    fn automate(&mut self, index: usize, slot: usize, tick: f64) -> Option<(f32, f32)> {
        let program = self.program.as_deref_mut()?;
        let track = program.tracks.get_mut(index)?;
        if track.automation.is_empty() {
            return None;
        }
        let mut gain = track.gain;
        let mut pan = track.pan;
        let mut volume_or_pan = false;
        let mut effects_changed = false;
        let mut synth_changed = false;
        for lane in &mut track.automation {
            let value = lane.value_at(tick);
            let changed = value.to_bits() != lane.last.to_bits();
            lane.last = value;
            match lane.target {
                LaneTarget::Volume => {
                    gain = daw_model::volume_gain(value);
                    volume_or_pan = true;
                }
                LaneTarget::Pan => {
                    pan = daw_model::value_pan(value);
                    volume_or_pan = true;
                }
                LaneTarget::BuiltinEffect { .. } => effects_changed |= changed,
                LaneTarget::PluginEffect { index, param } if changed => {
                    if let Some(Some(effect)) = self.effects.get_mut(index) {
                        effect.set_parameter(param, f64::from(value));
                    }
                }
                LaneTarget::Instrument { param } if changed => {
                    if let Some(Some(instrument)) = self.instruments.get_mut(slot) {
                        instrument.set_parameter(param, f64::from(value));
                    }
                }
                LaneTarget::Synth { .. } => synth_changed |= changed,
                LaneTarget::PluginEffect { .. } | LaneTarget::Instrument { .. } => {}
            }
        }
        // The synth takes its base settings with every automated one
        // applied (plain data: nothing is allocated).
        if synth_changed && let Some(mut settings) = track.synth_base {
            for lane in &track.automation {
                if let LaneTarget::Synth { param } = lane.target {
                    settings.set_param(param, lane.last);
                }
            }
            if let Some(Some(instrument)) = self.instruments.get_mut(slot) {
                instrument.apply_settings(&daw_dsp::InstrumentSettings::Synth(
                    crate::library::synth_params(&settings),
                ));
            }
        }
        if effects_changed {
            for (position, automated) in track.automated_effects.iter().enumerate() {
                let mut kind = automated.base.clone();
                for lane in &track.automation {
                    if let LaneTarget::BuiltinEffect { effect, param } = lane.target
                        && effect == position
                    {
                        kind.set_param(param, lane.last);
                    }
                }
                if let (Some(params), Some(Some(instance))) = (
                    crate::program::effect_params(&kind, automated.bpm),
                    self.effects.get_mut(automated.index),
                ) {
                    instance.set_params(&params);
                }
            }
        }
        volume_or_pan.then(|| pan_gains(if track.audible { gain } else { 0.0 }, pan))
    }

    /// Renders every instrument for `len` frames at `offset` into the mix,
    /// playing the song between the two tick positions if given.
    #[allow(clippy::too_many_lines, reason = "one track after another, in order")]
    fn render(&mut self, offset: usize, len: usize, song: Option<(f64, f64)>) {
        if len == 0 {
            return;
        }
        let position =
            self.song_position(song.map_or(self.position, |(from, _)| from), song.is_some());
        // Where the song is, in frames, for the tracks' audio clips.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "frame positions fit easily"
        )]
        let frame = match (song, &self.program) {
            (Some((from, _)), Some(program)) => {
                Some((program.tempo.seconds_at(from) * self.sample_rate).round() as i64)
            }
            _ => None,
        };
        #[allow(clippy::cast_possible_wrap, reason = "block sizes are small")]
        let block = len as i64;
        for slot in 0..MAX_SLOTS {
            if self.instruments[slot].is_none() {
                continue;
            }
            let track_index = self
                .program
                .as_ref()
                .and_then(|p| p.tracks.iter().position(|t| t.slot == slot));
            let count = match (song, track_index) {
                (Some((from, to)), Some(index)) => {
                    let notes = self.schedule(index, from, to, len);
                    self.schedule_controls(index, (from, to), len, notes)
                }
                _ => 0,
            };
            let automated = match (song, track_index) {
                (Some((from, _)), Some(index)) => self.automate(index, slot, from),
                _ => None,
            };
            let audio = match (&self.program, track_index, frame) {
                (Some(program), Some(index), Some(frame)) => {
                    program.tracks[index].audio_between(frame, frame + block)
                }
                _ => false,
            };
            let settle = self.settle_gains && song.is_some();
            let (start_gains, target_gains, has_effects) = match (&self.program, track_index) {
                (Some(program), Some(index)) => {
                    let track = &program.tracks[index];
                    let target = automated.unwrap_or((track.gain_left, track.gain_right));
                    (
                        if settle {
                            target
                        } else {
                            (track.current_left, track.current_right)
                        },
                        target,
                        track.effects.iter().any(|e| e.enabled),
                    )
                }
                _ => ((1.0, 1.0), (1.0, 1.0), false),
            };
            let Some(instrument) = self.instruments[slot].as_mut() else {
                continue;
            };
            let sounding = count > 0 || instrument.is_sounding() || audio;
            // A track with effects keeps running them: their tails (reverb,
            // echoes) go on after the instrument falls silent.
            if !sounding && !has_effects {
                // Silent: its volume and pan are simply where they should be
                // when it next sounds.
                if let (Some(program), Some(index)) = (self.program.as_deref_mut(), track_index) {
                    let track = &mut program.tracks[index];
                    (track.current_left, track.current_right) = target_gains;
                }
                continue;
            }
            let events = &mut self.events[..count];
            events.sort_unstable_by_key(Scheduled::order);
            self.notes.clear();
            for event in events.iter() {
                if self.notes.len() < self.notes.capacity() {
                    self.notes.push(BlockEvent {
                        offset: event.offset.min(len),
                        kind: event.kind,
                    });
                }
            }
            let left = &mut self.scratch_left[..len];
            let right = &mut self.scratch_right[..len];
            instrument.set_position(&position);
            if sounding {
                instrument.render_block(&self.notes, left, right);
            } else {
                left.fill(0.0);
                right.fill(0.0);
            }
            if audio
                && let (Some(program), Some(index), Some(frame)) =
                    (&self.program, track_index, frame)
            {
                program.tracks[index].mix_audio(frame, left, right);
            }
            if has_effects && let (Some(program), Some(index)) = (&self.program, track_index) {
                for effect in program.tracks[index].effects.iter().filter(|e| e.enabled) {
                    if let Some(Some(instance)) = self.effects.get_mut(effect.index) {
                        instance.set_position(&position);
                        instance.process(left, right);
                    }
                }
            }
            // Volume and pan, gliding over the block from where they were.
            let (from_left, from_right) = start_gains;
            let (to_left, to_right) = target_gains;
            #[allow(clippy::cast_precision_loss, reason = "block sizes are small")]
            let step = 1.0 / len as f32;
            let mut peak = self.meter_peaks[slot];
            for i in 0..len {
                #[allow(clippy::cast_precision_loss, reason = "block sizes are small")]
                let t = (i + 1) as f32 * step;
                let gain_left = from_left + (to_left - from_left) * t;
                let gain_right = from_right + (to_right - from_right) * t;
                let (l, r) = (left[i] * gain_left, right[i] * gain_right);
                peak = peak.max(l.abs()).max(r.abs());
                self.mix_left[offset + i] += l;
                self.mix_right[offset + i] += r;
            }
            self.meter_peaks[slot] = peak;
            if let (Some(program), Some(index)) = (self.program.as_deref_mut(), track_index) {
                let track = &mut program.tracks[index];
                track.current_left = to_left;
                track.current_right = to_right;
            }
        }
        if song.is_some() {
            self.settle_gains = false;
        }
        if let Some((from, to)) = song {
            let metronome = match self.recording {
                Recording::Active { .. } | Recording::Audio => self.metronome_recording,
                Recording::Idle | Recording::CountIn { .. } => self.metronome_playback,
            };
            if metronome {
                self.metronome(offset, len, from, to);
            } else {
                self.click.add_to(
                    &mut self.click_left[offset..offset + len],
                    &mut self.click_right[offset..offset + len],
                );
            }
        } else {
            self.click.add_to(
                &mut self.click_left[offset..offset + len],
                &mut self.click_right[offset..offset + len],
            );
        }
    }

    /// Collects the song's note starts and ends between two tick positions
    /// for one program track into `self.events`; returns how many.
    fn schedule(&mut self, index: usize, from: f64, to: f64, len: usize) -> usize {
        let Some(program) = &self.program else {
            return 0;
        };
        let track = &program.tracks[index];
        let state = &mut self.slots[track.slot];
        let start_seconds = program.tempo.seconds_at(from);
        let sample_rate = self.sample_rate;
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "frame offsets within one block"
        )]
        let offset_of = |tick: Ticks| -> usize {
            let seconds = program.tempo.seconds_at(tick.0 as f64) - start_seconds;
            ((seconds * sample_rate).round().max(0.0) as usize).min(len.saturating_sub(1))
        };
        #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
        let to_tick = Ticks(to.ceil() as i64);
        let mut count = 0;
        // Ends of held notes.
        for key in 0..128_u8 {
            if let Some(end) = state.held[usize::from(key)]
                && end < to_tick
            {
                if count < MAX_EVENTS {
                    self.events[count] = Scheduled::off(offset_of(end), key);
                    count += 1;
                }
                state.held[usize::from(key)] = None;
            }
        }
        // Starts of new notes.
        let cursor = self.cursors.get(index).copied().unwrap_or(0);
        let mut next = cursor;
        while next < track.notes.len() && track.notes[next].start < to_tick {
            let note = track.notes[next];
            next += 1;
            if count + 2 > MAX_EVENTS {
                break;
            }
            let at = offset_of(note.start);
            let slot = &mut state.held[usize::from(note.key & 0x7F)];
            if slot.is_some() {
                // The key is already sounding: end it first.
                self.events[count] = Scheduled::off(at, note.key);
                count += 1;
            }
            self.events[count] = Scheduled::on(at, note.key, note.velocity);
            count += 1;
            if note.end < to_tick {
                // Starts and ends inside this stretch.
                if count < MAX_EVENTS {
                    self.events[count] = Scheduled::off(
                        offset_of(note.end).max(at + 1).min(len.saturating_sub(1)),
                        note.key,
                    );
                    count += 1;
                }
                *slot = None;
            } else {
                *slot = Some(note.end);
            }
        }
        if let Some(cursor) = self.cursors.get_mut(index) {
            *cursor = next;
        }
        count
    }

    /// Adds the moves of one program track's curves between two tick
    /// positions to `self.events` after the first `count`, a look every
    /// [`CONTROL_STEP`] frames; returns how many events there are now.
    /// Outside the clips that have a curve, a controller with a resting
    /// place goes back to it once.
    fn schedule_controls(
        &mut self,
        index: usize,
        (from, to): (f64, f64),
        len: usize,
        mut count: usize,
    ) -> usize {
        let recording = match self.recording {
            Recording::Active { slot } => Some(slot),
            _ => None,
        };
        let Some(program) = self.program.as_deref_mut() else {
            return count;
        };
        let track = &mut program.tracks[index];
        let taken_over = recording == Some(track.slot);
        for control in &mut track.controls {
            if taken_over && self.live_override[control_index(control.control)] {
                continue;
            }
            let mut offset = 0;
            while offset < len && count < MAX_EVENTS {
                #[allow(clippy::cast_precision_loss, reason = "block sizes are small")]
                let tick = from + (to - from) * (offset as f64 / len as f64);
                let value = match control.value_at(tick) {
                    Some(value) => Some(value),
                    None if control.last.is_finite() => control.control.rest(),
                    None => None,
                };
                if let Some(value) = value
                    && (!control.last.is_finite() || (value - control.last).abs() > 1e-4)
                {
                    self.events[count] = Scheduled {
                        offset,
                        kind: BlockEventKind::Control(dsp_control(control.control, value)),
                    };
                    count += 1;
                    control.last = value;
                }
                offset += CONTROL_STEP;
            }
        }
        count
    }

    /// Adds metronome clicks on the beats between two tick positions.
    fn metronome(&mut self, offset: usize, len: usize, from: f64, to: f64) {
        let Some(program) = &self.program else { return };
        #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
        let first = Ticks(from.ceil() as i64);
        let position = program.meter.position(first);
        let beat_length = position.signature.beat_length();
        let bar_start = program.meter.bar_start(position.bar);
        // The first beat at or after `from`.
        let mut beat_index = i64::from(position.beat);
        let mut beat_at = bar_start + beat_length * beat_index;
        if beat_at < first {
            beat_index += 1;
            beat_at += beat_length;
        }
        let start_seconds = program.tempo.seconds_at(from);
        let mut cursor = 0;
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        while (beat_at.0 as f64) < to {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "frame offsets within one block"
            )]
            let at = (((program.tempo.seconds_at(beat_at.0 as f64) - start_seconds)
                * self.sample_rate)
                .round()
                .max(0.0) as usize)
                .min(len.saturating_sub(1));
            self.click.add_to(
                &mut self.click_left[offset + cursor..offset + at],
                &mut self.click_right[offset + cursor..offset + at],
            );
            cursor = at;
            let accent = beat_index % i64::from(position.signature.numerator) == 0;
            self.click.trigger(accent);
            beat_index += 1;
            beat_at += beat_length;
        }
        self.click.add_to(
            &mut self.click_left[offset + cursor..offset + len],
            &mut self.click_right[offset + cursor..offset + len],
        );
    }
}

/// Whether a take records a controller: the wheels, pressure and the
/// expression pedal for every instrument, any other controller for a plugin
/// (which may use it for anything); never the sustain pedal (which
/// lengthens the notes instead), bank select or the channel mode messages.
fn records(control: ControlKind, plugin: bool) -> bool {
    control.is_recordable()
        && (plugin
            || matches!(
                control,
                ControlKind::PitchBend
                    | ControlKind::Pressure
                    | daw_model::controls::MODULATION
                    | daw_model::controls::EXPRESSION
            ))
}

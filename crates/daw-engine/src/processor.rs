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

use daw_dsp::{Click, Instrument};
use daw_model::{Region, Ticks};

use crate::messages::{
    Command, EngineEvent, Garbage, LiveNote, NO_TARGET, Outgoing, RECORD_ACTIVE, RECORD_COUNT_IN,
    RECORD_IDLE, Shared,
};
use crate::program::Program;

/// How many instrument slots (tracks) the engine can play.
pub const MAX_SLOTS: usize = 256;

/// Events handled per instrument per stretch of audio; more are applied at
/// the end of the stretch (never reached in practice).
const MAX_EVENTS: usize = 512;

/// The most commands handled per audio block, so a flood of messages cannot
/// make one block late.
const MAX_COMMANDS_PER_BLOCK: usize = 256;

#[derive(Clone, Copy)]
struct Scheduled {
    offset: usize,
    key: u8,
    velocity: f32,
    on: bool,
}

const NO_EVENT: Scheduled = Scheduled {
    offset: 0,
    key: 0,
    velocity: 0.0,
    on: false,
};

/// Per-slot playback state.
struct SlotState {
    /// For each key sounding because of the song: where its note ends.
    held: [Option<Ticks>; 128],
}

#[derive(Clone, Copy)]
enum Recording {
    Idle,
    CountIn {
        slot: usize,
        /// Count-in position, in ticks from its start.
        position: f64,
        length: f64,
        bpm_ticks_per_second: f64,
        bar: f64,
        beat: f64,
    },
    Active {
        slot: usize,
    },
}

/// The audio thread's side of the engine. Call [`EngineProcessor::process`]
/// from the sound card's callback (or in a loop, for offline rendering).
pub struct EngineProcessor {
    sample_rate: f64,
    max_block: usize,
    commands: rtrb::Consumer<Command>,
    live: rtrb::Consumer<LiveNote>,
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
    recording: Recording,
    recorded_held: [Option<(Ticks, f32)>; 128],
    metronome_playback: bool,
    metronome_recording: bool,
    click: Click,
    scratch_left: Vec<f32>,
    scratch_right: Vec<f32>,
    mix_left: Vec<f32>,
    mix_right: Vec<f32>,
    events: Vec<Scheduled>,
    /// Garbage that could not be sent yet because the queue was full.
    pending: Option<Garbage>,
}

impl EngineProcessor {
    pub(crate) fn new(
        sample_rate: u32,
        max_block: usize,
        commands: rtrb::Consumer<Command>,
        live: rtrb::Consumer<LiveNote>,
        outgoing: rtrb::Producer<Outgoing>,
        shared: Arc<Shared>,
    ) -> Self {
        let max_block = max_block.max(1);
        Self {
            sample_rate: f64::from(sample_rate),
            max_block,
            commands,
            live,
            outgoing,
            shared,
            program: None,
            cursors: Vec::with_capacity(MAX_SLOTS),
            instruments: (0..MAX_SLOTS).map(|_| None).collect(),
            slots: (0..MAX_SLOTS)
                .map(|_| SlotState { held: [None; 128] })
                .collect(),
            playing: false,
            position: 0.0,
            play_started_at: 0.0,
            recording: Recording::Idle,
            recorded_held: [None; 128],
            metronome_playback: false,
            metronome_recording: true,
            click: Click::new(sample_rate),
            scratch_left: vec![0.0; max_block],
            scratch_right: vec![0.0; max_block],
            mix_left: vec![0.0; max_block],
            mix_right: vec![0.0; max_block],
            events: vec![NO_EVENT; MAX_EVENTS],
            pending: None,
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

        let frames = output.len() / channels;
        let mut peak = 0.0_f32;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(self.max_block);
            self.mix_left[..n].fill(0.0);
            self.mix_right[..n].fill(0.0);
            self.run(n);
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
        Shared::store_f32(&self.shared.peak, peak);
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            reason = "a ratio for display"
        )]
        let load =
            (started.elapsed().as_secs_f64() * self.sample_rate / frames.max(1) as f64) as f32;
        Shared::store_f32(&self.shared.load, load);
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
        for _ in 0..MAX_COMMANDS_PER_BLOCK {
            let Ok(note) = self.live.pop() else { break };
            if target == NO_TARGET {
                continue;
            }
            if note.velocity > 0.0 {
                self.note_on(target, note.key, note.velocity);
            } else {
                self.note_off(target, note.key);
            }
        }
    }

    fn note_on(&mut self, slot: usize, key: u8, velocity: f32) {
        if let Some(Some(instrument)) = self.instruments.get_mut(slot) {
            instrument.note_on(key, velocity);
        }
        if let Recording::Active { slot: target } = self.recording
            && target == slot
        {
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
        if let Some(Some(instrument)) = self.instruments.get_mut(slot) {
            instrument.note_off(key);
        }
        if let Recording::Active { slot: target } = self.recording
            && target == slot
        {
            self.close_recorded_note(key, self.position_ticks());
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Program(program) => self.install_program(program),
            Command::Instrument { slot, instrument } => {
                if slot < MAX_SLOTS {
                    self.slots[slot].held = [None; 128];
                    let old = std::mem::replace(&mut self.instruments[slot], instrument);
                    if let Some(old) = old {
                        self.discard(Garbage::Instrument(old));
                    }
                }
            }
            Command::Play => {
                if !self.playing {
                    self.playing = true;
                    self.play_started_at = self.position;
                    self.reset_cursors();
                }
            }
            Command::Stop => self.stop(),
            Command::Locate(at) => {
                #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
                let at = at.0.max(0) as f64;
                self.release_song_notes();
                self.position = at;
                self.play_started_at = at;
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
            } => self.start_recording(slot, count_in_bars),
            Command::NoteOn {
                slot,
                key,
                velocity,
            } => self.note_on(slot, key, velocity),
            Command::NoteOff { slot, key } => self.note_off(slot, key),
            Command::Panic => {
                for instrument in self.instruments.iter_mut().flatten() {
                    instrument.all_notes_off(true);
                }
                for slot in &mut self.slots {
                    slot.held = [None; 128];
                }
            }
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

    fn start_recording(&mut self, slot: usize, count_in_bars: u32) {
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

    fn begin_recording(&mut self, slot: usize) {
        self.recording = Recording::Active { slot };
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
        self.send(Outgoing::Event(EngineEvent::RecordingStarted { at }));
    }

    fn stop(&mut self) {
        let was_recording = !matches!(self.recording, Recording::Idle);
        if matches!(self.recording, Recording::Active { .. }) {
            self.close_all_recorded_notes(self.position_ticks());
        }
        self.recording = Recording::Idle;
        self.shared.recording.store(RECORD_IDLE, Ordering::Relaxed);
        self.shared.count_in_left.store(0, Ordering::Relaxed);
        self.release_song_notes();
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
                &mut self.mix_left[offset + cursor..offset + at],
                &mut self.mix_right[offset + cursor..offset + at],
            );
            cursor = at;
            self.click.trigger((click_at / bar).fract() < 1e-9);
            click_at += beat;
        }
        self.click.add_to(
            &mut self.mix_left[offset + cursor..offset + len],
            &mut self.mix_right[offset + cursor..offset + len],
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

    /// Renders every instrument for `len` frames at `offset` into the mix,
    /// playing the song between the two tick positions if given.
    fn render(&mut self, offset: usize, len: usize, song: Option<(f64, f64)>) {
        if len == 0 {
            return;
        }
        for slot in 0..MAX_SLOTS {
            if self.instruments[slot].is_none() {
                continue;
            }
            let track_index = self
                .program
                .as_ref()
                .and_then(|p| p.tracks.iter().position(|t| t.slot == slot));
            let count = match (song, track_index) {
                (Some((from, to)), Some(index)) => self.schedule(index, from, to, len),
                _ => 0,
            };
            let (gain_left, gain_right) = match (&self.program, track_index) {
                (Some(program), Some(index)) => (
                    program.tracks[index].gain_left,
                    program.tracks[index].gain_right,
                ),
                _ => (1.0, 1.0),
            };
            let Some(instrument) = self.instruments[slot].as_mut() else {
                continue;
            };
            if count == 0 && !instrument.is_sounding() {
                continue;
            }
            let events = &mut self.events[..count];
            events.sort_unstable_by_key(|e| (e.offset, e.on));
            let left = &mut self.scratch_left[..len];
            let right = &mut self.scratch_right[..len];
            let mut cursor = 0;
            for event in events.iter() {
                let at = event.offset.min(len);
                if at > cursor {
                    instrument.render(&mut left[cursor..at], &mut right[cursor..at]);
                    cursor = at;
                }
                if event.on {
                    instrument.note_on(event.key, event.velocity);
                } else {
                    instrument.note_off(event.key);
                }
            }
            if cursor < len {
                instrument.render(&mut left[cursor..], &mut right[cursor..]);
            }
            for i in 0..len {
                self.mix_left[offset + i] += left[i] * gain_left;
                self.mix_right[offset + i] += right[i] * gain_right;
            }
        }
        if let Some((from, to)) = song {
            let metronome = match self.recording {
                Recording::Active { .. } => self.metronome_recording,
                _ => self.metronome_playback,
            };
            if metronome {
                self.metronome(offset, len, from, to);
            } else {
                self.click.add_to(
                    &mut self.mix_left[offset..offset + len],
                    &mut self.mix_right[offset..offset + len],
                );
            }
        } else {
            self.click.add_to(
                &mut self.mix_left[offset..offset + len],
                &mut self.mix_right[offset..offset + len],
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
                    self.events[count] = Scheduled {
                        offset: offset_of(end),
                        key,
                        velocity: 0.0,
                        on: false,
                    };
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
                self.events[count] = Scheduled {
                    offset: at,
                    key: note.key,
                    velocity: 0.0,
                    on: false,
                };
                count += 1;
            }
            self.events[count] = Scheduled {
                offset: at,
                key: note.key,
                velocity: note.velocity,
                on: true,
            };
            count += 1;
            if note.end < to_tick {
                // Starts and ends inside this stretch.
                if count < MAX_EVENTS {
                    self.events[count] = Scheduled {
                        offset: offset_of(note.end).max(at + 1).min(len.saturating_sub(1)),
                        key: note.key,
                        velocity: 0.0,
                        on: false,
                    };
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
                &mut self.mix_left[offset + cursor..offset + at],
                &mut self.mix_right[offset + cursor..offset + at],
            );
            cursor = at;
            let accent = beat_index % i64::from(position.signature.numerator) == 0;
            self.click.trigger(accent);
            beat_index += 1;
            beat_at += beat_length;
        }
        self.click.add_to(
            &mut self.mix_left[offset + cursor..offset + len],
            &mut self.mix_right[offset + cursor..offset + len],
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Playing SoundFont (.sf2) files, through `rustysynth`.

use std::fmt;
use std::sync::Arc;

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::Instrument;

/// A SoundFont loaded into memory, shared by every instrument that plays it.
#[derive(Clone)]
pub struct SoundFontBank {
    font: Arc<SoundFont>,
}

/// A preset (one playable sound) inside a SoundFont.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetInfo {
    /// The preset's name.
    pub name: String,
    /// Its bank number.
    pub bank: u16,
    /// Its program number.
    pub program: u8,
}

/// Why a SoundFont could not be used.
#[derive(Debug)]
pub struct SoundFontError(String);

impl fmt::Display for SoundFontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SoundFontError {}

impl SoundFontBank {
    /// Reads a SoundFont from its bytes. Not for the audio thread.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SoundFontError> {
        let mut reader = bytes;
        let font = SoundFont::new(&mut reader)
            .map_err(|e| SoundFontError(format!("not a usable SoundFont: {e}")))?;
        if font.get_presets().is_empty() {
            return Err(SoundFontError(
                "the SoundFont contains no presets".to_owned(),
            ));
        }
        Ok(Self {
            font: Arc::new(font),
        })
    }

    /// Reads a SoundFont from its bytes, with every sound's attack and
    /// release changed to the shape's (decision 0031; what the shape does
    /// not set keeps the file's own). Not for the audio thread: it copies
    /// the whole file.
    pub fn from_bytes_shaped(
        bytes: &[u8],
        shape: crate::NoteShape,
    ) -> Result<Self, SoundFontError> {
        let shape = shape.sane();
        if shape.attack.is_none() && shape.release.is_none() {
            return Self::from_bytes(bytes);
        }
        let shaped = crate::sf2_shape::shaped(
            bytes,
            shape.attack.map(crate::sf2_shape::timecents),
            shape.release.map(crate::sf2_shape::timecents),
        )
        .map_err(|e| SoundFontError(format!("cannot change its note shape: {e}")))?;
        Self::from_bytes(&shaped)
    }

    /// The presets it contains, sorted by bank and program.
    pub fn presets(&self) -> Vec<PresetInfo> {
        let mut presets: Vec<PresetInfo> = self
            .font
            .get_presets()
            .iter()
            .map(|p| PresetInfo {
                name: p.get_name().to_owned(),
                bank: u16::try_from(p.get_bank_number()).unwrap_or(0),
                program: u8::try_from(p.get_patch_number()).unwrap_or(0),
            })
            .collect();
        presets.sort_by_key(|p| (p.bank, p.program));
        presets
    }

    /// The SoundFont's name, from its own information block.
    pub fn name(&self) -> String {
        self.font.get_info().get_bank_name().to_owned()
    }
}

/// How finely note starts are placed, in frames: the synthesizer computes
/// sound in blocks this long, and a note starts at the next block boundary.
/// 16 frames is a third of a millisecond at 48 kHz.
pub const SOUNDFONT_BLOCK: usize = 16;

/// Notes the player can sound at once.
pub const SOUNDFONT_POLYPHONY: usize = 128;

/// Plays one preset of a SoundFont as an instrument.
pub struct SoundFontPlayer {
    synth: Synthesizer,
    held: [bool; 128],
    held_count: u32,
    /// Frames left to render after the last release (tails and reverb).
    tail_frames: usize,
}

impl SoundFontPlayer {
    /// A player for one preset. Reserves all its memory here, so it must be
    /// created off the audio thread.
    pub fn new(
        bank: &SoundFontBank,
        sample_rate: u32,
        preset_bank: u16,
        program: u8,
    ) -> Result<Self, SoundFontError> {
        let mut settings = SynthesizerSettings::new(i32::try_from(sample_rate).unwrap_or(48_000));
        settings.block_size = SOUNDFONT_BLOCK;
        settings.maximum_polyphony = SOUNDFONT_POLYPHONY;
        settings.enable_reverb_and_chorus = true;
        let mut synth = Synthesizer::new(&bank.font, &settings)
            .map_err(|e| SoundFontError(format!("cannot play it: {e}")))?;
        // Twice the synthesizer's default (+6 dB), so the piano sits level
        // with the drum kits; the master bus's limiter catches the peaks of
        // a loud mix.
        synth.set_master_volume(PIANO_VOLUME);
        // Bank select, then program change, on channel 0.
        synth.process_midi_message(0, 0xB0, 0x00, i32::from(preset_bank >> 7));
        synth.process_midi_message(0, 0xB0, 0x20, i32::from(preset_bank & 0x7F));
        synth.process_midi_message(0, 0xC0, i32::from(program), 0);
        Ok(Self {
            synth,
            held: [false; 128],
            held_count: 0,
            tail_frames: 0,
        })
    }

    /// How much of the sound goes to the SoundFont's own reverb, 0 to 1
    /// (MIDI controller 91). New players start at the General MIDI default
    /// (40 of 127), a gentle room.
    pub fn set_reverb(&mut self, amount: f32) {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "rounded and clamped to 0..=127"
        )]
        let value = (amount.clamp(0.0, 1.0) * 127.0).round() as i32;
        self.synth.process_midi_message(0, 0xB0, 91, value);
    }
}

/// The synthesizer's master volume (its default is 0.5).
const PIANO_VOLUME: f32 = 1.0;

/// How long the player keeps rendering after its last note is released, so
/// release tails and reverb fade out naturally.
const TAIL_FRAMES: usize = 48_000 * 6;

impl Instrument for SoundFontPlayer {
    fn note_on(&mut self, key: u8, velocity: f32) {
        // MIDI velocity 0 means note off, so a very soft note is still 1.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "rounded and clamped to 1..=127"
        )]
        let velocity = ((velocity.clamp(0.0, 1.0) * 127.0).round() as i32).max(1);
        self.synth.note_on(0, i32::from(key), velocity);
        let slot = &mut self.held[usize::from(key & 0x7F)];
        if !*slot {
            *slot = true;
            self.held_count += 1;
        }
    }

    fn note_off(&mut self, key: u8) {
        self.synth.note_off(0, i32::from(key));
        let slot = &mut self.held[usize::from(key & 0x7F)];
        if *slot {
            *slot = false;
            self.held_count -= 1;
            self.tail_frames = TAIL_FRAMES;
        }
    }

    fn all_notes_off(&mut self, immediate: bool) {
        self.synth.note_off_all(immediate);
        self.held = [false; 128];
        self.held_count = 0;
        self.tail_frames = if immediate { 0 } else { TAIL_FRAMES };
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.synth.render(left, right);
        if self.held_count == 0 {
            self.tail_frames = self.tail_frames.saturating_sub(left.len());
        }
    }

    fn is_sounding(&self) -> bool {
        self.held_count > 0 || self.tail_frames > 0
    }
}

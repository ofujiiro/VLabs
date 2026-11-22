// SPDX-License-Identifier: GPL-3.0-or-later

//! What every instrument offers the engine.

/// A sound generator played by notes.
///
/// Every method is called on the audio thread and must not allocate, lock,
/// touch files or log. All memory an instrument needs is reserved when it is
/// created, off the audio thread.
pub trait Instrument: Send {
    /// Starts a note. `velocity` is from 0 to 1.
    fn note_on(&mut self, key: u8, velocity: f32);

    /// Releases a note.
    fn note_off(&mut self, key: u8);

    /// Releases every note; with `immediate`, silences them at once instead
    /// of letting them fade out.
    fn all_notes_off(&mut self, immediate: bool);

    /// Writes the next `left.len()` frames of sound (both slices have the
    /// same length), replacing what is in the buffers.
    fn render(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Whether the instrument is currently making any sound (lets the
    /// engine skip silent instruments).
    fn is_sounding(&self) -> bool {
        true
    }

    /// Changes the instrument's settings while it plays, without cutting
    /// off what is sounding (a drum kit's pad settings, for instance).
    /// Instruments without such settings ignore it.
    fn apply_settings(&mut self, settings: &InstrumentSettings) {
        let _ = settings;
    }

    /// Renders a block in which notes start and end at the given frames
    /// (sorted). The usual way splits the block at each note; plugins take
    /// the whole block with its notes in one go.
    fn render_block(&mut self, notes: &[NoteEvent], left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        let mut cursor = 0;
        for note in notes {
            let at = note.offset.min(len);
            if at > cursor {
                self.render(&mut left[cursor..at], &mut right[cursor..at]);
                cursor = at;
            }
            if note.on {
                self.note_on(note.key, note.velocity);
            } else {
                self.note_off(note.key);
            }
        }
        if cursor < len {
            self.render(&mut left[cursor..len], &mut right[cursor..len]);
        }
    }

    /// Changes one of the instrument's parameters (plugins), as a fraction
    /// of its range, from the next block on.
    fn set_parameter(&mut self, id: u32, value: f64) {
        let _ = (id, value);
    }

    /// Where the song is, given before each block (for instruments that
    /// follow the tempo).
    fn set_position(&mut self, position: &SongPosition) {
        let _ = position;
    }

    /// Called on the audio thread just before the instrument is handed back
    /// to be dropped elsewhere (a plugin stops processing here).
    fn retire(&mut self) {}

    /// For a plugin: which loaded plugin this is, so the rest of it can be
    /// found when this part comes back from the audio thread.
    fn plugin_instance(&self) -> Option<u64> {
        None
    }
}

/// A note starting or ending at a frame within a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    /// The frame within the block.
    pub offset: usize,
    /// The MIDI key.
    pub key: u8,
    /// How hard (for a start), from 0 to 1.
    pub velocity: f32,
    /// A start (`true`) or an end.
    pub on: bool,
}

/// Where the song is, for instruments and effects that follow it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongPosition {
    /// Whether the song is playing.
    pub playing: bool,
    /// Beats per minute.
    pub tempo: f64,
    /// The position in quarter notes from the start.
    pub beats: f64,
    /// Where the current bar starts, in quarter notes.
    pub bar_start: f64,
    /// The current bar, from 0.
    pub bar: u32,
    /// The time signature.
    pub numerator: u8,
    /// The time signature.
    pub denominator: u8,
    /// The position in frames from the start.
    pub frames: i64,
}

impl Default for SongPosition {
    fn default() -> Self {
        Self {
            playing: false,
            tempo: 120.0,
            beats: 0.0,
            bar_start: 0.0,
            bar: 0,
            numerator: 4,
            denominator: 4,
            frames: 0,
        }
    }
}

/// Settings an instrument takes while playing.
#[derive(Clone, Debug, PartialEq)]
pub enum InstrumentSettings {
    /// A drum kit's pad settings, indexed by each pad's main key.
    Drums(Box<crate::drums::PadTable>),
    /// The synth's settings (decision 0019).
    Synth(crate::synth::SynthParams),
    /// How notes start and end (decision 0031), for the instruments that
    /// play recorded sounds.
    Shape(NoteShape),
}

/// How notes start and end (decision 0031), in seconds; what is not set
/// keeps the sound's own. With a release, sounds that would play to their
/// end whatever the key does (a drum's hit) stop when their key comes up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoteShape {
    /// Time to rise from silence to full level.
    pub attack: Option<f32>,
    /// Time to fall silent once the key comes up.
    pub release: Option<f32>,
}

impl NoteShape {
    /// The shape with its times usable (finite, not negative).
    #[must_use]
    pub fn sane(self) -> Self {
        let sane = |v: Option<f32>| v.filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 60.0));
        Self {
            attack: sane(self.attack),
            release: sane(self.release),
        }
    }
}

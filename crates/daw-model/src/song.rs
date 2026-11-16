// SPDX-License-Identifier: GPL-3.0-or-later

//! The parts of a song: notes, patterns, tracks, clips.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::time::{Region, Ticks};

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(
    /// Identifies a note, stable across edits (used for selection).
    NoteId
);
id_type!(
    /// Identifies a pattern in the project's pattern pool.
    PatternId
);
id_type!(
    /// Identifies a track.
    TrackId
);
id_type!(
    /// Identifies a clip on a track.
    ClipId
);

/// The highest MIDI key.
pub const MAX_KEY: u8 = 127;

/// One note.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// Stable identity, for selection and editing.
    pub id: NoteId,
    /// Where the note starts, from the start of its pattern.
    pub start: Ticks,
    /// How long the note lasts (always positive).
    pub length: Ticks,
    /// The key, as a MIDI note number (60 is middle C).
    pub key: u8,
    /// How hard the note is played, from 0 (silent) to 1 (hardest).
    pub velocity: f32,
    /// A fine pitch offset in cents. Zero today; kept so microtonal
    /// tunings and per-note pitch bends can be added without changing the
    /// file format.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pitch_offset: f32,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

impl Note {
    /// Where the note ends.
    pub fn end(&self) -> Ticks {
        self.start + self.length
    }
}

/// The default velocity of a newly drawn note (100 on the MIDI scale).
pub const DEFAULT_VELOCITY: f32 = 100.0 / 127.0;

/// A reusable block of notes. Patterns can be placed many times on the
/// timeline (as clips) and edited in one place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    /// Identity.
    pub id: PatternId,
    /// The name shown to people.
    pub name: String,
    /// The pattern's length; it repeats every `length` when looped.
    pub length: Ticks,
    /// The notes, sorted by start, then key.
    pub notes: Vec<Note>,
}

impl Pattern {
    /// Sorts the notes into the canonical order (by start, then key).
    pub fn sort_notes(&mut self) {
        self.notes.sort_by_key(|n| (n.start, n.key, n.id));
    }

    /// A note by id.
    pub fn note(&self, id: NoteId) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    /// The span actually covered by notes, if there are any.
    pub fn used_region(&self) -> Option<Region> {
        let start = self.notes.iter().map(|n| n.start).min()?;
        let end = self.notes.iter().map(Note::end).max()?;
        Region::new(start, end)
    }
}

/// Where an instrument's sounds come from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundSource {
    /// A sound bundled with the program, by name.
    Builtin(String),
    /// A file chosen by the user, by path.
    File(String),
}

/// What makes the sound of an instrument track.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Instrument {
    /// A SoundFont (.sf2) player.
    SoundFont {
        /// Which SoundFont.
        source: SoundSource,
        /// The preset's bank number.
        bank: u16,
        /// The preset's program number.
        program: u8,
    },
}

impl Instrument {
    /// The bundled piano.
    pub fn builtin_piano() -> Self {
        Self::SoundFont {
            source: SoundSource::Builtin(BUILTIN_PIANO.to_owned()),
            bank: 0,
            program: 0,
        }
    }
}

/// The name of the bundled piano SoundFont.
pub const BUILTIN_PIANO: &str = "piano";

/// Volume, pan, mute and solo of a track.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelStrip {
    /// Linear gain: 1 is unchanged, 0 silent, up to 2 (+6 dB).
    pub gain: f32,
    /// Stereo position from -1 (left) to 1 (right).
    pub pan: f32,
    /// Silenced.
    pub mute: bool,
    /// Heard alone (with any other soloed tracks).
    pub solo: bool,
}

impl Default for ChannelStrip {
    fn default() -> Self {
        Self {
            gain: 1.0,
            pan: 0.0,
            mute: false,
            solo: false,
        }
    }
}

/// What a clip plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ClipContent {
    /// A pattern, repeated for the length of the clip.
    Pattern {
        /// The pattern.
        pattern: PatternId,
        /// Where in the pattern the clip starts playing.
        offset: Ticks,
    },
}

/// A placement of content on a track's timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    /// Identity.
    pub id: ClipId,
    /// Where on the timeline the clip starts.
    pub start: Ticks,
    /// How long the clip lasts.
    pub length: Ticks,
    /// What it plays.
    pub content: ClipContent,
}

impl Clip {
    /// Where the clip ends.
    pub fn end(&self) -> Ticks {
        self.start + self.length
    }
}

/// A track: an instrument with its clips and its channel strip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Identity.
    pub id: TrackId,
    /// The name shown to people.
    pub name: String,
    /// What makes the sound.
    pub instrument: Instrument,
    /// The clips, sorted by start.
    pub clips: Vec<Clip>,
    /// Volume, pan, mute, solo.
    pub strip: ChannelStrip,
}

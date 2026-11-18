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
id_type!(
    /// Identifies an effect in a chain.
    EffectId
);
id_type!(
    /// Identifies an audio file of the project.
    AudioId
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
    /// The track whose instrument plays it (patterns are placed on their
    /// own track's timeline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackId>,
    /// The step sequencer's step length for this pattern.
    #[serde(default = "default_step")]
    pub step: Ticks,
}

/// The step length of a new pattern: a sixteenth note.
pub fn default_step() -> Ticks {
    Ticks::note_value(1, 16)
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// The drum sampler with a bundled kit.
    Drums {
        /// Which kit, by name ([`ACOUSTIC_KIT`] or [`ELECTRONIC_KIT`]).
        kit: String,
        /// Pads whose settings differ from the defaults.
        #[serde(default)]
        pads: Vec<PadSetting>,
    },
    /// A third-party plugin instrument.
    Plugin(crate::plugin::PluginSetup),
    /// No instrument: the track plays the recorded or imported sound of its
    /// audio clips.
    Audio,
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

    /// A drum sampler with a bundled kit.
    pub fn drums(kit: &str) -> Self {
        Self::Drums {
            kit: kit.to_owned(),
            pads: Vec::new(),
        }
    }

    /// Whether this is a drum kit.
    pub fn is_drums(&self) -> bool {
        matches!(self, Self::Drums { .. })
    }

    /// Whether the track plays audio clips rather than notes.
    pub fn is_audio(&self) -> bool {
        matches!(self, Self::Audio)
    }

    /// The plugin, if this is a plugin instrument.
    pub fn plugin(&self) -> Option<&crate::plugin::PluginSetup> {
        match self {
            Self::Plugin(setup) => Some(setup),
            _ => None,
        }
    }

    /// Whether two instruments make their sound from the same source, so
    /// only settings differ (they can change while sound plays).
    pub fn same_source(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Drums { kit: a, .. }, Self::Drums { kit: b, .. }) => a == b,
            // A plugin's settings live in the plugin while it plays.
            (Self::Plugin(a), Self::Plugin(b)) => a.plugin == b.plugin,
            _ => self == other,
        }
    }

    /// The settings of the pad on `key` (the defaults if never changed).
    pub fn pad(&self, key: u8) -> PadSetting {
        match self {
            Self::Drums { pads, .. } => pads
                .iter()
                .find(|p| p.key == key)
                .copied()
                .unwrap_or_else(|| PadSetting::new(key)),
            Self::SoundFont { .. } | Self::Plugin(_) | Self::Audio => PadSetting::new(key),
        }
    }

    /// Changes a pad's settings (drum kits only).
    pub fn set_pad(&mut self, setting: PadSetting) {
        if let Self::Drums { pads, .. } = self {
            pads.retain(|p| p.key != setting.key);
            if setting != PadSetting::new(setting.key) {
                pads.push(setting);
                pads.sort_by_key(|p| p.key);
            }
        }
    }
}

/// The name of the bundled acoustic drum kit.
pub const ACOUSTIC_KIT: &str = "acoustic";
/// The name of the bundled electronic drum kit.
pub const ELECTRONIC_KIT: &str = "electronic";

/// The settings of one drum pad.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PadSetting {
    /// The pad's key.
    pub key: u8,
    /// Volume change, in dB.
    #[serde(default)]
    pub gain_db: f32,
    /// Position, from -1 (left) to 1 (right).
    #[serde(default)]
    pub pan: f32,
    /// Pitch change, in semitones.
    #[serde(default)]
    pub tune: f32,
    /// How much of the sound plays, from 0.05 to 1.
    #[serde(default = "full_length")]
    pub length: f32,
    /// Silenced.
    #[serde(default)]
    pub muted: bool,
}

fn full_length() -> f32 {
    1.0
}

impl PadSetting {
    /// The default settings of a pad.
    pub fn new(key: u8) -> Self {
        Self {
            key,
            gain_db: 0.0,
            pan: 0.0,
            tune: 0.0,
            length: 1.0,
            muted: false,
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ClipContent {
    /// A pattern, repeated for the length of the clip.
    Pattern {
        /// The pattern.
        pattern: PatternId,
        /// Where in the pattern the clip starts playing.
        offset: Ticks,
    },
    /// Part of an audio file, at its natural speed (decision 0017).
    Audio {
        /// The file.
        audio: AudioId,
        /// The frame of the file (at its own rate) heard at the clip's start.
        offset: u64,
        /// Volume: 1 is unchanged.
        #[serde(default = "unity", skip_serializing_if = "is_unity")]
        gain: f32,
        /// How long the sound takes to rise from silence at the start, in
        /// seconds.
        #[serde(default, skip_serializing_if = "is_zero")]
        fade_in: f32,
        /// How long the sound takes to fall to silence at the end, in
        /// seconds.
        #[serde(default, skip_serializing_if = "is_zero")]
        fade_out: f32,
    },
}

fn unity() -> f32 {
    1.0
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_unity(value: &f32) -> bool {
    value.to_bits() == 1.0_f32.to_bits()
}

impl ClipContent {
    /// Audio content: a file from `offset`, unchanged in volume, no fades.
    pub fn audio(audio: AudioId, offset: u64) -> Self {
        Self::Audio {
            audio,
            offset,
            gain: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
        }
    }
}

/// A placement of content on a track's timeline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

    /// The pattern the clip plays, if it plays one.
    pub fn pattern(&self) -> Option<PatternId> {
        match self.content {
            ClipContent::Pattern { pattern, .. } => Some(pattern),
            ClipContent::Audio { .. } => None,
        }
    }

    /// The audio file the clip plays, if it plays one.
    pub fn audio(&self) -> Option<AudioId> {
        match self.content {
            ClipContent::Audio { audio, .. } => Some(audio),
            ClipContent::Pattern { .. } => None,
        }
    }

    /// Whether the clip plays sound from an audio file.
    pub fn is_audio(&self) -> bool {
        matches!(self.content, ClipContent::Audio { .. })
    }
}

/// An audio file of the project: recorded, or imported. The sound itself is
/// kept outside the model (decision 0017); this says where to find it and
/// what it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFile {
    /// Identity.
    pub id: AudioId,
    /// The name shown to people (an imported file's name, "Recording 2").
    pub name: String,
    /// The name it is stored under, in the song file's audio folder and
    /// while the song is open.
    pub file: String,
    /// How long it is, in frames at its own rate.
    pub frames: u64,
    /// Its sample rate, in frames per second.
    pub sample_rate: u32,
    /// Its channels: 1 (mono) or 2 (stereo).
    pub channels: u16,
}

impl AudioFile {
    /// How long it lasts, in seconds.
    #[allow(clippy::cast_precision_loss, reason = "frame counts fit easily")]
    pub fn seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate.max(1))
    }

    /// A duration in seconds as a number of the file's frames (rounded).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rounded and clamped at zero"
    )]
    pub fn frames_in(&self, seconds: f64) -> u64 {
        (seconds * f64::from(self.sample_rate)).round().max(0.0) as u64
    }

    /// A number of the file's frames as seconds.
    #[allow(clippy::cast_precision_loss, reason = "frame counts fit easily")]
    pub fn seconds_in(&self, frames: u64) -> f64 {
        frames as f64 / f64::from(self.sample_rate.max(1))
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
    /// The effect chain, in order.
    #[serde(default)]
    pub effects: Vec<crate::mixer::Effect>,
    /// Settings that change over the song.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub automation: Vec<crate::automation::AutomationLane>,
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The parts of a song: notes, patterns, tracks, clips.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::expression::NoteExpression;
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
    /// How the pattern swings as it plays (decision 0018).
    #[serde(default, skip_serializing_if = "Swing::is_straight")]
    pub swing: Swing,
    /// What its notes sing, for a voice (decision 0027): only the notes
    /// that have a lyric of their own, by note.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lyrics: BTreeMap<NoteId, String>,
    /// How its notes are sung, for a voice (decision 0028): only the notes
    /// sung some other way than the plain one, by note.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub expression: BTreeMap<NoteId, NoteExpression>,
    /// What a MIDI controller's wheels, pedals and knobs do as it plays
    /// (decision 0033): one curve per controller, by controller.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<crate::ControlLane>,
}

/// The step length of a new pattern: a sixteenth note.
pub fn default_step() -> Ticks {
    Ticks::note_value(1, 16)
}

/// Swing (decision 0018): every second `unit` of a pattern plays late, and
/// everything between grid lines moves with it. Notes stay where they are
/// written; the warp applies as the pattern plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swing {
    /// How late the off-beats are, from 0 (straight) to 100 (as late as a
    /// dotted note: three to one). 67 is the triplet feel (two to one).
    pub percent: u8,
    /// The note value that swings: an eighth or a sixteenth.
    pub unit: Ticks,
}

impl Default for Swing {
    fn default() -> Self {
        Self {
            percent: 0,
            unit: Ticks::note_value(1, 16),
        }
    }
}

impl Swing {
    /// The most swing there is.
    pub const MAX_PERCENT: u8 = 100;

    /// Whether the pattern plays straight.
    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    pub fn is_straight(&self) -> bool {
        self.percent == 0 || self.unit <= Ticks::ZERO
    }

    /// Where a position written in the pattern plays. In each pair of
    /// units, the first is stretched by `(200 + percent) / 200` and the
    /// second squeezed to fit; exact in integers, rounded to the tick.
    pub fn apply(self, at: Ticks) -> Ticks {
        if self.is_straight() {
            return at;
        }
        let unit = i128::from(self.unit.0);
        let percent = i128::from(self.percent.min(Self::MAX_PERCENT));
        let period = 2 * unit;
        let at = i128::from(at.0);
        let (pair, into) = (at.div_euclid(period), at.rem_euclid(period));
        let swung = if into < unit {
            rounded_div(into * (200 + percent), 200)
        } else {
            rounded_div(
                unit * (200 + percent) + (into - unit) * (200 - percent),
                200,
            )
        };
        Ticks(i64::try_from(pair * period + swung).unwrap_or(i64::MAX))
    }

    /// Where a position that played (a note recorded into a swung pattern)
    /// is written: the inverse of [`Self::apply`].
    pub fn remove(self, at: Ticks) -> Ticks {
        if self.is_straight() {
            return at;
        }
        let unit = i128::from(self.unit.0);
        let percent = i128::from(self.percent.min(Self::MAX_PERCENT));
        let period = 2 * unit;
        let at = i128::from(at.0);
        let (pair, into) = (at.div_euclid(period), at.rem_euclid(period));
        let split = unit * (200 + percent);
        let written = if into * 200 < split {
            rounded_div(into * 200, 200 + percent)
        } else {
            unit + rounded_div(into * 200 - split, 200 - percent)
        };
        Ticks(i64::try_from(pair * period + written).unwrap_or(i64::MAX))
    }
}

/// `a / b` rounded to the nearest whole number (b positive).
fn rounded_div(a: i128, b: i128) -> i128 {
    (2 * a + b).div_euclid(2 * b)
}

/// A note as a clip plays it: positions on the song's timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayedNote {
    /// Where it starts.
    pub start: Ticks,
    /// Where it ends (after its start).
    pub end: Ticks,
    /// The key.
    pub key: u8,
    /// How hard, from 0 to 1.
    pub velocity: f32,
    /// The note of the pattern it comes from (for its lyric).
    pub note: NoteId,
}

impl Pattern {
    /// The notes a clip of this pattern plays, in order: the pattern
    /// repeated from the clip's offset for the clip's length, swung as it
    /// plays (decision 0018), every note cut at the clip's end. This is the
    /// one place that says what a clip plays: the engine and the exporters
    /// both use it.
    pub fn played_in(&self, clip: &Clip) -> Vec<PlayedNote> {
        let ClipContent::Pattern { offset, .. } = clip.content else {
            return Vec::new();
        };
        let mut notes = Vec::new();
        if self.length <= Ticks::ZERO {
            return notes;
        }
        let mut cycle_start = clip.start - offset;
        while cycle_start < clip.end() {
            for note in &self.notes {
                if note.start >= self.length {
                    continue;
                }
                let start = cycle_start + self.swing.apply(note.start);
                let end = (cycle_start + self.swing.apply(note.end())).min(clip.end());
                if start >= clip.start && start < clip.end() && end > start {
                    notes.push(PlayedNote {
                        start,
                        end,
                        key: note.key,
                        velocity: note.velocity,
                        note: note.id,
                    });
                }
            }
            cycle_start += self.length;
        }
        notes.sort_by_key(|n| (n.start, n.key));
        notes
    }

    /// The curves a clip of this pattern plays (decision 0033), each as
    /// points in song ticks: the pattern repeated from the clip's offset for
    /// the clip's length, swung as its notes are, with a point at the clip's
    /// start and end so the curve is exact inside the clip. Like
    /// [`Self::played_in`], the one place that says it, for the engine and
    /// the exporters.
    pub fn controls_played_in(&self, clip: &Clip) -> Vec<PlayedControl> {
        let ClipContent::Pattern { offset, .. } = clip.content else {
            return Vec::new();
        };
        if self.length <= Ticks::ZERO {
            return Vec::new();
        }
        let mut played = Vec::new();
        for lane in self.controls.iter().filter(|l| !l.points.is_empty()) {
            // The curve over one turn of the pattern, from its start to its
            // end, with the turn's ends pinned.
            #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
            let at = |t: Ticks| lane.value_at(t.0 as f64).unwrap_or(0.0);
            let mut turn: Vec<(Ticks, f32)> = vec![(Ticks::ZERO, at(Ticks::ZERO))];
            turn.extend(
                lane.points
                    .iter()
                    .filter(|p| p.at > Ticks::ZERO && p.at < self.length)
                    .map(|p| (p.at, p.value)),
            );
            turn.push((self.length, at(self.length)));
            let mut points: Vec<(Ticks, f32)> = Vec::new();
            let mut cycle_start = clip.start - offset;
            while cycle_start < clip.end() {
                for &(tick, value) in &turn {
                    let song = cycle_start + self.swing.apply(tick);
                    points.push((song, value));
                }
                cycle_start += self.length;
            }
            // Only what lies in the clip, with its edges pinned.
            let value_at = |t: Ticks| -> f32 {
                let next = points.partition_point(|p| p.0 <= t);
                match (next.checked_sub(1).map(|i| points[i]), points.get(next)) {
                    (Some(a), Some(b)) if b.0 > a.0 => {
                        #[allow(clippy::cast_precision_loss, reason = "tick spans fit easily")]
                        let f = (t.0 - a.0.0) as f32 / (b.0.0 - a.0.0) as f32;
                        a.1 + (b.1 - a.1) * f
                    }
                    (Some(a), _) => a.1,
                    (None, Some(b)) => b.1,
                    (None, None) => 0.0,
                }
            };
            let mut inside = vec![(clip.start, value_at(clip.start))];
            inside.extend(
                points
                    .iter()
                    .filter(|p| p.0 > clip.start && p.0 < clip.end())
                    .copied(),
            );
            inside.push((clip.end(), value_at(clip.end())));
            inside.dedup_by(|b, a| a.0 == b.0 && a.1.to_bits() == b.1.to_bits());
            played.push(PlayedControl {
                control: lane.control,
                points: inside,
            });
        }
        played
    }

    /// The curve of a controller, if the pattern has one (decision 0033).
    pub fn control(&self, control: crate::ControlKind) -> Option<&crate::ControlLane> {
        self.controls.iter().find(|l| l.control == control)
    }

    /// Sets a controller's curve (no points removes it), keeping the
    /// curves in the controllers' order.
    pub fn set_control(&mut self, control: crate::ControlKind, points: Vec<crate::ControlPoint>) {
        self.controls.retain(|l| l.control != control);
        let lane = crate::ControlLane::new(control, points);
        if !lane.points.is_empty() {
            self.controls.push(lane);
            self.controls.sort_by_key(|l| l.control);
        }
    }

    /// Sorts the notes into the canonical order (by start, then key).
    pub fn sort_notes(&mut self) {
        self.notes.sort_by_key(|n| (n.start, n.key, n.id));
    }

    /// A note by id.
    pub fn note(&self, id: NoteId) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    /// The lyric of a note (decision 0027), if it has one of its own.
    pub fn lyric(&self, id: NoteId) -> Option<&str> {
        self.lyrics.get(&id).map(String::as_str)
    }

    /// Sets a note's lyric; an empty one removes it (the note sings the
    /// default). Ignored for a note the pattern does not have.
    pub fn set_lyric(&mut self, id: NoteId, lyric: &str) {
        let lyric = lyric.trim();
        if lyric.is_empty() {
            self.lyrics.remove(&id);
        } else if self.note(id).is_some() {
            self.lyrics.insert(id, lyric.to_owned());
        }
    }

    /// How a note is sung (decision 0028), if some other way than the
    /// plain one.
    pub fn expression(&self, id: NoteId) -> Option<&NoteExpression> {
        self.expression.get(&id)
    }

    /// Sets how a note is sung (its numbers kept in their ranges); the
    /// plain way removes it. Ignored for a note the pattern does not have.
    pub fn set_expression(&mut self, id: NoteId, expression: NoteExpression) {
        let expression = expression.sanitized();
        if expression.is_plain() {
            self.expression.remove(&id);
        } else if self.note(id).is_some() {
            self.expression.insert(id, expression);
        }
    }

    /// Forgets the lyrics and expression of notes that are gone.
    pub fn tidy_note_data(&mut self) {
        if self.lyrics.is_empty() && self.expression.is_empty() {
            return;
        }
        let ids: std::collections::HashSet<NoteId> = self.notes.iter().map(|n| n.id).collect();
        self.lyrics.retain(|id, _| ids.contains(id));
        self.expression.retain(|id, _| ids.contains(id));
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
    /// The built-in subtractive synth (decision 0019).
    Synth(crate::synth::SynthSettings),
    /// An SFZ instrument (decision 0024): sounds the person has, played by
    /// the sampler as the `.sfz` file says.
    Sfz {
        /// Where the `.sfz` file is.
        file: String,
    },
    /// A custom kit (decision 0025): the person's own sounds, one on each
    /// key, played like a drum kit.
    Kit {
        /// Its pads, by key.
        #[serde(default)]
        pads: Vec<KitPad>,
    },
    /// A singing voice (decision 0027): a voicebank sings the notes'
    /// lyrics.
    Voice {
        /// The voicebank, by the name of its folder among the installed
        /// ones.
        voicebank: String,
    },
}

/// One pad of a custom kit (decision 0025): a sound the person brought,
/// played on its key, with the drum pads' settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KitPad {
    /// Its key, and its volume, position, pitch, length and mute.
    pub setting: PadSetting,
    /// Its name, as the step grid shows it.
    pub name: String,
    /// The song's audio file it plays.
    pub audio: AudioId,
}

/// The key a custom kit's first pad takes (C1, the usual kick key).
pub const KIT_FIRST_KEY: u8 = 36;

/// The most pads a custom kit holds: one on each key.
pub const KIT_MAX_PADS: usize = 128;

/// The key a new pad of a custom kit takes: the first free one from C1 up,
/// then below.
pub fn free_kit_key(pads: &[KitPad]) -> Option<u8> {
    (KIT_FIRST_KEY..=127)
        .chain(0..KIT_FIRST_KEY)
        .find(|key| pads.iter().all(|p| p.setting.key != *key))
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

    /// Whether this is a drum kit: a bundled one, or a custom kit
    /// (decision 0025), both played from the step grid.
    pub fn is_drums(&self) -> bool {
        matches!(self, Self::Drums { .. } | Self::Kit { .. })
    }

    /// Whether this is a custom kit (decision 0025).
    pub fn is_custom_kit(&self) -> bool {
        matches!(self, Self::Kit { .. })
    }

    /// A custom kit's pads (none for other instruments).
    pub fn kit_pads(&self) -> &[KitPad] {
        match self {
            Self::Kit { pads } => pads,
            _ => &[],
        }
    }

    /// Whether a note shape (decision 0031) changes how this instrument
    /// sounds: it plays recorded sounds itself.
    pub fn takes_shape(&self) -> bool {
        matches!(
            self,
            Self::SoundFont { .. } | Self::Drums { .. } | Self::Sfz { .. } | Self::Kit { .. }
        )
    }

    /// Whether a MIDI controller's wheels and pedals change how this
    /// instrument plays (decision 0033): everything but drum kits, voices
    /// (which sing their own pitch line) and audio tracks.
    pub fn takes_controls(&self) -> bool {
        matches!(
            self,
            Self::SoundFont { .. } | Self::Synth(_) | Self::Sfz { .. } | Self::Plugin(_)
        )
    }

    /// Whether the track plays audio clips rather than notes.
    pub fn is_audio(&self) -> bool {
        matches!(self, Self::Audio)
    }

    /// The synth's settings, if this is the built-in synth.
    pub fn synth(&self) -> Option<&crate::synth::SynthSettings> {
        match self {
            Self::Synth(settings) => Some(settings),
            _ => None,
        }
    }

    /// The `.sfz` file, if this is an SFZ instrument.
    pub fn sfz_file(&self) -> Option<&str> {
        match self {
            Self::Sfz { file } => Some(file),
            _ => None,
        }
    }

    /// The voicebank, if this is a singing voice.
    pub fn voicebank(&self) -> Option<&str> {
        match self {
            Self::Voice { voicebank } => Some(voicebank),
            _ => None,
        }
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
            // The synth takes new settings while it plays.
            (Self::Synth(_), Self::Synth(_)) => true,
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
            Self::Kit { pads } => pads
                .iter()
                .find(|p| p.setting.key == key)
                .map_or_else(|| PadSetting::new(key), |p| p.setting),
            Self::SoundFont { .. }
            | Self::Plugin(_)
            | Self::Audio
            | Self::Synth(_)
            | Self::Sfz { .. }
            | Self::Voice { .. } => PadSetting::new(key),
        }
    }

    /// Changes a pad's settings (drum kits only).
    pub fn set_pad(&mut self, setting: PadSetting) {
        match self {
            Self::Drums { pads, .. } => {
                pads.retain(|p| p.key != setting.key);
                if setting != PadSetting::new(setting.key) {
                    pads.push(setting);
                    pads.sort_by_key(|p| p.key);
                }
            }
            Self::Kit { pads } => {
                if let Some(pad) = pads.iter_mut().find(|p| p.setting.key == setting.key) {
                    pad.setting = setting;
                }
            }
            _ => {}
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

/// How a track's notes start and end (decision 0031), for the instruments
/// that play recorded sounds: SoundFonts, SFZ instruments, drum kits and
/// custom kits. What is not set keeps the instrument's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NoteShape {
    /// Seconds a note takes to rise from silence to full level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack: Option<f32>,
    /// Seconds a note takes to fall silent once its key comes up. With a
    /// release, the sounds of a kit stop at their note's end too, and a
    /// very short one cuts every note off as its key comes up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<f32>,
}

impl NoteShape {
    /// The shortest and longest attack, in seconds.
    pub const ATTACK_RANGE: (f32, f32) = (0.0, 2.0);
    /// The shortest and longest release, in seconds (the shortest still
    /// fades over a few milliseconds, so a cut never clicks).
    pub const RELEASE_RANGE: (f32, f32) = (0.005, 5.0);
    /// The release that cuts notes off at their end.
    pub const CUT_RELEASE: f32 = 0.01;

    /// The instrument's own shape.
    pub const NATURAL: Self = Self {
        attack: None,
        release: None,
    };

    /// Whether nothing is changed.
    pub fn is_natural(&self) -> bool {
        self.attack.is_none() && self.release.is_none()
    }

    /// Whether notes are cut off at their end.
    pub fn is_cut(&self) -> bool {
        self.release.is_some_and(|r| r <= Self::CUT_RELEASE + 1e-6)
    }

    /// The shape with its times within their ranges (a damaged file cannot
    /// ask for anything else).
    #[must_use]
    pub fn clamped(self) -> Self {
        let within = |value: Option<f32>, (low, high): (f32, f32)| {
            value.filter(|v| v.is_finite()).map(|v| v.clamp(low, high))
        };
        Self {
            attack: within(self.attack, Self::ATTACK_RANGE),
            release: within(self.release, Self::RELEASE_RANGE),
        }
    }
}

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

/// A controller's curve as a clip plays it (decision 0033): points in
/// song ticks, from the clip's start to its end, a straight line between.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayedControl {
    /// What it moves.
    pub control: crate::ControlKind,
    /// (song tick, value), in time order, the first at the clip's start and
    /// the last at its end.
    pub points: Vec<(Ticks, f32)>,
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
        /// Played backwards (decision 0031): from `offset` towards the
        /// file's start.
        #[serde(default, skip_serializing_if = "is_false")]
        reverse: bool,
        /// Pitch change in semitones (decision 0031), by playing faster or
        /// slower, as a tape does: up an octave plays twice as fast.
        #[serde(default, skip_serializing_if = "is_zero")]
        pitch: f32,
    },
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// The lowest and highest pitch change of an audio clip, in semitones.
pub const CLIP_PITCH_RANGE: (f32, f32) = (-24.0, 24.0);

/// How fast sound plays for a pitch change in semitones: 2 an octave up.
pub fn pitch_speed(semitones: f32) -> f64 {
    let semitones = if semitones.is_finite() {
        semitones.clamp(CLIP_PITCH_RANGE.0, CLIP_PITCH_RANGE.1)
    } else {
        0.0
    };
    2.0_f64.powf(f64::from(semitones) / 12.0)
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
            reverse: false,
            pitch: 0.0,
        }
    }

    /// For audio: how many seconds of its file a second of the clip plays
    /// (its pitch change's speed), and whether it plays backwards.
    pub fn audio_motion(&self) -> Option<(f64, bool)> {
        match *self {
            Self::Audio { pitch, reverse, .. } => Some((pitch_speed(pitch), reverse)),
            Self::Pattern { .. } => None,
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
    /// How its notes start and end (decision 0031), for instruments that
    /// play recorded sounds.
    #[serde(default, skip_serializing_if = "NoteShape::is_natural")]
    pub shape: NoteShape,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swing_delays_the_off_beats_only() {
        let sixteenth = Ticks::note_value(1, 16);
        let straight = Swing::default();
        assert!(straight.is_straight());
        assert_eq!(straight.apply(sixteenth), sixteenth);
        let full = Swing {
            percent: 100,
            unit: sixteenth,
        };
        // At full swing the off-beat sixteenth lands on a dotted sixteenth.
        assert_eq!(full.apply(sixteenth), Ticks::note_value(3, 32));
        for beat in 0..8 {
            let on = sixteenth * (2 * beat);
            assert_eq!(full.apply(on), on, "the beats stay");
        }
        let triplet = Swing {
            percent: 67,
            unit: Ticks::note_value(1, 8),
        };
        let off = triplet.apply(Ticks::note_value(1, 8));
        let ideal = Ticks::note_value(1, 6);
        assert!(
            (off - ideal).0.abs() < Ticks::note_value(1, 8).0 / 200,
            "67 % is within half a percent of the triplet feel"
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss, reason = "test arithmetic")]
    fn a_clip_plays_a_curve_repeated_from_its_offset_and_pinned_at_its_edges() {
        use crate::{ControlKind, ControlPoint};
        let bar = Ticks::WHOLE;
        let mut pattern = Pattern {
            id: PatternId(1),
            name: String::new(),
            length: bar,
            notes: Vec::new(),
            track: None,
            step: default_step(),
            swing: Swing::default(),
            lyrics: BTreeMap::new(),
            expression: BTreeMap::new(),
            controls: Vec::new(),
        };
        // The wheel rises over the first half of the bar, then holds.
        pattern.set_control(
            ControlKind::PitchBend,
            vec![
                ControlPoint {
                    at: Ticks::ZERO,
                    value: 0.0,
                },
                ControlPoint {
                    at: bar / 2,
                    value: 1.0,
                },
            ],
        );
        // A clip from bar 4 for a bar and a half, starting a quarter of the
        // way into the pattern.
        let clip = Clip {
            id: ClipId(9),
            start: bar * 4,
            length: bar + bar / 2,
            content: ClipContent::Pattern {
                pattern: PatternId(1),
                offset: bar / 4,
            },
        };
        let played = pattern.controls_played_in(&clip);
        assert_eq!(played.len(), 1);
        let curve = &played[0];
        assert_eq!(curve.control, ControlKind::PitchBend);
        let at = |t: Ticks| -> f32 {
            let next = curve.points.partition_point(|p| p.0 <= t);
            let (a, b) = (curve.points[next - 1], curve.points.get(next).copied());
            match b {
                Some(b) if b.0 > a.0 => {
                    a.1 + (b.1 - a.1) * (t.0 - a.0.0) as f32 / (b.0.0 - a.0.0) as f32
                }
                _ => a.1,
            }
        };
        assert_eq!(curve.points.first().map(|p| p.0), Some(clip.start));
        assert_eq!(curve.points.last().map(|p| p.0), Some(clip.end()));
        assert!((at(clip.start) - 0.5).abs() < 1e-6, "starts a quarter in");
        assert!(
            (at(bar * 4 + bar / 4) - 1.0).abs() < 1e-6,
            "the top at half a bar"
        );
        assert!((at(bar * 4 + bar / 2) - 1.0).abs() < 1e-6, "held");
        // The pattern's turn ends and begins again at the rest, three
        // quarters into the clip.
        let turn = bar * 4 + bar * 3 / 4;
        assert!(
            (at(turn) - 0.0).abs() < 1e-6,
            "the next turn starts at rest"
        );
        assert!((at(turn + bar / 4) - 0.5).abs() < 1e-6, "and rises again");
        assert!(
            (at(clip.end()) - 1.0).abs() < 1e-6,
            "pinned at the clip's end"
        );
        assert!(
            curve
                .points
                .iter()
                .all(|p| p.0 >= clip.start && p.0 <= clip.end()),
            "nothing outside the clip"
        );
    }

    #[test]
    fn swing_is_a_smooth_warp_that_can_be_undone() {
        let swing = Swing {
            percent: 45,
            unit: Ticks::note_value(1, 16),
        };
        let mut last = Ticks(i64::MIN);
        for step in -40..400 {
            let at = Ticks(step * 12_345);
            let played = swing.apply(at);
            assert!(played >= last, "the warp never goes backwards");
            last = played;
            let back = swing.remove(played);
            assert!((back - at).0.abs() <= 1, "{at:?} comes back as {back:?}");
        }
    }
}

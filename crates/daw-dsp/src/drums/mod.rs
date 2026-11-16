// SPDX-License-Identifier: GPL-3.0-or-later

//! Drums: the pads of a kit, the kits themselves (one recorded, one
//! synthesised), and the sampler that plays them.
//!
//! Pads sit on the General MIDI drum keys (kick on 36, snare on 38...), so
//! MIDI drum pads and drum patterns from other programs play the right
//! sounds, and a pattern plays sensibly on either kit.

mod bundle;
mod sampler;
mod synth;

use std::sync::Arc;

pub use bundle::{BUNDLE_MAGIC, BundleEntry, read_bundle, write_bundle};
pub use sampler::{DrumSampler, VOICES};

/// The sounds a kit can have, one per pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PadSound {
    /// Bass drum.
    Kick,
    /// Snare drum, struck in the middle.
    Snare,
    /// Hand clap.
    Clap,
    /// Hi-hat, closed.
    ClosedHat,
    /// Hi-hat, closed with the foot.
    PedalHat,
    /// Hi-hat, open.
    OpenHat,
    /// Snare drum, rim and head together.
    Rimshot,
    /// Snare drum, stick laid across the rim.
    SideStick,
    /// Floor tom.
    LowTom,
    /// Middle tom.
    MidTom,
    /// High tom.
    HighTom,
    /// Crash cymbal.
    Crash,
    /// Ride cymbal.
    Ride,
    /// Ride cymbal, struck on its bell.
    RideBell,
    /// Cowbell.
    Cowbell,
}

impl PadSound {
    /// Every pad, in the order the step sequencer lists them: the most
    /// used first.
    pub const ALL: [Self; 15] = [
        Self::Kick,
        Self::Snare,
        Self::Clap,
        Self::ClosedHat,
        Self::OpenHat,
        Self::PedalHat,
        Self::Rimshot,
        Self::SideStick,
        Self::LowTom,
        Self::MidTom,
        Self::HighTom,
        Self::Crash,
        Self::Ride,
        Self::RideBell,
        Self::Cowbell,
    ];

    /// The pad's General MIDI key.
    pub fn key(self) -> u8 {
        match self {
            Self::Kick => 36,
            Self::SideStick => 37,
            Self::Snare => 38,
            Self::Clap => 39,
            Self::Rimshot => 40,
            Self::LowTom => 41,
            Self::ClosedHat => 42,
            Self::PedalHat => 44,
            Self::MidTom => 45,
            Self::OpenHat => 46,
            Self::HighTom => 48,
            Self::Crash => 49,
            Self::Ride => 51,
            Self::RideBell => 53,
            Self::Cowbell => 56,
        }
    }

    /// The pad a key plays, including the other General MIDI keys for
    /// similar sounds (a second kick, more toms and cymbals).
    pub fn from_key(key: u8) -> Option<Self> {
        Some(match key {
            35 | 36 => Self::Kick,
            37 => Self::SideStick,
            38 => Self::Snare,
            39 => Self::Clap,
            40 => Self::Rimshot,
            41 | 43 => Self::LowTom,
            42 => Self::ClosedHat,
            44 => Self::PedalHat,
            45 | 47 => Self::MidTom,
            46 => Self::OpenHat,
            48 | 50 => Self::HighTom,
            49 | 52 | 55 | 57 => Self::Crash,
            51 | 59 => Self::Ride,
            53 => Self::RideBell,
            56 => Self::Cowbell,
            _ => return None,
        })
    }

    /// Pads in the same group cut each other off (an open hi-hat stops when
    /// the hi-hat closes).
    pub fn choke_group(self) -> Option<u8> {
        matches!(self, Self::ClosedHat | Self::PedalHat | Self::OpenHat).then_some(1)
    }

    /// The pad's position in [`PadSound::ALL`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// A recorded or synthesised sound, as stereo samples.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    /// The rate it was recorded at.
    pub sample_rate: u32,
    /// Left channel (or the only channel).
    pub left: Vec<f32>,
    /// Right channel; empty for a mono sound.
    pub right: Vec<f32>,
}

impl Sample {
    /// Length in frames.
    pub fn len(&self) -> usize {
        self.left.len()
    }

    /// Whether it holds no sound at all.
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    /// The right channel, or the left for a mono sound.
    pub fn right_or_left(&self) -> &[f32] {
        if self.right.is_empty() {
            &self.left
        } else {
            &self.right
        }
    }

    /// The loudest sample value.
    pub fn peak(&self) -> f32 {
        self.left
            .iter()
            .chain(&self.right)
            .fold(0.0, |m, s| m.max(s.abs()))
    }
}

/// One velocity layer of a pad: played for velocities up to `top`.
#[derive(Clone, Debug)]
pub struct Layer {
    /// The highest velocity (1 to 127) this layer plays.
    pub top: u8,
    /// The sound.
    pub sample: Arc<Sample>,
}

/// A pad of a kit: its sound in one or more velocity layers.
#[derive(Clone, Debug)]
pub struct Pad {
    /// Which sound it is.
    pub sound: PadSound,
    /// Layers from softest to loudest.
    pub layers: Vec<Layer>,
}

impl Pad {
    /// The layer for a velocity (1 to 127).
    pub fn layer(&self, velocity: u8) -> Option<&Layer> {
        self.layers
            .iter()
            .find(|l| velocity <= l.top)
            .or_else(|| self.layers.last())
    }
}

/// A complete kit, ready to play. Cheap to clone: the sounds are shared.
#[derive(Clone, Debug, Default)]
pub struct DrumKit {
    /// The pads, at most one per sound.
    pub pads: Vec<Pad>,
}

impl DrumKit {
    /// The kit synthesised by our own code: electronic drums in the
    /// spirit of classic drum machines.
    pub fn electronic(sample_rate: u32) -> Self {
        synth::electronic_kit(sample_rate)
    }

    /// A kit from a bundle of recorded sounds (see [`write_bundle`]).
    pub fn from_bundle(bytes: &[u8]) -> Result<Self, String> {
        let mut kit = Self::default();
        for entry in read_bundle(bytes)? {
            let sound = PadSound::from_key(entry.key)
                .ok_or_else(|| format!("no pad for key {}", entry.key))?;
            let layer = Layer {
                top: entry.top,
                sample: Arc::new(entry.sample),
            };
            match kit.pads.iter_mut().find(|p| p.sound == sound) {
                Some(pad) => pad.layers.push(layer),
                None => kit.pads.push(Pad {
                    sound,
                    layers: vec![layer],
                }),
            }
        }
        for pad in &mut kit.pads {
            pad.layers.sort_by_key(|l| l.top);
        }
        kit.sort();
        Ok(kit)
    }

    /// Adds the pads of `other` that this kit lacks.
    #[must_use]
    pub fn completed_with(mut self, other: &Self) -> Self {
        for pad in &other.pads {
            if self.pad(pad.sound).is_none() {
                self.pads.push(pad.clone());
            }
        }
        self.sort();
        self
    }

    /// The pad for a sound, if the kit has it.
    pub fn pad(&self, sound: PadSound) -> Option<&Pad> {
        self.pads.iter().find(|p| p.sound == sound)
    }

    fn sort(&mut self) {
        self.pads.sort_by_key(|p| p.sound.index());
    }
}

/// The settings a person can change on each pad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadParams {
    /// Volume change, in dB.
    pub gain_db: f32,
    /// Position, from -1 (left) to 1 (right).
    pub pan: f32,
    /// Pitch change, in semitones.
    pub tune: f32,
    /// How much of the sound plays, from 0.05 to 1 (shorter sounds fade
    /// out sooner).
    pub length: f32,
    /// Silenced.
    pub muted: bool,
}

impl Default for PadParams {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            pan: 0.0,
            tune: 0.0,
            length: 1.0,
            muted: false,
        }
    }
}

/// Pad settings for every key, as handed to a playing sampler.
pub type PadTable = [PadParams; 128];

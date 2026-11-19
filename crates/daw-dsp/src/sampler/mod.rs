// SPDX-License-Identifier: GPL-3.0-or-later

//! The sampler (decision 0024): plays a bank of recorded sounds, each
//! mapped to keys and velocities by a *region* that also says how it plays
//! (tuning, volume, envelope, loop, filter, choke group). SFZ instruments
//! are read into a [`SamplerBank`] by the file formats crate; the sampler
//! itself knows nothing of files.

mod player;

pub use player::{Sampler, VOICES};

use crate::synth::FilterMode;

/// One recorded sound of a bank, at its own sample rate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    /// Frames per second.
    pub sample_rate: u32,
    /// The left channel (the only one, for a mono sound).
    pub left: Vec<f32>,
    /// The right channel; empty for a mono sound.
    pub right: Vec<f32>,
    /// The loop the sound file holds, if any: its first and last frame.
    pub file_loop: Option<(u64, u64)>,
}

impl Sample {
    /// How many frames it holds.
    pub fn frames(&self) -> usize {
        self.left.len()
    }

    /// How much memory its samples take, in bytes.
    pub fn bytes(&self) -> usize {
        (self.left.len() + self.right.len()) * std::mem::size_of::<f32>()
    }

    /// The right channel, or the left for a mono sound.
    pub fn right_or_left(&self) -> &[f32] {
        if self.right.is_empty() {
            &self.left
        } else {
            &self.right
        }
    }
}

/// What starts a region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trigger {
    /// A key going down.
    #[default]
    Attack,
    /// A key coming up (the sound of a piano's damper, say).
    Release,
    /// A key going down while no other key is held.
    First,
    /// A key going down while another is held.
    Legato,
}

/// How a region's sound loops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    /// Plays once, from start to end; ends early when the key comes up.
    NoLoop,
    /// Plays once, to the end, whatever the key does.
    OneShot,
    /// Loops until the sound has faded out, after the key came up.
    Continuous,
    /// Loops while the key is held, then plays on to the end.
    Sustain,
}

/// How a region cut off by its choke group stops.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OffMode {
    /// In a few milliseconds.
    #[default]
    Fast,
    /// Through its envelope's release.
    Normal,
}

/// The shape of a crossfade between velocity or key layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrossfadeCurve {
    /// Keeps the loudness steady across the fade (square-root gains).
    #[default]
    Power,
    /// Straight-line gains.
    Gain,
}

/// An envelope: delay, attack, hold, decay, sustain and release, with how
/// much the velocity changes each part. Times in seconds; levels from 0
/// to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvelopeSpec {
    /// Silence before it starts.
    pub delay: f32,
    /// The level the attack starts from.
    pub start: f32,
    /// The time to rise to the top (a straight line).
    pub attack: f32,
    /// The time it stays at the top.
    pub hold: f32,
    /// The time to fall to the sustain level.
    pub decay: f32,
    /// The level held while the key is down.
    pub sustain: f32,
    /// The time to fall silent after the key comes up.
    pub release: f32,
    /// Added to the delay at full velocity.
    pub vel2delay: f32,
    /// Added to the attack at full velocity.
    pub vel2attack: f32,
    /// Added to the hold at full velocity.
    pub vel2hold: f32,
    /// Added to the decay at full velocity.
    pub vel2decay: f32,
    /// Added to the sustain level at full velocity.
    pub vel2sustain: f32,
    /// Added to the release at full velocity.
    pub vel2release: f32,
    /// How far it moves what it controls at the top (a filter envelope's
    /// cutoff, in cents); unused for the volume.
    pub depth: f32,
}

impl EnvelopeSpec {
    /// The volume envelope's defaults: full level at once, held, a very
    /// short release.
    pub fn amp() -> Self {
        Self {
            delay: 0.0,
            start: 0.0,
            attack: 0.0,
            hold: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.001,
            vel2delay: 0.0,
            vel2attack: 0.0,
            vel2hold: 0.0,
            vel2decay: 0.0,
            vel2sustain: 0.0,
            vel2release: 0.0,
            depth: 0.0,
        }
    }

    /// The filter envelope's defaults: no movement at all.
    pub fn filter() -> Self {
        Self {
            sustain: 0.0,
            ..Self::amp()
        }
    }
}

/// A region's filter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilterSpec {
    /// Its kind.
    pub mode: FilterMode,
    /// Its cutoff, in hertz.
    pub cutoff: f32,
    /// Its resonance, in decibels (0 for none).
    pub resonance: f32,
    /// How the cutoff follows the key, in cents per key.
    pub keytrack: f32,
    /// The key where key tracking leaves the cutoff as it is.
    pub keycenter: u8,
    /// How far the cutoff rises at full velocity, in cents.
    pub veltrack: f32,
    /// How far the cutoff wanders at random, in cents.
    pub random: f32,
    /// The cutoff's envelope (its depth in cents).
    pub envelope: EnvelopeSpec,
}

/// One region: a sound, the keys and velocities it plays for, and how it
/// plays. The defaults are SFZ's.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// The sound it plays (an index into the bank's sounds); `None` plays
    /// nothing but still cuts off its choke group's sounds.
    pub sample: Option<usize>,
    /// The lowest key it plays for.
    pub lokey: u8,
    /// The highest key it plays for.
    pub hikey: u8,
    /// The lowest velocity it plays for (1 to 127).
    pub lovel: u8,
    /// The highest velocity it plays for.
    pub hivel: u8,
    /// It plays when a random number for the note (0 to 1) is at least
    /// this…
    pub lorand: f32,
    /// …and below this.
    pub hirand: f32,
    /// How many regions take turns (round robin); 1 for none.
    pub seq_length: u32,
    /// Which turn is this region's, from 1.
    pub seq_position: u32,
    /// It plays only after this keyswitch was pressed.
    pub sw_last: Option<u8>,
    /// It plays only while this key is held down.
    pub sw_down: Option<u8>,
    /// It plays only while this key is up.
    pub sw_up: Option<u8>,
    /// It plays only if the key played before was this one.
    pub sw_previous: Option<u8>,
    /// What starts it.
    pub trigger: Trigger,
    /// The key at which the sound plays at its own pitch.
    pub pitch_keycenter: u8,
    /// How the pitch follows the key, in cents per key.
    pub pitch_keytrack: f32,
    /// Moves the pitch, in semitones.
    pub transpose: i32,
    /// Moves the pitch, in cents.
    pub tune: f32,
    /// How far the pitch rises at full velocity, in cents.
    pub pitch_veltrack: f32,
    /// How far the pitch wanders at random, in cents.
    pub pitch_random: f32,
    /// Its volume, in decibels.
    pub volume: f32,
    /// Its level, from 0 to 1 (multiplies the volume).
    pub amplitude: f32,
    /// Its position, from -1 (left) to 1 (right).
    pub pan: f32,
    /// Its stereo width, from -1 (swapped) through 0 (mono) to 1 (as
    /// recorded).
    pub width: f32,
    /// How much the velocity changes the volume, from -1 to 1.
    pub amp_veltrack: f32,
    /// Its own velocity curve (the gain at each velocity, 0 to 1), if it
    /// has one.
    pub amp_velcurve: Option<Box<[f32; 128]>>,
    /// How the volume follows the key, in decibels per key.
    pub amp_keytrack: f32,
    /// The key where key tracking leaves the volume as it is.
    pub amp_keycenter: u8,
    /// How far the volume wanders at random, in decibels.
    pub amp_random: f32,
    /// The velocity crossfade in: silent at or below `xfin_lovel`, full at
    /// or above `xfin_hivel`.
    pub xfin_lovel: u8,
    /// See `xfin_lovel`.
    pub xfin_hivel: u8,
    /// The velocity crossfade out: full at or below `xfout_lovel`, silent
    /// at or above `xfout_hivel`.
    pub xfout_lovel: u8,
    /// See `xfout_lovel`.
    pub xfout_hivel: u8,
    /// The key crossfade in, as for velocities.
    pub xfin_lokey: u8,
    /// See `xfin_lokey`.
    pub xfin_hikey: u8,
    /// The key crossfade out, as for velocities.
    pub xfout_lokey: u8,
    /// See `xfout_lokey`.
    pub xfout_hikey: u8,
    /// The shape of the velocity crossfades.
    pub xf_velcurve: CrossfadeCurve,
    /// The shape of the key crossfades.
    pub xf_keycurve: CrossfadeCurve,
    /// The volume envelope.
    pub ampeg: EnvelopeSpec,
    /// Where in the sound it starts, in frames.
    pub offset: u64,
    /// Up to this many frames are added to the start at random.
    pub offset_random: u64,
    /// The last frame it plays (`None`: the sound's last).
    pub end: Option<u64>,
    /// How it loops (`None`: as the sound file says, or not at all).
    pub loop_mode: Option<LoopMode>,
    /// The loop's first frame (`None`: the sound file's).
    pub loop_start: Option<u64>,
    /// The loop's last frame (`None`: the sound file's).
    pub loop_end: Option<u64>,
    /// Silence before it starts, in seconds.
    pub delay: f32,
    /// Up to this many seconds are added to the delay at random.
    pub delay_random: f32,
    /// Its choke group (0 for none).
    pub group: i64,
    /// The choke group that cuts it off, if any.
    pub off_by: Option<i64>,
    /// How it stops when cut off.
    pub off_mode: OffMode,
    /// The most sounds its group plays at once.
    pub polyphony: Option<u32>,
    /// The most sounds its group plays at once on one key.
    pub note_polyphony: Option<u32>,
    /// For a release region: how much quieter it is for each second the key
    /// was held, in decibels.
    pub rt_decay: f32,
    /// Its filter, if any.
    pub filter: Option<FilterSpec>,
}

impl Default for Region {
    fn default() -> Self {
        Self {
            sample: None,
            lokey: 0,
            hikey: 127,
            lovel: 1,
            hivel: 127,
            lorand: 0.0,
            hirand: 1.0,
            seq_length: 1,
            seq_position: 1,
            sw_last: None,
            sw_down: None,
            sw_up: None,
            sw_previous: None,
            trigger: Trigger::Attack,
            pitch_keycenter: 60,
            pitch_keytrack: 100.0,
            transpose: 0,
            tune: 0.0,
            pitch_veltrack: 0.0,
            pitch_random: 0.0,
            volume: 0.0,
            amplitude: 1.0,
            pan: 0.0,
            width: 1.0,
            amp_veltrack: 1.0,
            amp_velcurve: None,
            amp_keytrack: 0.0,
            amp_keycenter: 60,
            amp_random: 0.0,
            xfin_lovel: 0,
            xfin_hivel: 0,
            xfout_lovel: 127,
            xfout_hivel: 127,
            xfin_lokey: 0,
            xfin_hikey: 0,
            xfout_lokey: 127,
            xfout_hikey: 127,
            xf_velcurve: CrossfadeCurve::Power,
            xf_keycurve: CrossfadeCurve::Power,
            ampeg: EnvelopeSpec::amp(),
            offset: 0,
            offset_random: 0,
            end: None,
            loop_mode: None,
            loop_start: None,
            loop_end: None,
            delay: 0.0,
            delay_random: 0.0,
            group: 0,
            off_by: None,
            off_mode: OffMode::Fast,
            polyphony: None,
            note_polyphony: None,
            rt_decay: 0.0,
            filter: None,
        }
    }
}

impl Region {
    /// Whether it plays for a key.
    pub fn has_key(&self, key: u8) -> bool {
        (self.lokey..=self.hikey).contains(&key)
    }

    /// The gain of its velocity and key crossfades for a note.
    pub fn crossfade(&self, key: u8, velocity: u8) -> f32 {
        fade_in(velocity, self.xfin_lovel, self.xfin_hivel, self.xf_velcurve)
            * fade_out(
                velocity,
                self.xfout_lovel,
                self.xfout_hivel,
                self.xf_velcurve,
            )
            * fade_in(key, self.xfin_lokey, self.xfin_hikey, self.xf_keycurve)
            * fade_out(key, self.xfout_lokey, self.xfout_hikey, self.xf_keycurve)
    }

    /// The gain the velocity gives (1 to 127), following its curve and
    /// how much it tracks the velocity.
    pub fn velocity_gain(&self, velocity: u8) -> f32 {
        let tracking = self.amp_veltrack.clamp(-1.0, 1.0);
        // A negative tracking turns the curve around: soft notes are loud.
        let at = if tracking < 0.0 {
            127_u8.saturating_sub(velocity).max(1)
        } else {
            velocity
        };
        let curve = self.amp_velcurve.as_ref().map_or_else(
            || {
                let v = f32::from(at) / 127.0;
                v * v
            },
            |curve| curve[usize::from(at & 0x7F)],
        );
        1.0 + tracking.abs() * (curve.clamp(0.0, 1.0) - 1.0)
    }
}

/// The gain of a crossfade in at `value`.
fn fade_in(value: u8, low: u8, high: u8, curve: CrossfadeCurve) -> f32 {
    if value >= high {
        1.0
    } else if value <= low {
        0.0
    } else {
        shaped(f32::from(value - low) / f32::from(high - low), curve)
    }
}

/// The gain of a crossfade out at `value`.
fn fade_out(value: u8, low: u8, high: u8, curve: CrossfadeCurve) -> f32 {
    if value <= low {
        1.0
    } else if value >= high {
        0.0
    } else {
        shaped(f32::from(high - value) / f32::from(high - low), curve)
    }
}

fn shaped(position: f32, curve: CrossfadeCurve) -> f32 {
    match curve {
        CrossfadeCurve::Power => position.sqrt(),
        CrossfadeCurve::Gain => position,
    }
}

/// The keys that choose which regions play (articulations), and the one in
/// force at the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeySwitch {
    /// The lowest keyswitch.
    pub lokey: u8,
    /// The highest keyswitch.
    pub hikey: u8,
    /// The keyswitch in force before any is pressed.
    pub default: Option<u8>,
}

/// Parts of an instrument file the sampler does not play (the reader lists
/// them, and the person is told once).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeftOut {
    /// Low-frequency oscillators (vibrato, tremolo, filter sweeps).
    Lfos,
    /// Pitch envelopes and the extra envelopes of newer instruments.
    Envelopes,
    /// Changes made by controllers (the modulation wheel, say).
    ControllerChanges,
    /// The instrument's own effects (reverb, chorus…).
    Effects,
    /// A second filter, or a kind of filter the sampler lacks.
    Filters,
    /// Equalisers inside the instrument.
    Equalizers,
    /// Sounds played backwards.
    Reversed,
    /// Smoothed loop joins (loops play without the crossfade).
    LoopCrossfades,
    /// Some kinds of keyswitch.
    Keyswitches,
    /// Built-in waves other than the sine, and oscillators.
    Waves,
    /// Sounds stored inside the instrument file itself.
    EmbeddedSounds,
    /// Settings this program does not know, by name.
    Other(String),
}

/// A ready-to-play instrument: its regions, the sounds they play, and what
/// its file held that is not played.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SamplerBank {
    /// The regions, in the file's order.
    pub regions: Vec<Region>,
    /// The sounds the regions play.
    pub samples: Vec<Sample>,
    /// Its keyswitches, if it has any.
    pub keyswitch: Option<KeySwitch>,
    /// What its file held that is not played.
    pub left_out: Vec<LeftOut>,
    /// The sound files it names that could not be found or read.
    pub missing: Vec<String>,
}

impl SamplerBank {
    /// How much memory its sounds take, in bytes.
    pub fn sound_bytes(&self) -> usize {
        self.samples.iter().map(Sample::bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfades_and_velocity_curves() {
        let region = Region {
            xfin_lovel: 20,
            xfin_hivel: 60,
            xfout_lovel: 100,
            xfout_hivel: 120,
            ..Region::default()
        };
        assert_eq!(region.crossfade(60, 20), 0.0);
        assert_eq!(region.crossfade(60, 80), 1.0);
        assert_eq!(region.crossfade(60, 120), 0.0);
        let half = region.crossfade(60, 40);
        assert!((half - 0.5_f32.sqrt()).abs() < 1e-6, "power curve: {half}");
        assert_eq!(Region::default().crossfade(0, 127), 1.0, "no crossfade");

        let plain = Region::default();
        assert_eq!(plain.velocity_gain(127), 1.0);
        let soft = plain.velocity_gain(64);
        assert!((soft - (64.0_f32 / 127.0).powi(2)).abs() < 1e-6);
        let flat = Region {
            amp_veltrack: 0.0,
            ..Region::default()
        };
        assert_eq!(flat.velocity_gain(1), 1.0, "no velocity tracking");
        let mut curve = Box::new([0.0; 128]);
        curve[100] = 0.25;
        let custom = Region {
            amp_velcurve: Some(curve),
            ..Region::default()
        };
        assert_eq!(custom.velocity_gain(100), 0.25);
    }
}

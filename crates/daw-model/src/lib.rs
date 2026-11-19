// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod automation;
mod document;
mod mixer;
mod plugin;
mod project;
mod quantize;
mod song;
mod synth;
mod theory;
mod time;

pub use automation::{
    AutomationLane, AutomationPoint, AutomationTarget, gain_volume, pan_value, value_pan,
    volume_gain,
};
pub use document::{Document, HISTORY_LIMIT};
pub use mixer::{
    ChainPlace, CompressorSettings, DelaySettings, DelayTime, Effect, EffectKind, EffectParam,
    EqSettings, LimiterSettings, MasterBus, ReverbSettings,
};
pub use plugin::{PluginFormat, PluginRef, PluginSetup, PluginState, STATE_KEY, base64};
pub use project::Project;
pub use quantize::quantize_starts;
pub use song::{
    ACOUSTIC_KIT, AudioFile, AudioId, BUILTIN_PIANO, ChannelStrip, Clip, ClipContent, ClipId,
    DEFAULT_VELOCITY, ELECTRONIC_KIT, EffectId, Instrument, KIT_FIRST_KEY, KIT_MAX_PADS, KitPad,
    MAX_KEY, Note, NoteId, PadSetting, Pattern, PatternId, PlayedNote, SoundSource, Swing, Track,
    TrackId, default_step, free_kit_key,
};
pub use synth::{
    LfoShape, SynthEnvelope, SynthFilter, SynthOscillator, SynthPreset, SynthSettings, SynthWave,
};
pub use theory::{
    ChordKind, ChordShape, INTERVALS, KeyChange, KeyMap, Scale, ScaleKind, invert, invert_once,
};
pub use time::{
    BarPosition, MeterChange, MeterMap, Region, TEMPO_RANGE, TICKS_PER_QUARTER, TICKS_PER_WHOLE,
    TempoChange, TempoMap, TempoPiece, Ticks, TimeSignature, curve_through, seconds_per_tick,
};

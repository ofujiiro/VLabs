// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod document;
mod project;
mod quantize;
mod song;
mod time;

pub use document::{Document, HISTORY_LIMIT};
pub use project::Project;
pub use quantize::quantize_starts;
pub use song::{
    BUILTIN_PIANO, ChannelStrip, Clip, ClipContent, ClipId, DEFAULT_VELOCITY, Instrument, MAX_KEY,
    Note, NoteId, Pattern, PatternId, SoundSource, Track, TrackId,
};
pub use time::{
    BarPosition, MeterChange, MeterMap, Region, TEMPO_RANGE, TICKS_PER_QUARTER, TICKS_PER_WHOLE,
    TempoChange, TempoMap, Ticks, TimeSignature, seconds_per_tick,
};

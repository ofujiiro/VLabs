// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod click;
pub mod drums;
pub mod effects;
mod instrument;
pub mod sf2_writer;
mod sine;
mod soundfont;

pub use click::Click;
pub use instrument::{Instrument, InstrumentSettings, NoteEvent, SongPosition};
pub use sine::SineSynth;
pub use soundfont::{PresetInfo, SOUNDFONT_BLOCK, SoundFontBank, SoundFontError, SoundFontPlayer};

/// Converts decibels to a linear gain factor.
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts a MIDI key number to its frequency in hertz (A4 = 440 Hz).
pub fn key_to_hz(key: f32) -> f32 {
    440.0 * 2.0_f32.powf((key - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn conversions() {
        assert!((super::db_to_gain(-6.0) - 0.501).abs() < 0.001);
        assert!((super::key_to_hz(69.0) - 440.0).abs() < 1e-3);
        assert!((super::key_to_hz(60.0) - 261.626).abs() < 1e-2);
    }
}

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
}

/// Settings an instrument takes while playing.
#[derive(Clone, Debug, PartialEq)]
pub enum InstrumentSettings {
    /// A drum kit's pad settings, indexed by each pad's main key.
    Drums(Box<crate::drums::PadTable>),
}

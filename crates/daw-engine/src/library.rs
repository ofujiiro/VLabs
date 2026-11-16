// SPDX-License-Identifier: GPL-3.0-or-later

//! Where instruments get their sounds: loading SoundFonts and building
//! instrument instances, off the audio thread.

use std::collections::HashMap;

use daw_dsp::{Instrument, SineSynth, SoundFontBank, SoundFontPlayer};
use daw_model::{Instrument as InstrumentSpec, SoundSource};

/// Loads and caches sounds, and builds ready-to-play instruments.
pub struct SoundLibrary {
    builtin: HashMap<String, &'static [u8]>,
    loaded: HashMap<SoundSource, Result<SoundFontBank, String>>,
    soundfont_reverb: Option<f32>,
}

impl Default for SoundLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundLibrary {
    /// An empty library.
    pub fn new() -> Self {
        Self {
            builtin: HashMap::new(),
            loaded: HashMap::new(),
            soundfont_reverb: None,
        }
    }

    /// Overrides the reverb amount of every SoundFont instrument built from
    /// now on (0 to 1). Tests use 0 to measure dry notes.
    pub fn set_soundfont_reverb(&mut self, amount: f32) {
        self.soundfont_reverb = Some(amount);
    }

    /// Makes a bundled SoundFont available under `name`.
    pub fn register_builtin(&mut self, name: &str, bytes: &'static [u8]) {
        self.builtin.insert(name.to_owned(), bytes);
        self.loaded.remove(&SoundSource::Builtin(name.to_owned()));
    }

    /// The SoundFont for a source, loading it the first time.
    pub fn soundfont(&mut self, source: &SoundSource) -> Result<SoundFontBank, String> {
        if let Some(result) = self.loaded.get(source) {
            return result.clone();
        }
        let result = match source {
            SoundSource::Builtin(name) => match self.builtin.get(name) {
                Some(bytes) => SoundFontBank::from_bytes(bytes).map_err(|e| e.to_string()),
                None => Err(format!("the bundled sound \"{name}\" is missing")),
            },
            SoundSource::File(path) => std::fs::read(path)
                .map_err(|e| format!("{path}: {e}"))
                .and_then(|bytes| SoundFontBank::from_bytes(&bytes).map_err(|e| e.to_string())),
        };
        self.loaded.insert(source.clone(), result.clone());
        result
    }

    /// Builds an instrument for a track. If its sound cannot be loaded, a
    /// plain sine synth plays instead, so notes are still heard, and the
    /// reason is returned alongside.
    pub fn build(
        &mut self,
        spec: &InstrumentSpec,
        sample_rate: u32,
    ) -> (Box<dyn Instrument>, Option<String>) {
        match spec {
            InstrumentSpec::SoundFont {
                source,
                bank,
                program,
            } => {
                let result = self.soundfont(source).and_then(|font| {
                    SoundFontPlayer::new(&font, sample_rate, *bank, *program)
                        .map_err(|e| e.to_string())
                });
                match result {
                    Ok(mut player) => {
                        if let Some(amount) = self.soundfont_reverb {
                            player.set_reverb(amount);
                        }
                        (Box::new(player), None)
                    }
                    Err(problem) => (Box::new(SineSynth::new(sample_rate)), Some(problem)),
                }
            }
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Where instruments get their sounds: loading SoundFonts and building
//! instrument instances, off the audio thread.

use std::collections::HashMap;
use std::sync::Arc;

use daw_dsp::audio::AudioBuffer;
use daw_dsp::drums::{DrumKit, DrumSampler, PadParams, PadTable};
use daw_dsp::{Instrument, InstrumentSettings, SineSynth, SoundFontBank, SoundFontPlayer};
use daw_model::{
    ELECTRONIC_KIT, Instrument as InstrumentSpec, PadSetting, PluginFormat, PluginRef, SoundSource,
};
use daw_plugin_host::PluginInfo;

use crate::Problem;

/// Loads and caches sounds, and builds ready-to-play instruments. Cheap to
/// clone (loaded sounds are shared), so offline rendering can have its own.
#[derive(Clone)]
pub struct SoundLibrary {
    builtin: HashMap<String, &'static [u8]>,
    loaded: HashMap<SoundSource, Result<SoundFontBank, Problem>>,
    kits: HashMap<String, &'static [u8]>,
    loaded_kits: HashMap<(String, u32), Result<DrumKit, Problem>>,
    soundfont_reverb: Option<f32>,
    plugins: Vec<PluginInfo>,
    /// The sound of the song's audio files, by the name each is stored
    /// under, decoded and at the engine's sample rate.
    audio: HashMap<String, Arc<AudioBuffer>>,
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
            kits: HashMap::new(),
            loaded_kits: HashMap::new(),
            soundfont_reverb: None,
            plugins: Vec::new(),
            audio: HashMap::new(),
        }
    }

    /// Gives the engine the sound of an audio file, by the name it is
    /// stored under (decoded, and at the engine's sample rate: sound at
    /// another rate plays at the right speed but less cleanly).
    pub fn register_audio(&mut self, name: &str, sound: Arc<AudioBuffer>) {
        self.audio.insert(name.to_owned(), sound);
    }

    /// The sound of an audio file, if it has been given.
    pub fn audio(&self, name: &str) -> Option<&Arc<AudioBuffer>> {
        self.audio.get(name)
    }

    /// Forgets the sound of every audio file not in `keep` (another song
    /// was opened, say).
    pub fn retain_audio(&mut self, keep: impl Fn(&str) -> bool) {
        self.audio.retain(|name, _| keep(name));
    }

    /// The plugins found on this computer.
    pub fn register_plugins(&mut self, plugins: Vec<PluginInfo>) {
        self.plugins = plugins;
    }

    /// The plugins found on this computer.
    pub fn plugins(&self) -> &[PluginInfo] {
        &self.plugins
    }

    /// Where a song's plugin is on this computer: found by scanning, or at
    /// the place it was chosen from if that still holds it.
    pub fn find_plugin(&self, plugin: &PluginRef) -> Option<PluginInfo> {
        let format = match plugin.format {
            PluginFormat::Clap => daw_plugin_host::Format::Clap,
            PluginFormat::Vst3 => daw_plugin_host::Format::Vst3,
        };
        if let Some(found) = self
            .plugins
            .iter()
            .find(|p| p.format == format && p.id == plugin.id)
        {
            return Some(found.clone());
        }
        let path = std::path::PathBuf::from(&plugin.path);
        path.exists().then(|| PluginInfo {
            format,
            path,
            id: plugin.id.clone(),
            name: plugin.name.clone(),
            vendor: plugin.vendor.clone(),
            version: String::new(),
            kind: daw_plugin_host::Kind::Instrument,
        })
    }

    /// Makes a bundled recorded drum kit available under `name`.
    pub fn register_kit(&mut self, name: &str, bundle: &'static [u8]) {
        self.kits.insert(name.to_owned(), bundle);
        self.loaded_kits.retain(|(kit, _), _| kit != name);
    }

    /// A drum kit, loading it the first time. The electronic kit is
    /// synthesised; recorded kits borrow the sounds they lack from it.
    pub fn kit(&mut self, name: &str, sample_rate: u32) -> Result<DrumKit, Problem> {
        let key = (name.to_owned(), sample_rate);
        if let Some(result) = self.loaded_kits.get(&key) {
            return result.clone();
        }
        let electronic = || DrumKit::electronic(sample_rate);
        let result = if name == ELECTRONIC_KIT {
            Ok(electronic())
        } else {
            match self.kits.get(name) {
                Some(bundle) => DrumKit::from_bundle(bundle)
                    .map(|kit| kit.completed_with(&electronic()))
                    .map_err(|reason| Problem::UnreadableSound {
                        sound: name.to_owned(),
                        reason,
                    }),
                None => Err(Problem::MissingSound(name.to_owned())),
            }
        };
        self.loaded_kits.insert(key, result.clone());
        result
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
    pub fn soundfont(&mut self, source: &SoundSource) -> Result<SoundFontBank, Problem> {
        if let Some(result) = self.loaded.get(source) {
            return result.clone();
        }
        let unreadable = |sound: &str, reason: String| Problem::UnreadableSound {
            sound: sound.to_owned(),
            reason,
        };
        let result = match source {
            SoundSource::Builtin(name) => match self.builtin.get(name) {
                Some(bytes) => {
                    SoundFontBank::from_bytes(bytes).map_err(|e| unreadable(name, e.to_string()))
                }
                None => Err(Problem::MissingSound(name.clone())),
            },
            SoundSource::File(path) => std::fs::read(path)
                .map_err(|e| unreadable(path, e.to_string()))
                .and_then(|bytes| {
                    SoundFontBank::from_bytes(&bytes).map_err(|e| unreadable(path, e.to_string()))
                }),
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
    ) -> (Box<dyn Instrument>, Option<Problem>) {
        match spec {
            InstrumentSpec::SoundFont {
                source,
                bank,
                program,
            } => {
                let result = self.soundfont(source).and_then(|font| {
                    SoundFontPlayer::new(&font, sample_rate, *bank, *program).map_err(|e| {
                        Problem::UnreadableSound {
                            sound: format!("{source:?}"),
                            reason: e.to_string(),
                        }
                    })
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
            InstrumentSpec::Drums { kit, pads } => match self.kit(kit, sample_rate) {
                Ok(kit) => {
                    let mut sampler = DrumSampler::new(kit, sample_rate);
                    sampler.set_pads(&pad_table(pads));
                    (Box::new(sampler), None)
                }
                Err(problem) => {
                    // The electronic kit always exists: play that instead.
                    let mut sampler =
                        DrumSampler::new(DrumKit::electronic(sample_rate), sample_rate);
                    sampler.set_pads(&pad_table(pads));
                    (Box::new(sampler), Some(problem))
                }
            },
            // The engine loads plugins itself (they have a main-thread half);
            // an audio track has no instrument (its clips are mixed in).
            InstrumentSpec::Plugin(_) | InstrumentSpec::Audio => {
                (Box::new(crate::plugins::Silence), None)
            }
        }
    }
}

/// The settings a playing instrument takes from its project description,
/// if it has any.
pub(crate) fn instrument_settings(spec: &InstrumentSpec) -> Option<InstrumentSettings> {
    match spec {
        InstrumentSpec::Drums { pads, .. } => {
            Some(InstrumentSettings::Drums(Box::new(pad_table(pads))))
        }
        InstrumentSpec::SoundFont { .. } | InstrumentSpec::Plugin(_) | InstrumentSpec::Audio => {
            None
        }
    }
}

/// Every pad's settings, indexed by the pad's key.
fn pad_table(pads: &[PadSetting]) -> PadTable {
    let mut table = [PadParams::default(); 128];
    for pad in pads {
        table[usize::from(pad.key & 0x7F)] = PadParams {
            gain_db: pad.gain_db,
            pan: pad.pan,
            tune: pad.tune,
            length: pad.length,
            muted: pad.muted,
        };
    }
    table
}

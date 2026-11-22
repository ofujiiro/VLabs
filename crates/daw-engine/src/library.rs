// SPDX-License-Identifier: GPL-3.0-or-later

//! Where instruments get their sounds: loading SoundFonts and SFZ
//! instruments and building instrument instances, off the audio thread.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use daw_dsp::audio::AudioBuffer;
use daw_dsp::drums::{DrumKit, DrumSampler, PadParams, PadTable};
use daw_dsp::sampler::{EnvelopeSpec, LoopMode, Region, Sample, Sampler, SamplerBank};
use daw_dsp::synth::{EnvParams, FilterMode, OscParams, Synth, SynthParams, Wave};
use daw_dsp::{Instrument, InstrumentSettings, SineSynth, SoundFontBank, SoundFontPlayer};
use daw_model::{
    ELECTRONIC_KIT, Instrument as InstrumentSpec, LfoShape, PadSetting, PluginFormat, PluginRef,
    SoundSource, SynthEnvelope, SynthFilter, SynthOscillator, SynthSettings, SynthWave,
};
use daw_plugin_host::PluginInfo;

use crate::Problem;
use crate::voices::{VoiceReader, Voices};
use daw_dsp::voice::VoiceBank;

/// Reads an SFZ instrument and its sounds into a sampler bank. The engine
/// cannot read sound files itself (decision 0003), so the program hands it
/// the file formats' reader (decision 0024).
pub type SfzReader = fn(&Path) -> Result<SamplerBank, String>;

/// A SoundFont with a note shape: its source, attack and release (in
/// SoundFont timecents, so nearby times share one copy).
type ShapedKey = (SoundSource, Option<i16>, Option<i16>);

/// How many shaped SoundFonts are kept (each is a whole copy of its file).
const SHAPED_KEPT: usize = 3;

/// An SFZ instrument being read, or read.
type SfzLoad = Arc<OnceLock<Result<Arc<SamplerBank>, Problem>>>;

/// Loads and caches sounds, and builds ready-to-play instruments. Cheap to
/// clone (loaded sounds are shared), so offline rendering can have its own.
#[derive(Clone)]
pub struct SoundLibrary {
    builtin: HashMap<String, &'static [u8]>,
    loaded: HashMap<SoundSource, Result<SoundFontBank, Problem>>,
    /// SoundFonts read again with another note shape (decision 0031), the
    /// most recent few, by source and shape (in timecents).
    shaped: Vec<(ShapedKey, Result<SoundFontBank, Problem>)>,
    kits: HashMap<String, &'static [u8]>,
    loaded_kits: HashMap<(String, u32), Result<DrumKit, Problem>>,
    soundfont_reverb: Option<f32>,
    plugins: Vec<PluginInfo>,
    /// The sound of the song's audio files, by the name each is stored
    /// under, decoded and at the engine's sample rate.
    audio: HashMap<String, Arc<AudioBuffer>>,
    sfz_reader: Option<SfzReader>,
    /// The SFZ instruments read or being read, by file (shared by every
    /// copy of the library).
    sfz: Arc<Mutex<HashMap<String, SfzLoad>>>,
    /// SFZ instruments are read on a thread of their own, and their tracks
    /// stay silent until they are ready (else building one waits).
    background: bool,
    /// Singing voices: voicebanks and rendered phrases (decision 0027).
    voices: Voices,
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
            shaped: Vec::new(),
            kits: HashMap::new(),
            loaded_kits: HashMap::new(),
            soundfont_reverb: None,
            plugins: Vec::new(),
            audio: HashMap::new(),
            sfz_reader: None,
            sfz: Arc::new(Mutex::new(HashMap::new())),
            background: false,
            voices: Voices::new(),
        }
    }

    /// Lets the library read SFZ instruments.
    pub fn set_sfz_reader(&mut self, reader: SfzReader) {
        self.sfz_reader = Some(reader);
    }

    /// Reads SFZ instruments and voicebanks, and renders singing, in the
    /// background (the live engine), or waits for them (offline rendering,
    /// tests).
    pub fn set_background_loading(&mut self, background: bool) {
        self.background = background;
        self.voices.set_background(background);
    }

    /// Lets the library read voicebanks (decision 0027), by the name of
    /// their folder among the installed ones.
    pub fn set_voice_reader(&mut self, reader: VoiceReader) {
        self.voices.set_reader(reader);
    }

    /// The singing voices.
    pub(crate) fn voices(&self) -> &Voices {
        &self.voices
    }

    /// The SFZ instrument in `file`, reading it the first time (on a thread
    /// of its own when reading in the background).
    fn sfz_load(&self, file: &str) -> SfzLoad {
        let mut loads = self.sfz.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(load) = loads.get(file) {
            return Arc::clone(load);
        }
        let load: SfzLoad = Arc::new(OnceLock::new());
        loads.insert(file.to_owned(), Arc::clone(&load));
        drop(loads);
        let reader = self.sfz_reader;
        let cell = Arc::clone(&load);
        let shown = file.to_owned();
        let file = file.to_owned();
        let read = move || {
            let path = Path::new(&file);
            let result = if !path.is_file() {
                Err(Problem::MissingFile(file.clone()))
            } else if let Some(reader) = reader {
                // A file from anywhere may still find a corner of the
                // reader: it fails, and the track does not wait forever.
                std::panic::catch_unwind(|| reader(path))
                    .unwrap_or_else(|_| Err("the reader gave up on it".to_owned()))
                    .map(Arc::new)
                    .map_err(|reason| Problem::UnreadableSound {
                        sound: file.clone(),
                        reason,
                    })
            } else {
                Err(Problem::UnreadableSound {
                    sound: file.clone(),
                    reason: "SFZ instruments cannot be read here".to_owned(),
                })
            };
            let _ = cell.set(result);
        };
        if self.background {
            if let Err(error) = std::thread::Builder::new()
                .name("sfz-reader".to_owned())
                .spawn(read)
            {
                let _ = load.set(Err(Problem::UnreadableSound {
                    sound: shown,
                    reason: error.to_string(),
                }));
            }
        } else {
            read();
        }
        load
    }

    /// The SFZ instrument in `file` if it has been read (or failed to be).
    pub fn sfz(&self, file: &str) -> Option<Result<Arc<SamplerBank>, Problem>> {
        self.sfz
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(file)
            .and_then(|load| load.get().cloned())
    }

    /// Whether an instrument's sounds are still being read.
    pub fn is_loading(&self, spec: &InstrumentSpec) -> bool {
        if let InstrumentSpec::Voice { voicebank } = spec {
            return self.voices.is_loading(voicebank);
        }
        let InstrumentSpec::Sfz { file } = spec else {
            return false;
        };
        self.sfz
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(file.as_str())
            .is_some_and(|load| load.get().is_none())
    }

    /// Forgets the SFZ instruments not in `keep` (another song was opened):
    /// their memory goes once no track plays them.
    pub fn retain_sfz(&mut self, keep: impl Fn(&str) -> bool) {
        self.sfz
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|file, _| keep(file));
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
            SoundSource::File(path) if !std::path::Path::new(path).is_file() => {
                Err(Problem::MissingFile(path.clone()))
            }
            SoundSource::File(path) => std::fs::read(path)
                .map_err(|e| unreadable(path, e.to_string()))
                .and_then(|bytes| {
                    SoundFontBank::from_bytes(&bytes).map_err(|e| unreadable(path, e.to_string()))
                }),
        };
        self.loaded.insert(source.clone(), result.clone());
        result
    }

    /// The SoundFont for a source with a note shape (decision 0031): the
    /// file read again with every sound's attack and release changed.
    pub fn shaped_soundfont(
        &mut self,
        source: &SoundSource,
        shape: daw_model::NoteShape,
    ) -> Result<SoundFontBank, Problem> {
        let shape = crate::dsp_shape(shape);
        if shape.attack.is_none() && shape.release.is_none() {
            return self.soundfont(source);
        }
        let key: ShapedKey = (
            source.clone(),
            shape.attack.map(daw_dsp::sf2_shape::timecents),
            shape.release.map(daw_dsp::sf2_shape::timecents),
        );
        if let Some((_, result)) = self.shaped.iter().find(|(k, _)| *k == key) {
            return result.clone();
        }
        // The plain one first: a missing or unreadable file says so the
        // usual way.
        self.soundfont(source)?;
        let unreadable = |sound: &str, reason: String| Problem::UnreadableSound {
            sound: sound.to_owned(),
            reason,
        };
        let result = match source {
            SoundSource::Builtin(name) => match self.builtin.get(name) {
                Some(bytes) => SoundFontBank::from_bytes_shaped(bytes, shape)
                    .map_err(|e| unreadable(name, e.to_string())),
                None => Err(Problem::MissingSound(name.clone())),
            },
            SoundSource::File(path) => std::fs::read(path)
                .map_err(|e| unreadable(path, e.to_string()))
                .and_then(|bytes| {
                    SoundFontBank::from_bytes_shaped(&bytes, shape)
                        .map_err(|e| unreadable(path, e.to_string()))
                }),
        };
        if self.shaped.len() >= SHAPED_KEPT {
            let _oldest = self.shaped.remove(0);
        }
        self.shaped.push((key, result.clone()));
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
        let built = self.build_for_track(spec, daw_model::NoteShape::NATURAL, sample_rate);
        (built.instrument, built.problem)
    }

    /// Builds an instrument for a track with its note shape (decision 0031;
    /// a SoundFont takes it here, other instruments as a setting), saying
    /// whether it only stands in (silent) while its sounds are read in the
    /// background.
    pub(crate) fn build_for_track(
        &mut self,
        spec: &InstrumentSpec,
        shape: daw_model::NoteShape,
        sample_rate: u32,
    ) -> Built {
        let mut waiting = false;
        let (instrument, problem) = self.build_now(spec, shape, sample_rate, &mut waiting);
        Built {
            instrument,
            problem,
            waiting,
        }
    }

    fn build_now(
        &mut self,
        spec: &InstrumentSpec,
        shape: daw_model::NoteShape,
        sample_rate: u32,
        waiting: &mut bool,
    ) -> (Box<dyn Instrument>, Option<Problem>) {
        match spec {
            InstrumentSpec::SoundFont {
                source,
                bank,
                program,
            } => {
                let result = self.shaped_soundfont(source, shape).and_then(|font| {
                    SoundFontPlayer::new(&font, sample_rate, *bank, *program).map_err(|e| {
                        Problem::UnreadableSound {
                            sound: match source {
                                SoundSource::Builtin(name) | SoundSource::File(name) => {
                                    name.clone()
                                }
                            },
                            reason: e.to_string(),
                        }
                    })
                });
                match result {
                    Ok(mut player) => {
                        if let Some(amount) = self.soundfont_reverb {
                            player.set_reverb(amount);
                        }
                        // A set release says how notes end, and the
                        // SoundFont's own reverb would ring on past it (a
                        // cut would not sound cut): it is off then
                        // (decision 0031). A reverb effect on the track
                        // adds one on purpose.
                        if shape.release.is_some() {
                            player.set_reverb(0.0);
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
            InstrumentSpec::Synth(settings) => (
                Box::new(Synth::new(sample_rate, synth_params(settings))),
                None,
            ),
            InstrumentSpec::Sfz { file } => {
                let load = self.sfz_load(file);
                let result = if self.background {
                    load.get().cloned()
                } else {
                    Some(load.wait().clone())
                };
                match result {
                    // Silent until it is ready (the engine builds it again).
                    None => {
                        *waiting = true;
                        (Box::new(crate::plugins::Silence), None)
                    }
                    Some(Ok(bank)) => (Box::new(Sampler::new(bank, sample_rate)), None),
                    Some(Err(problem)) => (Box::new(SineSynth::new(sample_rate)), Some(problem)),
                }
            }
            // The engine loads plugins itself (they have a main-thread half),
            // and builds custom kits from the song's audio files; an audio
            // track has no instrument (its clips are mixed in).
            // A voice's notes, played live, sound at once as its "a" played
            // faster or slower (the song plays its rendered singing).
            InstrumentSpec::Voice { voicebank } => match self.voices.bank(voicebank) {
                None => {
                    *waiting = true;
                    (Box::new(crate::plugins::Silence), None)
                }
                Some(Ok(bank)) => match voice_preview(&bank, sample_rate) {
                    Some(sampler) => (Box::new(sampler), None),
                    None => (Box::new(SineSynth::new(sample_rate)), None),
                },
                Some(Err(reason)) => (
                    Box::new(crate::plugins::Silence),
                    Some(Problem::UnreadableSound {
                        sound: voicebank.clone(),
                        reason,
                    }),
                ),
            },
            InstrumentSpec::Plugin(_) | InstrumentSpec::Audio | InstrumentSpec::Kit { .. } => {
                (Box::new(crate::plugins::Silence), None)
            }
        }
    }
}

/// An instrument built for a track.
pub(crate) struct Built {
    /// What plays.
    pub(crate) instrument: Box<dyn Instrument>,
    /// What went wrong (a plain sound plays instead).
    pub(crate) problem: Option<Problem>,
    /// It is silent while its sounds are read: build it again once they
    /// are ready.
    pub(crate) waiting: bool,
}

/// The synth's sound settings from the song's (decision 0019). Plain
/// arithmetic, safe on the audio thread (automation uses it there).
pub(crate) fn synth_params(settings: &SynthSettings) -> SynthParams {
    let s = settings.repaired();
    let osc = |o: &SynthOscillator| OscParams {
        wave: match o.wave {
            SynthWave::Sine => Wave::Sine,
            SynthWave::Triangle => Wave::Triangle,
            SynthWave::Saw => Wave::Saw,
            SynthWave::Square => Wave::Square,
        },
        pitch: o.pitch(),
        level: o.level,
    };
    let env = |e: &SynthEnvelope| EnvParams {
        attack: e.attack,
        decay: e.decay,
        sustain: e.sustain,
        release: e.release,
    };
    SynthParams {
        osc: [osc(&s.osc1), osc(&s.osc2)],
        pulse_width: s.pulse_width,
        noise: s.noise,
        filter_mode: match s.filter {
            SynthFilter::LowPass24 => FilterMode::LowPass24,
            SynthFilter::LowPass12 => FilterMode::LowPass12,
            SynthFilter::HighPass => FilterMode::HighPass,
            SynthFilter::BandPass => FilterMode::BandPass,
        },
        cutoff_hz: s.cutoff_hz,
        resonance: s.resonance,
        filter_env_octaves: s.envelope_amount,
        key_tracking: s.key_tracking,
        amp_env: env(&s.amp_envelope),
        filter_env: env(&s.filter_envelope),
        lfo_shape: match s.lfo_shape {
            LfoShape::Sine => daw_dsp::synth::LfoShape::Sine,
            LfoShape::Triangle => daw_dsp::synth::LfoShape::Triangle,
            LfoShape::Square => daw_dsp::synth::LfoShape::Square,
            LfoShape::Saw => daw_dsp::synth::LfoShape::Saw,
            LfoShape::Random => daw_dsp::synth::LfoShape::Random,
        },
        lfo_rate_hz: s.lfo_rate_hz,
        vibrato_cents: s.vibrato_cents,
        wobble_octaves: s.wobble_octaves,
        tremolo: s.tremolo,
        mono: s.mono,
        glide_seconds: s.glide_seconds,
        velocity: s.velocity,
        gain: daw_dsp::db_to_gain(s.volume_db),
    }
}

/// The settings a playing instrument takes from its project description,
/// if it has any.
pub(crate) fn instrument_settings(spec: &InstrumentSpec) -> Option<InstrumentSettings> {
    match spec {
        InstrumentSpec::Drums { pads, .. } => {
            Some(InstrumentSettings::Drums(Box::new(pad_table(pads))))
        }
        InstrumentSpec::Synth(settings) => Some(InstrumentSettings::Synth(synth_params(settings))),
        InstrumentSpec::SoundFont { .. }
        | InstrumentSpec::Plugin(_)
        | InstrumentSpec::Audio
        | InstrumentSpec::Sfz { .. }
        | InstrumentSpec::Kit { .. }
        | InstrumentSpec::Voice { .. } => None,
    }
}

/// What a voice plays live (decision 0027): its "a", from the end of the
/// consonant to the end of the syllable, on every key, pitched from the
/// pitch it was recorded at. None when the voicebank has no "a" it can
/// read.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "keys, cents and frame positions are small"
)]
pub(crate) fn voice_preview(bank: &VoiceBank, sample_rate: u32) -> Option<Sampler> {
    let entry = bank.find(
        &daw_model::lyrics::alias_candidates(daw_model::lyrics::DEFAULT_LYRIC, None),
        60,
    )?;
    let recording = bank.sample(&entry.file).ok()?;
    let stretch = entry.stretch(recording.seconds());
    let rate = f64::from(recording.rate());
    let from = ((stretch.start + stretch.fixed) * rate) as usize;
    let to = ((stretch.end * rate) as usize).min(recording.samples().len());
    if to <= from + 64 {
        return None;
    }
    let hz = recording.pitch_between(stretch.start + stretch.fixed, stretch.end)?;
    let key = 69.0 + 12.0 * (hz / 440.0).log2();
    let centre = key.round().clamp(0.0, 127.0);
    let mut ampeg = EnvelopeSpec::amp();
    ampeg.attack = 0.01;
    ampeg.release = 0.08;
    let mut bank = SamplerBank::default();
    bank.samples.push(Sample {
        sample_rate: recording.rate(),
        left: recording.samples()[from..to].to_vec(),
        right: Vec::new(),
        file_loop: None,
    });
    bank.regions.push(Region {
        sample: Some(0),
        lokey: 0,
        hikey: 127,
        pitch_keycenter: centre as u8,
        tune: ((centre - key) * 100.0) as f32,
        amp_veltrack: 0.5,
        loop_mode: Some(LoopMode::NoLoop),
        ampeg,
        ..Region::default()
    });
    Some(Sampler::new(Arc::new(bank), sample_rate))
}

/// A custom kit's sampler (decision 0025): each pad's sound (at the engine's
/// rate) plays once on its key, with the pad's volume, position, pitch and
/// length; a length below full fades the sound out by its end.
pub(crate) fn kit_sampler(pads: &[(PadSetting, Arc<AudioBuffer>)], sample_rate: u32) -> Sampler {
    let mut bank = SamplerBank::default();
    for (setting, sound) in pads {
        if setting.muted || sound.frames() < 2 {
            continue;
        }
        let sample = Sample {
            sample_rate: sound.sample_rate(),
            left: sound.left().to_vec(),
            right: if sound.channel_count() > 1 {
                sound.right().to_vec()
            } else {
                Vec::new()
            },
            file_loop: None,
        };
        let length = setting.length.clamp(0.05, 1.0);
        let mut ampeg = EnvelopeSpec::amp();
        let mut end = None;
        if length < 1.0 {
            #[allow(clippy::cast_precision_loss, reason = "frame counts and rates")]
            let (frames, rate) = (
                sample.frames() as f32 * length,
                sample.sample_rate.max(1) as f32,
            );
            let seconds = frames / rate;
            ampeg.hold = seconds * 0.7;
            ampeg.decay = seconds * 0.3;
            ampeg.sustain = 0.0;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a positive frame count"
            )]
            let last = frames as u64;
            end = Some(last.max(1));
        }
        let key = setting.key & 0x7F;
        bank.regions.push(Region {
            sample: Some(bank.samples.len()),
            lokey: key,
            hikey: key,
            pitch_keycenter: key,
            tune: setting.tune.clamp(-24.0, 24.0) * 100.0,
            volume: setting.gain_db.clamp(-60.0, 24.0),
            pan: setting.pan.clamp(-1.0, 1.0),
            amp_veltrack: 0.5,
            loop_mode: Some(LoopMode::OneShot),
            ampeg,
            end,
            ..Region::default()
        });
        bank.samples.push(sample);
    }
    Sampler::new(Arc::new(bank), sample_rate)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_soundfont_file_that_is_gone_plays_a_plain_sound_and_says_where_it_was() {
        let mut library = SoundLibrary::new();
        let gone = std::env::temp_dir()
            .join(format!("gone-{}", std::process::id()))
            .join("Gone.sf2")
            .to_string_lossy()
            .into_owned();
        let spec = InstrumentSpec::SoundFont {
            source: SoundSource::File(gone.clone()),
            bank: 0,
            program: 0,
        };
        let (_plain, problem) = library.build(&spec, 48_000);
        assert_eq!(problem, Some(Problem::MissingFile(gone)));
    }
}

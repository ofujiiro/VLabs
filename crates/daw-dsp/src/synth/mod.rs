// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in subtractive synth (decision 0019): two oscillators and
//! noise into a filter and an amplifier, each with its own envelope, an
//! LFO for vibrato, wobble and tremolo, and sixteen voices or one with
//! glide.
//!
//! Its settings ([`SynthParams`]) are plain numbers in natural units; the
//! engine turns the song's settings into them. Every method runs on the
//! audio thread without allocating: the voices are a fixed array.

pub(crate) mod parts;

use parts::{EnvRates, Envelope, Random, VoiceFilter, wave};

use crate::instrument::InstrumentSettings;
use crate::{Instrument, key_to_hz};

/// The most notes the synth plays at once.
pub const VOICES: usize = 16;

/// Samples between two updates of pitches, cutoffs and the LFO.
const CONTROL: usize = 16;

/// The level of one voice at full velocity before the volume setting,
/// leaving room for chords.
const VOICE_GAIN: f32 = 0.3;

/// The most keys the single voice of mono mode remembers (for going back
/// to a key still held when the last one is let go).
const HELD_KEYS: usize = 16;

/// An oscillator's wave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    /// A pure tone.
    Sine,
    /// Soft and hollow (odd harmonics falling fast).
    Triangle,
    /// Bright and buzzy (every harmonic).
    Saw,
    /// Hollow and reedy (odd harmonics); its width changes its colour.
    Square,
}

/// The filter's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    /// Takes away the highs, steeply (24 dB per octave).
    LowPass24,
    /// Takes away the highs, gently (12 dB per octave).
    LowPass12,
    /// Takes away the lows.
    HighPass,
    /// Keeps a band around the cutoff.
    BandPass,
}

/// The LFO's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoShape {
    /// Smooth.
    Sine,
    /// Straight up and down.
    Triangle,
    /// Jumps between two values.
    Square,
    /// Falls, then jumps back up.
    Saw,
    /// A new random value each cycle.
    Random,
}

/// One oscillator's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OscParams {
    /// Its wave.
    pub wave: Wave,
    /// Its pitch above the key played, in semitones (octaves, semitones and
    /// cents together).
    pub pitch: f32,
    /// Its level, from 0 to 1.
    pub level: f32,
}

/// An envelope's settings, in seconds (and a level for the sustain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvParams {
    /// Time to rise from silence to the top.
    pub attack: f32,
    /// Time to fall from the top to the sustain level.
    pub decay: f32,
    /// The level held while the key is down, from 0 to 1.
    pub sustain: f32,
    /// Time to fall to silence once the key is up.
    pub release: f32,
}

impl Default for EnvParams {
    fn default() -> Self {
        Self {
            attack: 0.002,
            decay: 0.3,
            sustain: 1.0,
            release: 0.05,
        }
    }
}

/// Everything the synth's sound depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SynthParams {
    /// The two oscillators.
    pub osc: [OscParams; 2],
    /// The square waves' pulse width, from 0.05 to 0.95 (0.5 is square).
    pub pulse_width: f32,
    /// The noise level, from 0 to 1.
    pub noise: f32,
    /// The filter's kind.
    pub filter_mode: FilterMode,
    /// The filter's cutoff, in hertz.
    pub cutoff_hz: f32,
    /// The filter's resonance, from 0 to 1.
    pub resonance: f32,
    /// How far the filter envelope moves the cutoff, in octaves (negative
    /// moves it down).
    pub filter_env_octaves: f32,
    /// How much the cutoff follows the key played, from 0 (not at all) to 1
    /// (an octave per octave).
    pub key_tracking: f32,
    /// The volume envelope.
    pub amp_env: EnvParams,
    /// The filter envelope.
    pub filter_env: EnvParams,
    /// The LFO's shape.
    pub lfo_shape: LfoShape,
    /// The LFO's speed, in hertz.
    pub lfo_rate_hz: f32,
    /// How far the LFO moves the pitch, in cents.
    pub vibrato_cents: f32,
    /// How far the LFO moves the cutoff, in octaves.
    pub wobble_octaves: f32,
    /// How deep the LFO dips the volume, from 0 to 1.
    pub tremolo: f32,
    /// One voice only, gliding between notes.
    pub mono: bool,
    /// How long a glide takes, in seconds (mono only).
    pub glide_seconds: f32,
    /// How much velocity changes loudness and brightness, from 0 to 1.
    pub velocity: f32,
    /// The output level (linear).
    pub gain: f32,
}

impl Default for SynthParams {
    /// A plain saw through an open filter.
    fn default() -> Self {
        Self {
            osc: [
                OscParams {
                    wave: Wave::Saw,
                    pitch: 0.0,
                    level: 1.0,
                },
                OscParams {
                    wave: Wave::Saw,
                    pitch: 0.0,
                    level: 0.0,
                },
            ],
            pulse_width: 0.5,
            noise: 0.0,
            filter_mode: FilterMode::LowPass24,
            cutoff_hz: 20_000.0,
            resonance: 0.0,
            filter_env_octaves: 0.0,
            key_tracking: 0.0,
            amp_env: EnvParams::default(),
            filter_env: EnvParams::default(),
            lfo_shape: LfoShape::Sine,
            lfo_rate_hz: 5.0,
            vibrato_cents: 0.0,
            wobble_octaves: 0.0,
            tremolo: 0.0,
            mono: false,
            glide_seconds: 0.0,
            velocity: 0.5,
            gain: 1.0,
        }
    }
}

/// One voice: a note being played.
#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    key: u8,
    /// Whether its key is still down.
    held: bool,
    velocity: f32,
    /// When it started, to find the oldest.
    started: u64,
    /// The pitch now, as a key number (fractional while gliding).
    pitch: f32,
    /// The pitch it glides towards.
    target: f32,
    phase: [f64; 2],
    step: [f64; 2],
    amp: Envelope,
    filter_env: Envelope,
    filter: VoiceFilter,
    noise: Random,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            key: 0,
            held: false,
            velocity: 0.0,
            started: 0,
            pitch: 60.0,
            target: 60.0,
            phase: [0.0; 2],
            step: [0.0; 2],
            amp: Envelope::default(),
            filter_env: Envelope::default(),
            filter: VoiceFilter::default(),
            noise: Random::new(1),
        }
    }
}

/// The built-in subtractive synth.
pub struct Synth {
    params: SynthParams,
    amp_rates: EnvRates,
    filter_rates: EnvRates,
    voices: [Voice; VOICES],
    sample_rate: f32,
    /// Notes started so far (ages voices).
    notes: u64,
    /// Mono mode: the keys held, oldest first.
    held: [u8; HELD_KEYS],
    held_count: usize,
    /// Mono mode: the pitch the last note ended on (glides start there).
    last_pitch: Option<f32>,
    lfo_phase: f64,
    lfo_held: f32,
    lfo_smooth: f32,
    random: Random,
    /// The volume the tremolo leaves, at the last control update and at
    /// the one before (the block glides between them).
    tremolo: f32,
    last_tremolo: f32,
    /// Samples left before the next control update.
    until_control: usize,
}

impl Synth {
    /// A synth at a sample rate, with its settings.
    pub fn new(sample_rate: u32, params: SynthParams) -> Self {
        #[allow(clippy::cast_precision_loss, reason = "sample rates are small")]
        let sample_rate = sample_rate.max(1) as f32;
        let mut synth = Self {
            params,
            amp_rates: EnvRates::default(),
            filter_rates: EnvRates::default(),
            voices: [Voice::default(); VOICES],
            sample_rate,
            notes: 0,
            held: [0; HELD_KEYS],
            held_count: 0,
            last_pitch: None,
            lfo_phase: 0.0,
            lfo_held: 0.0,
            lfo_smooth: 0.0,
            random: Random::new(0x5EED_1234),
            tremolo: 1.0,
            last_tremolo: 1.0,
            until_control: 0,
        };
        synth.set_params(&params);
        synth
    }

    /// The settings in use.
    pub fn params(&self) -> &SynthParams {
        &self.params
    }

    /// Changes the settings while playing; sounding notes carry on with
    /// the new sound.
    pub fn set_params(&mut self, params: &SynthParams) {
        let was_mono = self.params.mono;
        self.params = *params;
        self.amp_rates = EnvRates::new(&params.amp_env, self.sample_rate);
        self.filter_rates = EnvRates::new(&params.filter_env, self.sample_rate);
        if params.mono != was_mono {
            // Switching between one voice and many lets go of everything.
            for voice in &mut self.voices {
                voice.held = false;
                voice.amp.release();
                voice.filter_env.release();
            }
            self.held_count = 0;
        }
        self.until_control = 0;
    }

    /// How many voices are sounding.
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    /// A voice to start a note on: a free one, else the quietest released
    /// one, else the oldest.
    fn free_voice(&self) -> usize {
        if let Some(free) = self.voices.iter().position(|v| !v.active) {
            return free;
        }
        let released = (0..VOICES)
            .filter(|&i| !self.voices[i].held)
            .min_by(|&a, &b| {
                self.voices[a]
                    .amp
                    .level()
                    .total_cmp(&self.voices[b].amp.level())
            });
        released.unwrap_or_else(|| {
            (0..VOICES)
                .min_by_key(|&i| self.voices[i].started)
                .unwrap_or(0)
        })
    }

    /// Starts a voice from silence (or takes over a stolen one).
    fn start_voice(&mut self, index: usize, key: u8, velocity: f32, pitch_from: f32) {
        self.notes += 1;
        let fresh = !self.voices[index].active;
        let starts = [self.random.next_unit(), self.random.next_unit()];
        let seed = self.notes;
        let voice = &mut self.voices[index];
        if fresh {
            voice.filter.reset();
            voice.phase = starts;
            voice.amp.stop();
            voice.filter_env.stop();
        }
        voice.active = true;
        voice.key = key;
        voice.held = true;
        voice.velocity = velocity.clamp(0.0, 1.0);
        voice.started = seed;
        voice.pitch = pitch_from;
        voice.target = f32::from(key);
        #[allow(clippy::cast_possible_truncation, reason = "any bits make a seed")]
        {
            voice.noise = Random::new(0x9E37_79B9 ^ seed as u32);
        }
        voice.amp.start();
        voice.filter_env.start();
        self.until_control = 0;
    }

    fn note_on_mono(&mut self, key: u8, velocity: f32) {
        self.forget_held(key);
        if self.held_count == HELD_KEYS {
            self.held.copy_within(1.., 0);
            self.held_count -= 1;
        }
        self.held[self.held_count] = key;
        self.held_count += 1;

        let target = f32::from(key);
        let glide = self.params.glide_seconds > 0.0;
        let voice = &mut self.voices[0];
        if voice.active && voice.held {
            // Legato: the sounding note moves to the new key.
            voice.key = key;
            voice.target = target;
            if !glide {
                voice.pitch = target;
            }
            self.until_control = 0;
            return;
        }
        let from = if glide {
            if voice.active {
                voice.pitch
            } else {
                self.last_pitch.unwrap_or(target)
            }
        } else {
            target
        };
        self.start_voice(0, key, velocity, from);
    }

    fn note_off_mono(&mut self, key: u8) {
        self.forget_held(key);
        let voice = &mut self.voices[0];
        if !voice.active || !voice.held || voice.key != key {
            return;
        }
        if self.held_count > 0 {
            // Back to the key still held most recently.
            let previous = self.held[self.held_count - 1];
            voice.key = previous;
            voice.target = f32::from(previous);
            if self.params.glide_seconds <= 0.0 {
                voice.pitch = voice.target;
            }
            self.until_control = 0;
        } else {
            voice.held = false;
            voice.amp.release();
            voice.filter_env.release();
        }
    }

    fn forget_held(&mut self, key: u8) {
        let mut kept = 0;
        for i in 0..self.held_count {
            if self.held[i] != key {
                self.held[kept] = self.held[i];
                kept += 1;
            }
        }
        self.held_count = kept;
    }

    /// The LFO's value now, from -1 to 1, and moves it on by `samples`.
    fn advance_lfo(&mut self, samples: usize) -> f32 {
        let t = self.lfo_phase;
        #[allow(clippy::cast_possible_truncation, reason = "a wave value")]
        let raw = match self.params.lfo_shape {
            LfoShape::Sine => (t * std::f64::consts::TAU).sin() as f32,
            LfoShape::Triangle => (1.0 - 4.0 * (t - 0.5).abs()) as f32,
            LfoShape::Square => {
                if t < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::Saw => (1.0 - 2.0 * t) as f32,
            LfoShape::Random => self.lfo_held,
        };
        #[allow(clippy::cast_precision_loss, reason = "a block of samples")]
        let advance = f64::from(self.params.lfo_rate_hz.clamp(0.01, 50.0)) * samples as f64
            / f64::from(self.sample_rate);
        let next = t + advance;
        if next >= 1.0 {
            self.lfo_held = self.random.next_bipolar();
        }
        self.lfo_phase = next.fract();
        // Smoothed over a few milliseconds, so jumps (square, random) do
        // not click.
        self.lfo_smooth += (raw - self.lfo_smooth) * 0.6;
        self.lfo_smooth
    }

    /// Works out pitches and cutoffs for the next few samples.
    fn control(&mut self) {
        let lfo = self.advance_lfo(CONTROL);
        let p = self.params;
        let rate = self.sample_rate;
        #[allow(clippy::cast_precision_loss, reason = "a block of samples")]
        let glide = if p.mono && p.glide_seconds > 0.0 {
            1.0 - (-(CONTROL as f32) / (p.glide_seconds / 4.0 * rate)).exp()
        } else {
            1.0
        };
        for voice in self.voices.iter_mut().filter(|v| v.active) {
            voice.pitch += (voice.target - voice.pitch) * glide;
            let vibrato = p.vibrato_cents / 100.0 * lfo;
            for (step, osc) in voice.step.iter_mut().zip(&p.osc) {
                let hz = key_to_hz(voice.pitch + osc.pitch + vibrato);
                *step = f64::from((hz / rate).min(0.49));
            }
            let softness = 1.0 - 0.5 * p.velocity * (1.0 - voice.velocity);
            let octaves = p.filter_env_octaves * voice.filter_env.level() * softness
                + p.wobble_octaves * lfo
                + p.key_tracking * (voice.pitch - 60.0) / 12.0;
            let cutoff = p.cutoff_hz * octaves.clamp(-12.0, 12.0).exp2();
            voice
                .filter
                .tune(p.filter_mode, cutoff, p.resonance, self.sample_rate);
        }
        let last = self.voices[0];
        if p.mono && last.active {
            self.last_pitch = Some(last.pitch);
        }
        // The tremolo glides to its new value over the block.
        self.until_control = CONTROL;
        self.last_tremolo = self.tremolo;
        self.tremolo = 1.0 - p.tremolo.clamp(0.0, 1.0) * (0.5 - 0.5 * lfo);
    }
}

/// Renders one voice into `out` (adding to it), its level starting at
/// `gain.0` and changing by `gain.1` each sample.
fn render_voice(
    voice: &mut Voice,
    params: &SynthParams,
    rates: (&EnvRates, &EnvRates),
    gain: (f32, f32),
    out: &mut [f32],
) {
    let (amp_rates, filter_rates) = rates;
    let width = f64::from(params.pulse_width.clamp(0.05, 0.95));
    let [one, two] = params.osc;
    let loudness = VOICE_GAIN * (1.0 - params.velocity * (1.0 - voice.velocity));
    let (mut level_gain, slope) = gain;
    for sample in out.iter_mut() {
        level_gain += slope;
        let mut mix = 0.0_f64;
        if one.level > 0.0 {
            mix += wave(one.wave, voice.phase[0], voice.step[0], width) * f64::from(one.level);
        }
        if two.level > 0.0 {
            mix += wave(two.wave, voice.phase[1], voice.step[1], width) * f64::from(two.level);
        }
        #[allow(clippy::cast_possible_truncation, reason = "audio samples")]
        let mut input = mix as f32;
        if params.noise > 0.0 {
            input += voice.noise.next_bipolar() * params.noise;
        }
        let filtered = voice.filter.process(input);
        let level = voice.amp.next(amp_rates);
        voice.filter_env.next(filter_rates);
        *sample += filtered * level * loudness * level_gain;
        for (phase, step) in voice.phase.iter_mut().zip(voice.step) {
            *phase += step;
            if *phase >= 1.0 {
                *phase -= 1.0;
            }
        }
        if voice.amp.is_idle() {
            voice.active = false;
            break;
        }
    }
}

impl Instrument for Synth {
    fn note_on(&mut self, key: u8, velocity: f32) {
        if self.params.mono {
            self.note_on_mono(key, velocity);
            return;
        }
        // A key already sounding starts again in its own voice.
        let index = self
            .voices
            .iter()
            .position(|v| v.active && v.held && v.key == key)
            .unwrap_or_else(|| self.free_voice());
        self.start_voice(index, key, velocity, f32::from(key));
    }

    fn note_off(&mut self, key: u8) {
        if self.params.mono {
            self.note_off_mono(key);
            return;
        }
        for voice in self
            .voices
            .iter_mut()
            .filter(|v| v.active && v.held && v.key == key)
        {
            voice.held = false;
            voice.amp.release();
            voice.filter_env.release();
        }
    }

    fn all_notes_off(&mut self, immediate: bool) {
        self.held_count = 0;
        for voice in &mut self.voices {
            voice.held = false;
            if immediate {
                voice.active = false;
                voice.amp.stop();
                voice.filter_env.stop();
            } else {
                voice.amp.release();
                voice.filter_env.release();
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        left[..len].fill(0.0);
        let mut done = 0;
        while done < len {
            if self.until_control == 0 {
                self.control();
            }
            let chunk = self.until_control.min(len - done);
            let out = &mut left[done..done + chunk];
            // Where in the block this chunk starts (a block can be split
            // between two calls).
            #[allow(clippy::cast_precision_loss, reason = "small counts")]
            let into = (CONTROL - self.until_control) as f32 / CONTROL as f32;
            let volume = self.params.gain.max(0.0);
            let from = volume * (self.last_tremolo + (self.tremolo - self.last_tremolo) * into);
            #[allow(clippy::cast_precision_loss, reason = "small counts")]
            let slope = (volume * self.tremolo - from) / self.until_control as f32;
            for voice in self.voices.iter_mut().filter(|v| v.active) {
                render_voice(
                    voice,
                    &self.params,
                    (&self.amp_rates, &self.filter_rates),
                    (from, slope),
                    out,
                );
            }
            self.until_control -= chunk;
            done += chunk;
        }
        right[..len].copy_from_slice(&left[..len]);
    }

    fn is_sounding(&self) -> bool {
        self.voices.iter().any(|v| v.active)
    }

    fn apply_settings(&mut self, settings: &InstrumentSettings) {
        if let InstrumentSettings::Synth(params) = settings {
            self.set_params(params);
        }
    }
}

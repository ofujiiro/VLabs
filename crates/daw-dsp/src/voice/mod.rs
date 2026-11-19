// SPDX-License-Identifier: GPL-3.0-or-later

//! Singing (decision 0027): the recordings of a classic UTAU voicebank,
//! each syllable shifted to its note's pitch and stretched to its length by
//! a resampler of our own (pitch-synchronous overlap-add, which keeps the
//! voice's formants), and the notes of a phrase joined the way UTAU's
//! wavtool joins them: each syllable starts its *preutterance* before its
//! note and fades in over the note before for its *overlap*.
//!
//! Decision 0028 adds expression: the pitch a phrase is sung at comes from
//! outside as a function of time (the model works it out from the notes'
//! pitch points and vibrato), and each note has a loudness, a colour (its
//! formants moved by reading the recording faster or slower inside each
//! grain) and a breath (a whisper of the same syllable mixed in).
//!
//! Nothing here runs on the audio thread: phrases are rendered in the
//! background and played as sound (decision 0017's audio clips).

mod analysis;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

pub use analysis::VoiceSample;

/// The fade-in at the start of a phrase and the fade-out at its end.
const PHRASE_FADE_IN: f64 = 0.005;
const PHRASE_FADE_OUT: f64 = 0.03;
/// The shortest crossfade between two notes.
const MIN_CROSSFADE: f64 = 0.002;
/// The share of the time between two notes' starts a syllable may take
/// before its note (UTAU shortens preutterance and overlap together when a
/// note is too short for the next one's).
const MAX_LEAD_SHARE: f64 = 0.5;
/// Breath is noise above this frequency (Hz).
const BREATH_FROM_HZ: f64 = 1000.0;
/// How much quieter the voice is at full breath.
const BREATH_TAKES: f32 = 0.3;
/// How much of the voice's own part above [`BREATH_FROM_HZ`] gives way at
/// full breath (a breathy voice has weaker high harmonics).
const BREATH_SOFTENS: f32 = 0.5;
/// How loud the breath is at full breath, against the voice (its highs
/// are then mostly breath); at half breath, half that.
const BREATH_LEVEL: f64 = 0.7;
/// How far colour may move the formants, as a ratio either way.
const MAX_FORMANT_RATIO: f64 = 1.5;

/// One line of a voicebank's `oto.ini`: where a syllable is in which
/// recording. Times are in milliseconds, as UTAU writes them.
#[derive(Clone, Debug, PartialEq)]
pub struct OtoEntry {
    /// The name notes ask for ("a か", "か", "ka").
    pub alias: String,
    /// The recording, relative to the voicebank's folder.
    pub file: String,
    /// Where the syllable starts in the recording.
    pub offset: f64,
    /// How long its consonant lasts after `offset` (never stretched).
    pub consonant: f64,
    /// Where it ends: negative, its length after `offset`; positive, how
    /// much of the recording's end is left out.
    pub cutoff: f64,
    /// How long after `offset` the note's beat falls (the syllable starts
    /// sounding this long before its note).
    pub preutterance: f64,
    /// How long after `offset` it overlaps the note before.
    pub overlap: f64,
}

/// Where a syllable is in its recording, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stretch {
    /// Where it starts.
    pub start: f64,
    /// How long its consonant lasts (played as recorded).
    pub fixed: f64,
    /// Where it ends.
    pub end: f64,
}

impl OtoEntry {
    /// Where the syllable is in a recording of `seconds`.
    pub fn stretch(&self, seconds: f64) -> Stretch {
        let start = (self.offset / 1000.0).clamp(0.0, seconds);
        let end = if self.cutoff < 0.0 {
            start - self.cutoff / 1000.0
        } else {
            seconds - self.cutoff / 1000.0
        };
        let end = end.min(seconds).max(start + 0.01);
        Stretch {
            start,
            fixed: (self.consonant / 1000.0).clamp(0.0, end - start),
            end,
        }
    }
}

/// Reads one recording of a voicebank, by its path in the voicebank.
pub type SampleLoader = Box<dyn Fn(&str) -> Result<VoiceSample, String> + Send + Sync>;

/// A voicebank, ready to sing: its syllables by alias, the prefix and
/// suffix each pitch adds (`prefix.map`), and its recordings, read and
/// analysed the first time a note needs one.
pub struct VoiceBank {
    entries: HashMap<String, OtoEntry>,
    affixes: Vec<(u8, String, String)>,
    loader: SampleLoader,
    samples: Mutex<HashMap<String, Result<Arc<VoiceSample>, String>>>,
}

impl std::fmt::Debug for VoiceBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceBank")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl VoiceBank {
    /// A voicebank from its `oto.ini` lines (the first line of an alias
    /// wins, as in UTAU), its `prefix.map` (key, prefix, suffix) and a way
    /// to read its recordings.
    pub fn new(
        entries: Vec<OtoEntry>,
        affixes: Vec<(u8, String, String)>,
        loader: SampleLoader,
    ) -> Self {
        let mut by_alias = HashMap::with_capacity(entries.len());
        for entry in entries {
            by_alias.entry(entry.alias.clone()).or_insert(entry);
        }
        Self {
            entries: by_alias,
            affixes,
            loader,
            samples: Mutex::new(HashMap::new()),
        }
    }

    /// How many syllables it has.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it has none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The syllable named `alias`, if it has one.
    pub fn entry(&self, alias: &str) -> Option<&OtoEntry> {
        self.entries.get(alias)
    }

    /// The prefix and suffix `prefix.map` gives a key (none when it has no
    /// map).
    pub fn affixes(&self, key: u8) -> (&str, &str) {
        self.affixes
            .iter()
            .find(|(k, _, _)| *k == key)
            .map_or(("", ""), |(_, prefix, suffix)| {
                (prefix.as_str(), suffix.as_str())
            })
    }

    /// The first of `candidates` (aliases, in the order to try them) the
    /// voicebank has for a note on `key`: with the key's prefix and suffix
    /// first, then without.
    pub fn find(&self, candidates: &[String], key: u8) -> Option<&OtoEntry> {
        let (prefix, suffix) = self.affixes(key);
        if !prefix.is_empty() || !suffix.is_empty() {
            for alias in candidates {
                if let Some(entry) = self.entries.get(&format!("{prefix}{alias}{suffix}")) {
                    return Some(entry);
                }
            }
        }
        candidates.iter().find_map(|alias| self.entries.get(alias))
    }

    /// A recording, read and analysed the first time.
    pub fn sample(&self, file: &str) -> Result<Arc<VoiceSample>, String> {
        let mut samples = self.samples.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(result) = samples.get(file) {
            return result.clone();
        }
        let result = (self.loader)(file).map(Arc::new);
        samples.insert(file.to_owned(), result.clone());
        result
    }
}

/// The frequency of a key (fractional keys bend between semitones).
pub fn key_hz(key: f64) -> f64 {
    440.0 * ((key - 69.0) / 12.0).exp2()
}

/// How a syllable sounds beyond its pitch (decision 0028).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colour {
    /// How much faster the recording is read inside each grain: the
    /// formants move up by this ratio (1 leaves them; below 1 is deeper).
    pub formant: f64,
    /// How much of a whisper of the syllable is mixed in, from 0 to 1.
    pub breath: f32,
    /// Where the whisper's randomness starts (the same seed, the same
    /// whisper).
    pub seed: u64,
}

impl Default for Colour {
    fn default() -> Self {
        Self {
            formant: 1.0,
            breath: 0.0,
            seed: 1,
        }
    }
}

/// Where the syllable is in its recording at each moment of the sung note
/// (seconds in, seconds out): its consonant as recorded, its vowel slowed
/// down to fill a longer note (or cut short).
fn source_clock(region: Stretch, length: f64) -> impl Fn(f64) -> f64 {
    let fixed = region
        .fixed
        .clamp(0.0, (region.end - region.start).max(0.0));
    let stretch_in = (region.end - region.start - fixed).max(1e-3);
    let stretch_out = (length - fixed).max(0.0);
    let speed = if stretch_out > stretch_in {
        stretch_in / stretch_out
    } else {
        1.0
    };
    move |t: f64| {
        let s = if t < fixed {
            region.start + t
        } else {
            region.start + fixed + (t - fixed) * speed
        };
        s.min(region.end)
    }
}

/// Shifts a syllable to the pitch `hz` gives at each moment and stretches
/// it to `length` seconds (decision 0027): its consonant plays as recorded,
/// its vowel is stretched to fill the rest (or cut short). The result is at
/// `rate`, whatever the recording's rate.
///
/// Time-domain pitch-synchronous overlap-add: a two-period grain is taken
/// around the mark nearest each moment of the recording and laid down one
/// target period after the last, so the pitch moves and the formants stay.
/// `colour` (decision 0028) reads each grain faster or slower, which moves
/// the formants and leaves the pitch, and mixes in a whisper for breath.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "frame counts are small and never negative"
)]
pub fn resample(
    sample: &VoiceSample,
    region: Stretch,
    length: f64,
    rate: u32,
    hz: impl Fn(f64) -> f64,
    colour: Colour,
) -> Vec<f32> {
    let rate_f = f64::from(rate.max(1));
    let source_rate = f64::from(sample.rate());
    let frames = (length.max(0.0) * rate_f).ceil() as usize;
    let mut out = vec![0.0_f32; frames];
    if frames == 0 {
        return out;
    }
    let formant = colour
        .formant
        .clamp(1.0 / MAX_FORMANT_RATIO, MAX_FORMANT_RATIO);
    let source_at = source_clock(region, length);
    let mut t = 0.0;
    while t < length {
        let mark = sample.mark_near(source_at(t) * source_rate);
        // Half the grain's width, as laid down: read faster, it is shorter.
        let half = mark.period / source_rate / formant;
        let step = if mark.voiced {
            1.0 / hz(t).clamp(20.0, 4000.0)
        } else {
            half
        };
        // Grains spaced closer than their width add up: scale them so the
        // level stays the recording's.
        let gain = (step / half).clamp(0.25, 2.0) as f32;
        let first = ((t - half) * rate_f).ceil().max(0.0) as usize;
        let last = (((t + half) * rate_f).floor().max(0.0) as usize).min(frames - 1);
        for (j, value) in out.iter_mut().enumerate().take(last + 1).skip(first) {
            let dt = j as f64 / rate_f - t;
            let window = f64::midpoint(1.0, (std::f64::consts::PI * dt / half).cos());
            let source = mark.at as f64 + dt * source_rate * formant;
            *value += gain * window as f32 * sample.at(source);
        }
        t += step.max(1.0 / rate_f);
    }
    if colour.breath > 0.0 {
        let breath = colour.breath.min(1.0);
        let air = high_passed(
            whisper(sample, &source_at, frames, rate, formant, colour.seed),
            rate,
        );
        let air_level = rms(&air);
        if air_level > 0.0 {
            let scale = (rms(&out) / air_level * BREATH_LEVEL) as f32;
            let highs = high_passed(out.clone(), rate);
            let voice = 1.0 - BREATH_TAKES * breath;
            for ((value, air), high) in out.iter_mut().zip(air).zip(highs) {
                *value = voice * (*value - BREATH_SOFTENS * breath * high) + air * scale * breath;
            }
        }
    }
    out
}

#[allow(clippy::cast_precision_loss, reason = "sample counts fit easily")]
fn rms(x: &[f32]) -> f64 {
    let sum: f64 = x.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    (sum / x.len().max(1) as f64).sqrt()
}

/// A small, fast source of randomness (xorshift), the same for the same
/// seed.
struct Noise(u64);

impl Noise {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number from 0 to 1.
    #[allow(clippy::cast_precision_loss, reason = "53 bits of randomness")]
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// A whisper of the syllable (decision 0028): grains one period of the
/// recording wide, taken around where the syllable is but at random places
/// within a period, with random signs, laid down at random spacings. Each
/// grain's spectrum is the recording's, so the vowel's formants stay; the
/// randomness takes away its pitch.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "frame counts are small and never negative"
)]
fn whisper(
    sample: &VoiceSample,
    source_at: &impl Fn(f64) -> f64,
    frames: usize,
    rate: u32,
    formant: f64,
    seed: u64,
) -> Vec<f32> {
    let rate_f = f64::from(rate.max(1));
    let source_rate = f64::from(sample.rate());
    let length = frames as f64 / rate_f;
    let mut out = vec![0.0_f32; frames];
    let mut noise = Noise::new(seed);
    let mut t = 0.0;
    while t < length && frames > 0 {
        let centre = source_at(t) * source_rate;
        let mark = sample.mark_near(centre);
        let period = mark.period / source_rate;
        let half = 0.5 * period / formant;
        let from = centre + (noise.unit() - 0.5) * mark.period;
        let sign = if noise.next() & 1 == 0 { 1.0 } else { -1.0 };
        let first = ((t - half) * rate_f).ceil().max(0.0) as usize;
        let last = (((t + half) * rate_f).floor().max(0.0) as usize).min(frames - 1);
        for (j, value) in out.iter_mut().enumerate().take(last + 1).skip(first) {
            let dt = j as f64 / rate_f - t;
            let window = f64::midpoint(1.0, (std::f64::consts::PI * dt / half).cos());
            *value += (sign * window) as f32 * sample.at(from + dt * source_rate * formant);
        }
        t += (period * (0.25 + 0.5 * noise.unit())).max(1.0 / rate_f);
    }
    out
}

/// Only the part of `x` above [`BREATH_FROM_HZ`] (two one-pole high-pass
/// filters in a row, 12 dB an octave).
#[allow(clippy::cast_possible_truncation, reason = "a filter coefficient")]
fn high_passed(mut x: Vec<f32>, rate: u32) -> Vec<f32> {
    let k = (-std::f64::consts::TAU * BREATH_FROM_HZ / f64::from(rate.max(1))).exp() as f32;
    for _ in 0..2 {
        let (mut last_in, mut last_out) = (0.0_f32, 0.0_f32);
        for value in &mut x {
            let output = k * (last_out + *value - last_in);
            last_in = *value;
            last_out = output;
            *value = output;
        }
    }
    x
}

/// One note of a phrase, ready to sing.
#[derive(Clone, Debug)]
pub struct SungNote {
    /// The recording its syllable is in.
    pub sample: Arc<VoiceSample>,
    /// Where the syllable is in it.
    pub region: Stretch,
    /// How long before the note the syllable starts (seconds).
    pub preutterance: f64,
    /// How long it overlaps the note before (seconds).
    pub overlap: f64,
    /// When the note starts, in seconds of the song.
    pub start: f64,
    /// How long it lasts.
    pub length: f64,
    /// How loud it is sung (1 leaves the recording's level).
    pub gain: f32,
    /// Its colour and breath (decision 0028).
    pub colour: Colour,
}

impl SungNote {
    /// A note singing `entry` from `sample`, at the recording's level, with
    /// no colour or breath.
    pub fn new(sample: Arc<VoiceSample>, entry: &OtoEntry, start: f64, length: f64) -> Self {
        let region = entry.stretch(sample.seconds());
        Self {
            region,
            sample,
            preutterance: (entry.preutterance / 1000.0).max(0.0),
            overlap: (entry.overlap / 1000.0).max(0.0),
            start,
            length,
            gain: 1.0,
            colour: Colour::default(),
        }
    }
}

/// A rendered phrase: mono sound starting at `start` seconds of the song.
#[derive(Clone, Debug, PartialEq)]
pub struct SungPhrase {
    /// When it starts in the song (before zero for a phrase whose first
    /// syllable starts before the song).
    pub start: f64,
    /// Its sound.
    pub samples: Vec<f32>,
}

/// Sings a phrase (decision 0027): notes in order, each touching or close
/// to the next, at the pitch `pitch` gives (a fractional key, 69 being A
/// 440) at each second of the song. Each syllable starts its preutterance
/// before its note and fades in over the note before for its overlap; when
/// a note is too short for the next one's preutterance, both preutterance
/// and overlap shrink together. Each note is sung at its gain and colour
/// (decision 0028). Rendered at `rate`.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "frame counts are small and never negative"
)]
pub fn sing_phrase(notes: &[SungNote], pitch: impl Fn(f64) -> f64, rate: u32) -> SungPhrase {
    if notes.is_empty() {
        return SungPhrase {
            start: 0.0,
            samples: Vec::new(),
        };
    }
    let mut notes = notes.to_vec();
    for i in 1..notes.len() {
        let room = (notes[i].start - notes[i - 1].start).max(0.0) * MAX_LEAD_SHARE;
        if notes[i].preutterance > room {
            let scale = room / notes[i].preutterance;
            notes[i].preutterance *= scale;
            notes[i].overlap *= scale;
        }
    }
    // Where each syllable's sound begins and ends in the song.
    let begins: Vec<f64> = notes.iter().map(|n| n.start - n.preutterance).collect();
    let ends: Vec<f64> = (0..notes.len())
        .map(|i| match notes.get(i + 1) {
            Some(next) => begins[i + 1] + next.overlap.max(MIN_CROSSFADE),
            None => notes[i].start + notes[i].length,
        })
        .collect();
    let start = begins.iter().copied().fold(f64::INFINITY, f64::min);
    let end = ends.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let rate_f = f64::from(rate.max(1));
    let mut samples = vec![0.0_f32; ((end - start).max(0.0) * rate_f).ceil() as usize + 1];
    for (i, note) in notes.iter().enumerate() {
        let length = (ends[i] - begins[i]).max(0.0);
        let begin = begins[i];
        let sung = resample(
            &note.sample,
            note.region,
            length,
            rate,
            |t| key_hz(pitch(begin + t)),
            note.colour,
        );
        let fade_in = if i == 0 {
            PHRASE_FADE_IN
        } else {
            note.overlap.max(MIN_CROSSFADE)
        };
        let fade_out = match notes.get(i + 1) {
            Some(next) => next.overlap.max(MIN_CROSSFADE),
            None => PHRASE_FADE_OUT,
        };
        let offset = ((begin - start) * rate_f).round().max(0.0) as usize;
        let total = sung.len() as f64 / rate_f;
        for (k, value) in sung.iter().enumerate() {
            let t = k as f64 / rate_f;
            let gain = (t / fade_in).min(1.0) * ((total - t) / fade_out).clamp(0.0, 1.0);
            if let Some(slot) = samples.get_mut(offset + k) {
                *slot += value * gain as f32 * note.gain;
            }
        }
    }
    SungPhrase { start, samples }
}

#[cfg(test)]
mod tests;

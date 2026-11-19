// SPDX-License-Identifier: GPL-3.0-or-later

//! Singing (decision 0027): the recordings of a classic UTAU voicebank,
//! each syllable shifted to its note's pitch and stretched to its length by
//! a resampler of our own (pitch-synchronous overlap-add, which keeps the
//! voice's formants), and the notes of a phrase joined the way UTAU's
//! wavtool joins them: each syllable starts its *preutterance* before its
//! note and fades in over the note before for its *overlap*.
//!
//! Nothing here runs on the audio thread: phrases are rendered in the
//! background and played as sound (decision 0017's audio clips).

mod analysis;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

pub use analysis::VoiceSample;

/// How long the pitch takes to glide from one note into the next, centred
/// on the note's start (seconds).
const PORTAMENTO_SECONDS: f64 = 0.08;
/// The fade-in at the start of a phrase and the fade-out at its end.
const PHRASE_FADE_IN: f64 = 0.005;
const PHRASE_FADE_OUT: f64 = 0.03;
/// The shortest crossfade between two notes.
const MIN_CROSSFADE: f64 = 0.002;
/// The share of the time between two notes' starts a syllable may take
/// before its note (UTAU shortens preutterance and overlap together when a
/// note is too short for the next one's).
const MAX_LEAD_SHARE: f64 = 0.5;

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

/// Shifts a syllable to the pitch `hz` gives at each moment and stretches
/// it to `length` seconds (decision 0027): its consonant plays as recorded,
/// its vowel is stretched to fill the rest (or cut short). The result is at
/// `rate`, whatever the recording's rate.
///
/// Time-domain pitch-synchronous overlap-add: a two-period grain is taken
/// around the mark nearest each moment of the recording and laid down one
/// target period after the last, so the pitch moves and the formants stay.
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
) -> Vec<f32> {
    let rate_f = f64::from(rate.max(1));
    let source_rate = f64::from(sample.rate());
    let frames = (length.max(0.0) * rate_f).ceil() as usize;
    let mut out = vec![0.0_f32; frames];
    if frames == 0 {
        return out;
    }
    let fixed = region
        .fixed
        .clamp(0.0, (region.end - region.start).max(0.0));
    let stretch_in = (region.end - region.start - fixed).max(1e-3);
    let stretch_out = (length - fixed).max(0.0);
    // Longer than the recording: the vowel slows down; shorter: cut.
    let speed = if stretch_out > stretch_in {
        stretch_in / stretch_out
    } else {
        1.0
    };
    let source_at = |t: f64| {
        let s = if t < fixed {
            region.start + t
        } else {
            region.start + fixed + (t - fixed) * speed
        };
        s.min(region.end)
    };
    let mut t = 0.0;
    while t < length {
        let mark = sample.mark_near(source_at(t) * source_rate);
        let half = mark.period / source_rate;
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
            let source = mark.at as f64 + dt * source_rate;
            *value += gain * window as f32 * sample.at(source);
        }
        t += step.max(1.0 / rate_f);
    }
    out
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
    /// Its key (MIDI number).
    pub key: f64,
}

impl SungNote {
    /// A note singing `entry` from `sample`.
    pub fn new(
        sample: Arc<VoiceSample>,
        entry: &OtoEntry,
        start: f64,
        length: f64,
        key: f64,
    ) -> Self {
        let region = entry.stretch(sample.seconds());
        Self {
            region,
            sample,
            preutterance: (entry.preutterance / 1000.0).max(0.0),
            overlap: (entry.overlap / 1000.0).max(0.0),
            start,
            length,
            key,
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

/// The pitch of a phrase at a moment of the song, in semitones: each
/// note's key, gliding into it from the note before around its start.
fn phrase_key(notes: &[SungNote], at: f64) -> f64 {
    let index = notes
        .partition_point(|n| n.start - PORTAMENTO_SECONDS / 2.0 <= at)
        .saturating_sub(1);
    let note = &notes[index];
    let Some(before) = index.checked_sub(1).map(|i| &notes[i]) else {
        return note.key;
    };
    let into = (at - (note.start - PORTAMENTO_SECONDS / 2.0)) / PORTAMENTO_SECONDS;
    if into >= 1.0 {
        return note.key;
    }
    let eased = 0.5 - 0.5 * (std::f64::consts::PI * into.clamp(0.0, 1.0)).cos();
    before.key + (note.key - before.key) * eased
}

/// Sings a phrase (decision 0027): notes in order, each touching or close
/// to the next. Each syllable starts its preutterance before its note and
/// fades in over the note before for its overlap; when a note is too short
/// for the next one's preutterance, both preutterance and overlap shrink
/// together. Rendered at `rate`.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "frame counts are small and never negative"
)]
pub fn sing_phrase(notes: &[SungNote], rate: u32) -> SungPhrase {
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
        let sung = resample(&note.sample, note.region, length, rate, |t| {
            key_hz(phrase_key(&notes, begin + t))
        });
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
                *slot += value * gain as f32;
            }
        }
    }
    SungPhrase { start, samples }
}

#[cfg(test)]
mod tests;

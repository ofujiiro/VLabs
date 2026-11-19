// SPDX-License-Identifier: GPL-3.0-or-later

//! Singing voices in the engine (decision 0027). A voice track's notes, as
//! the song plays them, are placed in seconds through the tempo map and cut
//! into phrases; each phrase is rendered on a worker thread and kept,
//! keyed by everything that shapes its sound, so an edit renders again only
//! the phrases it changed. Rendered phrases play as sound placed in the
//! song, exactly as audio clips do; nothing new runs on the audio thread.
//!
//! Voicebanks are read in the background the first time a track needs
//! one; until then (and while its phrases render) the track is silent.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use daw_dsp::audio::AudioBuffer;
use daw_dsp::voice::{OtoEntry, SungNote, VoiceBank, sing_phrase};
use daw_model::lyrics::{DEFAULT_LYRIC, alias_candidates, is_rest};
use daw_model::{Project, Track};

use crate::program::AudioSpan;

/// Reads a voicebank, by the name of its folder among the installed ones.
pub type VoiceReader = Arc<dyn Fn(&str) -> Result<VoiceBank, String> + Send + Sync>;

/// A rest at least this long (seconds) starts a new phrase.
const PHRASE_GAP: f64 = 0.05;

type BankLoad = Arc<OnceLock<Result<Arc<VoiceBank>, String>>>;

/// A phrase's state.
#[derive(Clone)]
enum Phrase {
    /// Being rendered.
    Rendering,
    /// Rendered: its sound, starting at this frame of the song.
    Ready { start: i64, sound: Arc<AudioBuffer> },
    /// Nothing could be sung (its recordings could not be read).
    Silent,
}

/// One note of a phrase to render.
#[derive(Clone)]
struct JobNote {
    entry: OtoEntry,
    start: f64,
    length: f64,
    key: u8,
}

/// A phrase to render.
struct Job {
    key: u64,
    bank: Arc<VoiceBank>,
    notes: Vec<JobNote>,
    rate: u32,
}

/// The voices of a library: shared by every copy of it (the live engine's
/// and an export's), each copy saying whether it renders in the background.
#[derive(Clone)]
pub(crate) struct Voices {
    reader: Option<VoiceReader>,
    background: bool,
    banks: Arc<Mutex<HashMap<String, BankLoad>>>,
    phrases: Arc<Mutex<HashMap<u64, Phrase>>>,
    worker: Arc<Mutex<Option<mpsc::Sender<Job>>>>,
    /// Counts the phrases rendered, so the engine knows when to play them.
    finished: Arc<AtomicU64>,
}

/// What a voice track sings, as far as it is rendered.
#[derive(Clone, Debug, Default)]
pub(crate) struct VoiceTrack {
    /// The phrases ready to play.
    pub spans: Vec<AudioSpan>,
    /// Whether some are still being rendered (or its voicebank read).
    pub pending: bool,
}

impl Voices {
    pub fn new() -> Self {
        Self {
            reader: None,
            background: false,
            banks: Arc::new(Mutex::new(HashMap::new())),
            phrases: Arc::new(Mutex::new(HashMap::new())),
            worker: Arc::new(Mutex::new(None)),
            finished: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn set_reader(&mut self, reader: VoiceReader) {
        self.reader = Some(reader);
    }

    pub fn set_background(&mut self, background: bool) {
        self.background = background;
    }

    /// How many phrases have been rendered so far.
    pub fn finished(&self) -> u64 {
        self.finished.load(Ordering::Acquire)
    }

    fn load(&self, name: &str) -> BankLoad {
        let mut banks = self.banks.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(load) = banks.get(name) {
            return Arc::clone(load);
        }
        let load: BankLoad = Arc::new(OnceLock::new());
        banks.insert(name.to_owned(), Arc::clone(&load));
        drop(banks);
        let reader = self.reader.clone();
        let cell = Arc::clone(&load);
        let name = name.to_owned();
        let read = move || {
            let result = match reader {
                Some(reader) => {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reader(&name)))
                        .unwrap_or_else(|_| Err("the reader gave up on it".to_owned()))
                        .map(Arc::new)
                }
                None => Err("voicebanks cannot be read here".to_owned()),
            };
            let _ = cell.set(result);
        };
        if self.background {
            if std::thread::Builder::new()
                .name("voicebank-reader".to_owned())
                .spawn(read)
                .is_err()
            {
                let _ = load.set(Err("no thread to read it on".to_owned()));
            }
        } else {
            read();
        }
        load
    }

    /// The voicebank named `name`, reading it the first time: `None` while
    /// it is being read in the background.
    pub fn bank(&self, name: &str) -> Option<Result<Arc<VoiceBank>, String>> {
        let load = self.load(name);
        if self.background {
            load.get().cloned()
        } else {
            Some(load.wait().clone())
        }
    }

    /// Whether a voicebank is still being read.
    pub fn is_loading(&self, name: &str) -> bool {
        self.banks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .is_some_and(|load| load.get().is_none())
    }

    /// Forgets the voicebanks not in `keep` and every rendered phrase (another
    /// song was opened).
    pub fn retain(&self, keep: impl Fn(&str) -> bool) {
        self.banks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|name, _| keep(name));
        self.phrases
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, phrase| matches!(phrase, Phrase::Rendering));
    }

    /// What a voice track sings: its rendered phrases as sound in the song,
    /// at `rate`. Phrases not rendered yet are started (or, outside the
    /// background, rendered at once).
    pub fn track(&self, project: &Project, track: &Track, rate: u32) -> VoiceTrack {
        let Some(name) = track.instrument.voicebank() else {
            return VoiceTrack::default();
        };
        let bank = match self.bank(name) {
            None => {
                return VoiceTrack {
                    spans: Vec::new(),
                    pending: true,
                };
            }
            Some(Err(_)) => return VoiceTrack::default(),
            Some(Ok(bank)) => bank,
        };
        let mut sung = VoiceTrack::default();
        for phrase in phrases(project, track) {
            let notes = sung_notes(&bank, &phrase);
            if notes.is_empty() {
                continue;
            }
            let key = phrase_key(name, rate, &notes);
            let known = self
                .phrases
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&key)
                .cloned();
            let state = if let Some(state) = known {
                state
            } else {
                let job = Job {
                    key,
                    bank: Arc::clone(&bank),
                    notes,
                    rate,
                };
                if self.background {
                    self.phrases
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(key, Phrase::Rendering);
                    self.send(job);
                    Phrase::Rendering
                } else {
                    let state = render(&job);
                    self.phrases
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(key, state.clone());
                    state
                }
            };
            match state {
                Phrase::Rendering => sung.pending = true,
                Phrase::Ready { start, sound } => sung.spans.push(span(start, sound)),
                Phrase::Silent => {}
            }
        }
        sung.spans.sort_by_key(|s| s.start);
        sung
    }

    /// Gives a phrase to the worker thread (starting it the first time).
    fn send(&self, job: Job) {
        let mut worker = self.worker.lock().unwrap_or_else(PoisonError::into_inner);
        let job = if let Some(sender) = worker.as_ref() {
            match sender.send(job) {
                Ok(()) => return,
                Err(mpsc::SendError(job)) => job,
            }
        } else {
            job
        };
        let (sender, receiver) = mpsc::channel::<Job>();
        let phrases = Arc::clone(&self.phrases);
        let finished = Arc::clone(&self.finished);
        let started = std::thread::Builder::new()
            .name("voice-renderer".to_owned())
            .spawn(move || {
                for job in receiver {
                    let state = render(&job);
                    phrases
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(job.key, state);
                    finished.fetch_add(1, Ordering::Release);
                }
            });
        if started.is_ok() && sender.send(job).is_ok() {
            *worker = Some(sender);
        }
    }
}

/// A rendered phrase as an audio span of the song (decision 0017's audio
/// clips): starting at frame `start` (before the song, its beginning is
/// left out).
fn span(start: i64, sound: Arc<AudioBuffer>) -> AudioSpan {
    let frames = i64::try_from(sound.frames()).unwrap_or(i64::MAX);
    let from = start.max(0);
    #[allow(clippy::cast_precision_loss, reason = "frame counts are small")]
    let offset = (from - start) as f64;
    AudioSpan {
        start: from,
        end: start.saturating_add(frames).max(from),
        sound,
        offset,
        step: 1.0,
        gain: 1.0,
        fade_in: 0,
        fade_out: 0,
    }
}

/// A note as the voice sings it: in seconds of the song, with its lyric.
#[derive(Clone, Debug, PartialEq)]
struct Sung {
    start: f64,
    end: f64,
    key: u8,
    lyric: String,
}

/// The notes a voice track sings, one at a time (a note starting before
/// the one before ends cuts it short), in phrases.
fn phrases(project: &Project, track: &Track) -> Vec<Vec<Sung>> {
    let mut sung: Vec<Sung> = track
        .clips
        .iter()
        .filter_map(|clip| Some((clip, project.pattern(clip.pattern()?)?)))
        .flat_map(|(clip, pattern)| {
            pattern.played_in(clip).into_iter().map(move |note| {
                let lyric = pattern.lyric(note.note).unwrap_or(DEFAULT_LYRIC);
                (note, lyric.to_owned())
            })
        })
        .filter(|(_, lyric)| !is_rest(lyric))
        .map(|(note, lyric)| Sung {
            start: project.tempo.seconds_at(note.start),
            end: project.tempo.seconds_at(note.end),
            key: note.key,
            lyric,
        })
        .collect();
    sung.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.key.cmp(&b.key)));
    for i in 1..sung.len() {
        if sung[i].start < sung[i - 1].end {
            sung[i - 1].end = sung[i].start;
        }
    }
    sung.retain(|n| n.end > n.start);
    let mut phrases: Vec<Vec<Sung>> = Vec::new();
    for note in sung {
        match phrases.last_mut() {
            Some(phrase)
                if phrase
                    .last()
                    .is_some_and(|last| note.start - last.end < PHRASE_GAP) =>
            {
                phrase.push(note);
            }
            _ => phrases.push(vec![note]),
        }
    }
    phrases
}

/// The recordings a phrase's notes sing: each lyric looked up after the
/// one before (VCV, then CV); a lyric the voicebank lacks sings "a", and a
/// note neither can be found for is left out.
fn sung_notes(bank: &VoiceBank, phrase: &[Sung]) -> Vec<JobNote> {
    phrase
        .iter()
        .enumerate()
        .filter_map(|(i, note)| {
            let previous = i.checked_sub(1).map(|j| phrase[j].lyric.as_str());
            let entry = bank
                .find(&alias_candidates(&note.lyric, previous), note.key)
                .or_else(|| bank.find(&alias_candidates(DEFAULT_LYRIC, previous), note.key))?;
            Some(JobNote {
                entry: entry.clone(),
                start: note.start,
                length: note.end - note.start,
                key: note.key,
            })
        })
        .collect()
}

/// Everything that shapes a phrase's sound, as one number.
fn phrase_key(voicebank: &str, rate: u32, notes: &[JobNote]) -> u64 {
    let mut hasher = DefaultHasher::new();
    voicebank.hash(&mut hasher);
    rate.hash(&mut hasher);
    for note in notes {
        note.entry.alias.hash(&mut hasher);
        note.entry.file.hash(&mut hasher);
        for value in [
            note.entry.offset,
            note.entry.consonant,
            note.entry.cutoff,
            note.entry.preutterance,
            note.entry.overlap,
            note.start,
            note.length,
        ] {
            value.to_bits().hash(&mut hasher);
        }
        note.key.hash(&mut hasher);
    }
    hasher.finish()
}

/// Renders a phrase (on the worker thread, or at once offline).
fn render(job: &Job) -> Phrase {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let notes: Vec<SungNote> = job
            .notes
            .iter()
            .filter_map(|note| {
                let sample = job.bank.sample(&note.entry.file).ok()?;
                Some(SungNote::new(
                    sample,
                    &note.entry,
                    note.start,
                    note.length,
                    f64::from(note.key),
                ))
            })
            .collect();
        if notes.is_empty() {
            return Phrase::Silent;
        }
        let phrase = sing_phrase(&notes, job.rate);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "song positions in frames are far below 2^63"
        )]
        let start = (phrase.start * f64::from(job.rate)).round() as i64;
        Phrase::Ready {
            start,
            sound: Arc::new(AudioBuffer::new(job.rate, vec![phrase.samples])),
        }
    }));
    result.unwrap_or(Phrase::Silent)
}

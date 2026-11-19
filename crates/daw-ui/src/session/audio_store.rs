// SPDX-License-Identifier: GPL-3.0-or-later

//! Where a song's sound lives while it is open (decision 0017): a folder of
//! audio files, and their decoded sound and loudness summaries in memory.
//!
//! A worker thread reads and decodes files, converts them to the engine's
//! rate and summarises them, one at a time, so the window never waits.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;

use daw_dsp::audio::{AudioBuffer, Peaks};
use daw_formats::audio::{AudioFormat, decode, read_file};

/// Where a newly imported file goes in the song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportPlace {
    /// The audio track it goes on (`None`: a new audio track).
    pub track: Option<daw_model::TrackId>,
    /// Where in the song it starts.
    pub at: daw_model::Ticks,
    /// The custom kit it becomes a pad of, in place of a clip (decision
    /// 0025).
    pub pad_of: Option<daw_model::TrackId>,
}

/// Work for the worker.
enum Job {
    /// Decode a file of the folder.
    Load { name: String },
    /// Copy a file from outside into the folder, then decode it.
    Import {
        source: PathBuf,
        name_base: String,
        place: ImportPlace,
    },
}

/// What the worker did.
pub(crate) enum Done {
    Loaded {
        name: String,
        result: Result<Ready, String>,
    },
    Imported {
        source: PathBuf,
        place: ImportPlace,
        result: Result<Imported, String>,
    },
}

/// Sound ready to play and draw.
pub(crate) struct Ready {
    pub sound: Arc<AudioBuffer>,
    pub peaks: Arc<Peaks>,
}

/// A file brought in from outside.
pub(crate) struct Imported {
    /// The name it is stored under in the folder.
    pub name: String,
    /// The name shown to people: the file's own name, without its ending.
    pub title: String,
    pub frames: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub ready: Ready,
}

/// What is known about one file of the folder.
#[derive(Clone)]
pub(crate) enum Entry {
    Loading,
    Ready(Arc<Peaks>),
    Failed(String),
}

/// The audio folder of this running copy of the program.
pub(crate) struct AudioStore {
    folder: PathBuf,
    /// Whether the folder is a temporary one this store made (and removes).
    temporary: bool,
    entries: HashMap<String, Entry>,
    jobs: Option<mpsc::Sender<Job>>,
    done: mpsc::Receiver<Done>,
    worker: Option<JoinHandle<()>>,
    /// Makes new file names unique to this run of the program.
    stamp: u64,
    counter: u64,
    imports_pending: usize,
}

impl AudioStore {
    /// A store in a temporary folder of its own, for sound at `engine_rate`.
    pub fn new(engine_rate: u32) -> Self {
        let stamp = RandomState::new().hash_one(std::process::id());
        let folder = std::env::temp_dir().join(format!(
            "{}-audio-{}-{:08x}",
            daw_ident::EXECUTABLE_NAME,
            std::process::id(),
            stamp & 0xFFFF_FFFF
        ));
        let (jobs, receive_jobs) = mpsc::channel::<Job>();
        let (send_done, done) = mpsc::channel();
        let worker_folder = folder.clone();
        let worker = std::thread::Builder::new()
            .name("audio-files".to_owned())
            .spawn(move || work(&receive_jobs, &send_done, &worker_folder, engine_rate))
            .ok();
        Self {
            folder,
            temporary: true,
            entries: HashMap::new(),
            jobs: Some(jobs),
            done,
            worker,
            stamp,
            counter: 0,
            imports_pending: 0,
        }
    }

    /// Keeps audio files in `folder` from now on (the program's recovery
    /// folder, so a crash keeps them). Call before any sound is stored.
    pub fn set_folder(&mut self, folder: PathBuf, engine_rate: u32) {
        if self.temporary {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
        // A new worker for the new folder.
        let (jobs, receive_jobs) = mpsc::channel::<Job>();
        let (send_done, done) = mpsc::channel();
        let worker_folder = folder.clone();
        let worker = std::thread::Builder::new()
            .name("audio-files".to_owned())
            .spawn(move || work(&receive_jobs, &send_done, &worker_folder, engine_rate))
            .ok();
        self.stop_worker();
        self.jobs = Some(jobs);
        self.done = done;
        self.worker = worker;
        self.folder = folder;
        self.temporary = false;
        self.entries.clear();
    }

    fn stop_worker(&mut self) {
        self.jobs = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    /// The folder (made if it does not exist yet).
    pub fn folder(&self) -> &Path {
        let _ = std::fs::create_dir_all(&self.folder);
        &self.folder
    }

    /// A name no file of the folder has, ending in `extension`.
    pub fn fresh_name(&mut self, extension: &str) -> String {
        self.counter += 1;
        let wanted = format!(
            "{:08x}-{}.{extension}",
            self.stamp & 0xFFFF_FFFF,
            self.counter
        );
        daw_formats::free_name(self.folder(), &wanted)
    }

    /// Starts decoding the files of `names` not known yet.
    pub fn load_missing<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        for name in names {
            if self.entries.contains_key(name) {
                continue;
            }
            self.entries.insert(name.to_owned(), Entry::Loading);
            if let Some(jobs) = &self.jobs {
                let _ = jobs.send(Job::Load {
                    name: name.to_owned(),
                });
            }
        }
    }

    /// Starts bringing a file from outside into the song.
    pub fn import(&mut self, source: PathBuf, place: ImportPlace) {
        self.counter += 1;
        let name_base = format!("{:08x}-{}", self.stamp & 0xFFFF_FFFF, self.counter);
        self.imports_pending += 1;
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(Job::Import {
                source,
                name_base,
                place,
            });
        }
    }

    /// Whether files are still being imported or read.
    pub fn is_busy(&self) -> bool {
        self.imports_pending > 0 || self.entries.values().any(|e| matches!(e, Entry::Loading))
    }

    /// Records sound that is ready already (a take just recorded).
    pub fn insert_ready(&mut self, name: &str, peaks: Arc<Peaks>) {
        self.entries.insert(name.to_owned(), Entry::Ready(peaks));
    }

    /// What the worker finished since the last call. Loaded files become
    /// known here; the caller gives their sound to the engine.
    pub fn poll(&mut self) -> Vec<Done> {
        let mut finished = Vec::new();
        while let Ok(done) = self.done.try_recv() {
            match &done {
                Done::Loaded { name, result } => {
                    let entry = match result {
                        Ok(ready) => Entry::Ready(Arc::clone(&ready.peaks)),
                        Err(reason) => Entry::Failed(reason.clone()),
                    };
                    self.entries.insert(name.clone(), entry);
                }
                Done::Imported { result, .. } => {
                    self.imports_pending = self.imports_pending.saturating_sub(1);
                    if let Ok(imported) = result {
                        self.entries.insert(
                            imported.name.clone(),
                            Entry::Ready(Arc::clone(&imported.ready.peaks)),
                        );
                    }
                }
            }
            finished.push(done);
        }
        finished
    }

    /// What is known about a file.
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }

    /// Forgets the files `keep` does not name (another song was opened).
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.entries.retain(|name, _| keep(name));
    }
}

impl Drop for AudioStore {
    fn drop(&mut self) {
        self.stop_worker();
        if self.temporary {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }
}

fn work(jobs: &mpsc::Receiver<Job>, done: &mpsc::Sender<Done>, folder: &Path, engine_rate: u32) {
    while let Ok(job) = jobs.recv() {
        let finished = match job {
            Job::Load { name } => Done::Loaded {
                result: load(&folder.join(&name), engine_rate),
                name,
            },
            Job::Import {
                source,
                name_base,
                place,
            } => Done::Imported {
                result: import(&source, folder, &name_base, engine_rate),
                source,
                place,
            },
        };
        if done.send(finished).is_err() {
            break;
        }
    }
}

/// Reads, decodes, converts and summarises a file.
fn load(path: &Path, engine_rate: u32) -> Result<Ready, String> {
    let bytes = read_file(path).map_err(|e| e.to_string())?;
    let decoded = decode(&bytes).map_err(|e| e.to_string())?;
    Ok(prepare(decoded.sample_rate, decoded.channels, engine_rate))
}

/// Sound ready for the engine: at its rate, with its summary.
fn prepare(sample_rate: u32, channels: Vec<Vec<f32>>, engine_rate: u32) -> Ready {
    let sound = AudioBuffer::new(sample_rate, channels).resampled(engine_rate);
    let peaks = Peaks::of(&sound);
    Ready {
        sound: Arc::new(sound),
        peaks: Arc::new(peaks),
    }
}

/// Brings a file from outside into the folder, as it is, and prepares it.
fn import(
    source: &Path,
    folder: &Path,
    name_base: &str,
    engine_rate: u32,
) -> Result<Imported, String> {
    let bytes = read_file(source).map_err(|e| e.to_string())?;
    let format = AudioFormat::sniff(&bytes)
        .ok_or_else(|| daw_formats::audio::AudioError::Unknown.to_string())?;
    let decoded = decode(&bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let name = daw_formats::free_name(folder, &format!("{name_base}.{}", format.extension()));
    daw_formats::write_atomically(&folder.join(&name), &bytes).map_err(|e| e.to_string())?;
    let title = source
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let frames = decoded.frames() as u64;
    let sample_rate = decoded.sample_rate;
    let channels = u16::try_from(decoded.channels.len()).unwrap_or(2);
    Ok(Imported {
        name,
        title,
        frames,
        sample_rate,
        channels,
        ready: prepare(sample_rate, decoded.channels, engine_rate),
    })
}

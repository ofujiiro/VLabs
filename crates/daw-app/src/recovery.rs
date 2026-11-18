// SPDX-License-Identifier: GPL-3.0-or-later

//! Autosave and crash recovery.
//!
//! Each running copy of the program owns a folder under the recovery
//! folder, with a lock file it keeps locked for as long as it runs. While
//! the song has unsaved changes, a copy of it is written there every few
//! seconds by a background thread (the interface only hands over a cheap
//! snapshot). A clean exit deletes the folder. At start-up, a folder whose
//! lock can be taken belonged to a copy of the program that did not exit
//! cleanly: if it holds a song, the person is offered to continue with it.
//!
//! Locks are released by the system when a process ends, however it ends,
//! so this works after crashes, power cuts and forced quits, and with
//! several copies of the program running at once.
//!
//! The song's audio files live in the folder's `audio` subfolder while the
//! program runs (decision 0017): recovery copies refer to them there, so
//! they are written once, and a recovered song comes back with its sound.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use daw_model::{AudioFile, AudioId, Project, Ticks};
use daw_ui::TakeNote;
use serde::{Deserialize, Serialize};

/// The least time between two autosaves.
const INTERVAL: Duration = Duration::from_secs(5);

/// The longest a safety copy is waited for (decision 0021).
const SAFETY_WAIT: Duration = Duration::from_secs(2);

const LOCK_FILE: &str = "lock";
const INFO_FILE: &str = "info.json";
const TAKE_FILE: &str = "take.json";
const AUDIO_FOLDER: &str = "audio";

fn song_file() -> String {
    format!("song.{}", daw_ident::PROJECT_FILE_EXTENSION)
}

/// What is written next to the recovery copy.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Info {
    /// The file the song was opened from or saved to, if any.
    path: Option<PathBuf>,
}

/// Work left behind by a copy of the program that did not exit cleanly.
pub struct Orphan {
    /// The song as it was last autosaved.
    pub project: Project,
    /// The file it belonged to, if it had one.
    pub path: Option<PathBuf>,
    /// When the copy was written.
    pub saved_at: SystemTime,
    /// The take of sound being recorded when it stopped, if one was.
    take: Option<TakeNote>,
    folder: PathBuf,
    /// Held so no other copy of the program claims the same work.
    _lock: File,
}

/// What the person is told about work found after a crash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found {
    /// How long before now the copy was written, in seconds.
    pub age_seconds: u64,
    /// Whether the song's own file was saved after the copy.
    pub newer_file: bool,
    /// Whether a take being recorded was brought back.
    pub take: bool,
}

enum Job {
    Write(Box<Project>, Option<PathBuf>),
    /// A copy at once; the sender is told when it is on disk.
    WriteNow(Box<Project>, Option<PathBuf>, mpsc::Sender<()>),
    Take(Option<TakeNote>),
    Clear,
    /// The program is ending: the writer stops (the session may still hold
    /// a sender, so the channel alone would never close).
    Stop,
}

/// Writes a recovery copy at once, for the session (decision 0021).
pub struct SafetyWriter {
    jobs: mpsc::Sender<Job>,
    has_copy: Arc<AtomicBool>,
}

impl daw_ui::SafetyCopy for SafetyWriter {
    fn write_now(&self, project: &Project, path: Option<&Path>) {
        let (done, wait) = mpsc::channel();
        let job = Job::WriteNow(Box::new(project.clone()), path.map(Path::to_path_buf), done);
        if self.jobs.send(job).is_ok() {
            self.has_copy.store(true, Ordering::Relaxed);
            let _ = wait.recv_timeout(SAFETY_WAIT);
        }
    }
}

/// The autosaver of this copy of the program.
pub struct Autosaver {
    folder: PathBuf,
    lock: Option<File>,
    jobs: Option<mpsc::Sender<Job>>,
    writer: Option<JoinHandle<()>>,
    written_version: Option<u64>,
    last_write: Option<Instant>,
    has_copy: Arc<AtomicBool>,
    /// The take note last sent to the writer.
    take: Option<TakeNote>,
    orphan: Option<Orphan>,
}

impl Autosaver {
    /// Claims a folder for this copy of the program, and finds the newest
    /// work left behind by a crash, if any.
    pub fn start(root: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(root)?;
        let orphan = find_orphan(root);
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let folder = root.join(format!("{}-{stamp}", std::process::id()));
        fs::create_dir_all(&folder)?;
        let lock = File::create(folder.join(LOCK_FILE))?;
        lock.lock()?;
        let (jobs, receiver) = mpsc::channel::<Job>();
        let target = folder.clone();
        let writer = std::thread::Builder::new()
            .name("autosave".to_owned())
            .spawn(move || write_jobs(&target, &receiver))
            .ok();
        Ok(Self {
            folder,
            lock: Some(lock),
            jobs: Some(jobs),
            writer,
            written_version: None,
            last_write: None,
            has_copy: Arc::new(AtomicBool::new(false)),
            take: None,
            orphan,
        })
    }

    /// A way for the session to write a copy at once (before a plugin
    /// loads, say).
    pub fn safety_writer(&self) -> Option<SafetyWriter> {
        Some(SafetyWriter {
            jobs: self.jobs.clone()?,
            has_copy: Arc::clone(&self.has_copy),
        })
    }

    /// Notes the take of sound being recorded (or that none is), so a
    /// crash in the middle of it does not lose it.
    pub fn note_take(&mut self, take: Option<TakeNote>) {
        if take == self.take {
            return;
        }
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(Job::Take(take.clone()));
        }
        self.take = take;
    }

    /// Where this copy of the program keeps the song's audio files.
    pub fn audio_folder(&self) -> PathBuf {
        self.folder.join(AUDIO_FOLDER)
    }

    /// The work found at start-up, to offer to the person, with its audio
    /// files copied into this copy's audio folder, a take that was being
    /// recorded put back as a clip, and what to tell the person about it.
    pub fn take_orphan(&mut self) -> Option<(Project, Option<PathBuf>, Found)> {
        let orphan = self.orphan.as_ref()?;
        let mut project = orphan.project.clone();
        let from = orphan.folder.join(AUDIO_FOLDER);
        let into = self.audio_folder();
        let _ = daw_formats::copy_audio(&mut project, &from, &into);
        let take = orphan
            .take
            .as_ref()
            .is_some_and(|note| recover_take(&mut project, note, &from, &into));
        let age_seconds = SystemTime::now()
            .duration_since(orphan.saved_at)
            .map_or(0, |d| d.as_secs());
        let newer_file = orphan
            .path
            .as_ref()
            .and_then(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
            .is_some_and(|modified| modified > orphan.saved_at);
        Some((
            project,
            orphan.path.clone(),
            Found {
                age_seconds,
                newer_file,
                take,
            },
        ))
    }

    /// Deletes the work found at start-up (after the person decided: if
    /// they continued with it, it is autosaved here from now on).
    pub fn forget_orphan(&mut self) {
        if let Some(orphan) = self.orphan.take() {
            let folder = orphan.folder.clone();
            drop(orphan); // Unlock first: Windows cannot delete open files.
            let _ = fs::remove_dir_all(folder);
        }
    }

    /// Called every frame: writes a copy when the song has unsaved changes
    /// that were not copied yet, and removes the copy once it is saved.
    /// `version` changes whenever the song does; `song` gives the song and
    /// where it is saved, and is asked only when a copy is due.
    pub fn tick(
        &mut self,
        version: u64,
        dirty: bool,
        song: impl FnOnce() -> (Project, Option<PathBuf>),
    ) {
        let Some(jobs) = &self.jobs else {
            return;
        };
        // While a take records, the song is kept even if it was saved: a
        // crash then brings back the song with the take.
        let dirty = dirty || self.take.is_some();
        if !dirty {
            if self.has_copy.load(Ordering::Relaxed) {
                let _ = jobs.send(Job::Clear);
                self.has_copy.store(false, Ordering::Relaxed);
                self.written_version = None;
            }
            return;
        }
        let due = self.last_write.is_none_or(|at| at.elapsed() >= INTERVAL);
        if self.written_version != Some(version) && due {
            let (project, path) = song();
            let _ = jobs.send(Job::Write(Box::new(project), path));
            self.written_version = Some(version);
            self.last_write = Some(Instant::now());
            self.has_copy.store(true, Ordering::Relaxed);
        }
    }

    /// Stops the writer thread, once it has written what it was given.
    fn stop_writer(&mut self) {
        if let Some(jobs) = self.jobs.take() {
            let _ = jobs.send(Job::Stop);
        }
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }

    /// A clean exit: stops the writer and deletes this copy's folder.
    pub fn finish(&mut self) {
        self.stop_writer();
        self.lock = None;
        let _ = fs::remove_dir_all(&self.folder);
    }
}

impl Drop for Autosaver {
    fn drop(&mut self) {
        // Dropping without `finish` (a panic unwinding) keeps the folder,
        // so the work is offered next time.
        self.stop_writer();
    }
}

fn write_song(folder: &Path, project: &Project, path: Option<PathBuf>) {
    // The song's file name first, then the song: the song is what marks
    // work to recover, so when it is there after a crash, its name is too.
    let info = serde_json::to_vec_pretty(&Info { path }).unwrap_or_default();
    if daw_formats::write_atomically(&folder.join(INFO_FILE), &info).is_ok() {
        let _ = daw_formats::save_project(project, &folder.join(song_file()), None);
    }
}

fn write_jobs(folder: &Path, jobs: &mpsc::Receiver<Job>) {
    while let Ok(job) = jobs.recv() {
        // Only the newest snapshot of the song matters: skip ones already
        // replaced (everyone waiting is told when the newest is written).
        let mut pending = vec![job];
        while let Ok(newer) = jobs.try_recv() {
            pending.push(newer);
        }
        let mut song: Option<(Box<Project>, Option<PathBuf>)> = None;
        let mut clear = false;
        let mut waiting = Vec::new();
        let mut stop = false;
        for job in pending {
            match job {
                Job::Stop => stop = true,
                Job::Write(project, path) => {
                    song = Some((project, path));
                    clear = false;
                }
                Job::WriteNow(project, path, done) => {
                    song = Some((project, path));
                    clear = false;
                    waiting.push(done);
                }
                Job::Clear => {
                    song = None;
                    clear = true;
                }
                Job::Take(Some(note)) => {
                    let bytes = serde_json::to_vec_pretty(&note).unwrap_or_default();
                    let _ = daw_formats::write_atomically(&folder.join(TAKE_FILE), &bytes);
                }
                Job::Take(None) => {
                    let _ = fs::remove_file(folder.join(TAKE_FILE));
                }
            }
        }
        if let Some((project, path)) = song {
            write_song(folder, &project, path);
        } else if clear {
            let _ = fs::remove_file(folder.join(song_file()));
            let _ = fs::remove_file(folder.join(INFO_FILE));
        }
        for done in waiting {
            let _ = done.send(());
        }
        if stop {
            break;
        }
    }
}

/// Puts a take that was being recorded when the program stopped back into
/// the song, as a clip where recording started (decision 0021). Its file,
/// left unfinished, is mended and copied into `into`. Returns whether a
/// take came back.
fn recover_take(project: &mut Project, note: &TakeNote, from: &Path, into: &Path) -> bool {
    if project.audio.iter().any(|f| f.file == note.file) {
        return false; // It ended and was saved with the song.
    }
    let finished = from.join(&note.file);
    let source = if finished.is_file() {
        Some(finished)
    } else {
        fs::read_dir(from).ok().and_then(|entries| {
            entries.flatten().map(|e| e.path()).find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| daw_formats::wav::is_partial_of(n, &note.file))
            })
        })
    };
    let Some(source) = source else {
        return false;
    };
    let Ok(repaired) = daw_formats::wav::repair(&source) else {
        return false;
    };
    if repaired.frames == 0 {
        return false;
    }
    let name = daw_formats::free_name(into, &note.file);
    if fs::create_dir_all(into).is_err() || fs::copy(&source, into.join(&name)).is_err() {
        return false;
    }
    let file = AudioFile {
        id: AudioId(0),
        name: note.name.clone(),
        file: name,
        frames: repaired.frames,
        sample_rate: repaired.sample_rate,
        channels: repaired.channels,
    };
    // The take starts late by the delay out and back in, as when recording
    // ends normally.
    let offset = file
        .frames_in(f64::from(note.latency.max(0.0)))
        .min(file.frames.saturating_sub(1));
    let seconds = file.seconds_in(file.frames - offset);
    let id = project.add_audio_file(file);
    let start = project.tempo.seconds_at(note.at);
    #[allow(clippy::cast_possible_truncation, reason = "a tick position")]
    let end = Ticks(project.tempo.ticks_at(start + seconds).round() as i64);
    let track = if project
        .track(note.track)
        .is_some_and(|t| t.instrument.is_audio())
    {
        note.track
    } else {
        project.add_audio_track(&note.name)
    };
    project
        .add_audio_clip(track, note.at, (end - note.at).max(Ticks(1)), id, offset)
        .is_some()
}

/// The newest folder left behind with a song in it. Empty leftovers are
/// tidied away.
fn find_orphan(root: &Path) -> Option<Orphan> {
    let mut found: Vec<(SystemTime, PathBuf, File)> = Vec::new();
    for entry in fs::read_dir(root).ok()?.flatten() {
        let folder = entry.path();
        if !folder.is_dir() {
            continue;
        }
        let Ok(lock) = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(folder.join(LOCK_FILE))
        else {
            continue;
        };
        if lock.try_lock().is_err() {
            continue; // Its program is still running.
        }
        let song = folder.join(song_file());
        if let Ok(modified) = fs::metadata(&song).and_then(|m| m.modified()) {
            found.push((modified, folder, lock));
        } else {
            drop(lock);
            let _ = fs::remove_dir_all(&folder);
        }
    }
    found.sort_by_key(|(modified, _, _)| *modified);
    while let Some((saved_at, folder, lock)) = found.pop() {
        if let Ok(project) = daw_formats::load_project(&folder.join(song_file()), None) {
            let info: Info = fs::read(folder.join(INFO_FILE))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let take = fs::read(folder.join(TAKE_FILE))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            return Some(Orphan {
                project,
                path: info.path,
                saved_at,
                take,
                folder,
                _lock: lock,
            });
        }
        // Unreadable: nothing can be recovered from it.
        drop(lock);
        let _ = fs::remove_dir_all(&folder);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "recovery-test-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn wait_for(path: &Path, present: bool) {
        for _ in 0..200 {
            if path.exists() == present {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!(
            "{} never {}",
            path.display(),
            if present { "appeared" } else { "went away" }
        );
    }

    #[test]
    fn unsaved_work_survives_a_crash_and_is_offered_once() {
        let root = temp_root("crash");
        let mut project = Project::starter("P", "T");
        project.title = "Recovered".to_owned();

        // A copy of the program autosaves, then "crashes": its lock goes
        // away (the system releases it) but its folder stays.
        let mut first = Autosaver::start(&root).unwrap();
        assert!(first.take_orphan().is_none());
        // Its song has a recording, kept in its audio folder.
        fs::create_dir_all(first.audio_folder()).unwrap();
        fs::write(first.audio_folder().join("take.wav"), b"sound").unwrap();
        let voice = project.add_audio_track("Voice");
        let take = project.add_audio_file(daw_model::AudioFile {
            id: daw_model::AudioId(0),
            name: "Recording 1".to_owned(),
            file: "take.wav".to_owned(),
            frames: 5,
            sample_rate: 48_000,
            channels: 1,
        });
        project.add_audio_clip(voice, daw_model::Ticks::ZERO, daw_model::Ticks(10), take, 0);
        first.tick(1, true, || {
            (project.clone(), Some(PathBuf::from("/music/song")))
        });
        let copy = first.folder.join(song_file());
        wait_for(&copy, true);
        let lock = first.lock.take();
        drop(lock);
        std::mem::forget(first); // No clean exit.

        let mut second = Autosaver::start(&root).unwrap();
        let (recovered, path, found) = second.take_orphan().expect("the work is offered");
        assert!(found.age_seconds < 60 && !found.newer_file && !found.take);
        assert_eq!(recovered.title, "Recovered");
        assert_eq!(path.as_deref(), Some(Path::new("/music/song")));
        let file = &recovered.audio[0].file;
        assert_eq!(
            fs::read(second.audio_folder().join(file)).unwrap(),
            b"sound",
            "the recording comes back with it"
        );
        second.forget_orphan();
        second.finish();

        let mut third = Autosaver::start(&root).unwrap();
        assert!(third.take_orphan().is_none(), "offered only once");
        third.finish();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_running_copy_is_left_alone_and_saved_work_is_cleared() {
        let root = temp_root("running");
        let project = Project::starter("P", "T");
        let mut first = Autosaver::start(&root).unwrap();
        first.tick(1, true, || (project.clone(), None));
        let copy = first.folder.join(song_file());
        wait_for(&copy, true);

        let mut second = Autosaver::start(&root).unwrap();
        assert!(
            second.take_orphan().is_none(),
            "the first copy is still running"
        );
        second.finish();

        first.tick(1, false, || (project.clone(), None));
        wait_for(&copy, false);
        let folder = first.folder.clone();
        first.finish();
        assert!(!folder.exists(), "a clean exit tidies up");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_take_being_recorded_comes_back_after_a_crash() {
        let root = temp_root("take");
        let mut project = Project::starter("P", "T");
        let voice = project.add_audio_track("Voice");
        let mut first = Autosaver::start(&root).unwrap();
        let audio = first.audio_folder();
        fs::create_dir_all(&audio).unwrap();
        // Recording has started: the take is written as it comes.
        let mut writer = daw_formats::wav::WavWriter::create_with_channels(
            &audio.join("take-1.wav"),
            daw_formats::wav::WavFormat::Pcm24,
            48_000,
            1,
        )
        .unwrap();
        writer.write_interleaved(&vec![0.25_f32; 48_000]).unwrap();
        let note = TakeNote {
            track: voice,
            at: Ticks::WHOLE,
            file: "take-1.wav".to_owned(),
            name: "Recording 1".to_owned(),
            latency: 0.0,
        };
        first.note_take(Some(note));
        // The song was saved before recording: it is kept anyway.
        first.tick(1, false, || (project.clone(), None));
        let copy = first.folder.join(song_file());
        wait_for(&copy, true);
        wait_for(&first.folder.join(TAKE_FILE), true);
        // Crash: the take's file is still being written.
        writer.flush_for_test();
        std::mem::forget(writer);
        let lock = first.lock.take();
        drop(lock);
        std::mem::forget(first);

        let mut second = Autosaver::start(&root).unwrap();
        let (recovered, _, found) = second.take_orphan().expect("the work is offered");
        assert!(found.take, "the take came back");
        let clip = recovered
            .track(voice)
            .unwrap()
            .clips
            .first()
            .copied()
            .expect("as a clip on its track");
        assert_eq!(clip.start, Ticks::WHOLE);
        let file = recovered.audio_file(clip.audio().unwrap()).unwrap();
        assert!(
            file.frames >= 40_000,
            "with what was written: {}",
            file.frames
        );
        assert!(second.audio_folder().join(&file.file).is_file());
        second.forget_orphan();
        second.finish();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_safety_copy_is_on_disk_when_it_returns() {
        use daw_ui::SafetyCopy;
        let root = temp_root("safety");
        let mut saver = Autosaver::start(&root).unwrap();
        let writer = saver.safety_writer().unwrap();
        writer.write_now(&Project::starter("P", "T"), None);
        assert!(saver.folder.join(song_file()).is_file(), "written at once");
        // Saving the song afterwards clears it, as for any copy.
        saver.tick(1, false, || (Project::starter("P", "T"), None));
        wait_for(&saver.folder.join(song_file()), false);
        saver.finish();
        let _ = fs::remove_dir_all(root);
    }
}

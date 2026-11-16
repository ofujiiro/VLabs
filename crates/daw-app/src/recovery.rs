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

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use daw_model::Project;
use serde::{Deserialize, Serialize};

/// The least time between two autosaves.
const INTERVAL: Duration = Duration::from_secs(5);

const LOCK_FILE: &str = "lock";
const INFO_FILE: &str = "info.json";

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
    folder: PathBuf,
    /// Held so no other copy of the program claims the same work.
    _lock: File,
}

enum Job {
    Write(Box<Project>, Option<PathBuf>),
    Clear,
}

/// The autosaver of this copy of the program.
pub struct Autosaver {
    folder: PathBuf,
    lock: Option<File>,
    jobs: Option<mpsc::Sender<Job>>,
    writer: Option<JoinHandle<()>>,
    written_revision: Option<u64>,
    last_write: Option<Instant>,
    has_copy: bool,
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
            written_revision: None,
            last_write: None,
            has_copy: false,
            orphan,
        })
    }

    /// The work found at start-up, to offer to the person.
    pub fn take_orphan(&mut self) -> Option<(Project, Option<PathBuf>)> {
        let orphan = self.orphan.as_ref()?;
        Some((orphan.project.clone(), orphan.path.clone()))
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
    pub fn tick(&mut self, project: &Project, path: Option<&Path>, revision: u64, dirty: bool) {
        let Some(jobs) = &self.jobs else {
            return;
        };
        if !dirty {
            if self.has_copy {
                let _ = jobs.send(Job::Clear);
                self.has_copy = false;
                self.written_revision = None;
            }
            return;
        }
        let due = self.last_write.is_none_or(|at| at.elapsed() >= INTERVAL);
        if self.written_revision != Some(revision) && due {
            let _ = jobs.send(Job::Write(
                Box::new(project.clone()),
                path.map(Path::to_path_buf),
            ));
            self.written_revision = Some(revision);
            self.last_write = Some(Instant::now());
            self.has_copy = true;
        }
    }

    /// A clean exit: stops the writer and deletes this copy's folder.
    pub fn finish(&mut self) {
        self.jobs = None;
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        self.lock = None;
        let _ = fs::remove_dir_all(&self.folder);
    }
}

impl Drop for Autosaver {
    fn drop(&mut self) {
        // Dropping without `finish` (a panic unwinding) keeps the folder,
        // so the work is offered next time.
        self.jobs = None;
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

fn write_jobs(folder: &Path, jobs: &mpsc::Receiver<Job>) {
    while let Ok(job) = jobs.recv() {
        // Only the newest snapshot matters: skip ones already replaced.
        let mut job = job;
        while let Ok(newer) = jobs.try_recv() {
            job = newer;
        }
        match job {
            Job::Write(project, path) => {
                let info = serde_json::to_vec_pretty(&Info { path }).unwrap_or_default();
                if daw_formats::save_project(&project, &folder.join(song_file())).is_ok() {
                    let _ = daw_formats::write_atomically(&folder.join(INFO_FILE), &info);
                }
            }
            Job::Clear => {
                let _ = fs::remove_file(folder.join(song_file()));
                let _ = fs::remove_file(folder.join(INFO_FILE));
            }
        }
    }
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
    while let Some((_, folder, lock)) = found.pop() {
        if let Ok(project) = daw_formats::load_project(&folder.join(song_file())) {
            let info: Info = fs::read(folder.join(INFO_FILE))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            return Some(Orphan {
                project,
                path: info.path,
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
        first.tick(&project, Some(Path::new("/music/song")), 1, true);
        let copy = first.folder.join(song_file());
        wait_for(&copy, true);
        let lock = first.lock.take();
        drop(lock);
        std::mem::forget(first); // No clean exit.

        let mut second = Autosaver::start(&root).unwrap();
        let (recovered, path) = second.take_orphan().expect("the work is offered");
        assert_eq!(recovered.title, "Recovered");
        assert_eq!(path.as_deref(), Some(Path::new("/music/song")));
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
        first.tick(&project, None, 1, true);
        let copy = first.folder.join(song_file());
        wait_for(&copy, true);

        let mut second = Autosaver::start(&root).unwrap();
        assert!(
            second.take_orphan().is_none(),
            "the first copy is still running"
        );
        second.finish();

        first.tick(&project, None, 1, false);
        wait_for(&copy, false);
        let folder = first.folder.clone();
        first.finish();
        assert!(!folder.exists(), "a clean exit tidies up");
        let _ = fs::remove_dir_all(root);
    }
}

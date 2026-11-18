// SPDX-License-Identifier: GPL-3.0-or-later

//! Adding, removing and installing plugins, off the interface thread
//! (decision 0023). Copying a large plugin, unpacking a download or
//! waiting for an installer takes a while; the window carries on and is
//! told what happened.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

use daw_plugin_host::install::{self, Bridge, Skip};
use daw_ui::{BridgeState, InstallerRun, PluginJobDone};

/// Something to do with plugins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    /// Add plugins from files, folders, downloads or installers.
    Add(Vec<PathBuf>),
    /// Remove a plugin the program added.
    Remove(PathBuf),
    /// Look for Wine and yabridge again and set up the Windows plugins.
    SetUpWindowsPlugins,
}

/// What a finished job did, and how things stand after it.
pub struct Finished {
    /// What to tell the person.
    pub done: PluginJobDone,
    /// The plugins added here, now.
    pub added: Vec<PathBuf>,
    /// Wine and yabridge, now.
    pub bridge: Bridge,
}

/// The worker: one job at a time, in order, on a thread of its own.
pub struct PluginJobs {
    jobs: mpsc::Sender<Job>,
    finished: mpsc::Receiver<Finished>,
    waiting: usize,
}

impl PluginJobs {
    /// Starts the worker for the program's plugin folder `own`; `wake` is
    /// called when a job finishes (to draw the window again).
    pub fn start(own: PathBuf, wake: impl Fn() + Send + 'static) -> Self {
        let (jobs, incoming) = mpsc::channel::<Job>();
        let (report, finished) = mpsc::channel();
        // The thread ends when the sender goes (the program closes); an
        // installer still open is left to finish on its own.
        let _ = std::thread::Builder::new()
            .name("plugin-jobs".to_owned())
            .spawn(move || {
                for job in incoming {
                    let done = run(job, &own);
                    let finished = Finished {
                        done,
                        added: install::own_plugins(&own).into_iter().collect(),
                        bridge: install::find_bridge(),
                    };
                    if report.send(finished).is_err() {
                        break;
                    }
                    wake();
                }
            });
        Self {
            jobs,
            finished,
            waiting: 0,
        }
    }

    /// Queues a job.
    pub fn send(&mut self, job: Job) {
        if self.jobs.send(job).is_ok() {
            self.waiting += 1;
        }
    }

    /// Whether a job is under way or waiting.
    pub fn busy(&self) -> bool {
        self.waiting > 0
    }

    /// The jobs finished since the last call.
    pub fn finished(&mut self) -> Vec<Finished> {
        let done: Vec<Finished> = self.finished.try_iter().collect();
        self.waiting = self.waiting.saturating_sub(done.len());
        done
    }
}

/// Wine and yabridge as the Plugins window shows them (Linux only).
pub fn bridge_state(bridge: &Bridge) -> Option<BridgeState> {
    cfg!(target_os = "linux").then(|| BridgeState {
        wine: bridge.wine.is_some(),
        yabridge: bridge.yabridgectl.is_some(),
    })
}

fn run(job: Job, own: &Path) -> PluginJobDone {
    match job {
        Job::Add(sources) => add(&sources, own),
        Job::Remove(plugin) => remove(&plugin, own),
        Job::SetUpWindowsPlugins => set_up(own),
    }
}

fn add(sources: &[PathBuf], own: &Path) -> PluginJobDone {
    let added = install::add(sources, own);
    let mut done = PluginJobDone {
        added: added.plugins,
        windows: added.windows,
        ..PluginJobDone::default()
    };
    let bridge = install::find_bridge();
    for (path, skip) in added.skipped {
        if skip == Skip::Installer {
            let run = run_installer(&path, &bridge);
            done.installers.push((path, run));
        } else {
            done.skipped.push((path, skip));
        }
    }
    let installed_through_wine = cfg!(target_os = "linux")
        && done
            .installers
            .iter()
            .any(|(_, run)| *run == InstallerRun::Finished);
    if (!done.windows.is_empty() || installed_through_wine) && bridge.is_ready() {
        done.bridge = Some(bridge_all(&bridge, own, false));
    }
    done
}

fn remove(plugin: &Path, own: &Path) -> PluginJobDone {
    let mut done = PluginJobDone::default();
    match install::remove(plugin, own) {
        Ok(()) => {
            done.removed.push(plugin.to_path_buf());
            let windows = plugin.starts_with(install::windows_folder(own));
            let bridge = install::find_bridge();
            if windows && bridge.is_ready() {
                // Only a failure is worth telling here.
                done.bridge = bridge_all(&bridge, own, true).err().map(Err);
            }
        }
        Err(error) => done.remove_failed = Some((plugin.to_path_buf(), error.to_string())),
    }
    done
}

fn set_up(own: &Path) -> PluginJobDone {
    let bridge = install::find_bridge();
    PluginJobDone {
        bridge: bridge.is_ready().then(|| bridge_all(&bridge, own, true)),
        ..PluginJobDone::default()
    }
}

/// Has yabridge set up the Windows plugins added here and those Windows
/// installers put inside Wine's setup.
fn bridge_all(bridge: &Bridge, own: &Path, prune: bool) -> Result<(), String> {
    let mut folders = vec![install::windows_folder(own)];
    folders.extend(install::wine_plugin_folders());
    install::bridge_windows_plugins(bridge, &folders, prune)
}

/// Runs a plugin's installer: on Windows it is opened as a double-click
/// would (it may ask for permission, and the person finishes it); on
/// Linux it runs through Wine, and the program waits for it.
fn run_installer(installer: &Path, bridge: &Bridge) -> InstallerRun {
    let is_msi = installer
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("msi"));
    if cfg!(windows) {
        // "start" opens it through the shell, which asks for permission
        // when the installer needs it.
        return match Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(installer)
            .spawn()
        {
            Ok(_) => InstallerRun::Opened,
            Err(error) => InstallerRun::Failed(error.to_string()),
        };
    }
    if !cfg!(target_os = "linux") {
        return InstallerRun::Failed("installers can only be run on Windows and Linux".to_owned());
    }
    let Some(wine) = &bridge.wine else {
        return InstallerRun::NeedsWine;
    };
    let mut command = Command::new(wine);
    if is_msi {
        command.args(["msiexec", "/i"]);
    }
    match command.arg(installer).status() {
        Ok(status) if status.success() => InstallerRun::Finished,
        Ok(status) => InstallerRun::Failed(status.code().map_or_else(
            || "it was stopped".to_owned(),
            |code| format!("it ended with code {code}"),
        )),
        Err(error) => InstallerRun::Failed(error.to_string()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn scratch(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("plugin-jobs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn wait(jobs: &mut PluginJobs) -> Finished {
        let start = Instant::now();
        loop {
            if let Some(finished) = jobs.finished().pop() {
                return finished;
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "the job never finished"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn plugins_are_added_and_removed_in_the_background() {
        let folder = scratch("add");
        let own = folder.join("data/plugins");
        let plugin = folder.join("Delay.clap");
        let magic: &[u8] = if cfg!(windows) {
            b"MZ\x90\x00"
        } else {
            b"\x7fELF"
        };
        std::fs::write(&plugin, magic).unwrap();
        let old = folder.join("Old.dll");
        std::fs::write(&old, b"MZ").unwrap();
        let mut jobs = PluginJobs::start(own.clone(), || {});
        jobs.send(Job::Add(vec![plugin, old.clone()]));
        assert!(jobs.busy());
        let finished = wait(&mut jobs);
        assert!(!jobs.busy());
        let placed = own.join("clap/Delay.clap");
        assert_eq!(finished.done.added, vec![placed.clone()]);
        assert_eq!(finished.done.skipped, vec![(old, Skip::Vst2)]);
        assert_eq!(finished.added, vec![placed.clone()]);

        jobs.send(Job::Remove(placed.clone()));
        let finished = wait(&mut jobs);
        assert_eq!(finished.done.removed, vec![placed.clone()]);
        assert_eq!(finished.added, Vec::<PathBuf>::new());
        assert!(!placed.exists());

        // A plugin installed elsewhere is not removed.
        let elsewhere = folder.join("Elsewhere.clap");
        std::fs::write(&elsewhere, magic).unwrap();
        jobs.send(Job::Remove(elsewhere.clone()));
        let finished = wait(&mut jobs);
        assert!(finished.done.remove_failed.is_some());
        assert!(elsewhere.exists());
        let _ = std::fs::remove_dir_all(folder);
    }
}

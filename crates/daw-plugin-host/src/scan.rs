// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding plugins (decision 0015).
//!
//! The standard folders are searched for plugin files; each file not seen
//! before is described in a separate run of the program (which loads it,
//! lists its plugins and prints them), one at a time with a time limit, so
//! a plugin that crashes or hangs only ends that run. What was found, and
//! which files failed, is cached by path, size and modification time.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{Format, PluginInfo};

/// The argument that makes the program describe one plugin file and exit.
pub const DESCRIBE_ARGUMENT: &str = "--describe-plugin";
/// What precedes the description in the program's output (plugins may print
/// their own messages too).
const MARKER: &str = "plugin-description: ";
/// How long describing one file may take.
const TIME_LIMIT: Duration = Duration::from_secs(20);
/// How deep folders are searched.
const MAX_DEPTH: usize = 8;

/// The folders plugins are installed in on this system, plus `CLAP_PATH`.
pub fn default_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(extra) = std::env::var_os("CLAP_PATH") {
        folders.extend(std::env::split_paths(&extra));
    }
    #[cfg(windows)]
    {
        let common = std::env::var_os("COMMONPROGRAMFILES").map(PathBuf::from);
        let local = std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("Programs").join("Common"));
        for base in [common, local].into_iter().flatten() {
            folders.push(base.join("CLAP"));
            folders.push(base.join("VST3"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            folders.push(home.join(".clap"));
            folders.push(home.join(".vst3"));
        }
        for base in ["/usr/lib", "/usr/local/lib"] {
            folders.push(Path::new(base).join("clap"));
            folders.push(Path::new(base).join("vst3"));
        }
    }
    folders
}

/// The plugin files in some folders: `.clap` files and `.vst3` bundles (a
/// bundle's insides are not searched).
pub fn plugin_files(folders: &[PathBuf]) -> Vec<(Format, PathBuf)> {
    let mut found = Vec::new();
    for folder in folders {
        walk(folder, 0, &mut found);
    }
    found.sort_by(|a, b| a.1.cmp(&b.1));
    found.dedup();
    found
}

fn walk(folder: &Path, depth: usize, found: &mut Vec<(Format, PathBuf)>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(format) = Format::of_path(&path) {
            found.push((format, path));
        } else if path.is_dir() {
            walk(&path, depth + 1, found);
        }
    }
}

/// Describes the plugins in one file, in this process. Used by the
/// separate run of the program; the program itself calls [`Scanner`].
pub fn describe(format: Format, path: &Path) -> Result<Vec<PluginInfo>, String> {
    match format {
        Format::Clap => crate::clap::describe(path),
        Format::Vst3 => crate::vst3::describe(path),
    }
}

/// What the separate run prints.
#[derive(Serialize, Deserialize)]
struct Description {
    result: Result<Vec<PluginInfo>, String>,
}

/// The separate run's work: describes one file and prints the result.
/// `arguments` are the ones after [`DESCRIBE_ARGUMENT`]: the format and the
/// path.
#[allow(clippy::print_stdout, reason = "the parent reads this from our output")]
pub fn describe_and_print(arguments: &[String]) {
    let result = match arguments {
        [name, path] => {
            let format = match name.as_str() {
                "clap" => Some(Format::Clap),
                "vst3" => Some(Format::Vst3),
                _ => None,
            };
            match format {
                Some(format) => {
                    crate::prepare_display();
                    describe(format, Path::new(path))
                }
                None => Err(format!("unknown plugin format {name}")),
            }
        }
        _ => Err("expected a format and a path".to_owned()),
    };
    let line = serde_json::to_string(&Description { result }).unwrap_or_default();
    println!("{MARKER}{line}");
}

/// What the cache remembers about one file.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CacheEntry {
    format: Format,
    path: PathBuf,
    size: u64,
    modified: u64,
    result: Result<Vec<PluginInfo>, String>,
}

/// A file's size and modification time (seconds), to notice changes.
fn stamp(path: &Path) -> (u64, u64) {
    let target = if path.is_dir() {
        // A bundle: its binary changes when the plugin is updated.
        let mut newest = (0, 0);
        let mut found = Vec::new();
        collect_files(path, 0, &mut found);
        for file in found {
            let (size, modified) = file_stamp(&file);
            newest = (newest.0 + size, newest.1.max(modified));
        }
        return newest;
    } else {
        path
    };
    file_stamp(target)
}

fn file_stamp(path: &Path) -> (u64, u64) {
    std::fs::metadata(path).map_or((0, 0), |m| {
        let modified = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        (m.len(), modified)
    })
}

fn collect_files(folder: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, depth + 1, found);
        } else {
            found.push(path);
        }
    }
}

/// What scanning has found so far.
#[derive(Clone, Debug, Default)]
pub struct Findings {
    /// Every plugin found, sorted by name.
    pub plugins: Vec<PluginInfo>,
    /// Files that could not be described, with the reason.
    pub problems: Vec<(PathBuf, String)>,
    /// How many files had to be opened (in a separate run of the program)
    /// because the cache did not know them, or they changed.
    pub opened: usize,
}

/// Scanning in the background.
pub struct Scanner {
    findings: Arc<Mutex<Findings>>,
    done: Arc<AtomicBool>,
    checked: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    thread: Option<JoinHandle<()>>,
}

impl Scanner {
    /// Starts scanning `folders`, describing new files by running `program`
    /// with [`DESCRIBE_ARGUMENT`], and keeping what is learnt in
    /// `cache_file` (if given).
    pub fn start(program: PathBuf, cache_file: Option<PathBuf>, folders: Vec<PathBuf>) -> Self {
        let findings = Arc::new(Mutex::new(Findings::default()));
        let done = Arc::new(AtomicBool::new(false));
        let checked = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));
        let thread = {
            let (findings, done, checked, total) = (
                Arc::clone(&findings),
                Arc::clone(&done),
                Arc::clone(&checked),
                Arc::clone(&total),
            );
            std::thread::Builder::new()
                .name("plugin-scan".to_owned())
                .spawn(move || {
                    scan(
                        &program,
                        cache_file.as_deref(),
                        &folders,
                        &findings,
                        &checked,
                        &total,
                    );
                    done.store(true, Ordering::Release);
                })
                .ok()
        };
        if thread.is_none() {
            done.store(true, Ordering::Release);
        }
        Self {
            findings,
            done,
            checked,
            total,
            thread,
        }
    }

    /// What has been found so far.
    pub fn findings(&self) -> Findings {
        self.findings
            .lock()
            .map_or_else(|_| Findings::default(), |f| f.clone())
    }

    /// Whether scanning has finished.
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    /// Files checked so far, and how many there are.
    pub fn progress(&self) -> (usize, usize) {
        (
            self.checked.load(Ordering::Acquire),
            self.total.load(Ordering::Acquire),
        )
    }

    /// Waits until scanning has finished.
    pub fn wait(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn scan(
    program: &Path,
    cache_file: Option<&Path>,
    folders: &[PathBuf],
    findings: &Mutex<Findings>,
    checked: &AtomicUsize,
    total: &AtomicUsize,
) {
    let cached: Vec<CacheEntry> = cache_file
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let cached: HashMap<(Format, PathBuf), CacheEntry> = cached
        .into_iter()
        .map(|e| ((e.format, e.path.clone()), e))
        .collect();
    let files = plugin_files(folders);
    total.store(files.len(), Ordering::Release);
    let mut entries = Vec::with_capacity(files.len());
    for (format, path) in files {
        let (size, modified) = stamp(&path);
        let known = cached
            .get(&(format, path.clone()))
            .filter(|entry| entry.size == size && entry.modified == modified);
        let opened = known.is_none();
        let entry = match known {
            Some(entry) => entry.clone(),
            None => CacheEntry {
                format,
                path: path.clone(),
                size,
                modified,
                result: describe_in_child(program, format, &path),
            },
        };
        if let Ok(mut findings) = findings.lock() {
            findings.opened += usize::from(opened);
            match &entry.result {
                Ok(plugins) => findings.plugins.extend(plugins.iter().cloned()),
                Err(problem) => findings.problems.push((path.clone(), problem.clone())),
            }
            findings.plugins.sort_by(|a, b| {
                (a.name.to_lowercase(), a.format).cmp(&(b.name.to_lowercase(), b.format))
            });
        }
        entries.push(entry);
        checked.fetch_add(1, Ordering::AcqRel);
    }
    if let Some(cache_file) = cache_file
        && let Ok(json) = serde_json::to_vec_pretty(&entries)
    {
        if let Some(folder) = cache_file.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = std::fs::write(cache_file, json);
    }
}

/// Describes a file by running the program on it, within the time limit.
fn describe_in_child(
    program: &Path,
    format: Format,
    path: &Path,
) -> Result<Vec<PluginInfo>, String> {
    let format_name = match format {
        Format::Clap => "clap",
        Format::Vst3 => "vst3",
    };
    let mut child = Command::new(program)
        .arg(DESCRIBE_ARGUMENT)
        .arg(format_name)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not check it: {e}"))?;
    // Read the output as it comes, so a chatty plugin cannot fill the pipe
    // and block.
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut output = String::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.read_to_string(&mut output);
        }
        output
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() > TIME_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break None,
        }
    };
    let output = reader.join().unwrap_or_default();
    let Some(status) = status else {
        return Err("it took too long to answer".to_owned());
    };
    let description = output
        .lines()
        .find_map(|line| line.strip_prefix(MARKER))
        .and_then(|json| serde_json::from_str::<Description>(json).ok());
    match description {
        Some(description) => description.result,
        None if status.success() => Err("it gave no description".to_owned()),
        None => Err("it crashed while being checked".to_owned()),
    }
}

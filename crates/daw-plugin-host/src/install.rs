// SPDX-License-Identifier: GPL-3.0-or-later

//! Putting plugins in place (decision 0023).
//!
//! The program keeps a plugin folder of its own (in its data folder,
//! always searched). Plugins the person adds, as files (`.vst3`, `.clap`),
//! folders or downloaded archives (`.zip`), are copied into it; nothing is
//! changed in the system's folders. A Windows plugin added on Linux goes
//! into a folder of its own, which yabridge (when installed) turns into
//! Linux plugins the search finds like any other.
//!
//! Archives come from anywhere: their entries' names are checked (nothing
//! may land outside the folder it is unpacked into) and sizes are limited.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::Format;
use crate::scan::{System, built_for};

/// The largest archive opened.
const MAX_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// The largest file unpacked from an archive.
const MAX_ENTRY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// How deep a folder is searched for plugins.
const MAX_DEPTH: usize = 8;

/// The program's own plugin folder, inside its data folder.
pub fn own_folder(data: &Path) -> PathBuf {
    data.join("plugins")
}

/// The folders of the program's own plugin folder the search looks in.
pub fn own_search_folders(own: &Path) -> Vec<PathBuf> {
    vec![own.join("clap"), own.join("vst3")]
}

/// Where Windows plugins added on Linux wait for yabridge.
pub fn windows_folder(own: &Path) -> PathBuf {
    own.join("windows")
}

/// Why something added was not put in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Skip {
    /// It holds no plugin the program knows.
    NotAPlugin,
    /// A VST2 plugin (`.dll`, or `.so` without a bundle): not supported yet.
    Vst2,
    /// An installer (`.exe`, `.msi`): run it instead.
    Installer,
    /// Built for a system this program cannot use (a Mac plugin, say).
    OtherSystem(System),
    /// Copying or unpacking failed: why.
    Failed(String),
}

/// What adding some files did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Added {
    /// Plugins put in place for this system.
    pub plugins: Vec<PathBuf>,
    /// Windows plugins put aside for yabridge (Linux only).
    pub windows: Vec<PathBuf>,
    /// What was not put in place, and why.
    pub skipped: Vec<(PathBuf, Skip)>,
}

/// Adds plugins from files, folders and archives into `own` (the
/// program's plugin folder). A plugin already there is replaced (a newer
/// version, say).
pub fn add(sources: &[PathBuf], own: &Path) -> Added {
    let mut added = Added::default();
    for source in sources {
        if source.is_dir() && Format::of_path(source).is_none() {
            let mut found = Vec::new();
            find(source, 0, &mut found);
            if found.is_empty() {
                added.skipped.push((source.clone(), Skip::NotAPlugin));
            }
            for item in found {
                add_one(&item, own, &mut added);
            }
        } else {
            add_one(source, own, &mut added);
        }
    }
    added
}

/// Plugin files, archives and installers below a folder.
fn find(folder: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        if name.starts_with("._") || name == "__MACOSX" {
            continue;
        }
        if Format::of_path(&path).is_some() || kind_of(&path).is_some() {
            found.push(path);
        } else if path.is_dir() {
            find(&path, depth + 1, found);
        }
    }
}

/// How many entries [`holds_plugins`] looks at before giving up.
const QUICK_LOOK_ENTRIES: usize = 5_000;

/// Whether a folder holds plugins (or archives, VST2 plugins or
/// installers), from a quick look: a folder dropped on the window goes to
/// the Plugins window when it does, to the sound library otherwise.
pub fn holds_plugins(folder: &Path) -> bool {
    fn look(folder: &Path, depth: usize, budget: &mut usize) -> bool {
        if depth > 4 {
            return false;
        }
        let Ok(entries) = fs::read_dir(folder) else {
            return false;
        };
        let mut folders = Vec::new();
        for entry in entries.flatten() {
            if *budget == 0 {
                return false;
            }
            *budget -= 1;
            let path = entry.path();
            if Format::of_path(&path).is_some()
                || matches!(kind_of(&path), Some(Other::Vst2 | Other::Installer))
            {
                return true;
            }
            if path.is_dir() {
                folders.push(path);
            }
        }
        folders.iter().any(|f| look(f, depth + 1, budget))
    }
    if Format::of_path(folder).is_some() {
        return true;
    }
    let mut budget = QUICK_LOOK_ENTRIES;
    look(folder, 0, &mut budget)
}

/// What kind of thing a file is, by its ending (other than plugins).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Other {
    Archive,
    Vst2,
    Installer,
}

fn kind_of(path: &Path) -> Option<Other> {
    let ending = path.extension()?.to_str()?.to_ascii_lowercase();
    match ending.as_str() {
        "zip" => Some(Other::Archive),
        "dll" => Some(Other::Vst2),
        "exe" | "msi" => Some(Other::Installer),
        _ => None,
    }
}

fn add_one(source: &Path, own: &Path, added: &mut Added) {
    if let Some(format) = Format::of_path(source) {
        match put_in_place(source, format, own) {
            Ok((place, true)) => added.windows.push(place),
            Ok((place, false)) => added.plugins.push(place),
            Err(skip) => added.skipped.push((source.to_path_buf(), skip)),
        }
        return;
    }
    match kind_of(source) {
        Some(Other::Archive) => match unpack(source) {
            Ok(folder) => {
                let mut found = Vec::new();
                find(&folder, 0, &mut found);
                let before = added.plugins.len() + added.windows.len();
                for item in found.iter().filter(|p| Format::of_path(p).is_some()) {
                    add_one(item, own, added);
                }
                if added.plugins.len() + added.windows.len() == before {
                    added.skipped.push((source.to_path_buf(), Skip::NotAPlugin));
                }
                let _ = fs::remove_dir_all(folder);
            }
            Err(error) => added
                .skipped
                .push((source.to_path_buf(), Skip::Failed(error.to_string()))),
        },
        Some(Other::Vst2) => added.skipped.push((source.to_path_buf(), Skip::Vst2)),
        Some(Other::Installer) => added.skipped.push((source.to_path_buf(), Skip::Installer)),
        None => added.skipped.push((source.to_path_buf(), Skip::NotAPlugin)),
    }
}

/// Copies one plugin (a file or a bundle) into place. Returns where it
/// went and whether it is a Windows plugin put aside for yabridge.
fn put_in_place(source: &Path, format: Format, own: &Path) -> Result<(PathBuf, bool), Skip> {
    let system = built_for(source);
    let bridged = match system {
        Some(System::Windows) if System::current() == System::Linux => true,
        Some(other) if other != System::current() => return Err(Skip::OtherSystem(other)),
        _ => false,
    };
    let base = if bridged {
        windows_folder(own)
    } else {
        own.to_path_buf()
    };
    let folder = base.join(match format {
        Format::Clap => "clap",
        Format::Vst3 => "vst3",
    });
    let name = source.file_name().ok_or(Skip::NotAPlugin)?;
    let target = folder.join(name);
    if target == source {
        return Ok((target, bridged));
    }
    let copied = (|| {
        fs::create_dir_all(&folder)?;
        if target.exists() {
            remove_path(&target)?;
        }
        copy_recursively(source, &target)
    })();
    copied.map_err(|e| Skip::Failed(e.to_string()))?;
    Ok((target, bridged))
}

fn remove_path(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn copy_recursively(from: &Path, to: &Path) -> io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_recursively(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

/// Unpacks an archive into a fresh folder beside the system's temporary
/// files; returns the folder. Entry names that would land outside it are
/// refused.
fn unpack(archive: &Path) -> io::Result<PathBuf> {
    if fs::metadata(archive)?.len() > MAX_ARCHIVE_BYTES {
        return Err(io::Error::other("the archive is unreasonably large"));
    }
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| io::Error::other(format!("not a usable archive: {e}")))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let folder =
        std::env::temp_dir().join(format!("plugin-archive-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&folder)?;
    let result = (|| {
        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|e| io::Error::other(e.to_string()))?;
            let name = entry
                .name()
                .map_err(|e| io::Error::other(e.to_string()))?
                .into_owned();
            let Some(relative) = safe_relative(&name) else {
                continue;
            };
            let target = folder.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(&target)?;
                continue;
            }
            if entry.size() > MAX_ENTRY_BYTES {
                return Err(io::Error::other(
                    "a file in the archive is unreasonably large",
                ));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = fs::File::create(&target)?;
            let copied = io::copy(&mut (&mut entry).take(MAX_ENTRY_BYTES + 1), &mut out)?;
            if copied > MAX_ENTRY_BYTES {
                return Err(io::Error::other(
                    "a file in the archive is unreasonably large",
                ));
            }
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o755));
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(folder),
        Err(error) => {
            let _ = fs::remove_dir_all(&folder);
            Err(error)
        }
    }
}

/// An archive entry's name as a path inside the folder it is unpacked
/// into, if it stays inside it.
fn safe_relative(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

/// Removes a plugin the program put in its folder. Plugins elsewhere are
/// refused (their own uninstallers own them).
pub fn remove(plugin: &Path, own: &Path) -> io::Result<()> {
    let inside = plugin
        .canonicalize()
        .ok()
        .zip(own.canonicalize().ok())
        .is_some_and(|(plugin, own)| plugin.starts_with(&own) && plugin != own);
    if !inside {
        return Err(io::Error::other(
            "only plugins added in the program can be removed here",
        ));
    }
    remove_path(plugin)
}

/// Whether a plugin is one the program put in its folder.
pub fn is_own(plugin: &Path, own: &Path) -> bool {
    plugin.starts_with(own)
}

/// Wine and yabridge, as found on this computer (Linux).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bridge {
    /// yabridge's command-line tool, if found.
    pub yabridgectl: Option<PathBuf>,
    /// Wine, if found.
    pub wine: Option<PathBuf>,
}

impl Bridge {
    /// Whether Windows plugins can be bridged.
    pub fn is_ready(&self) -> bool {
        self.yabridgectl.is_some() && self.wine.is_some()
    }
}

/// A program on the PATH, if it is there.
fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|folder| folder.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// Looks for Wine and yabridge (`yabridgectl` on the PATH, or where
/// yabridge's own instructions put it, `~/.local/share/yabridge`).
pub fn find_bridge() -> Bridge {
    if System::current() != System::Linux {
        return Bridge::default();
    }
    let yabridgectl = on_path("yabridgectl").or_else(|| {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".local/share/yabridge/yabridgectl"))
            .filter(|p| p.is_file())
    });
    Bridge {
        yabridgectl,
        wine: on_path("wine"),
    }
}

/// Makes yabridge bridge the Windows plugins in `folders`: adds each
/// folder to yabridge's list (once) and brings the bridges up to date; `prune`
/// also removes the bridges of Windows plugins that are gone (after one
/// is removed). The bridged plugins appear in `~/.vst3` and `~/.clap`,
/// which the search looks in.
pub fn bridge_windows_plugins(
    bridge: &Bridge,
    folders: &[PathBuf],
    prune: bool,
) -> Result<(), String> {
    let tool = bridge
        .yabridgectl
        .as_ref()
        .ok_or_else(|| "yabridge is not installed".to_owned())?;
    let run = |args: &[&std::ffi::OsStr]| -> Result<String, String> {
        let output = Command::new(tool)
            .args(args)
            .output()
            .map_err(|e| format!("could not run yabridgectl: {e}"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
        }
    };
    let listed = run(&["list".as_ref()]).unwrap_or_default();
    for folder in folders {
        fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let shown = folder.display().to_string();
        if !listed.lines().any(|line| line.trim() == shown) {
            run(&["add".as_ref(), folder.as_os_str()])?;
        }
    }
    if prune {
        run(&["sync".as_ref(), "--prune".as_ref()]).map(|_| ())
    } else {
        run(&["sync".as_ref()]).map(|_| ())
    }
}

/// The folders Windows installers put plugins in inside Wine's setup
/// (`WINEPREFIX`, or `~/.wine`), those that exist.
pub fn wine_plugin_folders() -> Vec<PathBuf> {
    let prefix = std::env::var_os("WINEPREFIX")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".wine")));
    let Some(prefix) = prefix else {
        return Vec::new();
    };
    let common = prefix
        .join("drive_c")
        .join("Program Files")
        .join("Common Files");
    [common.join("VST3"), common.join("CLAP")]
        .into_iter()
        .filter(|folder| folder.is_dir())
        .collect()
}

/// The plugins in the program's folder (for listing what can be removed).
pub fn own_plugins(own: &Path) -> BTreeSet<PathBuf> {
    let mut found = Vec::new();
    for folder in own_search_folders(own).into_iter().chain([
        windows_folder(own).join("vst3"),
        windows_folder(own).join("clap"),
    ]) {
        find(&folder, 0, &mut found);
    }
    found
        .into_iter()
        .filter(|p| Format::of_path(p).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("install-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// A VST3 bundle with a binary for a system (the binary's first bytes
    /// say which).
    fn bundle(at: &Path, name: &str, platform: &str, magic: &[u8]) -> PathBuf {
        let path = at.join(format!("{name}.vst3"));
        let binary = path.join("Contents").join(platform);
        fs::create_dir_all(&binary).unwrap();
        fs::write(binary.join(format!("{name}.so")), magic).unwrap();
        path
    }

    #[test]
    fn plugins_files_and_bundles_are_copied_into_place() {
        let source = scratch("source");
        let own = scratch("own").join("plugins");
        let native = if cfg!(windows) {
            "x86_64-win"
        } else {
            "x86_64-linux"
        };
        let synth = bundle(&source, "Synth", native, b"\x7fELF");
        let clap = source.join("Delay.clap");
        fs::write(&clap, b"\x7fELF0000").unwrap();
        let added = add(&[synth, clap], &own);
        assert_eq!(added.plugins.len(), 2, "{added:?}");
        assert!(own.join("vst3/Synth.vst3/Contents").is_dir());
        assert!(own.join("clap/Delay.clap").is_file());
        // Added again: replaced, not doubled.
        let again = add(&[source.join("Delay.clap")], &own);
        assert_eq!(again.plugins, vec![own.join("clap/Delay.clap")]);
        assert_eq!(own_plugins(&own).len(), 2);
        // Removed: only from the program's folder.
        remove(&own.join("clap/Delay.clap"), &own).unwrap();
        assert!(!own.join("clap/Delay.clap").exists());
        assert!(
            remove(&source.join("Delay.clap"), &own).is_err(),
            "not ours"
        );
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(own.parent().unwrap());
    }

    #[test]
    fn folders_and_archives_are_searched_and_other_files_explained() {
        let source = scratch("archive-source");
        let own = scratch("archive-own").join("plugins");
        // An archive as downloads come: a folder with a bundle and a
        // read-me inside, plus Mac leftovers.
        let archive = source.join("Download.zip");
        {
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            let native = if cfg!(windows) {
                "x86_64-win"
            } else {
                "x86_64-linux"
            };
            zip.start_file(
                format!("Pack/Reverb.vst3/Contents/{native}/Reverb.so"),
                options,
            )
            .unwrap();
            std::io::Write::write_all(&mut zip, b"\x7fELF").unwrap();
            zip.start_file("Pack/README.txt", options).unwrap();
            zip.start_file("__MACOSX/Pack/._Reverb.vst3", options)
                .unwrap();
            zip.start_file("../escape.clap", options).unwrap();
            std::io::Write::write_all(&mut zip, b"\x7fELF").unwrap();
            zip.finish().unwrap();
        }
        let folder = source.join("Folder");
        fs::create_dir_all(folder.join("deep")).unwrap();
        fs::write(folder.join("deep/Chorus.clap"), b"\x7fELF").unwrap();
        let dll = source.join("Old.dll");
        fs::write(&dll, b"MZ").unwrap();
        let installer = source.join("Setup.exe");
        fs::write(&installer, b"MZ").unwrap();
        let text = source.join("notes.txt");
        fs::write(&text, b"hello").unwrap();
        let added = add(
            &[
                archive,
                folder,
                dll.clone(),
                installer.clone(),
                text.clone(),
            ],
            &own,
        );
        assert!(own.join("vst3/Reverb.vst3").is_dir(), "{added:?}");
        assert!(own.join("clap/Chorus.clap").is_file());
        assert!(
            !own.parent().unwrap().join("escape.clap").exists(),
            "nothing escapes"
        );
        assert!(added.skipped.contains(&(dll, Skip::Vst2)));
        assert!(added.skipped.contains(&(installer, Skip::Installer)));
        assert!(added.skipped.contains(&(text, Skip::NotAPlugin)));
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(own.parent().unwrap());
    }

    #[test]
    fn windows_plugins_on_linux_are_put_aside_for_yabridge() {
        if System::current() != System::Linux {
            return;
        }
        let source = scratch("windows-source");
        let own = scratch("windows-own").join("plugins");
        let windows = bundle(&source, "WinSynth", "x86_64-win", b"MZ\x90\x00");
        let mac = bundle(&source, "MacSynth", "MacOS", b"\xcf\xfa\xed\xfe");
        let added = add(&[windows, mac.clone()], &own);
        assert_eq!(
            added.windows,
            vec![windows_folder(&own).join("vst3/WinSynth.vst3")]
        );
        assert_eq!(added.plugins, Vec::<PathBuf>::new());
        assert!(
            added
                .skipped
                .contains(&(mac, Skip::OtherSystem(System::Mac)))
        );
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(own.parent().unwrap());
    }

    #[test]
    fn yabridge_is_asked_to_bridge_the_windows_folder() {
        if System::current() != System::Linux {
            return;
        }
        // A stand-in for yabridgectl that writes down how it was called.
        let folder = scratch("yabridge");
        let log = folder.join("calls.txt");
        let tool = folder.join("yabridgectl");
        fs::write(
            &tool,
            format!("#!/bin/sh\necho \"$@\" >> '{}'\nexit 0\n", log.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let bridge = Bridge {
            yabridgectl: Some(tool),
            wine: Some(PathBuf::from("/usr/bin/wine")),
        };
        assert!(bridge.is_ready());
        let windows = folder.join("plugins/windows");
        bridge_windows_plugins(&bridge, std::slice::from_ref(&windows), false).unwrap();
        let calls = fs::read_to_string(&log).unwrap();
        assert!(
            calls.contains(&format!("add {}", windows.display())),
            "{calls}"
        );
        assert_eq!(calls.lines().last(), Some("sync"));
        // After a Windows plugin is removed, its bridge goes too.
        bridge_windows_plugins(&bridge, std::slice::from_ref(&windows), true).unwrap();
        let calls = fs::read_to_string(&log).unwrap();
        assert_eq!(calls.lines().last(), Some("sync --prune"));
        assert!(bridge_windows_plugins(&Bridge::default(), &[windows], false).is_err());
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn a_quick_look_tells_plugin_folders_from_sound_folders() {
        let folder = scratch("quick-look");
        fs::create_dir_all(folder.join("sounds/kicks")).unwrap();
        fs::write(folder.join("sounds/kicks/kick.wav"), b"RIFF").unwrap();
        assert!(!holds_plugins(&folder.join("sounds")));
        fs::create_dir_all(folder.join("plugins/vendor")).unwrap();
        fs::write(folder.join("plugins/vendor/Synth.clap"), b"\x7fELF").unwrap();
        assert!(holds_plugins(&folder.join("plugins")));
        assert!(holds_plugins(&folder));
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn archive_names_cannot_escape() {
        assert_eq!(safe_relative("a/b.clap"), Some(PathBuf::from("a/b.clap")));
        assert_eq!(safe_relative("./a.clap"), Some(PathBuf::from("a.clap")));
        assert_eq!(safe_relative("../a.clap"), None);
        assert_eq!(safe_relative("/etc/a.clap"), None);
        assert_eq!(safe_relative(""), None);
    }
}

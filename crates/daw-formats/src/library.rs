// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding sounds, MIDI files and instruments (SoundFonts, SFZ) in the
//! person's folders, for
//! the sound library (decision 0023). A folder may hold many thousands of
//! files: it is listed once, in the background, then browsed and searched
//! in memory. Nothing is copied or changed.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// The most files one folder's listing holds.
pub const MAX_ITEMS: usize = 500_000;

/// How deep folders are searched.
const MAX_DEPTH: usize = 24;

/// What a library file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemKind {
    /// A sound (WAV, AIFF, FLAC, MP3, Ogg Vorbis).
    Sound,
    /// A MIDI file: notes.
    Midi,
    /// A SoundFont: an instrument.
    SoundFont,
    /// An SFZ instrument (decision 0024).
    Sfz,
}

impl ItemKind {
    /// What a file is, by its name's ending.
    pub fn of(path: &Path) -> Option<Self> {
        let ending = path.extension()?.to_str()?.to_ascii_lowercase();
        match ending.as_str() {
            "wav" | "wave" | "aif" | "aiff" | "aifc" | "flac" | "mp3" | "ogg" | "oga" => {
                Some(Self::Sound)
            }
            "mid" | "midi" | "rmi" | "smf" => Some(Self::Midi),
            "sf2" => Some(Self::SoundFont),
            "sfz" => Some(Self::Sfz),
            _ => None,
        }
    }
}

/// One file in a library folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Its path inside the folder, its parts separated by `/`.
    pub relative: String,
    /// The same in lower case, for sorting and searching.
    pub lower: String,
    /// What it is.
    pub kind: ItemKind,
}

impl Item {
    /// The file's name (the last part of its path).
    pub fn name(&self) -> &str {
        self.relative
            .rsplit_once('/')
            .map_or(self.relative.as_str(), |(_, name)| name)
    }

    /// The folder it is in, inside the library folder ("" at the top).
    pub fn folder(&self) -> &str {
        self.relative
            .rsplit_once('/')
            .map_or("", |(folder, _)| folder)
    }
}

/// The files of a library folder, sorted by path (case ignored).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    /// The files.
    pub items: Vec<Item>,
    /// The folder held more than [`MAX_ITEMS`] files: the rest are left out.
    pub truncated: bool,
}

/// Lists a folder's sounds, MIDI files and SoundFonts, however deep
/// (hidden files and folders are skipped). Stops early, with what it found
/// so far, when `stop` is set.
pub fn list(root: &Path, stop: &AtomicBool) -> Listing {
    let mut listing = Listing::default();
    let mut folders = vec![(root.to_path_buf(), String::new(), 0_usize)];
    while let Some((folder, prefix, depth)) = folders.pop() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue; // A name the interface could not show anyway.
            };
            if name.starts_with('.') || name == "__MACOSX" {
                continue;
            }
            let relative = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            let path = entry.path();
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    folders.push((path, relative, depth + 1));
                }
            } else if let Some(kind) = ItemKind::of(&path) {
                if listing.items.len() >= MAX_ITEMS {
                    listing.truncated = true;
                    folders.clear();
                    break;
                }
                listing.items.push(Item {
                    lower: relative.to_lowercase(),
                    relative,
                    kind,
                });
            }
        }
    }
    listing.items.sort_by(|a, b| {
        a.lower
            .cmp(&b.lower)
            .then_with(|| a.relative.cmp(&b.relative))
    });
    listing
}

/// What a folder of the listing holds directly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inside {
    /// Its folders, by name.
    pub folders: Vec<String>,
    /// Its files, as indices into the listing.
    pub items: Vec<usize>,
}

impl Listing {
    /// The folders and files directly inside `folder` ("" for the top;
    /// parts separated by `/`).
    pub fn inside(&self, folder: &str) -> Inside {
        let prefix = if folder.is_empty() {
            String::new()
        } else {
            format!("{}/", folder.to_lowercase())
        };
        // How many parts the folder's own path has.
        let depth = if folder.is_empty() {
            0
        } else {
            folder.split('/').count()
        };
        let start = self.items.partition_point(|item| item.lower < prefix);
        let mut inside = Inside::default();
        for (index, item) in self.items.iter().enumerate().skip(start) {
            if !item.lower.starts_with(&prefix) {
                break;
            }
            // The parts are counted, not measured: lower case may change
            // a name's length, never its slashes.
            let mut parts = item.relative.split('/').skip(depth);
            let (Some(first), more) = (parts.next(), parts.next().is_some()) else {
                continue;
            };
            if more {
                // A folder inside, by its name as written.
                if inside
                    .folders
                    .last()
                    .is_none_or(|last| last.to_lowercase() != first.to_lowercase())
                {
                    inside.folders.push(first.to_owned());
                }
            } else {
                inside.items.push(index);
            }
        }
        inside.folders.sort_by_key(|name| name.to_lowercase());
        inside
            .folders
            .dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());
        inside
    }

    /// The files whose path holds every word of `search` (case ignored):
    /// at most `limit` of them, and how many there are in all.
    pub fn search(&self, search: &str, limit: usize) -> (Vec<usize>, usize) {
        let words: Vec<String> = search.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return (Vec::new(), 0);
        }
        let mut found = Vec::new();
        let mut total = 0;
        for (index, item) in self.items.iter().enumerate() {
            if words.iter().all(|word| item.lower.contains(word.as_str())) {
                total += 1;
                if found.len() < limit {
                    found.push(index);
                }
            }
        }
        (found, total)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("library-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn touch(root: &Path, relative: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn a_folder_of_samples_is_listed_browsed_and_searched() {
        let root = scratch("samples");
        for file in [
            "Drums/Kicks/Kick Hard.wav",
            "Drums/Kicks/kick soft.WAV",
            "Drums/Snares/Snare 1.aiff",
            "Drums/readme.txt",
            "Loops/Funk 120.flac",
            "Loops/.hidden.wav",
            ".cache/ignored.wav",
            "Grooves/Swing.mid",
            "Piano.sf2",
            "Vocal.mp3",
            "Strings/Strings.sfz",
        ] {
            touch(&root, file);
        }
        let listing = list(&root, &AtomicBool::new(false));
        let names: Vec<&str> = listing.items.iter().map(Item::name).collect();
        assert_eq!(
            names,
            [
                "Kick Hard.wav",
                "kick soft.WAV",
                "Snare 1.aiff",
                "Swing.mid",
                "Funk 120.flac",
                "Piano.sf2",
                "Strings.sfz",
                "Vocal.mp3"
            ]
        );
        assert!(!listing.truncated);
        let kinds: Vec<ItemKind> = listing.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds[3], ItemKind::Midi);
        assert_eq!(kinds[5], ItemKind::SoundFont);
        assert_eq!(kinds[6], ItemKind::Sfz);

        // Browsing: folders first, then the files right there.
        let top = listing.inside("");
        assert_eq!(top.folders, ["Drums", "Grooves", "Loops", "Strings"]);
        let top_names: Vec<&str> = top.items.iter().map(|&i| listing.items[i].name()).collect();
        assert_eq!(top_names, ["Piano.sf2", "Vocal.mp3"]);
        let drums = listing.inside("Drums");
        assert_eq!(drums.folders, ["Kicks", "Snares"]);
        assert_eq!(
            drums.items,
            Vec::<usize>::new(),
            "the text file is not listed"
        );
        let kicks = listing.inside("Drums/Kicks");
        assert_eq!(kicks.items.len(), 2);
        assert_eq!(listing.items[kicks.items[0]].folder(), "Drums/Kicks");

        // Searching: every word, anywhere in the path, case ignored.
        let (found, total) = listing.search("kick", 10);
        assert_eq!((found.len(), total), (2, 2));
        let (found, _) = listing.search("DRUMS hard", 10);
        assert_eq!(listing.items[found[0]].name(), "Kick Hard.wav");
        let (found, total) = listing.search("a", 1);
        assert_eq!(found.len(), 1, "at most the limit");
        assert!(total > 1, "but all are counted");
        assert_eq!(listing.search("   ", 10), (Vec::new(), 0));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn listing_stops_when_asked() {
        let root = scratch("stop");
        touch(&root, "a.wav");
        let listing = list(&root, &AtomicBool::new(true));
        assert_eq!(listing.items, Vec::<Item>::new());
        let _ = std::fs::remove_dir_all(root);
    }
}

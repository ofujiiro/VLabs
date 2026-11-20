// SPDX-License-Identifier: GPL-3.0-or-later

//! Classic UTAU voicebanks (decision 0027): a folder of WAV recordings with
//! an `oto.ini` saying where each syllable is in them (in the folder and in
//! its subfolders), a `prefix.map` for voicebanks recorded at several
//! pitches, a `character.txt` (name, picture, author) and a readme with the
//! creator's terms. Voicebanks made for OpenUtau may add a `character.yaml`
//! naming a large portrait of the character (decision 0029).
//!
//! UTAU wrote its text files in Shift-JIS; newer voicebanks often use
//! UTF-8. A file is read as UTF-8 when it is valid UTF-8, otherwise as
//! Shift-JIS. Japanese file names inside zip archives are decoded the same
//! way.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use daw_dsp::voice::{OtoEntry, VoiceBank, VoiceSample};

use crate::audio;

/// The largest archive and the largest file in one that are unpacked.
const MAX_ARCHIVE_BYTES: u64 = 4 << 30;
const MAX_ENTRY_BYTES: u64 = 512 << 20;
/// The largest text file read (an `oto.ini` of a big voicebank is a few
/// hundred kilobytes).
const MAX_TEXT_BYTES: u64 = 16 << 20;
/// How deep subfolders are searched for `oto.ini` files.
const MAX_DEPTH: usize = 3;
/// The largest picture file read, and the largest picture decoded.
const MAX_PICTURE_BYTES: u64 = 32 << 20;
const MAX_PICTURE_SIDE: u32 = 8192;
/// Pictures larger than this on their long side are scaled down when read.
const PICTURE_SIDE: u32 = 1024;

/// What a voicebank says about itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VoicebankInfo {
    /// Its folder.
    pub folder: PathBuf,
    /// Its name (`character.txt`, or the folder's name).
    pub name: String,
    /// Its picture, if it names one that exists.
    pub image: Option<PathBuf>,
    /// Its portrait, a larger picture of the character (OpenUtau's
    /// `character.yaml`), if it names one that exists.
    pub portrait: Option<PathBuf>,
    /// Who made it.
    pub author: Option<String>,
    /// Its web page.
    pub web: Option<String>,
    /// The creator's terms of use and other notes (the readme), as written.
    pub terms: String,
    /// How many syllables its `oto.ini` files name.
    pub syllables: usize,
}

impl VoicebankInfo {
    /// The picture to show for the voicebank (decision 0029): its portrait
    /// when it has one, otherwise its picture.
    pub fn picture(&self) -> Option<&Path> {
        self.portrait.as_deref().or(self.image.as_deref())
    }
}

/// A picture, decoded: its size and its pixels, row by row, four bytes each
/// (red, green, blue, alpha, not multiplied by alpha).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    /// Its width, in pixels.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
    /// Its pixels.
    pub rgba: Vec<u8>,
}

/// Reads a voicebank's picture (BMP, PNG or JPEG), scaled down to at most
/// 1024 pixels on its long side; files too large to be a picture, or
/// pictures too large to decode, are refused.
pub fn read_picture(path: &Path) -> Result<Picture, String> {
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_PICTURE_BYTES {
        return Err("the picture is too large".to_owned());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_PICTURE_SIDE);
    limits.max_image_height = Some(MAX_PICTURE_SIDE);
    limits.max_alloc = Some(512 << 20);
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| e.to_string())?;
    let decoded = if decoded.width().max(decoded.height()) > PICTURE_SIDE {
        decoded.thumbnail(PICTURE_SIDE, PICTURE_SIDE)
    } else {
        decoded
    };
    let rgba = decoded.to_rgba8();
    Ok(Picture {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// A top-level value of a `character.yaml` (`key: value`, quotes removed);
/// enough for the few plain values read from it.
fn yaml_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        if line.starts_with([' ', '\t', '-', '#']) {
            return None;
        }
        let (k, v) = line.split_once(':')?;
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
        (k.trim() == key && !v.is_empty()).then(|| v.to_owned())
    })
}

/// Text from a voicebank: UTF-8 when valid (without its byte-order mark),
/// otherwise Shift-JIS, as UTAU wrote it.
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
    }
}

fn read_text(path: &Path) -> io::Result<String> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_BYTES).read_to_end(&mut bytes)?;
    Ok(decode_text(&bytes))
}

/// The lines of an `oto.ini`: `file.wav=alias,offset,consonant,cutoff,
/// preutterance,overlap`. A line without an alias names the file's own
/// syllable (its name without `.wav`); missing or unreadable numbers are 0.
/// `folder` is put before each file name (the subfolder the `oto.ini` is
/// in, relative to the voicebank).
pub fn parse_oto(text: &str, folder: &str) -> Vec<OtoEntry> {
    text.lines()
        .filter_map(|line| {
            let (file, rest) = line.trim().split_once('=')?;
            let file = file.trim();
            if file.is_empty() {
                return None;
            }
            let mut fields = rest.split(',');
            let alias = fields.next().unwrap_or("").trim();
            let alias = if alias.is_empty() {
                file.strip_suffix(".wav")
                    .or_else(|| file.strip_suffix(".WAV"))
                    .unwrap_or(file)
            } else {
                alias
            };
            let mut number = || {
                fields
                    .next()
                    .and_then(|f| f.trim().parse::<f64>().ok())
                    .filter(|n| n.is_finite())
                    .unwrap_or(0.0)
            };
            let path = if folder.is_empty() {
                file.to_owned()
            } else {
                format!("{folder}/{file}")
            };
            Some(OtoEntry {
                alias: alias.to_owned(),
                file: path,
                offset: number(),
                consonant: number(),
                cutoff: number(),
                preutterance: number(),
                overlap: number(),
            })
        })
        .collect()
}

/// A note name as UTAU writes it ("C4", "C#4", "Db4") as a key (C4 is 60).
fn key_of(name: &str) -> Option<u8> {
    let mut chars = name.trim().chars().peekable();
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let shift = match chars.peek() {
        Some('#') => {
            chars.next();
            1
        }
        Some('b') => {
            chars.next();
            -1
        }
        _ => 0,
    };
    let octave: i32 = chars.collect::<String>().parse().ok()?;
    u8::try_from((octave + 1) * 12 + base + shift)
        .ok()
        .filter(|k| *k <= 127)
}

/// The lines of a `prefix.map`: a note name, then the prefix and suffix its
/// notes' aliases take, separated by tabs.
pub fn parse_prefix_map(text: &str) -> Vec<(u8, String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let key = key_of(fields.next()?)?;
            let prefix = fields.next().unwrap_or("").to_owned();
            let suffix = fields
                .next()
                .unwrap_or("")
                .trim_end_matches('\r')
                .to_owned();
            (!prefix.is_empty() || !suffix.is_empty()).then_some((key, prefix, suffix))
        })
        .collect()
}

/// The `oto.ini` files of a voicebank, with the folder each is in
/// (relative to the voicebank, with `/`).
fn oto_files(root: &Path) -> Vec<(PathBuf, String)> {
    fn visit(folder: &Path, relative: &str, depth: usize, found: &mut Vec<(PathBuf, String)>) {
        let Ok(entries) = fs::read_dir(folder) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    let inner = if relative.is_empty() {
                        name
                    } else {
                        format!("{relative}/{name}")
                    };
                    visit(&path, &inner, depth + 1, found);
                }
            } else if name.eq_ignore_ascii_case("oto.ini") {
                found.push((path, relative.to_owned()));
            }
        }
    }
    let mut found = Vec::new();
    visit(root, "", 0, &mut found);
    // The voicebank's own folder first: its lines win over subfolders'.
    found.sort_by_key(|(_, relative)| (relative.matches('/').count(), !relative.is_empty()));
    found
}

/// A `key=value` field of `character.txt`.
fn field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim().eq_ignore_ascii_case(key) && !v.trim().is_empty()).then(|| v.trim().to_owned())
    })
}

/// The readme of a voicebank: `readme.txt` (any case), or the first other
/// text file whose name says it holds the terms.
fn readme(root: &Path) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    let name = |p: &PathBuf| {
        p.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().to_lowercase())
    };
    entries
        .iter()
        .find(|p| name(p) == "readme.txt")
        .or_else(|| {
            entries.iter().find(|p| {
                let n = name(p);
                Path::new(&n)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
                    && (n.contains("readme") || n.contains("terms") || n.contains("規約"))
            })
        })
        .cloned()
}

/// What a voicebank in `folder` says about itself; an error when the folder
/// has no `oto.ini` (it is not a voicebank).
pub fn read_info(folder: &Path) -> Result<VoicebankInfo, String> {
    let otos = oto_files(folder);
    if otos.is_empty() {
        return Err("it has no oto.ini: it is not a UTAU voicebank".to_owned());
    }
    let syllables = otos
        .iter()
        .filter_map(|(path, relative)| Some(parse_oto(&read_text(path).ok()?, relative).len()))
        .sum();
    let character = read_text(&folder.join("character.txt")).unwrap_or_default();
    let yaml = read_text(&folder.join("character.yaml")).unwrap_or_default();
    let folder_name = folder
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    Ok(VoicebankInfo {
        folder: folder.to_path_buf(),
        name: field(&character, "name").unwrap_or(folder_name),
        image: field(&character, "image")
            .map(|image| folder.join(image))
            .filter(|path| path.is_file()),
        portrait: yaml_field(&yaml, "portrait")
            .map(|portrait| folder.join(portrait))
            .filter(|path| path.is_file()),
        author: field(&character, "author"),
        web: field(&character, "web"),
        terms: readme(folder)
            .and_then(|path| read_text(&path).ok())
            .unwrap_or_default(),
        syllables,
    })
}

/// Reads one recording of a voicebank as mono sound, analysed for singing.
fn load_recording(root: &Path, file: &str) -> Result<VoiceSample, String> {
    let path = file
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part));
    let bytes = audio::read_file(&path).map_err(|e| format!("{file}: {e}"))?;
    let decoded = audio::decode(&bytes).map_err(|e| format!("{file}: {e}"))?;
    let channels = decoded.channels.len().max(1);
    let frames = decoded.channels.first().map_or(0, Vec::len);
    #[allow(clippy::cast_precision_loss, reason = "one or two channels")]
    let mono: Vec<f32> = (0..frames)
        .map(|i| decoded.channels.iter().map(|c| c[i]).sum::<f32>() / channels as f32)
        .collect();
    Ok(VoiceSample::new(decoded.sample_rate, mono))
}

/// A voicebank in `folder`, ready to sing: its syllables from every
/// `oto.ini`, its `prefix.map`, and its recordings, read when first sung.
pub fn read_voicebank(folder: &Path) -> Result<VoiceBank, String> {
    let otos = oto_files(folder);
    if otos.is_empty() {
        return Err("it has no oto.ini: it is not a UTAU voicebank".to_owned());
    }
    let mut entries = Vec::new();
    for (path, relative) in &otos {
        let text = read_text(path).map_err(|e| format!("{}: {e}", path.display()))?;
        entries.extend(parse_oto(&text, relative));
    }
    let affixes = read_text(&folder.join("prefix.map"))
        .map_or_else(|_| Vec::new(), |text| parse_prefix_map(&text));
    let root = folder.to_path_buf();
    Ok(VoiceBank::new(
        entries,
        affixes,
        Box::new(move |file| load_recording(&root, file)),
    ))
}

/// The voicebanks installed in `root`, by name.
pub fn installed(root: &Path) -> Vec<VoicebankInfo> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<VoicebankInfo> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|p| read_info(&p).ok())
        .collect();
    found.sort_by_key(|info| info.name.to_lowercase());
    found
}

/// The voicebank inside a folder: the folder itself if it has an
/// `oto.ini` (or a `character.txt`), else the one folder in it that does
/// (archives often wrap a voicebank in a folder or two).
fn voicebank_root(folder: &Path, depth: usize) -> Option<PathBuf> {
    let has = |name: &str| folder.join(name).is_file();
    if has("oto.ini") || has("character.txt") || has("OTO.INI") {
        return Some(folder.to_path_buf());
    }
    if depth >= MAX_DEPTH {
        return None;
    }
    let mut inner: Vec<PathBuf> = fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.file_name().is_none_or(|n| n != "__MACOSX"))
        .collect();
    inner.sort();
    inner
        .into_iter()
        .find_map(|p| voicebank_root(&p, depth + 1))
}

/// A folder name that is safe on every system.
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_owned();
    if cleaned.is_empty() {
        "Voicebank".to_owned()
    } else {
        cleaned
    }
}

fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_folder(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// An archive entry's name: UTF-8 when valid, otherwise Shift-JIS (what
/// Japanese systems write), as a path inside the folder it is unpacked
/// into, if it stays inside it.
fn entry_path(raw: &[u8]) -> Option<PathBuf> {
    let name = decode_text(raw).replace('\\', "/");
    let mut clean = PathBuf::new();
    for component in Path::new(&name).components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

/// Unpacks a zip archive into `folder`.
fn unpack(archive: &Path, folder: &Path) -> io::Result<()> {
    if fs::metadata(archive)?.len() > MAX_ARCHIVE_BYTES {
        return Err(io::Error::other("the archive is unreasonably large"));
    }
    let mut zip = zip::ZipArchive::new(io::BufReader::new(fs::File::open(archive)?))
        .map_err(|e| io::Error::other(format!("not a usable archive: {e}")))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| io::Error::other(e.to_string()))?;
        let Some(relative) = entry_path(entry.name_raw()) else {
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
    }
    Ok(())
}

/// Puts a voicebank from a folder or a `.zip` into `root` (the program's
/// own voicebanks folder), under its name; returns what it says about
/// itself. Nothing is left behind when it is not a voicebank.
pub fn install(source: &Path, root: &Path) -> Result<VoicebankInfo, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let staging = root.join(format!(".adding-{}-{stamp}", std::process::id()));
    let result = (|| {
        let is_zip = source
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        if is_zip {
            unpack(source, &staging).map_err(|e| e.to_string())?;
        } else if source.is_dir() {
            copy_folder(source, &staging).map_err(|e| e.to_string())?;
        } else {
            return Err("choose a voicebank's folder or a .zip of one".to_owned());
        }
        let found = voicebank_root(&staging, 0)
            .ok_or_else(|| "it has no oto.ini: it is not a UTAU voicebank".to_owned())?;
        let info = read_info(&found)?;
        let base = safe_name(&info.name);
        let mut target = root.join(&base);
        let mut n = 2;
        while target.exists() {
            target = root.join(format!("{base} ({n})"));
            n += 1;
        }
        fs::rename(&found, &target).map_err(|e| e.to_string())?;
        read_info(&target)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Removes a voicebank from `root` (the program's own folder; voicebanks
/// elsewhere are refused).
pub fn remove(folder: &Path, root: &Path) -> io::Result<()> {
    let inside = folder
        .canonicalize()
        .ok()
        .zip(root.canonicalize().ok())
        .is_some_and(|(folder, root)| folder.parent() == Some(root.as_path()));
    if !inside {
        return Err(io::Error::other(
            "only voicebanks added here can be removed here",
        ));
    }
    fs::remove_dir_all(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oto_lines_are_read_as_utau_writes_them() {
        let text = "_ああいあうえあ.wav=- あ,250,120,-380,80,20\r\n\
                    _ああいあうえあ.wav=a い,900.5,100,-300,70,15\n\
                    ka.wav=,10,50,0,40,10\n\
                    broken line\n\
                    sa.wav=さ,x\n";
        let entries = parse_oto(text, "");
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].alias, "- あ");
        assert_eq!(entries[0].cutoff, -380.0);
        assert_eq!(entries[1].offset, 900.5);
        assert_eq!(entries[2].alias, "ka", "the file names the syllable");
        assert_eq!(entries[3].offset, 0.0, "unreadable numbers are 0");
        let inner = parse_oto("a.wav=あ,1,2,3,4,5", "C5");
        assert_eq!(inner[0].file, "C5/a.wav");
    }

    #[test]
    fn shift_jis_and_utf8_text_are_both_read() {
        let (sjis, _, _) = encoding_rs::SHIFT_JIS.encode("name=重音テト");
        assert_eq!(decode_text(&sjis), "name=重音テト");
        assert_eq!(decode_text("name=重音テト".as_bytes()), "name=重音テト");
        assert_eq!(
            decode_text(b"\xEF\xBB\xBFa=b"),
            "a=b",
            "without the byte-order mark"
        );
    }

    #[test]
    fn prefix_maps_give_each_key_its_affixes() {
        let map = parse_prefix_map("B7\t\t\nC4\t\t_low\nC#5\t\t↑\nDb5\thigh_\t\nbad\t\tx\n");
        assert_eq!(
            map,
            [
                (60, String::new(), "_low".to_owned()),
                (73, String::new(), "↑".to_owned()),
                (73, "high_".to_owned(), String::new()),
            ]
        );
    }
}

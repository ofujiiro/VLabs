// SPDX-License-Identifier: GPL-3.0-or-later

//! The native project file: a single file that is a ZIP container holding
//! `project.json` (the song, human-readable and versioned), the saved
//! settings of its plugins (`plugins/…`) and its audio files (`audio/…`,
//! decision 0017).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Cursor, Read, Seek, Write};
use std::path::Path;

use daw_model::Project;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zip::write::SimpleFileOptions;

use crate::atomic::write_atomically_with;

/// The version of the file format this program writes. Every version ever
/// released keeps sample files in `tests/data`, and a test proves they still
/// open.
pub const FORMAT_VERSION: u32 = 7;

/// The name of the song description inside the container.
const PROJECT_ENTRY: &str = "project.json";
/// A short note for anyone who opens the container with a ZIP tool.
const README_ENTRY: &str = "README.txt";
/// Where plugin settings are kept inside the container.
const PLUGIN_FOLDER: &str = "plugins/";
/// How `project.json` refers to a plugin's settings kept in the container.
const STATE_FILE_KEY: &str = "$state_file";
/// The largest plugin settings accepted (a damaged or hostile file must not
/// exhaust memory).
const MAX_PLUGIN_STATE: u64 = 256 * 1024 * 1024;

/// The largest `project.json` accepted (a damaged or hostile file must not
/// exhaust memory).
const MAX_PROJECT_JSON: u64 = 512 * 1024 * 1024;
/// Where audio files are kept inside the container.
const AUDIO_FOLDER: &str = "audio/";
/// The largest audio file accepted from a song file.
const MAX_AUDIO_FILE: u64 = crate::audio::MAX_FILE_BYTES;

/// Why a project could not be read or written.
#[derive(Debug)]
pub enum FileError {
    /// The file could not be read or written.
    Io(std::io::Error),
    /// The file is not a project file, or it is damaged.
    NotAProject(String),
    /// The file was saved by a newer version of the program.
    TooNew {
        /// The file's format version.
        found: u32,
    },
    /// An audio file of the song could not be found to save it: its name.
    MissingAudio(String),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::NotAProject(why) => write!(f, "this is not a usable project file ({why})"),
            Self::TooNew { found } => write!(
                f,
                "this project was saved by a newer version of {} (file format {found}; this version reads up to {FORMAT_VERSION})",
                daw_ident::PRODUCT_NAME
            ),
            Self::MissingAudio(name) => write!(
                f,
                "the sound of \"{name}\" has gone missing from the program's audio folder"
            ),
        }
    }
}

impl std::error::Error for FileError {}

impl From<std::io::Error> for FileError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// The outer layer of `project.json`.
#[derive(Serialize, Deserialize)]
struct Envelope {
    format_version: u32,
    /// The program and version that wrote the file (for people; not used).
    #[serde(default)]
    generator: String,
    project: Value,
}

/// Entries of the container: name and contents.
type Entries = Vec<(String, Vec<u8>)>;

/// The song description (`project.json`) and the plugin settings entries
/// of a project.
fn describe(project: &Project) -> Result<(Vec<u8>, Entries), FileError> {
    let mut value =
        serde_json::to_value(project).map_err(|e| FileError::NotAProject(e.to_string()))?;
    // Plugin settings move out of the description into entries of their own.
    let mut states = Vec::new();
    take_states(&mut value, &mut states)?;
    let envelope = Envelope {
        format_version: FORMAT_VERSION,
        generator: format!("{} {}", daw_ident::PRODUCT_NAME, env!("CARGO_PKG_VERSION")),
        project: value,
    };
    let json =
        serde_json::to_vec_pretty(&envelope).map_err(|e| FileError::NotAProject(e.to_string()))?;
    Ok((json, states))
}

/// Writes the container: the note, the description, the plugin settings
/// and, from `audio`, the project's audio files (stored as they are: audio
/// formats are compressed already, or barely shrink).
fn write_container<W: Write + Seek>(
    out: W,
    json: &[u8],
    states: &[(String, Vec<u8>)],
    audio: Option<(&Project, &Path)>,
) -> Result<W, FileError> {
    let mut zip = zip::ZipWriter::new(out);
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(README_ENTRY, deflated).map_err(zip_error)?;
    zip.write_all(readme().as_bytes())?;
    zip.start_file(PROJECT_ENTRY, deflated).map_err(zip_error)?;
    zip.write_all(json)?;
    for (name, bytes) in states {
        zip.start_file(name, deflated).map_err(zip_error)?;
        zip.write_all(bytes)?;
    }
    if let Some((project, folder)) = audio {
        let stored = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        let mut written = HashSet::new();
        for file in &project.audio {
            if !is_plain_name(&file.file) {
                return Err(FileError::MissingAudio(file.name.clone()));
            }
            if !written.insert(file.file.as_str()) {
                continue;
            }
            let mut source = File::open(folder.join(&file.file))
                .map_err(|_| FileError::MissingAudio(file.name.clone()))?;
            zip.start_file(format!("{AUDIO_FOLDER}{}", file.file), stored)
                .map_err(zip_error)?;
            io::copy(&mut source, &mut zip)?;
        }
    }
    zip.finish().map_err(zip_error)
}

/// The project file's bytes (without audio files).
pub fn project_to_bytes(project: &Project) -> Result<Vec<u8>, FileError> {
    let (json, states) = describe(project)?;
    let buffer = write_container(Cursor::new(Vec::new()), &json, &states, None)?;
    Ok(buffer.into_inner())
}

/// Replaces each plugin's settings in the description (`{"$state": …}`) by
/// a reference to an entry of its own (`{"$state_file": "plugins/1.state"}`),
/// collecting the entries.
fn take_states(value: &mut Value, states: &mut Vec<(String, Vec<u8>)>) -> Result<(), FileError> {
    match value {
        Value::Object(map) => {
            if map.len() == 1
                && let Some(Value::String(text)) = map.get(daw_model::STATE_KEY)
            {
                let bytes = daw_model::base64::decode(text).map_err(FileError::NotAProject)?;
                let name = format!("{PLUGIN_FOLDER}{}.state", states.len() + 1);
                states.push((name.clone(), bytes));
                map.clear();
                map.insert(STATE_FILE_KEY.to_owned(), Value::String(name));
                return Ok(());
            }
            for child in map.values_mut() {
                take_states(child, states)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                take_states(child, states)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Puts plugin settings kept in the container back into the description.
fn restore_states<R: Read + std::io::Seek>(
    value: &mut Value,
    zip: &mut zip::ZipArchive<R>,
) -> Result<(), FileError> {
    match value {
        Value::Object(map) => {
            if map.len() == 1
                && let Some(Value::String(name)) = map.get(STATE_FILE_KEY)
            {
                if !name.starts_with(PLUGIN_FOLDER) {
                    return Err(FileError::NotAProject(format!(
                        "a plugin's settings point outside the plugin folder ({name})"
                    )));
                }
                let mut entry = zip.by_name(name).map_err(|_| {
                    FileError::NotAProject(format!("a plugin's settings are missing ({name})"))
                })?;
                if entry.size() > MAX_PLUGIN_STATE {
                    return Err(FileError::NotAProject(format!(
                        "a plugin's settings are unreasonably large ({name})"
                    )));
                }
                let mut bytes = Vec::new();
                entry
                    .by_ref()
                    .take(MAX_PLUGIN_STATE + 1)
                    .read_to_end(&mut bytes)?;
                drop(entry);
                map.clear();
                map.insert(
                    daw_model::STATE_KEY.to_owned(),
                    Value::String(daw_model::base64::encode(&bytes)),
                );
                return Ok(());
            }
            for child in map.values_mut() {
                restore_states(child, zip)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                restore_states(child, zip)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn readme() -> String {
    format!(
        "This file is a {product} project.\n\n\
         It is a ZIP container. project.json describes the song in a human-readable,\n\
         versioned form; audio/ holds its recorded and imported sound, plugins/ the\n\
         saved settings of its plugins. Open it with {product} to work on it.\n",
        product = daw_ident::PRODUCT_NAME
    )
}

/// Reads a project from the file's bytes (leaving its audio files in the
/// container). Damaged or hostile input gives an error, never a crash.
pub fn project_from_bytes(bytes: &[u8]) -> Result<Project, FileError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| FileError::NotAProject(format!("not a ZIP container: {e}")))?;
    read_container(&mut zip)
}

/// Reads the song from an open container.
fn read_container<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Result<Project, FileError> {
    let mut entry = zip
        .by_name(PROJECT_ENTRY)
        .map_err(|_| FileError::NotAProject(format!("it contains no {PROJECT_ENTRY}")))?;
    if entry.size() > MAX_PROJECT_JSON {
        return Err(FileError::NotAProject(format!(
            "{PROJECT_ENTRY} is unreasonably large"
        )));
    }
    let mut json = Vec::new();
    entry
        .by_ref()
        .take(MAX_PROJECT_JSON + 1)
        .read_to_end(&mut json)?;
    drop(entry);
    if json.len() as u64 > MAX_PROJECT_JSON {
        return Err(FileError::NotAProject(format!(
            "{PROJECT_ENTRY} is unreasonably large"
        )));
    }
    let mut envelope: Envelope = serde_json::from_slice(&json)
        .map_err(|e| FileError::NotAProject(format!("{PROJECT_ENTRY} cannot be read: {e}")))?;
    restore_states(&mut envelope.project, zip)?;
    project_from_envelope(envelope)
}

/// Reads a project from the contents of `project.json`.
pub fn project_from_json(json: &[u8]) -> Result<Project, FileError> {
    let envelope: Envelope = serde_json::from_slice(json)
        .map_err(|e| FileError::NotAProject(format!("{PROJECT_ENTRY} cannot be read: {e}")))?;
    project_from_envelope(envelope)
}

fn project_from_envelope(envelope: Envelope) -> Result<Project, FileError> {
    if envelope.format_version > FORMAT_VERSION {
        return Err(FileError::TooNew {
            found: envelope.format_version,
        });
    }
    let value = migrate(envelope.project, envelope.format_version)?;
    let mut project: Project = serde_json::from_value(value)
        .map_err(|e| FileError::NotAProject(format!("the song description is incomplete: {e}")))?;
    repair(&mut project);
    Ok(project)
}

/// Brings a project written in an older format up to the current one, one
/// version at a time.
fn migrate(value: Value, from: u32) -> Result<Value, FileError> {
    if from == 0 {
        return Err(FileError::NotAProject(
            "format version 0 does not exist".to_owned(),
        ));
    }
    // Version 2 added drum kits, effect chains, the master bus and the
    // owner track of each pattern. Every new field has a default, and
    // owners are filled in by `repair`, so version 1 needs no rewriting;
    // the version number changed so that older copies of the program
    // refuse files whose new parts they would silently drop.
    // Version 3 added plugin instruments and effects (with their settings
    // in the container) and automation lanes; again every new field has a
    // default, so older versions need no rewriting.
    // Version 4 added audio tracks, audio clips and the audio files (in the
    // container); every new field has a default again.
    // Version 5 added the song's key changes, each pattern's swing and tempo
    // glides (decision 0018); every new field has a default again.
    // Version 6 added the built-in synth instrument (decision 0019); older
    // files have none, so they need no rewriting.
    // Version 7 added SFZ instruments (decision 0024); older files have
    // none, so they need no rewriting either.
    // Each future format change adds a step here: `8 => migrate_7_to_8(value)`.
    Ok(value)
}

/// Makes a loaded project internally consistent, whatever wrote it: notes
/// sorted, identifiers unique from now on.
fn repair(project: &mut Project) {
    // The maps over the song always start at its beginning, whatever the
    // file said.
    project.tempo.repair();
    project.meter.repair();
    project.key.repair();
    for pattern in &mut project.patterns {
        std::sync::Arc::make_mut(pattern).sort_notes();
    }
    // The synth's numbers stay in their ranges, whatever wrote them.
    for track in &mut project.tracks {
        if let daw_model::Instrument::Synth(settings) = &track.instrument {
            let repaired = settings.repaired();
            if repaired != *settings {
                std::sync::Arc::make_mut(track).instrument = daw_model::Instrument::Synth(repaired);
            }
        }
    }
    // A pattern without an owner belongs to the track that plays it.
    let owners: Vec<(daw_model::PatternId, daw_model::TrackId)> = project
        .tracks
        .iter()
        .flat_map(|t| {
            t.clips
                .iter()
                .filter_map(move |c| c.pattern().map(|p| (p, t.id)))
        })
        .collect();
    for pattern in &mut project.patterns {
        if pattern.track.is_none()
            && let Some((_, track)) = owners.iter().find(|(p, _)| *p == pattern.id)
        {
            std::sync::Arc::make_mut(pattern).track = Some(*track);
        }
    }
    project.repair_id_counter();
    // An audio file's stored name is a plain file name, so saving the song
    // later can never read a file outside the song's audio folder.
    for file in &mut project.audio {
        if !is_plain_name(&file.file) {
            file.file = format!("missing-{}.bin", file.id);
        }
    }
}

/// Saves a project, safely: the new file is complete on disk before it
/// replaces the old one, so a crash during saving never leaves a damaged
/// project behind. The project's audio files are copied in from `audio`, the
/// folder that holds them while the song is open; with `None` they are left
/// out (an autosave copy, whose audio stays in that folder).
pub fn save_project(project: &Project, path: &Path, audio: Option<&Path>) -> Result<(), FileError> {
    let (json, states) = describe(project)?;
    if let Some(folder) = audio
        && let Some(missing) = project
            .audio
            .iter()
            .find(|f| !is_plain_name(&f.file) || !folder.join(&f.file).is_file())
    {
        return Err(FileError::MissingAudio(missing.name.clone()));
    }
    let mut failure = None;
    let written = write_atomically_with(path, |file| {
        let result = write_container(
            BufWriter::new(file),
            &json,
            &states,
            audio.map(|a| (project, a)),
        )
        .and_then(|mut out| out.flush().map_err(FileError::Io));
        match result {
            Ok(()) => Ok(()),
            Err(FileError::Io(error)) => Err(error),
            Err(other) => {
                failure = Some(other);
                Err(io::Error::other("the song could not be written"))
            }
        }
    });
    match (written, failure) {
        (Ok(()), _) => Ok(()),
        (Err(_), Some(failure)) => Err(failure),
        (Err(error), None) => Err(FileError::Io(error)),
    }
}

/// Opens a project file. Its audio files are copied into `audio`, the folder
/// that holds them while the song is open (under a new name where a name is
/// taken there already, the song changed to match, so nothing there is ever
/// overwritten); with `None` they stay in the file. An audio file missing
/// from the file leaves its clips silent; the song still opens.
pub fn load_project(path: &Path, audio: Option<&Path>) -> Result<Project, FileError> {
    let file = File::open(path)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| FileError::NotAProject(format!("not a ZIP container: {e}")))?;
    let mut project = read_container(&mut zip)?;
    if let Some(folder) = audio {
        extract_audio(&mut zip, &mut project, folder)?;
    }
    Ok(project)
}

fn extract_audio<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    project: &mut Project,
    folder: &Path,
) -> Result<(), FileError> {
    std::fs::create_dir_all(folder)?;
    // An entry several audio files share is extracted once.
    let mut extracted: HashMap<String, String> = HashMap::new();
    for file in &mut project.audio {
        if let Some(name) = extracted.get(&file.file) {
            file.file.clone_from(name);
            continue;
        }
        let Ok(mut entry) = zip.by_name(&format!("{AUDIO_FOLDER}{}", file.file)) else {
            continue;
        };
        if entry.size() > MAX_AUDIO_FILE {
            return Err(FileError::NotAProject(format!(
                "an audio file is unreasonably large ({})",
                file.name
            )));
        }
        let name = free_name(folder, &file.file);
        let temporary = folder.join(format!(".{name}.partial"));
        let copied = (|| {
            let mut out = File::create(&temporary)?;
            let bytes = io::copy(&mut entry.by_ref().take(MAX_AUDIO_FILE + 1), &mut out)?;
            if bytes > MAX_AUDIO_FILE {
                return Err(io::Error::other("an audio file is unreasonably large"));
            }
            drop(out);
            std::fs::rename(&temporary, folder.join(&name))
        })();
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&temporary);
            return Err(FileError::Io(error));
        }
        extracted.insert(std::mem::take(&mut file.file), name.clone());
        file.file = name;
    }
    Ok(())
}

/// Copies a project's audio files from one folder into another (from the
/// audio folder of a copy of the program that crashed into this one's),
/// under new names where a name is taken already, the project changed to
/// match. Files missing from `from` are left out; their clips stay silent.
pub fn copy_audio(project: &mut Project, from: &Path, into: &Path) -> io::Result<()> {
    std::fs::create_dir_all(into)?;
    let mut moved: HashMap<String, String> = HashMap::new();
    for file in &mut project.audio {
        if let Some(name) = moved.get(&file.file) {
            file.file.clone_from(name);
            continue;
        }
        if !is_plain_name(&file.file) {
            continue;
        }
        let source = from.join(&file.file);
        if !source.is_file() {
            continue;
        }
        let name = free_name(into, &file.file);
        std::fs::copy(&source, into.join(&name))?;
        moved.insert(std::mem::take(&mut file.file), name.clone());
        file.file = name;
    }
    Ok(())
}

/// A plain file name: letters, digits, dots, dashes and underscores, not
/// starting with a dot, so it can never point outside its folder.
pub fn is_plain_name(name: &str) -> bool {
    (1..=120).contains(&name.len())
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// A name for `wanted` in `folder` that nothing there has: `wanted` itself
/// if it is a plain name and free, else a numbered one with its ending.
pub fn free_name(folder: &Path, wanted: &str) -> String {
    let plain = is_plain_name(wanted);
    if plain && !folder.join(wanted).exists() {
        return wanted.to_owned();
    }
    let path = Path::new(wanted);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| (1..=5).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("bin");
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|_| plain)
        .unwrap_or("audio");
    (2_u64..=u64::from(u32::MAX))
        .map(|n| format!("{stem}-{n}.{extension}"))
        .find(|candidate| !folder.join(candidate).exists())
        .unwrap_or_else(|| format!("{stem}.{extension}"))
}

#[allow(clippy::needless_pass_by_value, reason = "used with map_err")]
fn zip_error(error: zip::result::ZipError) -> FileError {
    FileError::Io(std::io::Error::other(error.to_string()))
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test values"
)]
mod tests {
    use super::*;
    use daw_model::{DEFAULT_VELOCITY, Note, NoteId, Ticks};

    fn sample_project() -> Project {
        let mut project = Project::starter("Pattern 1", "Piano");
        project.title = "Test song".to_owned();
        project.tempo.set(Ticks::quarters(8), 96.5);
        let (_, pattern) = project.first_pattern().unwrap();
        let triplet = Ticks::note_value(1, 12);
        for i in 0..6 {
            project.add_note(
                pattern,
                Note {
                    id: NoteId(0),
                    start: triplet * i,
                    length: triplet,
                    key: 60 + i as u8,
                    velocity: DEFAULT_VELOCITY,
                    pitch_offset: 0.0,
                },
            );
        }
        project
    }

    #[test]
    fn a_project_survives_saving_and_loading() {
        let project = sample_project();
        let bytes = project_to_bytes(&project).unwrap();
        let loaded = project_from_bytes(&bytes).unwrap();
        assert_eq!(loaded, project);
    }

    #[test]
    fn the_description_is_human_readable_json() {
        let bytes = project_to_bytes(&sample_project()).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut text = String::new();
        zip.by_name(PROJECT_ENTRY)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(text.contains(&format!("\"format_version\": {FORMAT_VERSION}")));
        assert!(text.contains("\"title\": \"Test song\""));
        assert!(text.contains('\n'), "pretty-printed");
        assert!(!text.contains("pitch_offset"), "zero offsets are left out");
    }

    #[test]
    fn damaged_files_give_errors_not_crashes() {
        assert!(matches!(
            project_from_bytes(b"not a zip"),
            Err(FileError::NotAProject(_))
        ));
        let mut bytes = project_to_bytes(&sample_project()).unwrap();
        let middle = bytes.len() / 2;
        bytes.truncate(middle);
        assert!(project_from_bytes(&bytes).is_err());
        assert!(matches!(
            project_from_json(b"{\"format_version\": 1}"),
            Err(FileError::NotAProject(_))
        ));
        assert!(matches!(
            project_from_json(b"[1, 2"),
            Err(FileError::NotAProject(_))
        ));
    }

    #[test]
    fn files_from_newer_versions_are_refused_clearly() {
        let json = br#"{"format_version": 999, "generator": "future", "project": {}}"#;
        let error = project_from_json(json).unwrap_err();
        assert!(matches!(error, FileError::TooNew { found: 999 }));
        assert!(error.to_string().contains("newer version"));
    }

    #[test]
    fn unsorted_notes_and_low_id_counters_are_repaired() {
        let project = sample_project();
        let mut value = serde_json::to_value(&project).unwrap();
        // Reverse the notes and reset the id counter, as a hand-edited file might.
        value["patterns"][0]["notes"]
            .as_array_mut()
            .unwrap()
            .reverse();
        value["next_id"] = serde_json::json!(1);
        let json = serde_json::to_vec(&serde_json::json!({"format_version": 1, "project": value}))
            .unwrap();
        let mut loaded = project_from_json(&json).unwrap();
        let notes = loaded.patterns[0].notes.clone();
        assert!(notes.windows(2).all(|w| w[0].start <= w[1].start));
        let fresh = loaded.allocate_id();
        assert!(notes.iter().all(|n| n.id.0 < fresh));
    }

    /// A folder of its own for one test.
    fn folder(name: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("daw-formats-native-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// A song with an audio track whose clip plays `take.wav`.
    fn song_with_audio() -> Project {
        let mut project = sample_project();
        let track = project.add_audio_track("Voice");
        let audio = project.add_audio_file(daw_model::AudioFile {
            id: daw_model::AudioId(0),
            name: "Take 1".to_owned(),
            file: "take.wav".to_owned(),
            frames: 4,
            sample_rate: 48_000,
            channels: 1,
        });
        project.add_audio_clip(track, Ticks::WHOLE, Ticks::QUARTER, audio, 0);
        project
    }

    #[test]
    fn audio_files_travel_inside_the_song_file() {
        let (open, other) = (folder("open"), folder("other"));
        std::fs::write(open.join("take.wav"), b"the sound").unwrap();
        let project = song_with_audio();
        let path = open.join("song.bin");
        save_project(&project, &path, Some(&open)).unwrap();

        let loaded = load_project(&path, Some(&other)).unwrap();
        assert_eq!(loaded, project, "the song comes back as it was");
        assert_eq!(std::fs::read(other.join("take.wav")).unwrap(), b"the sound");

        // Opened again into the same folder: the name is taken, so the
        // sound gets a new one and the song follows; nothing is overwritten.
        std::fs::write(other.join("take.wav"), b"other sound").unwrap();
        let again = load_project(&path, Some(&other)).unwrap();
        assert_eq!(again.audio[0].file, "take-2.wav");
        assert_eq!(
            std::fs::read(other.join("take-2.wav")).unwrap(),
            b"the sound"
        );
        assert_eq!(
            std::fs::read(other.join("take.wav")).unwrap(),
            b"other sound"
        );

        // Copying the files of a recovered song into another folder.
        let mut recovered = again.clone();
        let moved_into = folder("moved");
        copy_audio(&mut recovered, &other, &moved_into).unwrap();
        assert_eq!(recovered.audio[0].file, "take-2.wav");
        assert!(moved_into.join("take-2.wav").is_file());
        assert!(other.join("take-2.wav").is_file(), "copied, not moved");
        for path in [open, other, moved_into] {
            std::fs::remove_dir_all(path).ok();
        }
    }

    #[test]
    fn a_song_whose_sound_is_missing_is_not_saved_without_it() {
        let open = folder("missing");
        let error =
            save_project(&song_with_audio(), &open.join("song.bin"), Some(&open)).unwrap_err();
        assert!(matches!(&error, FileError::MissingAudio(name) if name == "Take 1"));
        assert!(!open.join("song.bin").exists(), "nothing written");
        // An autosave copy leaves the sound in its folder.
        save_project(&song_with_audio(), &open.join("copy.bin"), None).unwrap();
        let bytes = std::fs::read(open.join("copy.bin")).unwrap();
        let zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(
            zip.file_names()
                .all(|name| !name.unwrap().starts_with(AUDIO_FOLDER))
        );
        std::fs::remove_dir_all(open).ok();
    }

    #[test]
    fn audio_names_never_point_outside_the_audio_folder() {
        let mut project = song_with_audio();
        project.audio[0].file = "../../secret.txt".to_owned();
        let bytes = project_to_bytes(&project).unwrap();
        let loaded = project_from_bytes(&bytes).unwrap();
        assert!(
            is_plain_name(&loaded.audio[0].file),
            "{}",
            loaded.audio[0].file
        );
        let open = folder("hostile");
        assert!(matches!(
            save_project(&project, &open.join("song.bin"), Some(&open)),
            Err(FileError::MissingAudio(_))
        ));
        assert_eq!(free_name(&open, "../x.wav"), "audio-2.wav");
        assert_eq!(free_name(&open, "fine-name_1.flac"), "fine-name_1.flac");
        std::fs::remove_dir_all(open).ok();
    }
}

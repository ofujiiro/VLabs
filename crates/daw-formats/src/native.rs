// SPDX-License-Identifier: GPL-3.0-or-later

//! The native project file: a single file that is a ZIP container holding
//! `project.json` (the song, human-readable and versioned), the saved
//! settings of its plugins (`plugins/…`), and, in a later phase, its audio
//! files.

use std::fmt;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use daw_model::Project;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zip::write::SimpleFileOptions;

use crate::atomic::write_atomically;

/// The version of the file format this program writes. Every version ever
/// released keeps sample files in `tests/data`, and a test proves they still
/// open.
pub const FORMAT_VERSION: u32 = 3;

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

/// The project file's bytes.
pub fn project_to_bytes(project: &Project) -> Result<Vec<u8>, FileError> {
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

    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(README_ENTRY, options).map_err(zip_error)?;
        zip.write_all(readme().as_bytes())?;
        zip.start_file(PROJECT_ENTRY, options).map_err(zip_error)?;
        zip.write_all(&json)?;
        for (name, bytes) in &states {
            zip.start_file(name, options).map_err(zip_error)?;
            zip.write_all(bytes)?;
        }
        zip.finish().map_err(zip_error)?;
    }
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
         versioned form; later versions add the project's audio files and the saved\n\
         state of its plugins. Open it with {product} to work on it.\n",
        product = daw_ident::PRODUCT_NAME
    )
}

/// Reads a project from the file's bytes. Damaged or hostile input gives an
/// error, never a crash.
pub fn project_from_bytes(bytes: &[u8]) -> Result<Project, FileError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| FileError::NotAProject(format!("not a ZIP container: {e}")))?;
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
    restore_states(&mut envelope.project, &mut zip)?;
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
    // Each future format change adds a step here: `4 => migrate_3_to_4(value)`.
    Ok(value)
}

/// Makes a loaded project internally consistent, whatever wrote it: notes
/// sorted, identifiers unique from now on.
fn repair(project: &mut Project) {
    for pattern in &mut project.patterns {
        std::sync::Arc::make_mut(pattern).sort_notes();
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
}

/// Saves a project, safely: the new file is complete on disk before it
/// replaces the old one, so a crash during saving never leaves a damaged
/// project behind.
pub fn save_project(project: &Project, path: &Path) -> Result<(), FileError> {
    let bytes = project_to_bytes(project)?;
    write_atomically(path, &bytes)?;
    Ok(())
}

/// Opens a project file.
pub fn load_project(path: &Path) -> Result<Project, FileError> {
    let bytes = std::fs::read(path)?;
    project_from_bytes(&bytes)
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
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The native project file: a single file that is a ZIP container holding
//! `project.json` (the song, human-readable and versioned) and, in later
//! phases, the audio files and saved plugin states.

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
pub const FORMAT_VERSION: u32 = 1;

/// The name of the song description inside the container.
const PROJECT_ENTRY: &str = "project.json";
/// A short note for anyone who opens the container with a ZIP tool.
const README_ENTRY: &str = "README.txt";

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
    let envelope = Envelope {
        format_version: FORMAT_VERSION,
        generator: format!("{} {}", daw_ident::PRODUCT_NAME, env!("CARGO_PKG_VERSION")),
        project: serde_json::to_value(project)
            .map_err(|e| FileError::NotAProject(e.to_string()))?,
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
        zip.finish().map_err(zip_error)?;
    }
    Ok(buffer.into_inner())
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
    if json.len() as u64 > MAX_PROJECT_JSON {
        return Err(FileError::NotAProject(format!(
            "{PROJECT_ENTRY} is unreasonably large"
        )));
    }
    project_from_json(&json)
}

/// Reads a project from the contents of `project.json`.
pub fn project_from_json(json: &[u8]) -> Result<Project, FileError> {
    let envelope: Envelope = serde_json::from_slice(json)
        .map_err(|e| FileError::NotAProject(format!("{PROJECT_ENTRY} cannot be read: {e}")))?;
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
    // Each future format change adds a step here: `2 => migrate_1_to_2(value)`.
    // Version 1 is the first, so there is nothing to do yet.
    Ok(value)
}

/// Makes a loaded project internally consistent, whatever wrote it: notes
/// sorted, identifiers unique from now on.
fn repair(project: &mut Project) {
    for pattern in &mut project.patterns {
        std::sync::Arc::make_mut(pattern).sort_notes();
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
        assert!(text.contains("\"format_version\": 1"));
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

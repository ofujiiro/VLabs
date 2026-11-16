// SPDX-License-Identifier: GPL-3.0-or-later

//! Every version of the project format ever released keeps sample files in
//! `tests/data`, and this test proves the current code still opens them.
//! When the format changes: bump `FORMAT_VERSION`, add a migration step, and
//! add a sample file for the new version (run this test with
//! `WRITE_FORMAT_SAMPLE=1` to write one from the current code).

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test code"
)]

use std::path::PathBuf;

use daw_formats::{FORMAT_VERSION, load_project, save_project};
use daw_model::{DEFAULT_VELOCITY, Note, NoteId, Project, Ticks};

fn data_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
}

/// The project every sample file contains.
fn reference_project() -> Project {
    let mut project = Project::starter("Pattern 1", "Piano");
    "Format sample".clone_into(&mut project.title);
    project.tempo.set(Ticks::quarters(8), 90.0);
    let (_, pattern) = project.first_pattern().unwrap();
    let triplet = Ticks::note_value(1, 12);
    for (i, key) in [60_u8, 64, 67, 72].iter().enumerate() {
        project.add_note(
            pattern,
            Note {
                id: NoteId(0),
                start: triplet * i as i64,
                length: triplet,
                key: *key,
                velocity: DEFAULT_VELOCITY,
                pitch_offset: 0.0,
            },
        );
    }
    project
}

#[test]
fn every_released_format_version_still_opens() {
    let sample = data_folder().join(format!("format-v{FORMAT_VERSION}.project-sample"));
    if std::env::var_os("WRITE_FORMAT_SAMPLE").is_some() && !sample.exists() {
        save_project(&reference_project(), &sample).unwrap();
    }
    let mut found = 0;
    for entry in std::fs::read_dir(data_folder()).unwrap().flatten() {
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".project-sample") {
            continue;
        }
        found += 1;
        let project = load_project(&path)
            .unwrap_or_else(|e| panic!("{} no longer opens: {e}", path.display()));
        assert_eq!(project.title, "Format sample", "{}", path.display());
        assert_eq!(
            project.tempo.bpm_at(Ticks::quarters(9)),
            90.0,
            "{}",
            path.display()
        );
        let (_, pattern) = project.first_pattern().unwrap();
        let keys: Vec<u8> = project
            .pattern(pattern)
            .unwrap()
            .notes
            .iter()
            .map(|n| n.key)
            .collect();
        assert_eq!(keys, [60, 64, 67, 72], "{}", path.display());
        assert_eq!(
            project.pattern(pattern).unwrap().notes[1].start,
            Ticks::note_value(1, 12),
            "triplets stay exact"
        );
    }
    assert!(
        found >= FORMAT_VERSION as usize,
        "a sample file is missing for some format version (found {found})"
    );
}

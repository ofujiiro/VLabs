// SPDX-License-Identifier: GPL-3.0-or-later

//! Songs in and out (decision 0020): opening another program's song
//! (DAWproject, MIDI), importing MIDI files into the song, exporting a
//! DAWproject or a MIDI file.

use std::path::{Path, PathBuf};

use daw_formats::dawproject::{self, ImportNames};
use daw_formats::midi::{self, ImportedTrack};
use daw_model::{Instrument, Ticks};

use super::Session;
use crate::text;

/// The kinds of song file that open as a song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongFile {
    /// A song of this program.
    Native,
    /// A DAWproject file from another program.
    DawProject,
    /// A Standard MIDI File.
    Midi,
}

impl SongFile {
    /// The kind of song a file is, by its ending.
    pub fn of(path: &Path) -> Option<Self> {
        let ending = path.extension()?.to_str()?.to_ascii_lowercase();
        match ending.as_str() {
            e if e == daw_ident::PROJECT_FILE_EXTENSION => Some(Self::Native),
            "dawproject" => Some(Self::DawProject),
            "mid" | "midi" | "smf" | "kar" | "rmi" => Some(Self::Midi),
            _ => None,
        }
    }
}

/// A name for an imported MIDI track: the file's own, else its
/// instrument's.
fn midi_track_name(track: &ImportedTrack) -> String {
    track.name.clone().unwrap_or_else(|| {
        match &track.instrument {
            Instrument::Drums { .. } => text::DRUMS_TRACK,
            Instrument::Synth(_) => text::SYNTH_TRACK,
            _ => text::FIRST_TRACK,
        }
        .to_owned()
    })
}

/// The file's name, for messages.
fn shown(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl Session {
    /// Opens any song file: a song of this program, or another program's song as a
    /// new, unsaved song (Save then asks where, so the file is never
    /// overwritten).
    pub fn open_any(&mut self, path: &Path) -> Result<(), String> {
        match SongFile::of(path) {
            Some(SongFile::DawProject) => self.open_dawproject(path),
            Some(SongFile::Midi) => self.open_midi(path),
            Some(SongFile::Native) | None => self.open(path),
        }
    }

    fn open_dawproject(&mut self, path: &Path) -> Result<(), String> {
        let number = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let names = ImportNames {
            track: &|n| text::numbered(text::TRACK, number(n)),
            pattern: &|n| text::pattern_name(number(n)),
        };
        match dawproject::import(path, self.audio.folder(), &names) {
            Ok(imported) => {
                let mut project = imported.project;
                if project.title.trim().is_empty() {
                    project.title = file_stem(path);
                }
                self.replace(project, None, false);
                self.set_status(text::opened_other(&shown(path), &imported.left_out));
                Ok(())
            }
            Err(error) => {
                let message = text::could_not_open(&error.to_string());
                self.set_status(message.clone());
                Err(message)
            }
        }
    }

    fn open_midi(&mut self, path: &Path) -> Result<(), String> {
        let result = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| midi::import(&bytes).map_err(|e| e.to_string()));
        match result {
            Ok(song) => {
                let mut project = song.into_project(midi_track_name, &text::pattern_name(1));
                if project.title.trim().is_empty() {
                    project.title = file_stem(path);
                }
                self.replace(project, None, false);
                self.set_status(text::opened_other(&shown(path), &[]));
                Ok(())
            }
            Err(error) => {
                let message = text::could_not_open(&error);
                self.set_status(message.clone());
                Err(message)
            }
        }
    }

    /// Imports files into the song: sound files as audio clips, MIDI files
    /// as new tracks of notes, each at `at` (sound on `track` if it is an
    /// audio track). Song files are not imported (they open).
    pub fn import_files(
        &mut self,
        files: Vec<PathBuf>,
        track: Option<daw_model::TrackId>,
        at: Ticks,
    ) {
        let (midi_files, others): (Vec<PathBuf>, Vec<PathBuf>) = files
            .into_iter()
            .filter(|f| {
                !matches!(
                    SongFile::of(f),
                    Some(SongFile::Native | SongFile::DawProject)
                )
            })
            .partition(|f| SongFile::of(f) == Some(SongFile::Midi));
        for file in midi_files {
            self.import_midi(&file, at);
        }
        if !others.is_empty() {
            self.import_audio(others, track, at);
        }
    }

    /// Adds a MIDI file's tracks to the song at `at` (one undo step); the
    /// song keeps its tempo.
    pub fn import_midi(&mut self, path: &Path, at: Ticks) {
        let song = match std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| midi::import(&bytes).map_err(|e| e.to_string()))
        {
            Ok(song) => song,
            Err(error) => {
                self.set_status(text::could_not_import(&shown(path), &error));
                return;
            }
        };
        if song.tracks.is_empty() {
            self.set_status(text::no_notes_in(&shown(path)));
            return;
        }
        let pattern = text::pattern_name(1);
        let added = self.doc.edit(text::EDIT_IMPORT_MIDI, |p| {
            song.add_to(p, at, midi_track_name, &pattern)
        });
        if let Some(&first) = added.first() {
            self.focus_track(first);
        }
        self.set_status(text::imported_midi(&shown(path), added.len()));
    }

    /// Writes the song as a DAWproject file (its sound and plugin
    /// settings inside).
    pub fn export_dawproject(&mut self, path: &Path) -> Result<PathBuf, String> {
        let path = crate::export::ExportTarget::DawProject.with_extension(path);
        self.capture_plugin_states();
        let result = dawproject::export(self.project(), self.audio.folder(), &path);
        self.report_export(result.map_err(|e| e.to_string()), path)
    }

    /// Writes the song's notes and tempo as a MIDI file.
    pub fn export_midi(&mut self, path: &Path) -> Result<PathBuf, String> {
        let path = crate::export::ExportTarget::Midi.with_extension(path);
        let bytes = midi::write(&midi::song_to_midi(self.project()));
        let result = daw_formats::write_atomically(&path, &bytes).map_err(|e| e.to_string());
        self.report_export(result, path)
    }

    fn report_export(
        &mut self,
        result: Result<(), String>,
        path: PathBuf,
    ) -> Result<PathBuf, String> {
        match result {
            Ok(()) => {
                self.set_status(text::exported(&path.display().to_string()));
                Ok(path)
            }
            Err(error) => {
                let message = text::could_not_export(&error);
                self.set_status(message.clone());
                Err(message)
            }
        }
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

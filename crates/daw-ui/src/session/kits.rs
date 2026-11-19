// SPDX-License-Identifier: GPL-3.0-or-later

//! Custom kits (decision 0025): drum kits of the person's own sounds, each
//! on its own key, played from the step grid. Their sounds are brought into
//! the song as imported audio is, then become pads.

use std::path::PathBuf;

use daw_formats::library::ItemKind;
use daw_model::{AudioFile, Instrument, KIT_MAX_PADS, KitPad, Ticks, TrackId};

use super::Session;
use super::audio_store::ImportPlace;
use crate::text;

impl Session {
    /// Adds an empty custom kit (named after `name`, or "My kit") with a
    /// one-bar beat over the song, puts it in focus and opens its beat.
    pub fn add_kit_track(&mut self, name: Option<&str>) -> TrackId {
        let name = self.unique_track_name(name.unwrap_or(text::KIT_TRACK));
        let song = self.song_length();
        let track = self.doc.edit(text::EDIT_ADD_KIT, |p| {
            let track = p.add_track(&name, Instrument::Kit { pads: Vec::new() });
            let bar = p.meter.initial().bar_length();
            let pattern = p.add_pattern_for(track, &text::beat_name(1), bar);
            p.add_clip(track, Ticks::ZERO, song, pattern);
            track
        });
        self.open_track(track);
        track
    }

    /// Whether a track is a custom kit.
    pub fn is_custom_kit(&self, track: TrackId) -> bool {
        self.project()
            .track(track)
            .is_some_and(|t| t.instrument.is_custom_kit())
    }

    /// A custom kit's pads, by key (none for other tracks).
    pub fn kit_pads(&self, track: TrackId) -> &[KitPad] {
        self.project()
            .track(track)
            .map_or(&[], |t| t.instrument.kit_pads())
    }

    /// Adds sounds to a custom kit: each is copied into the song and read
    /// in the background, then becomes a pad on its own key (one undo step
    /// each). Files that are not sounds are skipped, and a kit takes at most
    /// 128 sounds. Gives how many are on their way.
    pub fn add_kit_sounds(&mut self, track: TrackId, files: Vec<PathBuf>) -> usize {
        if !self.is_custom_kit(track) {
            return 0;
        }
        let room = KIT_MAX_PADS.saturating_sub(self.kit_pads(track).len());
        let sounds: Vec<PathBuf> = files
            .into_iter()
            .filter(|f| ItemKind::of(f) == Some(ItemKind::Sound))
            .collect();
        let name = self
            .project()
            .track(track)
            .map_or_else(String::new, |t| t.name.clone());
        if sounds.len() > room {
            self.set_status(text::kit_full(&name));
        }
        let count = sounds.len().min(room);
        for file in sounds.into_iter().take(room) {
            self.audio.import(
                file,
                ImportPlace {
                    track: None,
                    at: Ticks::ZERO,
                    pad_of: Some(track),
                },
            );
        }
        if count > 0 {
            self.set_status(text::adding_kit_sounds(count, &name));
        }
        count
    }

    /// Makes a custom kit of sounds (a folder of the library, say): a new
    /// kit named `name`, each sound a pad.
    pub fn make_kit(&mut self, name: &str, files: Vec<PathBuf>) -> TrackId {
        let track = self.add_kit_track(Some(name));
        self.add_kit_sounds(track, files);
        track
    }

    /// Removes a pad from a custom kit, with the hits on its row
    /// (undoable).
    pub fn remove_kit_pad(&mut self, track: TrackId, key: u8) {
        if !self.kit_pads(track).iter().any(|p| p.setting.key == key) {
            return;
        }
        self.doc
            .edit(text::EDIT_REMOVE_PAD, |p| p.remove_kit_pad(track, key));
    }

    /// A sound brought in for a custom kit: it becomes a pad (one undo
    /// step), unless the kit is gone or full.
    pub(super) fn add_imported_pad(&mut self, track: TrackId, file: &AudioFile, shown: &str) {
        let name = self
            .project()
            .track(track)
            .filter(|t| t.instrument.is_custom_kit())
            .map(|t| t.name.clone());
        let Some(kit) = name else {
            return; // The kit was removed meanwhile.
        };
        let added = self.doc.edit(text::EDIT_ADD_PAD, |p| {
            let audio = p.add_audio_file(file.clone());
            let key = p.add_kit_pad(track, &file.name, audio);
            if key.is_none() {
                p.prune_audio();
            }
            key
        });
        match added {
            Some(_) => self.set_status(text::added_to_kit(shown, &kit)),
            None => self.set_status(text::kit_full(&kit)),
        }
    }
}

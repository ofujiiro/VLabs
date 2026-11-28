// SPDX-License-Identifier: GPL-3.0-or-later

//! What the program remembers between sessions. The application shell
//! reads and writes the file; this module says what is in it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::keys::LayoutFamily;

/// Light, dark, or whatever the system uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    /// Follow the system setting.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// How the keyboard's labels are chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelChoice {
    /// Guess from the system, then learn from key presses.
    #[default]
    Automatic,
    /// A fixed layout family.
    Fixed(LayoutFamily),
}

/// The most recent files kept in the menu.
pub const RECENT_FILES: usize = 8;

/// The most folders kept in each folder list (plugins, sound library).
pub const MAX_FOLDERS: usize = 64;

/// Everything remembered between sessions. Missing or unreadable fields
/// take their defaults, so old and damaged settings files still load.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools, reason = "independent preferences")]
pub struct Settings {
    /// Light or dark.
    pub theme: ThemeChoice,
    /// Click while playing.
    pub metronome: bool,
    /// Click while recording.
    pub metronome_when_recording: bool,
    /// Click volume, 0 to 1.
    pub metronome_volume: f32,
    /// Bars of clicks before recording starts.
    pub count_in_bars: u32,
    /// Move recorded notes onto the grid.
    pub snap_recording: bool,
    /// Clips in the song land on bars and beats; off, anywhere (decision
    /// 0035).
    pub snap_clips: bool,
    /// The note the computer keyboard's lowest key (Z) plays.
    pub keyboard_base: u8,
    /// How hard computer-keyboard notes play, 0 to 1.
    pub keyboard_velocity: f32,
    /// Whether the on-screen keyboard is shown.
    pub show_keyboard: bool,
    /// Whether the mixer is shown (in place of the keyboard).
    pub show_mixer: bool,
    /// How the on-screen keyboard is labelled.
    pub key_labels: LabelChoice,
    /// Shortcuts changed from the defaults (see [`crate::Keymap`]).
    pub shortcuts: BTreeMap<String, Vec<String>>,
    /// Recently opened or saved songs, newest first.
    pub recent_files: Vec<PathBuf>,
    /// Folders searched for plugins besides the usual ones (decision 0023).
    pub plugin_folders: Vec<PathBuf>,
    /// The sound library's folders (decision 0023).
    pub sound_folders: Vec<PathBuf>,
    /// Whether the library is shown (in place of the keyboard).
    pub show_library: bool,
    /// The MIDI controllers' buttons and pads that run commands (decision
    /// 0033), by command.
    pub midi_buttons: BTreeMap<String, daw_model::MidiControl>,
    /// The MIDI controllers switched off, by name (decision 0033).
    pub midi_off: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            metronome: false,
            metronome_when_recording: true,
            metronome_volume: 0.8,
            count_in_bars: 1,
            snap_recording: false,
            snap_clips: true,
            keyboard_base: 48,
            keyboard_velocity: 0.8,
            show_keyboard: true,
            show_mixer: false,
            key_labels: LabelChoice::Automatic,
            shortcuts: BTreeMap::new(),
            recent_files: Vec::new(),
            plugin_folders: Vec::new(),
            sound_folders: Vec::new(),
            show_library: false,
            midi_buttons: BTreeMap::new(),
            midi_off: Vec::new(),
        }
    }
}

impl Settings {
    /// Reads settings from JSON; anything unreadable gives the defaults.
    pub fn from_json(json: &str) -> Self {
        let mut settings: Self = serde_json::from_str(json).unwrap_or_default();
        settings.sanitize();
        settings
    }

    /// The settings as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Puts every value back into its allowed range.
    pub fn sanitize(&mut self) {
        if !self.metronome_volume.is_finite() {
            self.metronome_volume = 0.8;
        }
        self.metronome_volume = self.metronome_volume.clamp(0.0, 1.0);
        self.count_in_bars = self.count_in_bars.min(4);
        self.keyboard_base = clamp_base(self.keyboard_base);
        if !self.keyboard_velocity.is_finite() {
            self.keyboard_velocity = 0.8;
        }
        self.keyboard_velocity = self.keyboard_velocity.clamp(0.05, 1.0);
        self.recent_files.truncate(RECENT_FILES);
        for folders in [&mut self.plugin_folders, &mut self.sound_folders] {
            let mut seen = std::collections::BTreeSet::new();
            folders.retain(|f| f.is_absolute() && seen.insert(f.clone()));
            folders.truncate(MAX_FOLDERS);
        }
        // One command per button, commands this version knows.
        let mut taken = std::collections::BTreeSet::new();
        self.midi_buttons.retain(|id, control| {
            *control = control.sane();
            crate::keys::Command::from_id(id).is_some() && taken.insert(*control)
        });
        let mut seen = std::collections::BTreeSet::new();
        self.midi_off
            .retain(|name| !name.is_empty() && seen.insert(name.clone()));
        self.midi_off.truncate(MAX_FOLDERS);
    }

    /// Puts a file at the top of the recent list.
    pub fn remember_file(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| *p != path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(RECENT_FILES);
    }
}

/// Keeps the computer keyboard's lowest key on a C, with all its notes
/// inside the MIDI range.
pub fn clamp_base(base: u8) -> u8 {
    let highest_c = (127 - crate::keys::HIGHEST_OFFSET) / 12 * 12;
    (base / 12 * 12).min(highest_c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_survive_damage() {
        let mut settings = Settings {
            theme: ThemeChoice::Dark,
            count_in_bars: 2,
            ..Settings::default()
        };
        settings.remember_file(PathBuf::from("a"));
        settings.remember_file(PathBuf::from("b"));
        settings.remember_file(PathBuf::from("a"));
        assert_eq!(
            settings.recent_files,
            [PathBuf::from("a"), PathBuf::from("b")]
        );
        assert_eq!(Settings::from_json(&settings.to_json()), settings);

        assert_eq!(Settings::from_json("not json"), Settings::default());
        let partial = Settings::from_json(r#"{"theme":"light","count_in_bars":99}"#);
        assert_eq!(partial.theme, ThemeChoice::Light);
        assert_eq!(partial.count_in_bars, 4, "out-of-range values are clamped");
        assert!(partial.show_keyboard, "missing values take their defaults");
    }

    #[test]
    fn folder_lists_keep_each_folder_once() {
        let root = if cfg!(windows) { "C:\\" } else { "/" };
        let sounds = PathBuf::from(root).join("sounds");
        let json = serde_json::json!({
            "sound_folders": [sounds, sounds, "relative/folder"],
            "plugin_folders": (0..100).map(|i| PathBuf::from(root).join(i.to_string())).collect::<Vec<_>>(),
        });
        let settings = Settings::from_json(&json.to_string());
        assert_eq!(settings.sound_folders, vec![sounds]);
        assert_eq!(settings.plugin_folders.len(), MAX_FOLDERS);
    }

    #[test]
    fn midi_buttons_and_switched_off_controllers_are_remembered() {
        let pad = daw_model::MidiControl {
            channel: 9,
            source: daw_model::MidiSource::Note(36),
        };
        let mut settings = Settings::default();
        settings.midi_buttons.insert("play_stop".to_owned(), pad);
        settings.midi_off.push("Keystation 49 MIDI 1".to_owned());
        assert_eq!(Settings::from_json(&settings.to_json()), settings);
        // A command this version does not know, or a second command on the
        // same pad, is dropped.
        let json = serde_json::json!({
            "midi_buttons": {
                "frobnicate": {"channel": 0, "source": {"controller": 20}},
                "play_stop": {"channel": 9, "source": {"note": 36}},
                "record": {"channel": 9, "source": {"note": 36}},
            },
            "midi_off": ["A", "A", ""],
        });
        let settings = Settings::from_json(&json.to_string());
        assert_eq!(settings.midi_buttons.len(), 1);
        assert_eq!(settings.midi_buttons.get("play_stop"), Some(&pad));
        assert_eq!(settings.midi_off, ["A"]);
    }

    #[test]
    fn the_keyboard_stays_in_range() {
        assert_eq!(clamp_base(50), 48);
        assert_eq!(clamp_base(127), 96);
        assert!(clamp_base(255) + crate::keys::HIGHEST_OFFSET <= 127);
    }
}

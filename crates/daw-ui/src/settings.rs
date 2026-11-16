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
            keyboard_base: 48,
            keyboard_velocity: 0.8,
            show_keyboard: true,
            show_mixer: false,
            key_labels: LabelChoice::Automatic,
            shortcuts: BTreeMap::new(),
            recent_files: Vec::new(),
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
    fn the_keyboard_stays_in_range() {
        assert_eq!(clamp_base(50), 48);
        assert_eq!(clamp_base(127), 96);
        assert!(clamp_base(255) + crate::keys::HIGHEST_OFFSET <= 127);
    }
}

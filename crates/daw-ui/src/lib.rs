// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

pub mod control_lane;
pub mod effect_ui;
pub mod export;
pub mod icons;
pub mod keyboard_panel;
pub mod keys;
pub mod library_panel;
pub mod midi;
pub mod midi_ui;
pub mod mixer;
pub mod piano_roll;
pub mod pitch_line;
pub mod plugin_manager;
pub mod session;
pub mod settings;
pub mod shape_ui;
pub mod signature_row;
pub mod song_view;
pub mod sound_editor;
pub mod step_grid;
pub mod synth_ui;
pub mod tempo_lane;
pub mod text;
pub mod theme;
pub mod transport;
pub mod voice_picture;
pub mod voicebanks;
pub mod widgets;
pub mod workspace;

pub use export::{ExportFormat, ExportTarget};
pub use keys::{Command, KeyLabels, Keymap, LayoutFamily, Shortcut};
pub use plugin_manager::{BridgeState, InstallerRun, PluginJobDone, PluginPlaces};
pub use session::{
    CopiedNote, Grid, LaneChoice, LiveTake, PluginSearch, SafetyCopy, Session, SoundState,
    TakeNote, View,
};
pub use settings::{LabelChoice, Settings, ThemeChoice};
pub use workspace::{Host, Intent, RecoveryDetails, Request, Workspace};

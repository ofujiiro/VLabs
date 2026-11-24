// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod clap;
pub mod install;
pub mod scan;
mod vst3;
mod window;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

pub use window::prepare_display;

/// A plugin format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// CLAP (`.clap` files).
    Clap,
    /// VST3 (`.vst3` bundles or files).
    Vst3,
}

impl Format {
    /// The format of a plugin file, from its extension.
    pub fn of_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("clap") {
            Some(Self::Clap)
        } else if extension.eq_ignore_ascii_case("vst3") {
            Some(Self::Vst3)
        } else {
            None
        }
    }
}

/// What a plugin does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// It makes sound from notes.
    Instrument,
    /// It changes sound.
    Effect,
    /// Something else (a note effect, an analyser…): not offered for now.
    Other,
}

/// One plugin, as scanning found it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Its format.
    pub format: Format,
    /// The file it is in (one file can hold several plugins).
    pub path: PathBuf,
    /// Its identity within the format: the CLAP id, or the VST3 class id
    /// in hexadecimal.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Who made it.
    pub vendor: String,
    /// Its version.
    pub version: String,
    /// What it does.
    pub kind: Kind,
}

/// How the host introduces itself to plugins.
#[derive(Clone, Debug)]
pub struct HostIdentity {
    /// The program's name.
    pub name: String,
    /// Who makes it.
    pub vendor: String,
    /// Its web address.
    pub url: String,
    /// Its version.
    pub version: String,
}

static IDENTITY: OnceLock<HostIdentity> = OnceLock::new();

/// Sets how the host introduces itself to plugins (once, at start).
pub fn set_identity(identity: HostIdentity) {
    let _ = IDENTITY.set(identity);
}

fn identity() -> HostIdentity {
    IDENTITY.get().cloned().unwrap_or(HostIdentity {
        name: "Plugin host".to_owned(),
        vendor: String::new(),
        url: String::new(),
        version: "0.0.0".to_owned(),
    })
}

/// The audio settings a plugin is prepared for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioConfig {
    /// Frames per second.
    pub sample_rate: u32,
    /// The most frames in one processing call.
    pub max_block: usize,
}

/// One of a plugin's parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
    /// Its identity (stable across versions of the plugin).
    pub id: u32,
    /// Its name.
    pub name: String,
    /// The group it belongs to ("Filter 1"), if the plugin says.
    pub module: String,
    /// Its default value, as a fraction of its range.
    pub default: f64,
    /// How many steps it has (0 for a continuous parameter).
    pub steps: u32,
    /// Whether the host may change it over time.
    pub automatable: bool,
}

/// Something that happens at a frame within one processing call. Values of
/// parameters are fractions of their range, from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A note starts.
    NoteOn {
        /// The frame within the block.
        frame: u32,
        /// The MIDI key.
        key: u8,
        /// How hard, from 0 to 1.
        velocity: f32,
    },
    /// A note is released.
    NoteOff {
        /// The frame within the block.
        frame: u32,
        /// The MIDI key.
        key: u8,
    },
    /// A parameter changes.
    Param {
        /// The frame within the block.
        frame: u32,
        /// The parameter's identity.
        id: u32,
        /// Its new value, as a fraction of its range.
        value: f64,
    },
    /// The pitch wheel moves (decision 0033): sent to a CLAP plugin as
    /// MIDI, to a VST3 plugin as the parameter it maps the wheel to.
    PitchBend {
        /// The frame within the block.
        frame: u32,
        /// From -1 (all the way down) through 0 (at rest) to 1.
        value: f32,
    },
    /// A numbered MIDI controller moves (the modulation wheel is 1).
    Controller {
        /// The frame within the block.
        frame: u32,
        /// The controller's number, 0 to 127.
        number: u8,
        /// Its value, from 0 to 1.
        value: f32,
    },
    /// Channel pressure (aftertouch) changes.
    Pressure {
        /// The frame within the block.
        frame: u32,
        /// From 0 to 1.
        value: f32,
    },
}

impl Event {
    /// The frame within the block.
    pub fn frame(&self) -> u32 {
        match *self {
            Self::NoteOn { frame, .. }
            | Self::NoteOff { frame, .. }
            | Self::Param { frame, .. }
            | Self::PitchBend { frame, .. }
            | Self::Controller { frame, .. }
            | Self::Pressure { frame, .. } => frame,
        }
    }

    /// The MIDI bytes of a controller's move (pitch bend, a controller,
    /// pressure) on the first channel; `None` for other events.
    pub fn midi_bytes(&self) -> Option<[u8; 3]> {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "rounded and clamped to MIDI's ranges"
        )]
        let seven = |value: f32| (value.clamp(0.0, 1.0) * 127.0).round() as u8;
        match *self {
            Self::PitchBend { value, .. } => {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "rounded and clamped to 14 bits"
                )]
                let bend = ((value.clamp(-1.0, 1.0) + 1.0) * 8192.0)
                    .round()
                    .clamp(0.0, 16_383.0) as u16;
                #[allow(clippy::cast_possible_truncation, reason = "two 7-bit halves")]
                Some([0xE0, (bend & 0x7F) as u8, (bend >> 7) as u8])
            }
            Self::Controller { number, value, .. } => Some([0xB0, number & 0x7F, seven(value)]),
            Self::Pressure { value, .. } => Some([0xD0, seven(value), 0]),
            Self::NoteOn { .. } | Self::NoteOff { .. } | Self::Param { .. } => None,
        }
    }
}

/// The most events one processing call takes (more are dropped).
pub const MAX_EVENTS: usize = 1024;

/// Where the song is, for plugins that follow it (tempo-synced effects,
/// arpeggiators).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transport {
    /// Whether the song is playing.
    pub playing: bool,
    /// Beats per minute.
    pub tempo: f64,
    /// The position, in quarter notes from the start of the song.
    pub beats: f64,
    /// Where the current bar starts, in quarter notes.
    pub bar_start: f64,
    /// The current bar's number, from 0.
    pub bar: u32,
    /// The time signature.
    pub numerator: u8,
    /// The time signature.
    pub denominator: u8,
    /// The position in frames from the start of the song.
    pub frames: i64,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            playing: false,
            tempo: 120.0,
            beats: 0.0,
            bar_start: 0.0,
            bar: 0,
            numerator: 4,
            denominator: 4,
            frames: 0,
        }
    }
}

/// A plugin just loaded: the handle stays on the thread that loaded it, the
/// processor goes to the audio thread.
pub struct LoadedPlugin {
    /// The main-thread half.
    pub handle: PluginHandle,
    /// The audio-thread half.
    pub processor: PluginProcessor,
}

/// Loads a plugin, ready to process at `config`.
pub fn load(info: &PluginInfo, config: AudioConfig) -> Result<LoadedPlugin, String> {
    let (main, audio): (Box<dyn MainSide>, Box<dyn AudioSide>) = match info.format {
        Format::Clap => {
            let (main, audio) = clap::load(info, config)?;
            (Box::new(main), Box::new(audio))
        }
        Format::Vst3 => {
            let (main, audio) = vst3::load(info, config)?;
            (Box::new(main), Box::new(audio))
        }
    };
    Ok(LoadedPlugin {
        handle: PluginHandle {
            info: info.clone(),
            main,
            window: None,
            has_editor: None,
        },
        processor: PluginProcessor { audio },
    })
}

/// The main-thread half of a plugin. It must stay on the thread that loaded
/// it.
pub struct PluginHandle {
    info: PluginInfo,
    main: Box<dyn MainSide>,
    window: Option<window::EditorWindow>,
    /// Whether the plugin has an editor, once asked. Asking a VST3 plugin
    /// makes a whole editor, which takes time, and some plugins start over
    /// when another editor is made beside the open one (MT Power Drum Kit
    /// muted itself again): the interface asks every frame.
    has_editor: Option<bool>,
}

impl PluginHandle {
    /// What was loaded.
    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    /// The plugin's parameters, in its own order.
    pub fn params(&mut self) -> Vec<ParamInfo> {
        self.main.params()
    }

    /// A parameter's current value, as a fraction of its range.
    pub fn param_value(&mut self, id: u32) -> Option<f64> {
        self.main.param_value(id)
    }

    /// How the plugin writes a value of a parameter ("1.2 kHz").
    pub fn param_text(&mut self, id: u32, value: f64) -> Option<String> {
        self.main.param_text(id, value)
    }

    /// Everything needed to bring the plugin back as it is now.
    pub fn save_state(&mut self) -> Result<Vec<u8>, String> {
        self.main.save_state()
    }

    /// Brings back a state saved by [`Self::save_state`].
    pub fn load_state(&mut self, state: &[u8]) -> Result<(), String> {
        self.main.load_state(state)
    }

    /// Whether the plugin has an editor of its own (asked of the plugin
    /// once).
    pub fn has_editor(&mut self) -> bool {
        if self.window.is_some() {
            return true;
        }
        *self
            .has_editor
            .get_or_insert_with(|| self.main.has_editor())
    }

    /// Opens the plugin's editor in a window of its own titled `title` (or
    /// brings it back if it is open).
    pub fn open_editor(&mut self, title: &str) -> Result<(), String> {
        if let Some(window) = &self.window {
            window.raise();
            return Ok(());
        }
        let window = window::EditorWindow::new(title, 640, 480)?;
        match self.main.attach_editor(window.handle()) {
            Ok((width, height)) => {
                window.resize(width, height);
                window.show();
                self.window = Some(window);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Closes the editor, if it is open.
    pub fn close_editor(&mut self) {
        if self.window.take().is_some() {
            self.main.detach_editor();
        }
    }

    /// Whether the editor is open.
    pub fn is_editor_open(&self) -> bool {
        self.window.is_some()
    }

    /// The housekeeping a plugin asks of its host: callbacks, timers, file
    /// descriptors, and the editor window's own events (its close button, a
    /// new size). Call often (every frame) while an editor is open, and
    /// regularly otherwise.
    pub fn idle(&mut self) {
        self.main.idle();
        let mut close = self.main.editor_closed_by_plugin();
        if let Some(window) = &self.window {
            close |= window.close_requested();
            if let Some((width, height)) = self.main.take_requested_size() {
                window.resize(width, height);
            }
        }
        if close {
            self.close_editor();
        }
    }

    /// The parameter the person last moved in the plugin's editor, once.
    pub fn take_touched_param(&mut self) -> Option<u32> {
        self.main.take_touched_param()
    }

    /// Whether the plugin's settings changed since the last call (the song
    /// then has unsaved changes).
    pub fn take_changed(&mut self) -> bool {
        self.main.take_changed()
    }
}

impl Drop for PluginHandle {
    fn drop(&mut self) {
        self.close_editor();
    }
}

/// The audio-thread half of a plugin.
pub struct PluginProcessor {
    audio: Box<dyn AudioSide>,
}

impl PluginProcessor {
    /// Processes one block: `left` and `right` hold the input (for an
    /// effect) and receive the output. `events` are sorted by frame, all
    /// within the block. Allocation-free in our code; the plugin's own call
    /// runs with allocation permitted.
    pub fn process(
        &mut self,
        transport: &Transport,
        events: &[Event],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        self.audio.process(transport, events, left, right);
    }

    /// Stops processing, before the processor goes back to the main thread
    /// to be dropped. Call on the audio thread.
    pub fn stop(&mut self) {
        self.audio.stop();
    }
}

/// What each format provides on the main thread.
trait MainSide {
    fn params(&mut self) -> Vec<ParamInfo>;
    fn param_value(&mut self, id: u32) -> Option<f64>;
    fn param_text(&mut self, id: u32, value: f64) -> Option<String>;
    fn save_state(&mut self) -> Result<Vec<u8>, String>;
    fn load_state(&mut self, state: &[u8]) -> Result<(), String>;
    fn has_editor(&mut self) -> bool;
    fn attach_editor(&mut self, parent: window::ParentHandle) -> Result<(u32, u32), String>;
    fn detach_editor(&mut self);
    fn take_requested_size(&mut self) -> Option<(u32, u32)>;
    fn editor_closed_by_plugin(&mut self) -> bool;
    fn idle(&mut self);
    fn take_touched_param(&mut self) -> Option<u32>;
    fn take_changed(&mut self) -> bool;
}

/// What each format provides on the audio thread.
trait AudioSide: Send {
    fn process(
        &mut self,
        transport: &Transport,
        events: &[Event],
        left: &mut [f32],
        right: &mut [f32],
    );
    fn stop(&mut self);
}

/// Why a plugin library could not be loaded, in the system's words (the
/// loader's own message says only that loading failed).
fn load_error(error: &libloading::Error) -> String {
    std::error::Error::source(error).map_or_else(|| error.to_string(), ToString::to_string)
}

/// The file of a plugin, made absolute for messages.
fn shown(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    /// A plugin's main side that counts how often it is asked whether it
    /// has an editor.
    struct Counting(Rc<Cell<u32>>);

    impl MainSide for Counting {
        fn params(&mut self) -> Vec<ParamInfo> {
            Vec::new()
        }
        fn param_value(&mut self, _: u32) -> Option<f64> {
            None
        }
        fn param_text(&mut self, _: u32, _: f64) -> Option<String> {
            None
        }
        fn save_state(&mut self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn load_state(&mut self, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
        fn has_editor(&mut self) -> bool {
            self.0.set(self.0.get() + 1);
            true
        }
        fn attach_editor(&mut self, _: window::ParentHandle) -> Result<(u32, u32), String> {
            Err("no window here".to_owned())
        }
        fn detach_editor(&mut self) {}
        fn take_requested_size(&mut self) -> Option<(u32, u32)> {
            None
        }
        fn editor_closed_by_plugin(&mut self) -> bool {
            false
        }
        fn idle(&mut self) {}
        fn take_touched_param(&mut self) -> Option<u32> {
            None
        }
        fn take_changed(&mut self) -> bool {
            false
        }
    }

    #[test]
    fn a_plugin_is_asked_once_whether_it_has_an_editor() {
        let asked = Rc::new(Cell::new(0));
        let mut handle = PluginHandle {
            info: PluginInfo {
                format: Format::Vst3,
                path: PathBuf::from("Stand-in.vst3"),
                id: "0".to_owned(),
                name: "Stand-in".to_owned(),
                vendor: String::new(),
                version: String::new(),
                kind: Kind::Instrument,
            },
            main: Box::new(Counting(Rc::clone(&asked))),
            window: None,
            has_editor: None,
        };
        // The interface asks every frame.
        for _ in 0..100 {
            assert!(handle.has_editor());
        }
        assert_eq!(asked.get(), 1, "making an editor to find out is costly");
    }
}

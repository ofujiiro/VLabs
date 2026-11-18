// SPDX-License-Identifier: GPL-3.0-or-later

//! DAWproject 1.0 (decision 0020), the open exchange format of Bitwig,
//! Studio One, Cubase and others: a ZIP holding `project.xml`,
//! `metadata.xml`, the audio files and the plugins' settings.
//!
//! Exporting writes the tracks (with their mixer settings, instruments and
//! effects), pattern clips as the notes they play, audio clips with their
//! files, automation lanes, the tempo (glides as linear tempo automation)
//! and the time signatures. VLabs's own instruments and effects travel as
//! devices of VLabs's own with their exact settings, so VLabs reads them
//! back unchanged; the EQ, compressor and limiter are also written as
//! DAWproject's standard devices.
//!
//! Importing reads what VLabs can play and says what it left out. Files
//! from outside are untrusted: sizes are limited and damage gives an
//! error, never a crash.

mod export;
mod import;
mod xml;

use std::fmt;

pub use export::export;
pub use import::{ImportNames, Imported, import};

/// What could not be brought in from another program's project (shown to
/// the person after opening it).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LeftOut {
    /// A device VLabs cannot play: its name (VST2 and AU plugins, other
    /// programs' own instruments and effects, a noise gate).
    Device(String),
    /// Group, effect and submix tracks and sends (VLabs has no buses yet);
    /// the tracks inside them are kept.
    Routing,
    /// Audio that another program stretched in time; VLabs plays it at its
    /// own speed.
    StretchedAudio,
    /// An audio file the project names but does not hold: its name.
    MissingAudio(String),
    /// Clips of a clip launcher (scenes).
    Scenes,
}

/// Why a DAWproject file could not be read or written.
#[derive(Debug)]
pub enum DawProjectError {
    /// The file could not be read or written.
    Io(std::io::Error),
    /// It is not a DAWproject file, or it is damaged: why.
    NotAProject(String),
    /// An audio file of the song could not be found to export it: its name.
    MissingAudio(String),
}

impl fmt::Display for DawProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::NotAProject(why) => write!(f, "this is not a usable DAWproject file ({why})"),
            Self::MissingAudio(name) => write!(
                f,
                "the sound of \"{name}\" has gone missing from the program's audio folder"
            ),
        }
    }
}

impl std::error::Error for DawProjectError {}

impl From<std::io::Error> for DawProjectError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<zip::result::ZipError> for DawProjectError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::NotAProject(error.to_string())
    }
}

/// The ID of one of VLabs's own devices (piano, drums, synth, effects).
fn own_device_id(kind: &str) -> String {
    format!("{}.{kind}", daw_ident::APPLICATION_ID)
}

/// Whether a device is one of VLabs's own.
fn is_own_device(id: &str) -> bool {
    id.strip_prefix(daw_ident::APPLICATION_ID)
        .is_some_and(|rest| rest.starts_with('.'))
}

/// A VST3 class id (32 hexadecimal digits) in the order plugins report it
/// on this system, turned into the canonical order DAWproject and
/// `.vstpreset` files use. On Windows the first eight bytes are in COM
/// order (two little-endian groups); elsewhere they already match.
pub fn vst3_id_to_canonical(raw: &str, windows: bool) -> String {
    swap_vst3_id(raw, windows)
}

/// The inverse of [`vst3_id_to_canonical`] (the swap is its own inverse).
pub fn vst3_id_from_canonical(canonical: &str, windows: bool) -> String {
    swap_vst3_id(canonical, windows)
}

fn swap_vst3_id(id: &str, windows: bool) -> String {
    let id = id.trim().to_ascii_uppercase();
    if !windows || id.len() != 32 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return id;
    }
    let bytes: Vec<&str> = (0..16).map(|i| &id[2 * i..2 * i + 2]).collect();
    let order = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];
    order.iter().map(|&i| bytes[i]).collect()
}

/// A `.vstpreset` file from a VST3 plugin's saved settings as VLabs keeps
/// them (two blocks, each with its length: the component's state, then the
/// controller's), for the plugin with `class_id` (canonical, 32 digits).
fn vstpreset(class_id: &str, state: &[u8]) -> Option<Vec<u8>> {
    let (component, controller) = split_vst3_state(state)?;
    let mut out = Vec::with_capacity(48 + state.len() + 48);
    out.extend_from_slice(b"VST3");
    out.extend_from_slice(&1_i32.to_le_bytes());
    let mut id = class_id.as_bytes().to_vec();
    id.resize(32, b'0');
    out.extend_from_slice(&id);
    let list_at = 48 + component.len() + controller.len();
    out.extend_from_slice(&(list_at as u64).to_le_bytes());
    out.extend_from_slice(component);
    out.extend_from_slice(controller);
    out.extend_from_slice(b"List");
    out.extend_from_slice(&2_i32.to_le_bytes());
    for (tag, offset, size) in [
        (b"Comp", 48, component.len()),
        (b"Cont", 48 + component.len(), controller.len()),
    ] {
        out.extend_from_slice(tag);
        out.extend_from_slice(&(offset as u64).to_le_bytes());
        out.extend_from_slice(&(size as u64).to_le_bytes());
    }
    Some(out)
}

/// The two blocks of VLabs's VST3 settings.
fn split_vst3_state(state: &[u8]) -> Option<(&[u8], &[u8])> {
    let block = |at: usize| -> Option<(&[u8], usize)> {
        let length = u32::from_le_bytes(state.get(at..at + 4)?.try_into().ok()?) as usize;
        let end = at.checked_add(4)?.checked_add(length)?;
        Some((state.get(at + 4..end)?, end))
    };
    let (component, next) = block(0)?;
    let (controller, _) = block(next)?;
    Some((component, controller))
}

/// VLabs's VST3 settings from a `.vstpreset` file (and the class id it
/// names), if it is one.
fn from_vstpreset(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
    if bytes.get(..4) != Some(b"VST3") {
        return None;
    }
    let class_id = String::from_utf8_lossy(bytes.get(8..40)?).into_owned();
    let list_at = usize::try_from(u64::from_le_bytes(bytes.get(40..48)?.try_into().ok()?)).ok()?;
    if bytes.get(list_at..list_at + 4)? != b"List" {
        return None;
    }
    let count = i32::from_le_bytes(bytes.get(list_at + 4..list_at + 8)?.try_into().ok()?);
    let mut component: &[u8] = &[];
    let mut controller: &[u8] = &[];
    for index in 0..usize::try_from(count.clamp(0, 64)).ok()? {
        let at = list_at + 8 + index * 20;
        let entry = bytes.get(at..at + 20)?;
        let offset = usize::try_from(u64::from_le_bytes(entry[4..12].try_into().ok()?)).ok()?;
        let size = usize::try_from(u64::from_le_bytes(entry[12..20].try_into().ok()?)).ok()?;
        let data = bytes.get(offset..offset.checked_add(size)?)?;
        match &entry[..4] {
            b"Comp" => component = data,
            b"Cont" => controller = data,
            _ => {}
        }
    }
    let mut state = Vec::with_capacity(8 + component.len() + controller.len());
    for block in [component, controller] {
        state.extend_from_slice(&u32::try_from(block.len()).ok()?.to_le_bytes());
        state.extend_from_slice(block);
    }
    Some((class_id, state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vst3_ids_swap_on_windows_only() {
        let raw = "00112233445566778899AABBCCDDEEFF";
        assert_eq!(vst3_id_to_canonical(raw, false), raw);
        let canonical = vst3_id_to_canonical(raw, true);
        assert_eq!(canonical, "33221100554477668899AABBCCDDEEFF");
        assert_eq!(vst3_id_from_canonical(&canonical, true), raw);
        assert_eq!(vst3_id_to_canonical("not an id", true), "NOT AN ID");
    }

    #[test]
    fn vst3_settings_travel_as_vstpresets() {
        let mut state = Vec::new();
        for block in [&b"component"[..], &b"ctl"[..]] {
            state.extend_from_slice(&u32::try_from(block.len()).unwrap().to_le_bytes());
            state.extend_from_slice(block);
        }
        let id = "ABCDEF0123456789ABCDEF0123456789";
        let preset = vstpreset(id, &state).unwrap();
        let (class_id, back) = from_vstpreset(&preset).unwrap();
        assert_eq!(class_id, id);
        assert_eq!(back, state);
        assert!(from_vstpreset(b"VST3 nonsense").is_none());
        assert!(vstpreset(id, b"\xff\xff\xff\xff").is_none());
    }

    #[test]
    fn own_devices_are_recognised() {
        assert!(is_own_device(&own_device_id("synth")));
        assert!(!is_own_device("com.example.synth"));
        assert!(!is_own_device(daw_ident::APPLICATION_ID));
    }
}

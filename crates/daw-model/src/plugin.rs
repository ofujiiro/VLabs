// SPDX-License-Identifier: GPL-3.0-or-later

//! Third-party plugins as a song refers to them, and their saved settings
//! (decision 0015).
//!
//! A plugin's settings are opaque bytes that only the plugin understands.
//! In the song description they appear as `{"$state": "<base64>"}`; the
//! native file format moves them into entries of their own in the container,
//! so `project.json` stays small and readable.

use std::sync::Arc;

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The two plugin formats of v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginFormat {
    /// CLAP.
    Clap,
    /// VST3.
    Vst3,
}

/// Which plugin: enough to find it again on this computer or another.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginRef {
    /// Its format.
    pub format: PluginFormat,
    /// Its identity within the format (the CLAP id, or the VST3 class id).
    pub id: String,
    /// Its name, to show (and to tell people what is missing).
    pub name: String,
    /// Who made it.
    #[serde(default)]
    pub vendor: String,
    /// Where it was found when it was chosen (a hint: the plugin is found
    /// by format and identity).
    #[serde(default)]
    pub path: String,
}

/// A plugin's saved settings, as the plugin wrote them.
#[derive(Clone, Debug)]
pub struct PluginState(Arc<[u8]>);

impl PluginState {
    /// Settings from the bytes a plugin saved.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Arc::from(bytes))
    }

    /// The bytes, to hand back to the plugin.
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl PartialEq for PluginState {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

/// The key a plugin state is written under.
pub const STATE_KEY: &str = "$state";

impl Serialize for PluginState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(STATE_KEY, &base64::encode(&self.0))?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for PluginState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StateVisitor;
        impl<'de> Visitor<'de> for StateVisitor {
            type Value = PluginState;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "a map with a \"{STATE_KEY}\" entry")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<PluginState, A::Error> {
                let mut bytes = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == STATE_KEY {
                        let text: String = map.next_value()?;
                        bytes = Some(base64::decode(&text).map_err(de::Error::custom)?);
                    } else {
                        let _: de::IgnoredAny = map.next_value()?;
                    }
                }
                bytes
                    .map(PluginState::new)
                    .ok_or_else(|| de::Error::missing_field(STATE_KEY))
            }
        }
        deserializer.deserialize_map(StateVisitor)
    }
}

/// A plugin in a song: which one, and its settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginSetup {
    /// Which plugin.
    pub plugin: PluginRef,
    /// Its settings when last saved (none: as it starts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<PluginState>,
}

impl PluginSetup {
    /// A plugin with its default settings.
    pub fn new(plugin: PluginRef) -> Self {
        Self {
            plugin,
            state: None,
        }
    }
}

/// Base64 (standard alphabet, with padding), for plugin settings in text.
pub mod base64 {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Bytes as base64 text.
    pub fn encode(bytes: &[u8]) -> String {
        let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
                if i <= chunk.len() {
                    text.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
                } else {
                    text.push('=');
                }
            }
        }
        text
    }

    /// Base64 text as bytes.
    pub fn decode(text: &str) -> Result<Vec<u8>, String> {
        let value = |c: u8| -> Result<u32, String> {
            match c {
                b'A'..=b'Z' => Ok(u32::from(c - b'A')),
                b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
                b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
                b'+' => Ok(62),
                b'/' => Ok(63),
                _ => Err("damaged plugin settings (not base64)".to_owned()),
            }
        };
        let clean: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
        if !clean.len().is_multiple_of(4) {
            return Err("damaged plugin settings (cut short)".to_owned());
        }
        let mut bytes = Vec::with_capacity(clean.len() / 4 * 3);
        for chunk in clean.chunks(4) {
            let padding = chunk.iter().rev().take_while(|&&c| c == b'=').count();
            let mut n = 0_u32;
            for &c in &chunk[..4 - padding] {
                n = (n << 6) | value(c)?;
            }
            n <<= 6 * padding;
            let [_, b0, b1, b2] = n.to_be_bytes();
            let decoded = [b0, b1, b2];
            bytes.extend_from_slice(&decoded[..3 - padding.min(2)]);
        }
        Ok(bytes)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn round_trips_any_length() {
            for length in 0..40_u32 {
                // Bytes of every kind: the low byte of a running sum.
                let bytes: Vec<u8> = (0..length)
                    .map(|i| (i * 37 + 11).to_le_bytes()[0])
                    .collect();
                let text = encode(&bytes);
                assert_eq!(decode(&text).unwrap(), bytes, "length {length}");
            }
            assert_eq!(encode(b"Man"), "TWFu");
            assert_eq!(encode(b"Ma"), "TWE=");
            assert_eq!(encode(b"M"), "TQ==");
            assert!(decode("TQ=").is_err());
            assert!(decode("T*==").is_err());
        }
    }
}

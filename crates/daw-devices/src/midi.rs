// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI keyboards: every MIDI input is connected automatically, including
//! ones plugged in while the program runs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use midir::{MidiInput, MidiInputConnection};

/// How often the port list is checked for keyboards coming and going.
const SCAN_INTERVAL: Duration = Duration::from_millis(1500);

/// A MIDI message VLabs understands (others are ignored).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiMessage {
    /// A key went down.
    NoteOn {
        /// MIDI channel, 0 to 15.
        channel: u8,
        /// Key number.
        key: u8,
        /// Velocity, 1 to 127.
        velocity: u8,
    },
    /// A key came up.
    NoteOff {
        /// MIDI channel, 0 to 15.
        channel: u8,
        /// Key number.
        key: u8,
    },
    /// A controller moved (sustain pedal, mod wheel, knobs).
    ControlChange {
        /// MIDI channel, 0 to 15.
        channel: u8,
        /// Controller number.
        controller: u8,
        /// Its value, 0 to 127.
        value: u8,
    },
    /// The pitch wheel moved; centre is 0, range -8192 to 8191.
    PitchBend {
        /// MIDI channel, 0 to 15.
        channel: u8,
        /// The bend.
        value: i16,
    },
}

/// Reads one MIDI message from its bytes.
pub fn parse(bytes: &[u8]) -> Option<MidiMessage> {
    let (&status, data) = bytes.split_first()?;
    let channel = status & 0x0F;
    let byte = |i: usize| data.get(i).map(|b| b & 0x7F);
    match status & 0xF0 {
        0x80 => Some(MidiMessage::NoteOff {
            channel,
            key: byte(0)?,
        }),
        // A note-on with velocity 0 is a note-off, by convention.
        0x90 => match byte(1)? {
            0 => Some(MidiMessage::NoteOff {
                channel,
                key: byte(0)?,
            }),
            velocity => Some(MidiMessage::NoteOn {
                channel,
                key: byte(0)?,
                velocity,
            }),
        },
        0xB0 => Some(MidiMessage::ControlChange {
            channel,
            controller: byte(0)?,
            value: byte(1)?,
        }),
        0xE0 => {
            let value = (i16::from(byte(1)?) << 7 | i16::from(byte(0)?)) - 8192;
            Some(MidiMessage::PitchBend { channel, value })
        }
        _ => None,
    }
}

type Handler = Arc<dyn Fn(MidiMessage) + Send + Sync>;

struct Shared {
    connected: Mutex<Vec<String>>,
    problem: Mutex<Option<String>>,
    stop: AtomicBool,
}

/// All MIDI inputs, connected automatically. Dropping it disconnects them.
pub struct MidiInputs {
    shared: Arc<Shared>,
    watcher: Option<JoinHandle<()>>,
}

impl MidiInputs {
    /// Starts watching for MIDI inputs; every message from any of them is
    /// passed to `on_message` (on a MIDI thread, not the audio thread).
    pub fn start(
        client_name: &str,
        on_message: impl Fn(MidiMessage) + Send + Sync + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            connected: Mutex::new(Vec::new()),
            problem: Mutex::new(None),
            stop: AtomicBool::new(false),
        });
        let handler: Handler = Arc::new(on_message);
        let watcher_shared = Arc::clone(&shared);
        let name = client_name.to_owned();
        let watcher = std::thread::Builder::new()
            .name("midi-watcher".to_owned())
            .spawn(move || watch(&watcher_shared, &name, &handler))
            .ok();
        Self { shared, watcher }
    }

    /// The names of the connected inputs.
    pub fn connected(&self) -> Vec<String> {
        self.shared
            .connected
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Why MIDI is unavailable, if it is.
    pub fn problem(&self) -> Option<String> {
        self.shared
            .problem
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for MidiInputs {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
    }
}

fn watch(shared: &Arc<Shared>, client_name: &str, handler: &Handler) {
    let mut connections: HashMap<String, (String, MidiInputConnection<()>)> = HashMap::new();
    let mut waited = SCAN_INTERVAL; // scan at once
    while !shared.stop.load(Ordering::Relaxed) {
        if waited >= SCAN_INTERVAL {
            waited = Duration::ZERO;
            scan(shared, client_name, handler, &mut connections);
        }
        std::thread::sleep(Duration::from_millis(100));
        waited += Duration::from_millis(100);
    }
}

fn scan(
    shared: &Arc<Shared>,
    client_name: &str,
    handler: &Handler,
    connections: &mut HashMap<String, (String, MidiInputConnection<()>)>,
) {
    let input = match MidiInput::new(client_name) {
        Ok(input) => input,
        Err(error) => {
            *shared
                .problem
                .lock()
                .unwrap_or_else(PoisonError::into_inner) =
                Some(format!("MIDI is unavailable: {error}"));
            connections.clear();
            shared
                .connected
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
            return;
        }
    };
    *shared
        .problem
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = None;
    let ports: Vec<(String, String)> = input
        .ports()
        .iter()
        .filter_map(|port| {
            let name = input.port_name(port).ok()?;
            // Skip the system's loopback port and our own ports.
            let skip = name.contains("Through") || name.contains(client_name);
            (!skip).then(|| (port.id(), name))
        })
        .collect();
    // Unplugged keyboards.
    connections.retain(|id, _| ports.iter().any(|(port_id, _)| port_id == id));
    // Newly plugged keyboards.
    for (id, name) in ports {
        if connections.contains_key(&id) {
            continue;
        }
        let Ok(input) = MidiInput::new(client_name) else {
            continue;
        };
        let Some(port) = input.find_port_by_id(&id) else {
            continue;
        };
        let handler = Arc::clone(handler);
        let connection = input.connect(
            &port,
            "input",
            move |_time, bytes, ()| {
                if let Some(message) = parse(bytes) {
                    handler(message);
                }
            },
            (),
        );
        if let Ok(connection) = connection {
            connections.insert(id, (name, connection));
        }
    }
    let mut names: Vec<String> = connections.values().map(|(name, _)| name.clone()).collect();
    names.sort();
    *shared
        .connected
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = names;
}

#[cfg(test)]
mod tests {
    use super::{MidiMessage, parse};

    #[test]
    fn messages_are_parsed() {
        assert_eq!(
            parse(&[0x90, 60, 100]),
            Some(MidiMessage::NoteOn {
                channel: 0,
                key: 60,
                velocity: 100
            })
        );
        assert_eq!(
            parse(&[0x93, 61, 0]),
            Some(MidiMessage::NoteOff {
                channel: 3,
                key: 61
            })
        );
        assert_eq!(
            parse(&[0x80, 62, 64]),
            Some(MidiMessage::NoteOff {
                channel: 0,
                key: 62
            })
        );
        assert_eq!(
            parse(&[0xB0, 64, 127]),
            Some(MidiMessage::ControlChange {
                channel: 0,
                controller: 64,
                value: 127
            })
        );
        assert_eq!(
            parse(&[0xE0, 0x00, 0x40]),
            Some(MidiMessage::PitchBend {
                channel: 0,
                value: 0
            })
        );
        assert_eq!(
            parse(&[0xE0, 0x7F, 0x7F]),
            Some(MidiMessage::PitchBend {
                channel: 0,
                value: 8191
            })
        );
        assert_eq!(parse(&[0xF8]), None, "clock messages are ignored");
        assert_eq!(parse(&[0x90, 60]), None, "truncated messages are ignored");
        assert_eq!(parse(&[]), None);
    }
}

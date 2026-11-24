// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI controllers (decision 0033): where each message from a controller
//! goes. Keys, pads, the wheels and the pedals play the instrument in focus,
//! straight through the engine's queue; transport buttons, and controls
//! given a job in the program, go to the window.
//!
//! The decision is made on the MIDI thread by [`route`], from a small table
//! the window keeps up to date ([`RouteTable`]); [`MidiRouter`] carries it
//! out. Both are plain data and tested without a device.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

use daw_engine::{Control, MidiSender};

/// The controller number of the sustain pedal: the engine holds notes for
/// it.
pub const SUSTAIN: u8 = daw_dsp::SUSTAIN;

/// A message from a MIDI controller, as the program takes it (the device
/// layer reads the bytes; the program shell turns its messages into these).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerInput {
    /// A key or pad went down (velocity 1 to 127; 0 is a release).
    NoteOn {
        /// The MIDI channel, 0 to 15.
        channel: u8,
        /// The key.
        key: u8,
        /// How hard.
        velocity: u8,
    },
    /// A key or pad came up.
    NoteOff {
        /// The MIDI channel, 0 to 15.
        channel: u8,
        /// The key.
        key: u8,
    },
    /// A knob, fader, button or pedal moved: a numbered controller.
    Controller {
        /// The MIDI channel, 0 to 15.
        channel: u8,
        /// The controller's number, 0 to 127.
        number: u8,
        /// Its value, 0 to 127.
        value: u8,
    },
    /// The pitch wheel moved: -8192 (all the way down) to 8191, 0 at rest.
    PitchBend {
        /// The MIDI channel, 0 to 15.
        channel: u8,
        /// The bend.
        value: i16,
    },
    /// How hard the keys are pressed once down (channel aftertouch).
    Pressure {
        /// The MIDI channel, 0 to 15.
        channel: u8,
        /// The pressure, 0 to 127.
        value: u8,
    },
    /// A transport button.
    Transport(TransportButton),
}

/// What a controller's transport buttons ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportButton {
    /// Play (nothing if already playing).
    Play,
    /// Stop (a second stop goes back to the start, as Stop does).
    Stop,
    /// Record, or stop recording.
    Record,
    /// Back to the start.
    Rewind,
}

/// Where a message from a controller goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Route {
    /// To the instrument in focus, through the engine's queue: a note (the
    /// window is told too, to light up its key), the sustain pedal, or a
    /// controller's move.
    Play(Live),
    /// To the window.
    Window,
    /// Nowhere: bank select and the channel mode messages (a stray button
    /// must not swap the instrument's sound or cut it off).
    Drop,
}

/// What the instrument in focus is given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Live {
    /// A key went down (velocity from 0 to 1).
    NoteOn {
        /// The key.
        key: u8,
        /// How hard, from 0 to 1.
        velocity: f32,
    },
    /// A key came up.
    NoteOff {
        /// The key.
        key: u8,
    },
    /// The sustain pedal went down (`true`) or came up.
    Sustain(bool),
    /// A wheel, pedal or knob moved.
    Control(Control),
}

/// What the window tells the MIDI thread, so it can decide where messages
/// go without asking the window each time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteTable {}

/// Where a message goes, given the window's table.
pub fn route(_table: &RouteTable, input: &ControllerInput) -> Route {
    match *input {
        ControllerInput::NoteOn { key, velocity, .. } if velocity > 0 => {
            Route::Play(Live::NoteOn {
                key: key & 0x7F,
                velocity: f32::from(velocity.min(127)) / 127.0,
            })
        }
        ControllerInput::NoteOn { key, .. } | ControllerInput::NoteOff { key, .. } => {
            Route::Play(Live::NoteOff { key: key & 0x7F })
        }
        ControllerInput::Controller {
            number: SUSTAIN,
            value,
            ..
        } => Route::Play(Live::Sustain(value >= 64)),
        // Bank select, and the channel mode messages (all sound off, reset,
        // local control, all notes off, omni, mono, poly).
        ControllerInput::Controller {
            number: 0 | 32 | 120..=127,
            ..
        } => Route::Drop,
        ControllerInput::Controller { number, value, .. } => {
            Route::Play(Live::Control(Control::Controller {
                number: number & 0x7F,
                value: f32::from(value.min(127)) / 127.0,
            }))
        }
        ControllerInput::PitchBend { value, .. } => {
            Route::Play(Live::Control(Control::PitchBend(bend(value))))
        }
        ControllerInput::Pressure { value, .. } => Route::Play(Live::Control(Control::Pressure(
            f32::from(value.min(127)) / 127.0,
        ))),
        ControllerInput::Transport(_) => Route::Window,
    }
}

/// The pitch wheel's position from -1 to 1: all the way up is exactly 1,
/// though MIDI's top value is one step short of the bottom's mirror.
fn bend(value: i16) -> f32 {
    let value = f32::from(value.clamp(-8192, 8191));
    if value >= 0.0 {
        value / 8191.0
    } else {
        value / 8192.0
    }
}

/// The MIDI thread's end: decides where each message goes ([`route`]) and
/// sends it there. Cheap to clone and share between MIDI threads.
#[derive(Clone)]
pub struct MidiRouter {
    table: Arc<Mutex<RouteTable>>,
    window: mpsc::Sender<ControllerInput>,
    engine: Arc<MidiSender>,
}

impl MidiRouter {
    /// A router sending to `engine` and to the window's `window` queue,
    /// deciding with `table` (which the window keeps up to date).
    pub fn new(
        table: Arc<Mutex<RouteTable>>,
        window: mpsc::Sender<ControllerInput>,
        engine: Arc<MidiSender>,
    ) -> Self {
        Self {
            table,
            window,
            engine,
        }
    }

    /// Sends one message where it goes. Returns whether the window was
    /// given something (and should be redrawn).
    pub fn handle(&self, input: ControllerInput) -> bool {
        let route = {
            let table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
            route(&table, &input)
        };
        match route {
            Route::Play(live) => {
                match live {
                    Live::NoteOn { key, velocity } => self.engine.note_on(key, velocity),
                    Live::NoteOff { key } => self.engine.note_off(key),
                    Live::Sustain(down) => self.engine.sustain(down),
                    Live::Control(control) => self.engine.control(control),
                }
                // Keys light up on the on-screen keyboard.
                matches!(live, Live::NoteOn { .. } | Live::NoteOff { .. })
                    && self.window.send(input).is_ok()
            }
            Route::Window => self.window.send(input).is_ok(),
            Route::Drop => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(input: ControllerInput) -> Route {
        route(&RouteTable::default(), &input)
    }

    #[test]
    fn keys_wheels_and_pedals_play_the_instrument_in_focus() {
        assert_eq!(
            play(ControllerInput::NoteOn {
                channel: 9,
                key: 36,
                velocity: 127
            }),
            Route::Play(Live::NoteOn {
                key: 36,
                velocity: 1.0
            })
        );
        assert_eq!(
            play(ControllerInput::NoteOn {
                channel: 0,
                key: 60,
                velocity: 0
            }),
            Route::Play(Live::NoteOff { key: 60 }),
            "a note-on with no velocity is a release"
        );
        assert_eq!(
            play(ControllerInput::Controller {
                channel: 0,
                number: SUSTAIN,
                value: 64
            }),
            Route::Play(Live::Sustain(true)),
            "the pedal is down from half way"
        );
        assert_eq!(
            play(ControllerInput::Controller {
                channel: 0,
                number: 1,
                value: 127
            }),
            Route::Play(Live::Control(Control::Controller {
                number: 1,
                value: 1.0
            }))
        );
        assert_eq!(
            play(ControllerInput::Pressure {
                channel: 0,
                value: 0
            }),
            Route::Play(Live::Control(Control::Pressure(0.0)))
        );
    }

    #[test]
    fn the_pitch_wheel_reaches_both_ends_exactly() {
        let wheel = |value| match play(ControllerInput::PitchBend { channel: 0, value }) {
            Route::Play(Live::Control(Control::PitchBend(bend))) => bend,
            other => panic!("{other:?}"),
        };
        assert!((wheel(8191) - 1.0).abs() < f32::EPSILON);
        assert!((wheel(-8192) + 1.0).abs() < f32::EPSILON);
        assert!(wheel(0).abs() < f32::EPSILON);
        assert!((wheel(4096) - 0.5).abs() < 0.001);
    }

    #[test]
    fn bank_select_and_channel_mode_messages_are_dropped() {
        for number in [0, 32, 120, 121, 123, 127] {
            assert_eq!(
                play(ControllerInput::Controller {
                    channel: 0,
                    number,
                    value: 127
                }),
                Route::Drop,
                "controller {number}"
            );
        }
    }

    #[test]
    fn transport_buttons_go_to_the_window() {
        assert_eq!(
            play(ControllerInput::Transport(TransportButton::Record)),
            Route::Window
        );
    }
}

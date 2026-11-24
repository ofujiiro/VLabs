// SPDX-License-Identifier: GPL-3.0-or-later

//! What every instrument offers the engine.

/// A sound generator played by notes.
///
/// Every method is called on the audio thread and must not allocate, lock,
/// touch files or log. All memory an instrument needs is reserved when it is
/// created, off the audio thread.
pub trait Instrument: Send {
    /// Starts a note. `velocity` is from 0 to 1.
    fn note_on(&mut self, key: u8, velocity: f32);

    /// Releases a note.
    fn note_off(&mut self, key: u8);

    /// Releases every note; with `immediate`, silences them at once instead
    /// of letting them fade out.
    fn all_notes_off(&mut self, immediate: bool);

    /// Writes the next `left.len()` frames of sound (both slices have the
    /// same length), replacing what is in the buffers.
    fn render(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Whether the instrument is currently making any sound (lets the
    /// engine skip silent instruments).
    fn is_sounding(&self) -> bool {
        true
    }

    /// Changes the instrument's settings while it plays, without cutting
    /// off what is sounding (a drum kit's pad settings, for instance).
    /// Instruments without such settings ignore it.
    fn apply_settings(&mut self, settings: &InstrumentSettings) {
        let _ = settings;
    }

    /// A controller of a MIDI keyboard moved (decision 0033): the pitch
    /// wheel, a numbered controller, pressure. Instruments ignore what they
    /// have no use for.
    fn control(&mut self, control: Control) {
        let _ = control;
    }

    /// Renders a block in which notes start and end, and controllers move,
    /// at the given frames (sorted). The usual way splits the block at each
    /// event; plugins take the whole block with its events in one go.
    fn render_block(&mut self, events: &[BlockEvent], left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        let mut cursor = 0;
        for event in events {
            let at = event.offset.min(len);
            if at > cursor {
                self.render(&mut left[cursor..at], &mut right[cursor..at]);
                cursor = at;
            }
            match event.kind {
                BlockEventKind::NoteOn { key, velocity } => self.note_on(key, velocity),
                BlockEventKind::NoteOff { key } => self.note_off(key),
                BlockEventKind::Control(control) => self.control(control),
            }
        }
        if cursor < len {
            self.render(&mut left[cursor..len], &mut right[cursor..len]);
        }
    }

    /// Changes one of the instrument's parameters (plugins), as a fraction
    /// of its range, from the next block on.
    fn set_parameter(&mut self, id: u32, value: f64) {
        let _ = (id, value);
    }

    /// Where the song is, given before each block (for instruments that
    /// follow the tempo).
    fn set_position(&mut self, position: &SongPosition) {
        let _ = position;
    }

    /// Called on the audio thread just before the instrument is handed back
    /// to be dropped elsewhere (a plugin stops processing here).
    fn retire(&mut self) {}

    /// For a plugin: which loaded plugin this is, so the rest of it can be
    /// found when this part comes back from the audio thread.
    fn plugin_instance(&self) -> Option<u64> {
        None
    }
}

/// Something that happens at a frame within a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockEvent {
    /// The frame within the block.
    pub offset: usize,
    /// What happens.
    pub kind: BlockEventKind,
}

/// What happens at a frame within a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockEventKind {
    /// A note starts.
    NoteOn {
        /// The MIDI key.
        key: u8,
        /// How hard, from 0 to 1.
        velocity: f32,
    },
    /// A note ends.
    NoteOff {
        /// The MIDI key.
        key: u8,
    },
    /// A controller moves (decision 0033).
    Control(Control),
}

/// The controller number of the modulation wheel.
pub const MODULATION: u8 = 1;
/// The controller number of the expression pedal.
pub const EXPRESSION: u8 = 11;
/// The controller number of the sustain pedal (the engine holds notes for
/// it; instruments never see it).
pub const SUSTAIN: u8 = 64;

/// A controller of a MIDI keyboard, as instruments take it (decision 0033).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    /// The pitch wheel, from -1 (all the way down) through 0 (at rest) to 1
    /// (all the way up). How far that bends is the instrument's bend range.
    PitchBend(f32),
    /// A numbered controller (1 is the modulation wheel, 11 the expression
    /// pedal), from 0 to 1.
    Controller {
        /// The controller's number, 0 to 127.
        number: u8,
        /// Its value, from 0 to 1.
        value: f32,
    },
    /// How hard the keys are pressed once down (channel aftertouch), from 0
    /// to 1.
    Pressure(f32),
}

impl Control {
    /// The controllers that have a resting place, at rest: the wheel
    /// centred, no modulation, no pressure, full expression.
    pub const REST: [Self; 4] = [
        Self::PitchBend(0.0),
        Self::Controller {
            number: MODULATION,
            value: 0.0,
        },
        Self::Pressure(0.0),
        Self::Controller {
            number: EXPRESSION,
            value: 1.0,
        },
    ];

    /// The same control with its value in range (a damaged file or a
    /// misbehaving device may send anything).
    #[must_use]
    pub fn sane(self) -> Self {
        let unit = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        match self {
            Self::PitchBend(v) => Self::PitchBend(if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }),
            Self::Controller { number, value } => Self::Controller {
                number: number & 0x7F,
                value: unit(value),
            },
            Self::Pressure(v) => Self::Pressure(unit(v)),
        }
    }
}

/// Where the controllers that the included instruments answer stand
/// (decision 0033): kept by each instrument, updated by
/// [`Instrument::control`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Performance {
    /// The pitch wheel, -1 to 1.
    pub bend: f32,
    /// The modulation wheel, 0 to 1.
    pub modulation: f32,
    /// Pressure (aftertouch), 0 to 1.
    pub pressure: f32,
    /// The expression pedal, 0 to 1 (1 is full loudness).
    pub expression: f32,
}

impl Default for Performance {
    fn default() -> Self {
        Self::REST
    }
}

impl Performance {
    /// Everything at rest.
    pub const REST: Self = Self {
        bend: 0.0,
        modulation: 0.0,
        pressure: 0.0,
        expression: 1.0,
    };

    /// Takes a controller's move; says whether it was one of these.
    pub fn take(&mut self, control: Control) -> bool {
        match control.sane() {
            Control::PitchBend(v) => self.bend = v,
            Control::Pressure(v) => self.pressure = v,
            Control::Controller {
                number: MODULATION,
                value,
            } => self.modulation = value,
            Control::Controller {
                number: EXPRESSION,
                value,
            } => self.expression = value,
            Control::Controller { .. } => return false,
        }
        true
    }

    /// The bend in semitones, for an instrument that bends `down` semitones
    /// with the wheel all the way down and `up` all the way up.
    pub fn bend_semitones(&self, down: f32, up: f32) -> f32 {
        if self.bend >= 0.0 {
            self.bend * up
        } else {
            self.bend * down
        }
    }

    /// How much vibrato the wheel and pressure ask for together, 0 to 1.
    pub fn vibrato(&self) -> f32 {
        self.modulation.max(self.pressure)
    }

    /// The expression pedal as a gain: a curve that sounds even (the pedal
    /// half way is about -12 dB, as General MIDI's expression is).
    pub fn expression_gain(&self) -> f32 {
        self.expression * self.expression
    }
}

/// Where the song is, for instruments and effects that follow it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongPosition {
    /// Whether the song is playing.
    pub playing: bool,
    /// Beats per minute.
    pub tempo: f64,
    /// The position in quarter notes from the start.
    pub beats: f64,
    /// Where the current bar starts, in quarter notes.
    pub bar_start: f64,
    /// The current bar, from 0.
    pub bar: u32,
    /// The time signature.
    pub numerator: u8,
    /// The time signature.
    pub denominator: u8,
    /// The position in frames from the start.
    pub frames: i64,
}

impl Default for SongPosition {
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

/// Settings an instrument takes while playing.
#[derive(Clone, Debug, PartialEq)]
pub enum InstrumentSettings {
    /// A drum kit's pad settings, indexed by each pad's main key.
    Drums(Box<crate::drums::PadTable>),
    /// The synth's settings (decision 0019).
    Synth(crate::synth::SynthParams),
    /// How notes start and end (decision 0031), for the instruments that
    /// play recorded sounds.
    Shape(NoteShape),
}

/// How notes start and end (decision 0031), in seconds; what is not set
/// keeps the sound's own. With a release, sounds that would play to their
/// end whatever the key does (a drum's hit) stop when their key comes up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoteShape {
    /// Time to rise from silence to full level.
    pub attack: Option<f32>,
    /// Time to fall silent once the key comes up.
    pub release: Option<f32>,
}

impl NoteShape {
    /// The shape with its times usable (finite, not negative).
    #[must_use]
    pub fn sane(self) -> Self {
        let sane = |v: Option<f32>| v.filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 60.0));
        Self {
            attack: sane(self.attack),
            release: sane(self.release),
        }
    }
}

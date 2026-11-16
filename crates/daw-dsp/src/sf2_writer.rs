// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing SoundFont 2 files.
//!
//! Used to build the bundled piano from its source samples, and by tests to
//! make small SoundFonts on the fly. Supports what those need: 16-bit mono
//! samples with optional loops, instruments made of key and velocity zones,
//! and presets that play one instrument.

/// A generator: a SoundFont parameter set on a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Generator {
    /// Stereo position, -500 (left) to 500 (right), in tenths of a percent.
    Pan(i16),
    /// Volume envelope attack, in timecents (see [`timecents`]).
    AttackVolEnv(i16),
    /// Volume envelope hold, in timecents.
    HoldVolEnv(i16),
    /// Volume envelope decay, in timecents.
    DecayVolEnv(i16),
    /// Volume envelope sustain, as attenuation in centibels (0 is full level).
    SustainVolEnv(i16),
    /// Volume envelope release, in timecents.
    ReleaseVolEnv(i16),
    /// Attenuation in centibels.
    InitialAttenuation(i16),
    /// Fine tuning in cents.
    FineTune(i16),
    /// 0: no loop; 1: loop continuously; 3: loop until release, then play on.
    SampleModes(u16),
    /// The key at which the sample plays at its recorded pitch.
    OverridingRootKey(u16),
    /// Reverb send, in tenths of a percent.
    ReverbEffectsSend(i16),
}

impl Generator {
    fn encode(self) -> (u16, [u8; 2]) {
        let signed = |v: i16| v.to_le_bytes();
        let unsigned = |v: u16| v.to_le_bytes();
        match self {
            Self::Pan(v) => (17, signed(v)),
            Self::ReverbEffectsSend(v) => (16, signed(v)),
            Self::AttackVolEnv(v) => (34, signed(v)),
            Self::HoldVolEnv(v) => (35, signed(v)),
            Self::DecayVolEnv(v) => (36, signed(v)),
            Self::SustainVolEnv(v) => (37, signed(v)),
            Self::ReleaseVolEnv(v) => (38, signed(v)),
            Self::InitialAttenuation(v) => (48, signed(v)),
            Self::FineTune(v) => (52, signed(v)),
            Self::SampleModes(v) => (54, unsigned(v)),
            Self::OverridingRootKey(v) => (58, unsigned(v)),
        }
    }
}

/// Converts seconds to SoundFont timecents.
pub fn timecents(seconds: f32) -> i16 {
    if seconds <= 0.001 {
        return -12_000;
    }
    #[allow(clippy::cast_possible_truncation, reason = "clamped to the i16 range")]
    let value = (1200.0 * seconds.log2()).round().clamp(-12_000.0, 8_000.0) as i16;
    value
}

/// A sample: 16-bit mono audio.
#[derive(Clone, Debug)]
pub struct Sample {
    /// Up to 20 characters.
    pub name: String,
    /// The audio.
    pub data: Vec<i16>,
    /// Its sample rate.
    pub sample_rate: u32,
    /// The MIDI key it was recorded at.
    pub root_key: u8,
    /// Pitch correction in cents.
    pub correction: i8,
    /// Loop start and end, as offsets into `data`.
    pub loop_points: Option<(u32, u32)>,
}

/// A zone of an instrument: which keys and velocities play which sample.
#[derive(Clone, Debug)]
pub struct Zone {
    /// Lowest and highest key.
    pub keys: (u8, u8),
    /// Lowest and highest velocity.
    pub velocities: (u8, u8),
    /// Index into the builder's samples.
    pub sample: usize,
    /// Other parameters.
    pub generators: Vec<Generator>,
}

/// An instrument: a set of zones.
#[derive(Clone, Debug)]
pub struct InstrumentDef {
    /// Up to 20 characters.
    pub name: String,
    /// Parameters applied to every zone, unless a zone overrides them.
    pub global: Vec<Generator>,
    /// The zones.
    pub zones: Vec<Zone>,
}

/// A preset: what people choose; plays one instrument.
#[derive(Clone, Debug)]
pub struct PresetDef {
    /// Up to 20 characters.
    pub name: String,
    /// Bank number.
    pub bank: u16,
    /// Program number.
    pub program: u16,
    /// Index into the builder's instruments.
    pub instrument: usize,
}

/// Everything in a SoundFont, ready to be written.
#[derive(Clone, Debug, Default)]
pub struct SoundFontDef {
    /// The bank's name.
    pub name: String,
    /// Copyright or licence notice.
    pub copyright: String,
    /// A comment: source and processing.
    pub comment: String,
    /// The samples.
    pub samples: Vec<Sample>,
    /// The instruments.
    pub instruments: Vec<InstrumentDef>,
    /// The presets.
    pub presets: Vec<PresetDef>,
}

fn chunk(id: [u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 9);
    out.extend_from_slice(&id);
    out.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn list(kind: [u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut body = kind.to_vec();
    for c in chunks {
        body.extend_from_slice(c);
    }
    chunk(*b"LIST", &body)
}

/// A zero-terminated string padded to an even length.
fn zstr(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = text.bytes().filter(u8::is_ascii).collect();
    bytes.push(0);
    if bytes.len() % 2 == 1 {
        bytes.push(0);
    }
    bytes
}

/// A fixed 20-byte name field.
fn name20(text: &str) -> [u8; 20] {
    let mut field = [0_u8; 20];
    for (slot, byte) in field
        .iter_mut()
        .zip(text.bytes().filter(u8::is_ascii).take(19))
    {
        *slot = byte;
    }
    field
}

fn u16le(value: usize) -> [u8; 2] {
    u16::try_from(value).unwrap_or(u16::MAX).to_le_bytes()
}

impl SoundFontDef {
    /// The SoundFont file's bytes.
    #[allow(clippy::too_many_lines, reason = "the file layout, written in order")]
    pub fn to_bytes(&self) -> Vec<u8> {
        // INFO
        let mut info = vec![
            chunk(*b"ifil", &[2, 0, 1, 0]),
            chunk(*b"isng", &zstr("EMU8000")),
            chunk(*b"INAM", &zstr(&self.name)),
        ];
        if !self.copyright.is_empty() {
            info.push(chunk(*b"ICOP", &zstr(&self.copyright)));
        }
        if !self.comment.is_empty() {
            info.push(chunk(*b"ICMT", &zstr(&self.comment)));
        }

        // Sample data: each sample followed by 46 zero samples (required).
        let mut smpl = Vec::new();
        let mut offsets = Vec::with_capacity(self.samples.len());
        for sample in &self.samples {
            let start = smpl.len() / 2;
            for value in &sample.data {
                smpl.extend_from_slice(&value.to_le_bytes());
            }
            smpl.resize(smpl.len() + 46 * 2, 0);
            offsets.push(start);
        }

        // Presets: one zone each, pointing at an instrument.
        let mut phdr = Vec::new();
        let mut pbag = Vec::new();
        let mut pgen = Vec::new();
        for (index, preset) in self.presets.iter().enumerate() {
            phdr.extend_from_slice(&name20(&preset.name));
            phdr.extend_from_slice(&preset.program.to_le_bytes());
            phdr.extend_from_slice(&preset.bank.to_le_bytes());
            phdr.extend_from_slice(&u16le(index));
            phdr.extend_from_slice(&[0; 12]);
            pbag.extend_from_slice(&u16le(pgen.len() / 4));
            pbag.extend_from_slice(&[0, 0]);
            pgen.extend_from_slice(&41_u16.to_le_bytes());
            pgen.extend_from_slice(&u16le(preset.instrument));
        }
        phdr.extend_from_slice(&name20("EOP"));
        phdr.extend_from_slice(&[0; 4]);
        phdr.extend_from_slice(&u16le(self.presets.len()));
        phdr.extend_from_slice(&[0; 12]);
        pbag.extend_from_slice(&u16le(pgen.len() / 4));
        pbag.extend_from_slice(&[0, 0]);
        pgen.extend_from_slice(&[0; 4]);

        // Instruments: an optional global zone, then the zones.
        let mut inst = Vec::new();
        let mut ibag = Vec::new();
        let mut igen = Vec::new();
        let mut bag_count = 0;
        for instrument in &self.instruments {
            inst.extend_from_slice(&name20(&instrument.name));
            inst.extend_from_slice(&u16le(bag_count));
            if !instrument.global.is_empty() {
                ibag.extend_from_slice(&u16le(igen.len() / 4));
                ibag.extend_from_slice(&[0, 0]);
                bag_count += 1;
                for generator in &instrument.global {
                    let (id, amount) = generator.encode();
                    igen.extend_from_slice(&id.to_le_bytes());
                    igen.extend_from_slice(&amount);
                }
            }
            for zone in &instrument.zones {
                ibag.extend_from_slice(&u16le(igen.len() / 4));
                ibag.extend_from_slice(&[0, 0]);
                bag_count += 1;
                // keyRange first, velRange second, sampleID last (required order).
                igen.extend_from_slice(&43_u16.to_le_bytes());
                igen.extend_from_slice(&[zone.keys.0, zone.keys.1]);
                igen.extend_from_slice(&44_u16.to_le_bytes());
                igen.extend_from_slice(&[zone.velocities.0, zone.velocities.1]);
                for generator in &zone.generators {
                    let (id, amount) = generator.encode();
                    igen.extend_from_slice(&id.to_le_bytes());
                    igen.extend_from_slice(&amount);
                }
                igen.extend_from_slice(&53_u16.to_le_bytes());
                igen.extend_from_slice(&u16le(zone.sample));
            }
        }
        inst.extend_from_slice(&name20("EOI"));
        inst.extend_from_slice(&u16le(bag_count));
        ibag.extend_from_slice(&u16le(igen.len() / 4));
        ibag.extend_from_slice(&[0, 0]);
        igen.extend_from_slice(&[0; 4]);

        // Sample headers.
        let mut shdr = Vec::new();
        for (sample, &start) in self.samples.iter().zip(&offsets) {
            let end = start + sample.data.len();
            let (loop_start, loop_end) = sample.loop_points.map_or((start, end), |(s, e)| {
                (start + s as usize, start + e as usize)
            });
            shdr.extend_from_slice(&name20(&sample.name));
            for value in [start, end, loop_start, loop_end] {
                shdr.extend_from_slice(&u32::try_from(value).unwrap_or(u32::MAX).to_le_bytes());
            }
            shdr.extend_from_slice(&sample.sample_rate.to_le_bytes());
            shdr.push(sample.root_key);
            shdr.extend_from_slice(&sample.correction.to_le_bytes());
            shdr.extend_from_slice(&0_u16.to_le_bytes()); // no link
            shdr.extend_from_slice(&1_u16.to_le_bytes()); // mono sample
        }
        shdr.extend_from_slice(&name20("EOS"));
        shdr.extend_from_slice(&[0; 26]);

        let empty_mod = vec![0_u8; 10];
        let body = [
            list(*b"INFO", &info),
            list(*b"sdta", &[chunk(*b"smpl", &smpl)]),
            list(
                *b"pdta",
                &[
                    chunk(*b"phdr", &phdr),
                    chunk(*b"pbag", &pbag),
                    chunk(*b"pmod", &empty_mod),
                    chunk(*b"pgen", &pgen),
                    chunk(*b"inst", &inst),
                    chunk(*b"ibag", &ibag),
                    chunk(*b"imod", &empty_mod),
                    chunk(*b"igen", &igen),
                    chunk(*b"shdr", &shdr),
                ],
            ),
        ];
        let mut riff = b"sfbk".to_vec();
        for part in body {
            riff.extend_from_slice(&part);
        }
        chunk(*b"RIFF", &riff)
    }
}

/// A one-preset SoundFont that plays a looped sine wave at every key:
/// for tests.
pub fn sine_test_font(sample_rate: u32) -> SoundFontDef {
    // One second of a 440 Hz sine; 440 cycles fit exactly, so the whole
    // sample loops seamlessly.
    let frames = sample_rate as usize;
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "test signal"
    )]
    let data: Vec<i16> = (0..frames)
        .map(|i| {
            ((i as f32 * 440.0 / sample_rate as f32 * std::f32::consts::TAU).sin() * 16_000.0)
                as i16
        })
        .collect();
    let length = u32::try_from(frames).unwrap_or(u32::MAX);
    SoundFontDef {
        name: "Test sine".to_owned(),
        copyright: "CC0".to_owned(),
        comment: "Generated for tests".to_owned(),
        samples: vec![Sample {
            name: "sine".to_owned(),
            data,
            sample_rate,
            root_key: 69,
            correction: 0,
            loop_points: Some((0, length)),
        }],
        instruments: vec![InstrumentDef {
            name: "Sine".to_owned(),
            global: vec![
                Generator::ReleaseVolEnv(timecents(0.05)),
                Generator::ReverbEffectsSend(0),
            ],
            zones: vec![Zone {
                keys: (0, 127),
                velocities: (0, 127),
                sample: 0,
                generators: vec![Generator::SampleModes(1)],
            }],
        }],
        presets: vec![PresetDef {
            name: "Sine".to_owned(),
            bank: 0,
            program: 0,
            instrument: 0,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::{sine_test_font, timecents};
    use crate::{Instrument, SoundFontBank, SoundFontPlayer};

    #[test]
    fn timecents_match_the_specification() {
        assert_eq!(timecents(1.0), 0);
        assert_eq!(timecents(2.0), 1200);
        assert_eq!(timecents(0.5), -1200);
        assert_eq!(timecents(0.0), -12_000);
    }

    #[test]
    fn a_written_soundfont_loads_and_plays_in_tune() {
        let bytes = sine_test_font(48_000).to_bytes();
        let bank = SoundFontBank::from_bytes(&bytes).expect("our own SoundFont must load");
        let presets = bank.presets();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "Sine");

        let mut player = SoundFontPlayer::new(&bank, 48_000, 0, 0).unwrap();
        player.note_on(60, 0.8);
        let mut left = vec![0.0; 24_000];
        let mut right = vec![0.0; 24_000];
        player.render(&mut left, &mut right);
        let steady = &left[4_800..];
        let peak = steady.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.05, "the note should sound (peak {peak})");
        // Count upward zero crossings: middle C is 261.6 Hz.
        let crossings = steady
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        #[allow(clippy::cast_precision_loss, reason = "test arithmetic")]
        let hz = crossings as f32 / (steady.len() as f32 / 48_000.0);
        assert!((hz - 261.6).abs() < 3.0, "middle C measured at {hz} Hz");
    }
}

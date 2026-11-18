// SPDX-License-Identifier: GPL-3.0-or-later

//! Standard MIDI Files (decision 0020): our own reader and writer (the
//! format is small, and `midly`'s licence is not one we accept), how a song
//! becomes a MIDI file, and how a MIDI file becomes tracks of a song.
//!
//! Files from outside are untrusted: every length is checked, the number
//! of events is limited, and damage gives an error, never a crash.

use std::collections::HashMap;
use std::fmt;

use daw_model::{
    ACOUSTIC_KIT, ChainPlace, EffectKind, Instrument, KeyMap, LimiterSettings, MeterMap, Note,
    NoteId, Project, Scale, ScaleKind, SynthPreset, TICKS_PER_QUARTER, TempoChange, TempoMap,
    Ticks, TimeSignature, TrackId,
};

/// The resolution exported files use: 960 ticks per quarter note, the most
/// widely supported (triplets and quintuplets are exact).
pub const EXPORT_PPQ: u16 = 960;

/// The largest MIDI file read.
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
/// The most tracks read.
const MAX_TRACKS: usize = 4096;
/// The most events read, over all tracks.
const MAX_EVENTS: usize = 8_000_000;

/// The MIDI channel drums play on (channel 10, counted from 1).
const DRUM_CHANNEL: u8 = 9;

/// Why a MIDI file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiError {
    /// It is not a MIDI file.
    NotMidi,
    /// It is a MIDI file, but damaged: what is wrong.
    Damaged(String),
    /// It is larger than any MIDI file should be.
    TooLarge,
}

impl fmt::Display for MidiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotMidi => write!(f, "this is not a MIDI file"),
            Self::Damaged(why) => write!(f, "the MIDI file is damaged ({why})"),
            Self::TooLarge => write!(f, "the MIDI file is unreasonably large"),
        }
    }
}

impl std::error::Error for MidiError {}

/// How a file counts time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    /// Ticks per quarter note.
    PerQuarter(u16),
    /// Frames per second and ticks per frame (time in seconds, no tempo).
    Smpte {
        /// Frames per second (24, 25, 29 for 29.97 drop-frame, 30).
        fps: u8,
        /// Ticks per frame.
        per_frame: u8,
    },
}

/// One event, at a tick counted from the start of its track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// When, in the file's ticks.
    pub tick: u64,
    /// What.
    pub kind: EventKind,
}

/// The events VLabs uses; everything else is skipped when reading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// A note starts (velocity 1 to 127).
    NoteOn {
        /// The channel, from 0.
        channel: u8,
        /// The key.
        key: u8,
        /// How hard.
        velocity: u8,
    },
    /// A note ends.
    NoteOff {
        /// The channel, from 0.
        channel: u8,
        /// The key.
        key: u8,
    },
    /// The channel's sound changes (General MIDI program, from 0).
    Program {
        /// The channel, from 0.
        channel: u8,
        /// The program.
        program: u8,
    },
    /// Microseconds per quarter note from here on.
    Tempo(u32),
    /// A time signature: beats, and the beat's note value as a power of two.
    TimeSignature {
        /// Beats per bar.
        numerator: u8,
        /// The beat: 2 is a quarter note, 3 an eighth.
        denominator_power: u8,
    },
    /// A key signature: sharps (positive) or flats (negative), and minor.
    KeySignature {
        /// Sharps or flats.
        sharps: i8,
        /// Minor rather than major.
        minor: bool,
    },
    /// The track's name (or, in the first track of a format 1 file, the
    /// song's).
    TrackName(String),
}

/// One track of a MIDI file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    /// Its events, in time order.
    pub events: Vec<Event>,
    /// Where it ends (its end-of-track event), at or after its last event.
    pub end: u64,
}

impl Track {
    /// A track of these events, ending with the last of them.
    pub fn new(events: Vec<Event>) -> Self {
        let end = events.iter().map(|e| e.tick).max().unwrap_or(0);
        Self { events, end }
    }
}

/// A whole MIDI file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiFile {
    /// 0 (one track), 1 (tracks played together) or 2 (separate songs).
    pub format: u16,
    /// How time is counted.
    pub timing: Timing,
    /// The tracks.
    pub tracks: Vec<Track>,
}

// ---------------------------------------------------------------------------
// Reading

/// A reader over untrusted bytes: every read is checked.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn damaged(why: &str) -> MidiError {
        MidiError::Damaged(why.to_owned())
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], MidiError> {
        let end = self
            .at
            .checked_add(count)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| Self::damaged("it ends too soon"))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, MidiError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, MidiError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A variable-length quantity: at most four bytes.
    fn vlq(&mut self) -> Result<u32, MidiError> {
        let mut value = 0_u32;
        for _ in 0..4 {
            let byte = self.byte()?;
            value = (value << 7) | u32::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Self::damaged("a length is too long"))
    }

    fn is_done(&self) -> bool {
        self.at >= self.bytes.len()
    }
}

/// Reads a MIDI file.
pub fn read(bytes: &[u8]) -> Result<MidiFile, MidiError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(MidiError::TooLarge);
    }
    // Some files wrap the MIDI data in a RIFF "RMID" container.
    let bytes = unwrap_rmid(bytes);
    if bytes.get(..4) != Some(b"MThd") {
        return Err(MidiError::NotMidi);
    }
    let mut reader = Reader { bytes, at: 4 };
    let header_length = reader.u32()? as usize;
    if header_length < 6 {
        return Err(Reader::damaged("its header is too short"));
    }
    let header = reader.take(header_length)?;
    let format = u16::from_be_bytes([header[0], header[1]]);
    let track_count = usize::from(u16::from_be_bytes([header[2], header[3]]));
    let division = u16::from_be_bytes([header[4], header[5]]);
    if format > 2 {
        return Err(Reader::damaged("its format is unknown"));
    }
    let timing = if division & 0x8000 == 0 {
        if division == 0 {
            return Err(Reader::damaged("its time resolution is zero"));
        }
        Timing::PerQuarter(division)
    } else {
        let [high, low] = division.to_be_bytes();
        let fps = high.cast_signed().wrapping_neg().cast_unsigned();
        if !matches!(fps, 24 | 25 | 29 | 30) || low == 0 {
            return Err(Reader::damaged("its time code is unknown"));
        }
        Timing::Smpte {
            fps,
            per_frame: low,
        }
    };
    let mut tracks = Vec::new();
    let mut events = 0_usize;
    while !reader.is_done() && tracks.len() < track_count.min(MAX_TRACKS) {
        let id = reader.take(4)?;
        let length = reader.u32()? as usize;
        let body = reader.take(length.min(bytes.len() - reader.at))?;
        if id != b"MTrk" {
            continue; // Unknown chunks are skipped.
        }
        let track = read_track(body, &mut events)?;
        tracks.push(track);
    }
    if tracks.is_empty() && track_count > 0 {
        return Err(Reader::damaged("it has no tracks"));
    }
    Ok(MidiFile {
        format,
        timing,
        tracks,
    })
}

/// The MIDI data inside a RIFF "RMID" file, or the bytes as they are.
fn unwrap_rmid(bytes: &[u8]) -> &[u8] {
    if bytes.get(..4) == Some(b"RIFF")
        && bytes.get(8..12) == Some(b"RMID")
        && let Some(at) = bytes.windows(4).position(|w| w == b"MThd")
    {
        return &bytes[at..];
    }
    bytes
}

fn read_track(body: &[u8], events: &mut usize) -> Result<Track, MidiError> {
    let mut reader = Reader { bytes: body, at: 0 };
    let mut track = Vec::new();
    let mut tick = 0_u64;
    let mut end = None;
    let mut running: Option<u8> = None;
    while !reader.is_done() {
        *events += 1;
        if *events > MAX_EVENTS {
            return Err(MidiError::TooLarge);
        }
        tick += u64::from(reader.vlq()?);
        let mut status = reader.byte()?;
        let mut first = None;
        if status < 0x80 {
            // Running status: the byte read is the first data byte.
            first = Some(status);
            status = running.ok_or_else(|| Reader::damaged("an event has no status"))?;
        }
        match status {
            0xFF => {
                running = None;
                let kind = reader.byte()?;
                let length = reader.vlq()? as usize;
                let data = reader.take(length)?;
                if kind == 0x2F {
                    end = Some(tick);
                    break;
                }
                if let Some(kind) = meta_event(kind, data) {
                    track.push(Event { tick, kind });
                }
            }
            0xF0 | 0xF7 => {
                running = None;
                let length = reader.vlq()? as usize;
                reader.take(length)?;
            }
            0x80..=0xEF => {
                running = Some(status);
                let channel = status & 0x0F;
                let data1 = match first {
                    Some(byte) => byte,
                    None => reader.byte()?,
                } & 0x7F;
                let kind = status & 0xF0;
                let data2 = if matches!(kind, 0xC0 | 0xD0) {
                    0
                } else {
                    reader.byte()? & 0x7F
                };
                let event = match kind {
                    0x90 if data2 > 0 => Some(EventKind::NoteOn {
                        channel,
                        key: data1,
                        velocity: data2,
                    }),
                    0x80 | 0x90 => Some(EventKind::NoteOff {
                        channel,
                        key: data1,
                    }),
                    0xC0 => Some(EventKind::Program {
                        channel,
                        program: data1,
                    }),
                    _ => None,
                };
                if let Some(kind) = event {
                    track.push(Event { tick, kind });
                }
            }
            _ => {
                // System messages (F1 to FE) do not belong in files; their
                // length is unknown, so the rest of the track is unreadable.
                break;
            }
        }
    }
    let last = track.last().map_or(0, |e: &Event| e.tick);
    Ok(Track {
        events: track,
        end: end.unwrap_or(last).max(last),
    })
}

fn meta_event(kind: u8, data: &[u8]) -> Option<EventKind> {
    match (kind, data) {
        (0x51, [a, b, c]) => {
            let micros = u32::from_be_bytes([0, *a, *b, *c]);
            (micros > 0).then_some(EventKind::Tempo(micros))
        }
        (0x58, [numerator, power, ..]) if *numerator > 0 && *power <= 6 => {
            Some(EventKind::TimeSignature {
                numerator: *numerator,
                denominator_power: *power,
            })
        }
        (0x59, [sharps, minor]) => {
            #[allow(clippy::cast_possible_wrap, reason = "a signed byte")]
            let sharps = *sharps as i8;
            (-7..=7)
                .contains(&sharps)
                .then_some(EventKind::KeySignature {
                    sharps,
                    minor: *minor == 1,
                })
        }
        (0x03, name) => Some(EventKind::TrackName(
            String::from_utf8_lossy(name)
                .trim()
                .chars()
                .take(200)
                .collect(),
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Writing

fn push_vlq(out: &mut Vec<u8>, value: u32) {
    let mut bytes = [0_u8; 5];
    let mut count = 0;
    let mut rest = value;
    loop {
        bytes[count] = (rest & 0x7F) as u8;
        count += 1;
        rest >>= 7;
        if rest == 0 {
            break;
        }
    }
    for i in (0..count).rev() {
        let more = if i > 0 { 0x80 } else { 0 };
        out.push(bytes[i] | more);
    }
}

/// The order of events at the same tick: names and the conductor's
/// changes first, then note ends, then program changes, then note starts.
fn event_order(kind: &EventKind) -> u8 {
    match kind {
        EventKind::TrackName(_) => 0,
        EventKind::Tempo(_) | EventKind::TimeSignature { .. } | EventKind::KeySignature { .. } => 1,
        EventKind::NoteOff { .. } => 2,
        EventKind::Program { .. } => 3,
        EventKind::NoteOn { .. } => 4,
    }
}

/// Writes a MIDI file.
pub fn write(file: &MidiFile) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6_u32.to_be_bytes());
    out.extend_from_slice(&file.format.to_be_bytes());
    let count = u16::try_from(file.tracks.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&count.to_be_bytes());
    let division = match file.timing {
        Timing::PerQuarter(ppq) => ppq & 0x7FFF,
        Timing::Smpte { fps, per_frame } => {
            #[allow(clippy::cast_sign_loss, reason = "the format stores it so")]
            let high = (-(i16::from(fps))) as u16 & 0xFF;
            (high << 8) | u16::from(per_frame)
        }
    };
    out.extend_from_slice(&division.to_be_bytes());
    for track in file.tracks.iter().take(usize::from(count)) {
        let mut events: Vec<&Event> = track.events.iter().collect();
        events.sort_by_key(|e| (e.tick, event_order(&e.kind)));
        let mut body = Vec::new();
        let mut last = 0_u64;
        let delta = |from: u64, to: u64| {
            u32::try_from(to.saturating_sub(from))
                .unwrap_or(0x0FFF_FFFF)
                .min(0x0FFF_FFFF)
        };
        for event in events {
            push_vlq(&mut body, delta(last, event.tick));
            last = event.tick;
            write_event(&mut body, &event.kind);
        }
        push_vlq(&mut body, delta(last, track.end));
        body.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        out.extend_from_slice(b"MTrk");
        let length = u32::try_from(body.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&body);
    }
    out
}

fn write_event(body: &mut Vec<u8>, kind: &EventKind) {
    match kind {
        EventKind::NoteOn {
            channel,
            key,
            velocity,
        } => body.extend_from_slice(&[
            0x90 | (channel & 0x0F),
            key & 0x7F,
            (*velocity).clamp(1, 127),
        ]),
        EventKind::NoteOff { channel, key } => {
            body.extend_from_slice(&[0x80 | (channel & 0x0F), key & 0x7F, 64]);
        }
        EventKind::Program { channel, program } => {
            body.extend_from_slice(&[0xC0 | (channel & 0x0F), program & 0x7F]);
        }
        EventKind::Tempo(micros) => {
            let [_, a, b, c] = (*micros).clamp(1, 0x00FF_FFFF).to_be_bytes();
            body.extend_from_slice(&[0xFF, 0x51, 0x03, a, b, c]);
        }
        EventKind::TimeSignature {
            numerator,
            denominator_power,
        } => {
            // A metronome click every beat (24 clocks per quarter note).
            let clocks = (96_u32 >> denominator_power).clamp(1, 255) as u8;
            body.extend_from_slice(&[0xFF, 0x58, 0x04, *numerator, *denominator_power, clocks, 8]);
        }
        EventKind::KeySignature { sharps, minor } => {
            #[allow(clippy::cast_sign_loss, reason = "a signed byte")]
            let sharps = *sharps as u8;
            body.extend_from_slice(&[0xFF, 0x59, 0x02, sharps, u8::from(*minor)]);
        }
        EventKind::TrackName(name) => {
            let bytes = name.as_bytes();
            let length = u32::try_from(bytes.len().min(0xFFFF)).unwrap_or(0);
            body.extend_from_slice(&[0xFF, 0x03]);
            push_vlq(body, length);
            body.extend_from_slice(&bytes[..length as usize]);
        }
    }
}

// ---------------------------------------------------------------------------
// A song as a MIDI file

/// A track of events in the order a file holds them.
fn sorted_track(mut events: Vec<Event>) -> Track {
    events.sort_by_key(|e| (e.tick, event_order(&e.kind)));
    Track::new(events)
}

/// Our ticks as the exported file's (rounded to the nearest).
fn to_file_ticks(at: Ticks) -> u64 {
    let ratio = TICKS_PER_QUARTER / i64::from(EXPORT_PPQ);
    u64::try_from((at.0.max(0) + ratio / 2) / ratio).unwrap_or(0)
}

/// Sharps (positive) or flats of a key's signature, and whether it is
/// minor: major and minor keys by their own signature, modes by their
/// parent major key's.
fn key_signature(scale: Scale) -> (i8, bool) {
    // Sharps of the major key on each pitch class (D flat, not C sharp).
    const MAJOR: [i8; 12] = [0, -5, 2, -3, 4, -1, 6, 1, -4, 3, -2, 5];
    let (major_tonic, minor) = match scale.kind {
        ScaleKind::Major | ScaleKind::MajorPentatonic => (scale.tonic, false),
        ScaleKind::Minor
        | ScaleKind::HarmonicMinor
        | ScaleKind::MelodicMinor
        | ScaleKind::MinorPentatonic
        | ScaleKind::Blues => (scale.tonic + 3, true),
        ScaleKind::Dorian => (scale.tonic + 10, false),
        ScaleKind::Phrygian => (scale.tonic + 8, false),
        ScaleKind::Lydian => (scale.tonic + 7, false),
        ScaleKind::Mixolydian => (scale.tonic + 5, false),
        ScaleKind::Locrian => (scale.tonic + 1, false),
    };
    (MAJOR[usize::from(major_tonic % 12)], minor)
}

/// The General MIDI program nearest to a track's instrument.
fn program_for(instrument: &Instrument) -> u8 {
    match instrument {
        Instrument::Synth(settings) => match SynthPreset::matching(settings) {
            Some(SynthPreset::SoftKeys) => 4,
            Some(SynthPreset::WarmPad) => 89,
            Some(SynthPreset::Bass) => 38,
            Some(SynthPreset::SubBass) => 39,
            Some(SynthPreset::Pluck) => 45,
            Some(SynthPreset::Brass) => 62,
            Some(SynthPreset::Lead | SynthPreset::Plain) | None => 81,
        },
        _ => 0,
    }
}

/// The tempo events of a song: its changes, and every glide as steps a
/// sixteenth note long, each lasting exactly as long as the glide over it
/// (decision 0018).
fn tempo_events(tempo: &TempoMap) -> Vec<Event> {
    let micros = |bpm: f64| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "tempos are in range"
        )]
        let micros = (60_000_000.0 / bpm).round() as u32;
        EventKind::Tempo(micros)
    };
    let step = Ticks::note_value(1, 16);
    let changes = tempo.changes();
    let mut events = Vec::new();
    for (index, change) in changes.iter().enumerate() {
        let next = changes.get(index + 1);
        match next {
            Some(next) if change.glide => {
                let mut at = change.at;
                while at < next.at {
                    let end = (at + step).min(next.at);
                    let seconds = tempo.seconds_at(end) - tempo.seconds_at(at);
                    let bpm = (end - at).as_quarters() * 60.0 / seconds.max(1e-9);
                    events.push(Event {
                        tick: to_file_ticks(at),
                        kind: micros(bpm),
                    });
                    at = end;
                }
            }
            _ => events.push(Event {
                tick: to_file_ticks(change.at),
                kind: micros(change.bpm),
            }),
        }
    }
    events
}

/// A song as a MIDI file (decision 0020): the conductor track (title,
/// tempo, time and key signatures), then one track per instrument track
/// with the notes the song plays: every clip's repeats, cut at the clip's
/// end, swung. Audio tracks are left out.
#[allow(clippy::too_many_lines, reason = "one pass over the song")]
pub fn song_to_midi(project: &Project) -> MidiFile {
    let mut conductor = Vec::new();
    if !project.title.is_empty() {
        conductor.push(Event {
            tick: 0,
            kind: EventKind::TrackName(project.title.clone()),
        });
    }
    conductor.extend(tempo_events(&project.tempo));
    for change in project.meter.changes() {
        let signature = change.signature;
        conductor.push(Event {
            tick: to_file_ticks(project.meter.bar_start(change.bar)),
            kind: EventKind::TimeSignature {
                numerator: signature.numerator,
                denominator_power: u8::try_from(signature.denominator.trailing_zeros())
                    .unwrap_or(2),
            },
        });
    }
    for change in project.key.changes() {
        let at = to_file_ticks(project.meter.bar_start(change.bar));
        let (sharps, minor) = match change.scale {
            Some(scale) => key_signature(scale),
            None if change.bar == 0 => continue,
            None => (0, false),
        };
        conductor.push(Event {
            tick: at,
            kind: EventKind::KeySignature { sharps, minor },
        });
    }
    let mut tracks = vec![sorted_track(conductor)];
    let mut next_channel = 0_u8;
    for track in project.tracks.iter().filter(|t| !t.instrument.is_audio()) {
        let channel = if track.instrument.is_drums() {
            DRUM_CHANNEL
        } else {
            let channel = next_channel;
            next_channel = (next_channel + 1) % 16;
            if next_channel == DRUM_CHANNEL {
                next_channel += 1;
            }
            channel
        };
        let mut events = vec![Event {
            tick: 0,
            kind: EventKind::TrackName(track.name.clone()),
        }];
        if !track.instrument.is_drums() {
            events.push(Event {
                tick: 0,
                kind: EventKind::Program {
                    channel,
                    program: program_for(&track.instrument),
                },
            });
        }
        let mut notes: Vec<(u64, u64, u8, u8)> = track
            .clips
            .iter()
            .filter_map(|clip| Some((clip, project.pattern(clip.pattern()?)?)))
            .flat_map(|(clip, pattern)| pattern.played_in(clip))
            .map(|n| {
                let start = to_file_ticks(n.start);
                let end = to_file_ticks(n.end).max(start + 1);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a velocity from 1 to 127"
                )]
                let velocity = (n.velocity * 127.0).round().clamp(1.0, 127.0) as u8;
                (start, end, n.key, velocity)
            })
            .collect();
        notes.sort_unstable();
        // A key started again before it ends: the earlier note ends there,
        // so every start has its own end.
        let mut ends: HashMap<u8, usize> = HashMap::new();
        for index in 0..notes.len() {
            let (start, _, key, _) = notes[index];
            if let Some(&previous) = ends.get(&key)
                && notes[previous].1 > start
            {
                notes[previous].1 = start.max(notes[previous].0 + 1);
            }
            ends.insert(key, index);
        }
        for (start, end, key, velocity) in notes {
            events.push(Event {
                tick: start,
                kind: EventKind::NoteOn {
                    channel,
                    key,
                    velocity,
                },
            });
            events.push(Event {
                tick: end,
                kind: EventKind::NoteOff { channel, key },
            });
        }
        tracks.push(sorted_track(events));
    }
    MidiFile {
        format: 1,
        timing: Timing::PerQuarter(EXPORT_PPQ),
        tracks,
    }
}

// ---------------------------------------------------------------------------
// A MIDI file as tracks of a song

/// One track of notes read from a MIDI file: one channel of one of its
/// tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedTrack {
    /// The name the file gives its track, if any.
    pub name: Option<String>,
    /// The MIDI channel, from 0.
    pub channel: u8,
    /// The instrument chosen for it: drums on channel 10, the synth for
    /// General MIDI synth sounds, otherwise the piano.
    pub instrument: Instrument,
    /// Its notes, from the start of the file.
    pub notes: Vec<Note>,
}

/// What a MIDI file holds, as VLabs understands it.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedSong {
    /// The song's name, from the first track's name in a format 1 file.
    pub title: Option<String>,
    /// The file's tempo changes.
    pub tempo: TempoMap,
    /// The file's time signatures (each at the nearest bar line).
    pub meter: MeterMap,
    /// The file's key signatures (each at the nearest bar line).
    pub key: KeyMap,
    /// The tracks with notes.
    pub tracks: Vec<ImportedTrack>,
}

/// The instrument for a channel and its General MIDI program.
fn instrument_for(channel: u8, program: Option<u8>) -> Instrument {
    if channel == DRUM_CHANNEL {
        return Instrument::drums(ACOUSTIC_KIT);
    }
    let preset = match program {
        Some(38 | 39) => Some(SynthPreset::Bass),
        Some(80..=87) => Some(SynthPreset::Lead),
        Some(50 | 51 | 88..=103) => Some(SynthPreset::WarmPad),
        Some(62 | 63) => Some(SynthPreset::Brass),
        _ => None,
    };
    preset.map_or_else(Instrument::builtin_piano, |p| {
        Instrument::Synth(p.settings())
    })
}

/// The key a signature names.
fn scale_for(sharps: i8, minor: bool) -> Scale {
    // The major key with each number of sharps, from seven flats.
    const MAJOR: [u8; 15] = [11, 6, 1, 8, 3, 10, 5, 0, 7, 2, 9, 4, 11, 6, 1];
    let index = usize::try_from(i16::from(sharps.clamp(-7, 7)) + 7).unwrap_or(7);
    let tonic = MAJOR[index];
    if minor {
        Scale::new(tonic + 9, ScaleKind::Minor)
    } else {
        Scale::new(tonic, ScaleKind::Major)
    }
}

/// Reads a MIDI file into tracks of notes, with its tempo, time and key
/// signatures.
#[allow(clippy::too_many_lines, reason = "one pass over the file")]
pub fn import(bytes: &[u8]) -> Result<ImportedSong, MidiError> {
    let file = read(bytes)?;
    // Positions as our ticks.
    let to_ticks = |tick: u64| -> Ticks {
        let tick = i128::from(tick);
        let value = match file.timing {
            Timing::PerQuarter(ppq) => {
                let ppq = i128::from(ppq);
                (tick * i128::from(TICKS_PER_QUARTER) + ppq / 2) / ppq
            }
            // Time in seconds: read at 120 beats per minute.
            Timing::Smpte { fps, per_frame } => {
                let per_second = i128::from(fps) * i128::from(per_frame);
                (tick * 2 * i128::from(TICKS_PER_QUARTER) + per_second / 2) / per_second
            }
        };
        Ticks(i64::try_from(value).unwrap_or(i64::MAX / 4))
    };
    // Format 2 files hold separate songs: they are laid one after another.
    let mut offsets = vec![0_u64; file.tracks.len()];
    if file.format == 2 {
        let mut at = 0;
        for (offset, track) in offsets.iter_mut().zip(&file.tracks) {
            *offset = at;
            at += track.end;
        }
    }
    let mut conductor: Vec<(u64, &EventKind)> = file
        .tracks
        .iter()
        .zip(&offsets)
        .flat_map(|(track, offset)| track.events.iter().map(move |e| (e.tick + offset, &e.kind)))
        .filter(|(_, kind)| {
            matches!(
                kind,
                EventKind::Tempo(_)
                    | EventKind::TimeSignature { .. }
                    | EventKind::KeySignature { .. }
            )
        })
        .collect();
    conductor.sort_by_key(|(tick, _)| *tick);

    let tempo = if matches!(file.timing, Timing::Smpte { .. }) {
        TempoMap::constant(120.0)
    } else {
        TempoMap::from_changes(conductor.iter().filter_map(|(tick, kind)| match kind {
            EventKind::Tempo(micros) => Some(TempoChange::new(
                to_ticks(*tick),
                60_000_000.0 / f64::from(*micros),
            )),
            _ => None,
        }))
    };
    let mut meter = MeterMap::default();
    let mut key = KeyMap::default();
    for (tick, kind) in &conductor {
        let at = to_ticks(*tick);
        // The nearest bar line under the signatures so far.
        let nearest_bar = |meter: &MeterMap| {
            let position = meter.position(at);
            let start = meter.bar_start(position.bar);
            let next = meter.bar_start(position.bar + 1);
            if (at - start).0 * 2 < (next - start).0 {
                position.bar
            } else {
                position.bar + 1
            }
        };
        match kind {
            EventKind::TimeSignature {
                numerator,
                denominator_power,
            } => {
                let denominator = 1_u32 << denominator_power;
                let signature = u8::try_from(denominator)
                    .ok()
                    .and_then(|d| TimeSignature::new(*numerator, d));
                if let Some(signature) = signature {
                    meter.set(nearest_bar(&meter), signature);
                }
            }
            EventKind::KeySignature { sharps, minor } => {
                key.set(nearest_bar(&meter), Some(scale_for(*sharps, *minor)));
            }
            _ => {}
        }
    }
    meter.repair();
    key.repair();

    let mut title = None;
    let mut tracks = Vec::new();
    for (index, (track, offset)) in file.tracks.iter().zip(&offsets).enumerate() {
        let name = track.events.iter().find_map(|e| match &e.kind {
            EventKind::TrackName(name) if !name.is_empty() => Some(name.clone()),
            _ => None,
        });
        let has_notes = track
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::NoteOn { .. }));
        if index == 0 && file.format == 1 && !has_notes {
            title.clone_from(&name);
        }
        let end = track.end + offset;
        let mut programs: [Option<u8>; 16] = [None; 16];
        let mut held: HashMap<(u8, u8), Vec<(u64, u8)>> = HashMap::new();
        let mut by_channel: Vec<Vec<Note>> = vec![Vec::new(); 16];
        let note = |start: u64, finish: u64, key: u8, velocity: u8| Note {
            id: NoteId(0),
            start: to_ticks(start),
            length: (to_ticks(finish) - to_ticks(start)).max(Ticks(1)),
            key,
            velocity: f32::from(velocity) / 127.0,
            pitch_offset: 0.0,
        };
        for event in &track.events {
            let tick = event.tick + offset;
            match event.kind {
                EventKind::Program { channel, program } => {
                    let slot = &mut programs[usize::from(channel)];
                    if slot.is_none() && by_channel[usize::from(channel)].is_empty() {
                        *slot = Some(program);
                    }
                }
                EventKind::NoteOn {
                    channel,
                    key,
                    velocity,
                } => held
                    .entry((channel, key))
                    .or_default()
                    .push((tick, velocity)),
                EventKind::NoteOff { channel, key } => {
                    if let Some(starts) = held.get_mut(&(channel, key))
                        && !starts.is_empty()
                    {
                        let (start, velocity) = starts.remove(0);
                        by_channel[usize::from(channel)].push(note(start, tick, key, velocity));
                    }
                }
                _ => {}
            }
        }
        // Notes still held when the track ends end there.
        for ((channel, key), starts) in held {
            for (start, velocity) in starts {
                by_channel[usize::from(channel)].push(note(
                    start,
                    end.max(start + 1),
                    key,
                    velocity,
                ));
            }
        }
        for (channel, mut notes) in by_channel.into_iter().enumerate() {
            if notes.is_empty() {
                continue;
            }
            notes.sort_by_key(|n| (n.start, n.key));
            let channel = u8::try_from(channel).unwrap_or(0);
            tracks.push(ImportedTrack {
                name: name.clone(),
                channel,
                instrument: instrument_for(channel, programs[usize::from(channel)]),
                notes,
            });
        }
    }
    Ok(ImportedSong {
        title,
        tempo,
        meter,
        key,
        tracks,
    })
}

impl ImportedSong {
    /// The song as a new project: the file's tempo, signatures and key,
    /// a track for each of its tracks, a limiter on the master like every
    /// new song. `name` names each track (from the file's own name, if
    /// any), `pattern` its pattern.
    pub fn into_project(self, name: impl Fn(&ImportedTrack) -> String, pattern: &str) -> Project {
        let mut project = Project::empty();
        if let Some(title) = &self.title {
            title.clone_into(&mut project.title);
        }
        project.tempo = self.tempo.clone();
        project.meter = self.meter.clone();
        project.key = self.key.clone();
        self.add_to(&mut project, Ticks::ZERO, name, pattern);
        project.add_effect(
            ChainPlace::Master,
            EffectKind::Limiter(LimiterSettings::default()),
        );
        project
    }

    /// Adds the file's tracks to a song, their notes starting at `at` (the
    /// song keeps its own tempo and signatures). Each track gets one
    /// pattern of whole bars and one clip playing it. Returns the new
    /// tracks.
    pub fn add_to(
        &self,
        project: &mut Project,
        at: Ticks,
        name: impl Fn(&ImportedTrack) -> String,
        pattern: &str,
    ) -> Vec<TrackId> {
        let at = at.max(Ticks::ZERO);
        let mut added = Vec::new();
        for imported in &self.tracks {
            let track = project.add_track(&name(imported), imported.instrument.clone());
            let end = imported
                .notes
                .iter()
                .map(Note::end)
                .max()
                .unwrap_or(Ticks::ZERO);
            // Whole bars, from the bar where the notes land.
            let bars_from = project.meter.position(at).bar;
            let start = project.meter.bar_start(bars_from);
            let shift = at - start;
            let mut bars = 1;
            while project.meter.bar_start(bars_from + bars) - start < end + shift && bars < 100_000
            {
                bars += 1;
            }
            let length = project.meter.bar_start(bars_from + bars) - start;
            let id = project.add_pattern_for(track, pattern, length);
            let notes: Vec<Note> = imported
                .notes
                .iter()
                .map(|n| Note {
                    start: n.start + shift,
                    ..*n
                })
                .filter(|n| n.start < length)
                .collect();
            project.add_notes(id, &notes);
            project.add_clip(track, start, length, id);
            added.push(track);
        }
        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_length_numbers_round_trip() {
        for value in [0, 1, 127, 128, 8191, 16_383, 16_384, 0x0FFF_FFFF] {
            let mut out = Vec::new();
            push_vlq(&mut out, value);
            let mut reader = Reader { bytes: &out, at: 0 };
            assert_eq!(reader.vlq().unwrap(), value);
            assert!(reader.is_done());
        }
    }

    #[test]
    fn key_signatures_match_the_keys() {
        assert_eq!(key_signature(Scale::new(0, ScaleKind::Major)), (0, false));
        assert_eq!(key_signature(Scale::new(9, ScaleKind::Minor)), (0, true));
        assert_eq!(key_signature(Scale::new(2, ScaleKind::Major)), (2, false));
        assert_eq!(key_signature(Scale::new(5, ScaleKind::Major)), (-1, false));
        assert_eq!(key_signature(Scale::new(2, ScaleKind::Dorian)), (0, false));
        assert_eq!(
            key_signature(Scale::new(7, ScaleKind::Mixolydian)),
            (0, false)
        );
        assert_eq!(key_signature(Scale::new(10, ScaleKind::Minor)), (-5, true));
        for sharps in -7..=7 {
            for minor in [false, true] {
                let scale = scale_for(sharps, minor);
                let (back, back_minor) = key_signature(scale);
                assert_eq!(back_minor, minor);
                // Six sharps and six flats name the same key (F# or Gb).
                assert!(back == sharps || (back - sharps).abs() == 12 || sharps.abs() == 7);
            }
        }
    }

    #[test]
    fn damaged_files_give_errors_not_crashes() {
        assert_eq!(read(b"hello"), Err(MidiError::NotMidi));
        assert!(read(b"MThd").is_err());
        assert!(
            read(b"MThd\0\0\0\x06\0\x01\0\x01\0\0").is_err(),
            "zero resolution"
        );
        let mut file = b"MThd\0\0\0\x06\0\x00\0\x01\x01\xe0MTrk\0\0\0\x04".to_vec();
        file.extend_from_slice(&[0x00, 0x90, 0x3C]); // cut off
        assert!(read(&file).is_err());
        // Every prefix of a real file fails cleanly or reads.
        let song = song_to_midi(&Project::starter("P", "T"));
        let bytes = write(&song);
        for cut in 0..bytes.len() {
            let _ = read(&bytes[..cut]);
        }
    }
}

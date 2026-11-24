// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading SFZ instruments (decision 0024): a text file of regions, each
//! naming a sound file and saying when and how it plays, read into a
//! ready-to-play [`SamplerBank`] with its sounds decoded.
//!
//! The file comes from anywhere: what this reader does not understand is
//! listed (the person is told), never a crash; sound files that cannot be
//! found or read are left out and named; and an instrument may not hold
//! more than [`MAX_SOUND_BYTES`] of decoded sound.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use daw_dsp::sampler::{
    CrossfadeCurve, EnvelopeSpec, FilterSpec, KeySwitch, LeftOut, LoopMode, OffMode, Region,
    Sample, SamplerBank, Trigger,
};
use daw_dsp::synth::FilterMode;

use crate::audio;

/// The most decoded sound one instrument may hold, in bytes (8 GB).
pub const MAX_SOUND_BYTES: usize = 8 * 1024 * 1024 * 1024;

/// The most text an instrument's files may hold, includes counted.
const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;

/// How deep includes may go.
const MAX_INCLUDE_DEPTH: usize = 16;

/// The most regions an instrument may have.
const MAX_REGIONS: usize = 200_000;

/// The most settings named one by one in the "left out" list.
const MAX_OTHER_NAMES: usize = 8;

/// The frames of one cycle of the built-in sine.
const SINE_FRAMES: usize = 2048;

/// Why an SFZ instrument could not be read.
#[derive(Debug)]
pub enum SfzError {
    /// The instrument's file could not be read.
    Io(std::io::Error),
    /// It holds no region that can play.
    NoRegions,
    /// None of its sounds could be read: the first sound and why.
    NoSounds(String),
    /// Its sounds take more than [`MAX_SOUND_BYTES`].
    TooLarge,
    /// Its text is far larger than any instrument's.
    TooMuchText,
}

impl fmt::Display for SfzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::NoRegions => write!(f, "it is not an SFZ instrument, or it plays nothing"),
            Self::NoSounds(why) => write!(f, "none of its sounds could be read ({why})"),
            Self::TooLarge => write!(f, "its sounds would take more than 8 GB of memory"),
            Self::TooMuchText => write!(f, "it is too large to be an SFZ instrument"),
        }
    }
}

impl std::error::Error for SfzError {}

impl From<std::io::Error> for SfzError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Reads an SFZ instrument and the sounds it names.
pub fn load(path: &Path) -> Result<SamplerBank, SfzError> {
    let folder = path.parent().unwrap_or_else(|| Path::new(""));
    let text = read_text(path)?;
    let parsed = parse(&text, folder)?;
    finish(parsed, folder)
}

/// Reads a text file of the instrument (any encoding: what is not UTF-8 is
/// replaced, which only matters in names).
fn read_text(path: &Path) -> Result<String, SfzError> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > MAX_TEXT_BYTES as u64 {
        return Err(SfzError::TooMuchText);
    }
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ----- The text

/// One piece of the text: a header or a setting.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Header(String),
    Opcode(String, String),
}

/// Reads the text into headers and settings, following `#define` and
/// `#include`.
struct Tokens<'a> {
    folder: &'a Path,
    defines: Vec<(String, String)>,
    pieces: Vec<Piece>,
    text_bytes: usize,
    /// Included files that could not be found or read.
    missing: Vec<String>,
}

impl Tokens<'_> {
    fn read(&mut self, text: &str, depth: usize) -> Result<(), SfzError> {
        self.text_bytes += text.len();
        if self.text_bytes > MAX_TEXT_BYTES {
            return Err(SfzError::TooMuchText);
        }
        let text = strip_comments(text);
        let bytes = text.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at].is_ascii_whitespace() {
                at += 1;
                continue;
            }
            if bytes[at] == b'<' {
                let end = text[at..].find('>').map_or(bytes.len(), |i| at + i);
                let name = self
                    .substitute(text[at + 1..end].trim())
                    .to_ascii_lowercase();
                self.pieces.push(Piece::Header(name));
                at = (end + 1).min(bytes.len());
                continue;
            }
            if text[at..].starts_with("#define") {
                at += "#define".len();
                let (name, after) = word(&text, at);
                let (value, after) = word(&text, after);
                let name = name.strip_prefix('$').unwrap_or(name);
                if !name.is_empty() {
                    let value = self.substitute(value).into_owned();
                    self.defines.push((name.to_owned(), value));
                }
                at = after;
                continue;
            }
            if text[at..].starts_with("#include") {
                at += "#include".len();
                let (file, after) = quoted(&text, at);
                at = after;
                let file = self.substitute(file).into_owned();
                self.include(&file, depth)?;
                continue;
            }
            // A setting: name=value. The value runs to the end of the line
            // or to the next setting on it (sound paths hold spaces).
            let start = at;
            while at < bytes.len()
                && !bytes[at].is_ascii_whitespace()
                && bytes[at] != b'='
                && bytes[at] != b'<'
            {
                at += 1;
            }
            if at >= bytes.len() || bytes[at] != b'=' {
                // Not a setting: skip the word.
                at = at.max(start + 1);
                continue;
            }
            let name = self.substitute(&text[start..at]).to_ascii_lowercase();
            at += 1;
            let value_start = at;
            while at < bytes.len()
                && bytes[at] != b'\n'
                && bytes[at] != b'\r'
                && bytes[at] != b'<'
                && !(bytes[at].is_ascii_whitespace() && next_is_setting(bytes, at))
            {
                at += 1;
            }
            let value = self.substitute(text[value_start..at].trim()).into_owned();
            if !name.is_empty() {
                self.pieces.push(Piece::Opcode(name, value));
            }
        }
        Ok(())
    }

    /// Reads an included file in place (its path relative to the
    /// instrument's folder, else to the including file's).
    fn include(&mut self, file: &str, depth: usize) -> Result<(), SfzError> {
        if depth >= MAX_INCLUDE_DEPTH {
            self.missing.push(file.to_owned());
            return Ok(());
        }
        let Some(path) = find_file(self.folder, file) else {
            self.missing.push(file.to_owned());
            return Ok(());
        };
        match read_text(&path) {
            Ok(text) => self.read(&text, depth + 1),
            Err(SfzError::TooMuchText) => Err(SfzError::TooMuchText),
            Err(_) => {
                self.missing.push(file.to_owned());
                Ok(())
            }
        }
    }

    /// Replaces each `$NAME` defined so far by its value.
    fn substitute<'t>(&self, text: &'t str) -> Cow<'t, str> {
        if !text.contains('$') || self.defines.is_empty() {
            return Cow::Borrowed(text);
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(dollar) = rest.find('$') {
            out.push_str(&rest[..dollar]);
            let after = &rest[dollar + 1..];
            let length = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            let name = &after[..length];
            if let Some((_, value)) = self.defines.iter().rev().find(|(n, _)| n == name) {
                out.push_str(value);
            } else {
                out.push('$');
                out.push_str(name);
            }
            rest = &after[length..];
        }
        out.push_str(rest);
        Cow::Owned(out)
    }
}

/// The text without its comments (`// to the end of the line` and
/// `/* anything */`).
fn strip_comments(text: &str) -> Cow<'_, str> {
    if !text.contains("//") && !text.contains("/*") {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let line = rest.find("//");
        let block = rest.find("/*");
        match (line, block) {
            (Some(l), b) if b.is_none_or(|b| l < b) => {
                out.push_str(&rest[..l]);
                rest = rest[l..].find('\n').map_or("", |n| &rest[l + n..]);
            }
            (_, Some(b)) => {
                out.push_str(&rest[..b]);
                out.push(' ');
                rest = rest[b + 2..]
                    .find("*/")
                    .map_or("", |e| &rest[b + 2 + e + 2..]);
            }
            _ => {
                out.push_str(rest);
                return Cow::Owned(out);
            }
        }
    }
}

/// The next word after `at` on the same line, and where it ends.
fn word(text: &str, at: usize) -> (&str, usize) {
    let bytes = text.as_bytes();
    let mut start = at;
    while start < bytes.len() && (bytes[start] == b' ' || bytes[start] == b'\t') {
        start += 1;
    }
    let mut end = start;
    while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
        end += 1;
    }
    (&text[start..end], end)
}

/// The next quoted text after `at` (or word, if it is not quoted), and
/// where it ends.
fn quoted(text: &str, at: usize) -> (&str, usize) {
    let rest = &text[at..];
    let trimmed = rest.trim_start_matches([' ', '\t']);
    let start = at + rest.len() - trimmed.len();
    if let Some(inner) = trimmed.strip_prefix('"') {
        let end = inner.find(['"', '\n']).unwrap_or(inner.len());
        return (&inner[..end], (start + 1 + end + 1).min(text.len()));
    }
    word(text, at)
}

/// Whether, after the white space at `at`, the line goes on with another
/// setting (`name=`), a header or a directive.
fn next_is_setting(bytes: &[u8], at: usize) -> bool {
    let mut i = at;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || matches!(bytes[i], b'<' | b'#' | b'\n' | b'\r') {
        return true;
    }
    let start = i;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
    {
        i += 1;
    }
    i > start && i < bytes.len() && bytes[i] == b'='
}

// ----- Regions

/// Where a region's sound comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Sound {
    /// A file, as the instrument names it (with its default path).
    File(String),
    /// The built-in sine, at the pitch of a key.
    Sine(u8),
    /// Nothing (the region only cuts off its choke group).
    Silence,
}

/// A region read from the text, before its sound is found.
#[derive(Clone, Debug)]
struct Parsed {
    region: Region,
    sound: Sound,
    /// Its key center is the one its sound file holds.
    keycenter_from_sound: bool,
}

/// The whole instrument's text, read.
#[derive(Debug, Default)]
struct ParsedInstrument {
    regions: Vec<Parsed>,
    keyswitch: Option<KeySwitch>,
    left_out: BTreeSet<LeftOut>,
    missing: Vec<String>,
}

/// The instrument-wide settings of `<control>`.
#[derive(Clone)]
struct Control {
    default_path: String,
    key_offset: i32,
    /// The controllers' starting values (0 to 127).
    cc: [f32; 128],
}

impl Default for Control {
    fn default() -> Self {
        let mut cc = [0.0; 128];
        // MIDI's usual starting values: volume, pan, expression.
        cc[7] = 100.0;
        cc[10] = 64.0;
        cc[11] = 127.0;
        Self {
            default_path: String::new(),
            key_offset: 0,
            cc,
        }
    }
}

/// Which level settings go to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    None,
    Control,
    Global,
    Master,
    Group,
    Region,
    Ignored,
}

/// The keyswitch settings met across the instrument.
#[derive(Default)]
struct Switches {
    low: Option<u8>,
    high: Option<u8>,
    default: Option<u8>,
    used: BTreeSet<u8>,
}

/// Settings, by name and value, in order.
type Settings = Vec<(String, String)>;

/// A region's settings (from `<global>` down) and the control settings it
/// was read under.
struct Finished {
    settings: Settings,
    default_path: String,
    key_offset: i32,
}

/// Reads an instrument's text (includes found from `folder`).
fn parse(text: &str, folder: &Path) -> Result<ParsedInstrument, SfzError> {
    let mut tokens = Tokens {
        folder,
        defines: Vec::new(),
        pieces: Vec::new(),
        text_bytes: 0,
        missing: Vec::new(),
    };
    tokens.read(text, 0)?;
    let mut instrument = ParsedInstrument {
        missing: tokens.missing,
        ..ParsedInstrument::default()
    };
    let mut control = Control::default();
    // The settings of the levels above the region, and the region's own:
    // each region is everything from `<global>` down, in order.
    let mut levels: [Settings; 4] = Default::default();
    let mut level = Level::None;
    let mut finished: Vec<Finished> = Vec::new();
    let mut finish = |levels: &[Settings; 4], control: &Control| {
        if finished.len() < MAX_REGIONS {
            finished.push(Finished {
                settings: levels.concat(),
                default_path: control.default_path.clone(),
                key_offset: control.key_offset,
            });
        }
    };
    for piece in tokens.pieces {
        match piece {
            Piece::Header(name) => {
                if level == Level::Region {
                    finish(&levels, &control);
                }
                level = match name.as_str() {
                    "control" => Level::Control,
                    "global" => {
                        levels.iter_mut().for_each(Vec::clear);
                        Level::Global
                    }
                    "master" => {
                        levels[1..].iter_mut().for_each(Vec::clear);
                        Level::Master
                    }
                    "group" => {
                        levels[2..].iter_mut().for_each(Vec::clear);
                        Level::Group
                    }
                    "region" => {
                        levels[3].clear();
                        Level::Region
                    }
                    "effect" => {
                        instrument.left_out.insert(LeftOut::Effects);
                        Level::Ignored
                    }
                    "sample" => {
                        instrument.left_out.insert(LeftOut::EmbeddedSounds);
                        Level::Ignored
                    }
                    // `<curve>` serves controller changes (left out where
                    // they are used); `<midi>` names controls.
                    _ => Level::Ignored,
                };
            }
            Piece::Opcode(name, value) => match level {
                Level::Control => control_setting(&mut control, &name, &value),
                Level::Global => levels[0].push((name, value)),
                Level::Master => levels[1].push((name, value)),
                Level::Group => levels[2].push((name, value)),
                Level::Region => levels[3].push((name, value)),
                Level::None | Level::Ignored => {}
            },
        }
    }
    if level == Level::Region {
        finish(&levels, &control);
    }
    let mut switches = Switches::default();
    let mut other = BTreeSet::new();
    for region in &finished {
        let mut reading = RegionReader::new(region.key_offset, &control.cc);
        for (name, value) in &region.settings {
            reading.setting(
                name,
                value,
                &mut instrument.left_out,
                &mut other,
                &mut switches,
            );
        }
        if let Some(parsed) = reading.finish(&region.default_path, &mut instrument.left_out) {
            if let Some(key) = parsed.region.sw_last {
                switches.used.insert(key);
            }
            instrument.regions.push(parsed);
        }
    }
    instrument
        .left_out
        .extend(other.into_iter().take(MAX_OTHER_NAMES).map(LeftOut::Other));
    instrument.keyswitch = keyswitch(&switches);
    Ok(instrument)
}

/// A `<control>` setting.
fn control_setting(control: &mut Control, name: &str, value: &str) {
    match name {
        "default_path" => value.clone_into(&mut control.default_path),
        "octave_offset" => {
            control.key_offset = control.key_offset % 12 + 12 * int(value).unwrap_or(0);
        }
        "note_offset" => {
            control.key_offset =
                control.key_offset - control.key_offset % 12 + int(value).unwrap_or(0);
        }
        _ => {
            if let Some(number) = cc_number(name, "set_cc").or_else(|| cc_number(name, "set_hdcc"))
                && let Some(v) = real(value)
            {
                // `set_hdcc` gives 0 to 1; `set_cc` 0 to 127.
                let v = if name.starts_with("set_hdcc") {
                    v * 127.0
                } else {
                    v
                };
                control.cc[number] = v.clamp(0.0, 127.0);
            }
        }
    }
}

/// The keyswitches, from the settings met (a range given, or the keys the
/// regions use; the default given, or the lowest).
fn keyswitch(switches: &Switches) -> Option<KeySwitch> {
    let (first, last) = (switches.used.first()?, switches.used.last()?);
    Some(KeySwitch {
        lokey: switches.low.unwrap_or(*first).min(*first),
        hikey: switches.high.unwrap_or(*last).max(*last),
        default: switches.default.or(Some(*first)),
    })
}

/// A crossfade range by a controller: in or out, low and high.
#[derive(Clone, Copy, Default)]
struct CcFade {
    in_low: Option<f32>,
    in_high: Option<f32>,
    out_low: Option<f32>,
    out_high: Option<f32>,
}

/// Turns one region's settings, in order, into a [`Region`].
struct RegionReader<'a> {
    region: Region,
    sound: Option<String>,
    keycenter_from_sound: bool,
    /// It can never play (a key range below 0, a channel other than 1…).
    silent: bool,
    key_offset: i32,
    cc: &'a [f32; 128],
    /// Controller conditions: (controller, lowest, highest).
    cc_ranges: BTreeMap<usize, (f32, f32)>,
    cc_fades: BTreeMap<usize, CcFade>,
    xf_cccurve: CrossfadeCurve,
    /// The filter's kind, if given; `unknown_filter` if of a kind the
    /// sampler lacks.
    filter_kind: Option<FilterMode>,
    unknown_filter: bool,
    cutoff: Option<f32>,
    resonance: f32,
    fil_keytrack: f32,
    fil_keycenter: u8,
    fil_veltrack: f32,
    fil_random: f32,
    fileg: EnvelopeSpec,
    extra_volume: f32,
    extra_amplitude: f32,
    extra_tune: f32,
    /// Settings moved by controllers: (what, controller) and how far at
    /// the controller's top, applied at its starting value.
    modulations: BTreeMap<(String, usize), f32>,
    /// How deep its LFOs, and its pitch and extra envelopes, go, by
    /// setting.
    lfo_depths: BTreeMap<String, f32>,
    envelope_depths: BTreeMap<String, f32>,
}

impl<'a> RegionReader<'a> {
    fn new(key_offset: i32, cc: &'a [f32; 128]) -> Self {
        Self {
            region: Region::default(),
            sound: None,
            keycenter_from_sound: false,
            silent: false,
            key_offset,
            cc,
            cc_ranges: BTreeMap::new(),
            cc_fades: BTreeMap::new(),
            xf_cccurve: CrossfadeCurve::Power,
            filter_kind: None,
            unknown_filter: false,
            cutoff: None,
            resonance: 0.0,
            fil_keytrack: 0.0,
            fil_keycenter: 60,
            fil_veltrack: 0.0,
            fil_random: 0.0,
            fileg: EnvelopeSpec::filter(),
            extra_volume: 0.0,
            extra_amplitude: 1.0,
            extra_tune: 0.0,
            modulations: BTreeMap::new(),
            lfo_depths: BTreeMap::new(),
            envelope_depths: BTreeMap::new(),
        }
    }

    /// A key setting's value: a note number or name, moved by the
    /// instrument's offsets.
    fn key(&self, value: &str) -> Option<i32> {
        key_number(value).map(|key| key + self.key_offset)
    }

    /// A key setting that must be a key (0 to 127).
    fn key_u8(&self, value: &str) -> Option<u8> {
        self.key(value)
            .and_then(|key| u8::try_from(key).ok())
            .filter(|&key| key <= 127)
    }

    #[allow(clippy::too_many_lines, reason = "one arm per setting")]
    fn setting(
        &mut self,
        name: &str,
        value: &str,
        left_out: &mut BTreeSet<LeftOut>,
        other: &mut BTreeSet<String>,
        switches: &mut Switches,
    ) {
        let r = &mut self.region;
        let f = || real(value);
        match name {
            "sample" => self.sound = Some(value.to_owned()),
            "lokey" => match self.key(value) {
                Some(key) if key > 127 => self.silent = true,
                Some(key) => self.region.lokey = u8::try_from(key.max(0)).unwrap_or(0),
                None => {}
            },
            "hikey" => match self.key(value) {
                Some(key) if key < 0 => self.silent = true,
                Some(key) => self.region.hikey = u8::try_from(key.min(127)).unwrap_or(127),
                None => {}
            },
            "key" => match self.key(value) {
                Some(key) if (0..=127).contains(&key) => {
                    let key = u8::try_from(key).unwrap_or(0);
                    (self.region.lokey, self.region.hikey) = (key, key);
                    self.region.pitch_keycenter = key;
                    self.keycenter_from_sound = false;
                }
                Some(_) => self.silent = true,
                None => {}
            },
            "pitch_keycenter" => {
                if value.eq_ignore_ascii_case("sample") {
                    self.keycenter_from_sound = true;
                } else if let Some(key) = self.key_u8(value) {
                    self.region.pitch_keycenter = key;
                    self.keycenter_from_sound = false;
                }
            }
            "lovel" => r.lovel = velocity(value).unwrap_or(r.lovel),
            "hivel" => r.hivel = velocity(value).unwrap_or(r.hivel),
            "lorand" => r.lorand = f().unwrap_or(r.lorand),
            "hirand" => r.hirand = f().unwrap_or(r.hirand),
            "seq_length" => r.seq_length = count(value).unwrap_or(1).max(1),
            "seq_position" => r.seq_position = count(value).unwrap_or(1).max(1),
            "sw_last" => self.region.sw_last = self.key_u8(value),
            "sw_lokey" => switches.low = self.key_u8(value).or(switches.low),
            "sw_hikey" => switches.high = self.key_u8(value).or(switches.high),
            "sw_default" => {
                if switches.default.is_none() {
                    switches.default = self.key_u8(value);
                }
            }
            "sw_down" => self.region.sw_down = self.key_u8(value),
            "sw_up" => self.region.sw_up = self.key_u8(value),
            "sw_previous" => self.region.sw_previous = self.key_u8(value),
            "sw_vel" => {
                left_out.insert(LeftOut::Keyswitches);
            }
            "trigger" => {
                r.trigger = match value.to_ascii_lowercase().as_str() {
                    "release" | "release_key" => Trigger::Release,
                    "first" => Trigger::First,
                    "legato" => Trigger::Legato,
                    _ => Trigger::Attack,
                };
            }
            // The program plays on channel 1, and chooses regions as if the
            // pitch wheel and aftertouch were at rest: regions for other
            // values never play.
            "lochan" => self.silent |= f().is_some_and(|v| v > 1.0),
            "hichan" => self.silent |= f().is_some_and(|v| v < 1.0),
            "lobend" | "lochanaft" | "lopolyaft" => self.silent |= f().is_some_and(|v| v > 0.0),
            "hibend" | "hichanaft" | "hipolyaft" => self.silent |= f().is_some_and(|v| v < 0.0),
            "transpose" => r.transpose = int(value).unwrap_or(0),
            "tune" | "pitch" => r.tune = f().unwrap_or(0.0),
            "pitch_keytrack" => r.pitch_keytrack = f().unwrap_or(100.0),
            // The pitch wheel's range (decision 0033).
            "bend_up" => r.bend_up = f().unwrap_or(200.0).clamp(-9600.0, 9600.0),
            "bend_down" => r.bend_down = f().unwrap_or(-200.0).clamp(-9600.0, 9600.0),
            "pitch_veltrack" => r.pitch_veltrack = f().unwrap_or(0.0),
            "pitch_random" => r.pitch_random = f().unwrap_or(0.0),
            "volume" | "gain" => r.volume = f().unwrap_or(0.0),
            "amplitude" => r.amplitude = f().map_or(1.0, |v| v / 100.0),
            "pan" => r.pan = f().map_or(0.0, |v| v / 100.0),
            "width" => r.width = f().map_or(1.0, |v| v / 100.0),
            "amp_veltrack" => r.amp_veltrack = f().map_or(1.0, |v| v / 100.0),
            "amp_keytrack" => r.amp_keytrack = f().unwrap_or(0.0),
            "amp_keycenter" => self.region.amp_keycenter = self.key_u8(value).unwrap_or(60),
            "amp_random" => r.amp_random = f().unwrap_or(0.0),
            "xfin_lovel" => r.xfin_lovel = velocity_or_zero(value),
            "xfin_hivel" => r.xfin_hivel = velocity_or_zero(value),
            "xfout_lovel" => r.xfout_lovel = velocity_or_zero(value),
            "xfout_hivel" => r.xfout_hivel = velocity_or_zero(value),
            "xfin_lokey" => self.region.xfin_lokey = self.key_u8(value).unwrap_or(0),
            "xfin_hikey" => self.region.xfin_hikey = self.key_u8(value).unwrap_or(0),
            "xfout_lokey" => self.region.xfout_lokey = self.key_u8(value).unwrap_or(127),
            "xfout_hikey" => self.region.xfout_hikey = self.key_u8(value).unwrap_or(127),
            "xf_velcurve" => r.xf_velcurve = curve(value),
            "xf_keycurve" => r.xf_keycurve = curve(value),
            "xf_cccurve" => self.xf_cccurve = curve(value),
            "offset" => r.offset = frames(value).unwrap_or(0),
            "offset_random" => r.offset_random = frames(value).unwrap_or(0),
            "end" => match f() {
                // `end=0` or `end=-1` silences a region.
                Some(v) if v < 1.0 => self.silent = true,
                Some(_) => r.end = frames(value),
                None => {}
            },
            "loop_mode" | "loopmode" => {
                r.loop_mode = match value.to_ascii_lowercase().as_str() {
                    "no_loop" => Some(LoopMode::NoLoop),
                    "one_shot" => Some(LoopMode::OneShot),
                    "loop_continuous" => Some(LoopMode::Continuous),
                    "loop_sustain" => Some(LoopMode::Sustain),
                    _ => r.loop_mode,
                };
            }
            "loop_start" | "loopstart" => r.loop_start = frames(value),
            "loop_end" | "loopend" => r.loop_end = frames(value),
            "delay" => r.delay = f().unwrap_or(0.0),
            "delay_random" => r.delay_random = f().unwrap_or(0.0),
            "group" | "polyphony_group" => r.group = int64(value).unwrap_or(0),
            // `off_by=0` would make every ungrouped region cut it off.
            "off_by" | "offby" => r.off_by = int64(value).filter(|&g| g != 0),
            "off_mode" => {
                r.off_mode = if value.eq_ignore_ascii_case("fast") {
                    OffMode::Fast
                } else {
                    OffMode::Normal
                };
            }
            "polyphony" => r.polyphony = count(value),
            "note_polyphony" => r.note_polyphony = count(value),
            "rt_decay" => r.rt_decay = f().unwrap_or(0.0),
            "fil_type" | "filtype" => {
                self.filter_kind = filter_mode(value);
                self.unknown_filter = self.filter_kind.is_none();
            }
            "cutoff" => self.cutoff = f(),
            "resonance" => self.resonance = f().unwrap_or(0.0),
            "fil_keytrack" => self.fil_keytrack = f().unwrap_or(0.0),
            "fil_keycenter" => self.fil_keycenter = self.key_u8(value).unwrap_or(60),
            "fil_veltrack" => self.fil_veltrack = f().unwrap_or(0.0),
            "fil_random" => self.fil_random = f().unwrap_or(0.0),
            "group_volume" | "master_volume" | "global_volume" => {
                self.extra_volume += f().unwrap_or(0.0);
            }
            "group_amplitude" | "master_amplitude" | "global_amplitude" => {
                self.extra_amplitude *= f().map_or(1.0, |v| v / 100.0);
            }
            "group_tune" | "master_tune" | "global_tune" => {
                self.extra_tune += f().unwrap_or(0.0);
            }
            "direction" => {
                if value.eq_ignore_ascii_case("reverse") {
                    left_out.insert(LeftOut::Reversed);
                }
            }
            "loop_crossfade" | "loopcrossfade" => {
                left_out.insert(LeftOut::LoopCrossfades);
            }
            _ => self.other_setting(name, value, left_out, other),
        }
    }

    /// The settings with numbers in their names (envelopes, curves,
    /// controllers), and those left out.
    fn other_setting(
        &mut self,
        name: &str,
        value: &str,
        left_out: &mut BTreeSet<LeftOut>,
        other: &mut BTreeSet<String>,
    ) {
        if let Some(stage) = name.strip_prefix("ampeg_")
            && envelope_setting(&mut self.region.ampeg, stage, value, true)
        {
            return;
        }
        if let Some(stage) = name.strip_prefix("fileg_")
            && envelope_setting(&mut self.fileg, stage, value, false)
        {
            return;
        }
        if let Some(point) = cc_number(name, "amp_velcurve_") {
            let curve = self.region.amp_velcurve.get_or_insert_with(|| {
                // Points not given follow the curve through those given:
                // filled in when the region is finished.
                Box::new([f32::NAN; 128])
            });
            curve[point] = real(value).unwrap_or(f32::NAN);
            return;
        }
        if let Some(number) = cc_number(name, "locc") {
            let range = self.cc_ranges.entry(number).or_insert((0.0, 127.0));
            range.0 = real(value).unwrap_or(0.0);
            return;
        }
        if let Some(number) = cc_number(name, "hicc") {
            let range = self.cc_ranges.entry(number).or_insert((0.0, 127.0));
            range.1 = real(value).unwrap_or(127.0);
            return;
        }
        for (prefix, which) in [
            ("xfin_locc", 0),
            ("xfin_hicc", 1),
            ("xfout_locc", 2),
            ("xfout_hicc", 3),
        ] {
            if let Some(number) = cc_number(name, prefix) {
                let v = real(value);
                let fade = self.cc_fades.entry(number).or_default();
                match which {
                    0 => fade.in_low = v,
                    1 => fade.in_high = v,
                    2 => fade.out_low = v,
                    _ => fade.out_high = v,
                }
                return;
            }
        }
        if let Some((target, number)) = modulation(name) {
            self.modulations
                .insert((target.to_owned(), number), real(value).unwrap_or(0.0));
            return;
        }
        match depth_kind(name) {
            Some(Depth::Lfo) => {
                self.lfo_depths
                    .insert(name.to_owned(), real(value).unwrap_or(0.0));
                return;
            }
            Some(Depth::Envelope) => {
                self.envelope_depths
                    .insert(name.to_owned(), real(value).unwrap_or(0.0));
                return;
            }
            Some(Depth::Setting) => return,
            None => {}
        }
        if let Some(part) = numbered_part(name, "eq") {
            // An equaliser band changes the sound only with a gain.
            if part == "gain" && real(value).is_some_and(|g| g != 0.0) {
                left_out.insert(LeftOut::Equalizers);
            }
            return;
        }
        match family(name) {
            Some(Family::LeftOut(part)) => {
                left_out.insert(part);
            }
            Some(Family::Ignored) => {}
            None => {
                other.insert(name.to_owned());
            }
        }
    }

    /// Applies the settings controllers move, at the controllers' starting
    /// values; notes LFOs and envelopes that move anything. `None` if the
    /// region is started by a controller, never by a key.
    fn apply_modulations(&mut self, left_out: &mut BTreeSet<LeftOut>) -> Option<(f32, f32)> {
        let mut lfo: f32 = self.lfo_depths.values().map(|d| d.abs()).sum();
        let mut envelope: f32 = self.envelope_depths.values().map(|d| d.abs()).sum();
        let (mut cutoff_cents, mut offset) = (0.0_f32, 0.0_f32);
        for ((target, number), depth) in &self.modulations {
            let amount = depth * self.cc[*number] / 127.0;
            let (amp, fil) = (&mut self.region.ampeg, &mut self.fileg);
            match target.as_str() {
                "ampeg_delay" => amp.delay += amount,
                "ampeg_start" => amp.start += amount / 100.0,
                "ampeg_attack" => amp.attack += amount,
                "ampeg_hold" => amp.hold += amount,
                "ampeg_decay" => amp.decay += amount,
                "ampeg_sustain" => amp.sustain += amount / 100.0,
                "ampeg_release" => amp.release += amount,
                "fileg_delay" => fil.delay += amount,
                "fileg_start" => fil.start += amount / 100.0,
                "fileg_attack" => fil.attack += amount,
                "fileg_hold" => fil.hold += amount,
                "fileg_decay" => fil.decay += amount,
                "fileg_sustain" => fil.sustain += amount / 100.0,
                "fileg_release" => fil.release += amount,
                "fileg_depth" => fil.depth += amount,
                "cutoff" => cutoff_cents += amount,
                "resonance" => self.resonance += amount,
                "volume" | "gain" => self.region.volume += amount,
                "pan" => self.region.pan += amount / 100.0,
                "width" => self.region.width += amount / 100.0,
                "tune" | "pitch" => self.region.tune += amount,
                "offset" => offset += amount,
                "delay" => self.region.delay += amount,
                // Started by a controller moving, never by a key.
                "on_lo" | "on_hi" => return None,
                other => match depth_kind(other) {
                    Some(Depth::Lfo) => lfo += amount.abs(),
                    Some(Depth::Envelope) => envelope += amount.abs(),
                    Some(Depth::Setting) => {}
                    // How a controller's movement is smoothed or shaped
                    // changes nothing at its starting value.
                    None if other.ends_with("_curve")
                        || other.ends_with("_smooth")
                        || other.ends_with("_step")
                        || other == "label" => {}
                    None => {
                        if amount != 0.0 {
                            left_out.insert(LeftOut::ControllerChanges);
                        }
                    }
                },
            }
        }
        if lfo > 0.0 {
            left_out.insert(LeftOut::Lfos);
        }
        if envelope > 0.0 {
            left_out.insert(LeftOut::Envelopes);
        }
        Some((cutoff_cents, offset))
    }

    /// The region, if it can ever play.
    fn finish(mut self, default_path: &str, left_out: &mut BTreeSet<LeftOut>) -> Option<Parsed> {
        if self.silent || self.region.lokey > self.region.hikey {
            return None;
        }
        // Controllers at their starting values decide some regions (a
        // piano's pedal-down sounds, say) and some crossfades.
        for (&number, &(low, high)) in &self.cc_ranges {
            let value = self.cc[number];
            if value < low || value > high {
                return None;
            }
        }
        let mut cc_gain = 1.0;
        for (&number, fade) in &self.cc_fades {
            cc_gain *= cc_crossfade(self.cc[number], fade, self.xf_cccurve);
        }
        if cc_gain <= 0.0 {
            return None;
        }
        let (cutoff_cents, offset) = self.apply_modulations(left_out)?;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a frame position"
        )]
        {
            self.region.offset = (self.region.offset as f32 + offset).max(0.0) as u64;
        }
        if let Some(cutoff) = &mut self.cutoff {
            *cutoff *= 2.0_f32.powf(cutoff_cents.clamp(-12_000.0, 12_000.0) / 1200.0);
        }
        self.region.amplitude *= cc_gain * self.extra_amplitude;
        self.region.volume += self.extra_volume;
        self.region.tune += self.extra_tune;
        if let Some(curve) = &mut self.region.amp_velcurve {
            fill_curve(curve);
        }
        if let Some(cutoff) = self.cutoff {
            if self.unknown_filter {
                left_out.insert(LeftOut::Filters);
            } else {
                self.region.filter = Some(FilterSpec {
                    mode: self.filter_kind.unwrap_or(FilterMode::LowPass12),
                    cutoff,
                    resonance: self.resonance,
                    keytrack: self.fil_keytrack,
                    keycenter: self.fil_keycenter,
                    veltrack: self.fil_veltrack,
                    random: self.fil_random,
                    envelope: self.fileg,
                });
            }
        }
        let sound = match self.sound.as_deref().map(str::trim) {
            None | Some("") => return None,
            Some(name) if name.starts_with('*') => match name.to_ascii_lowercase().as_str() {
                "*sine" => Sound::Sine(self.region.pitch_keycenter),
                "*silence" => Sound::Silence,
                _ => {
                    left_out.insert(LeftOut::Waves);
                    return None;
                }
            },
            Some(name) => Sound::File(format!("{default_path}{name}")),
        };
        Some(Parsed {
            region: self.region,
            sound,
            keycenter_from_sound: self.keycenter_from_sound,
        })
    }
}

/// An envelope setting (`attack`, `vel2attack`…); whether it was one.
fn envelope_setting(envelope: &mut EnvelopeSpec, stage: &str, value: &str, amp: bool) -> bool {
    let Some(v) = real(value) else {
        return matches!(
            stage,
            "delay" | "start" | "attack" | "hold" | "decay" | "sustain" | "release" | "depth"
        );
    };
    let percent = v / 100.0;
    match stage {
        "delay" => envelope.delay = v,
        "start" => envelope.start = percent,
        "attack" => envelope.attack = v,
        "hold" => envelope.hold = v,
        "decay" => envelope.decay = v,
        "sustain" => envelope.sustain = percent,
        "release" => envelope.release = v,
        "vel2delay" => envelope.vel2delay = v,
        "vel2attack" => envelope.vel2attack = v,
        "vel2hold" => envelope.vel2hold = v,
        "vel2decay" => envelope.vel2decay = v,
        "vel2sustain" => envelope.vel2sustain = percent,
        "vel2release" => envelope.vel2release = v,
        "depth" if !amp => envelope.depth = v,
        _ => return false,
    }
    true
}

/// The gain of a crossfade by a controller at `value`.
fn cc_crossfade(value: f32, fade: &CcFade, curve: CrossfadeCurve) -> f32 {
    let shape = |t: f32| match curve {
        CrossfadeCurve::Power => t.clamp(0.0, 1.0).sqrt(),
        CrossfadeCurve::Gain => t.clamp(0.0, 1.0),
    };
    let mut gain = 1.0;
    if fade.in_low.is_some() || fade.in_high.is_some() {
        let (low, high) = (fade.in_low.unwrap_or(0.0), fade.in_high.unwrap_or(0.0));
        gain *= if value >= high {
            1.0
        } else if value <= low {
            0.0
        } else {
            shape((value - low) / (high - low))
        };
    }
    if fade.out_low.is_some() || fade.out_high.is_some() {
        let (low, high) = (
            fade.out_low.unwrap_or(127.0),
            fade.out_high.unwrap_or(127.0),
        );
        gain *= if value <= low {
            1.0
        } else if value >= high {
            0.0
        } else {
            shape((high - value) / (high - low))
        };
    }
    gain
}

/// Fills the points of a velocity curve not given, in straight lines
/// between those given (0 at velocity 0 and 1 at 127 unless given).
fn fill_curve(curve: &mut [f32; 128]) {
    if curve[0].is_nan() {
        curve[0] = 0.0;
    }
    if curve[127].is_nan() {
        curve[127] = 1.0;
    }
    let mut last = 0;
    for i in 1..128 {
        if curve[i].is_nan() {
            continue;
        }
        let (from, to) = (curve[last], curve[i]);
        for (step, point) in curve[last + 1..i].iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "velocities")]
            let t = (step + 1) as f32 / (i - last) as f32;
            *point = from + (to - from) * t;
        }
        last = i;
    }
}

/// What a setting this reader does not play is.
enum Family {
    LeftOut(LeftOut),
    /// It changes nothing the program plays (a label, a pitch bend range…).
    Ignored,
}

/// What kind of setting `name` is, if this reader knows it.
fn family(name: &str) -> Option<Family> {
    let part = if name.starts_with("label_cc")
        || name.starts_with("hint_")
        || name.ends_with("_label")
        // Aftertouch never comes: what it would move stays put.
        || name.contains("chanaft")
        || name.contains("polyaft")
        || matches!(
            name,
            "bend_step"
                | "bend_smooth"
                | "image"
                | "md5"
                | "note_selfmask"
                | "rt_dead"
                | "sustain_sw"
                | "sostenuto_sw"
                | "sustain_cc"
                | "sostenuto_cc"
                | "sustain_lo"
                | "sostenuto_lo"
                | "count"
                | "sample_quality"
                | "lobpm"
                | "hibpm"
                | "lotimer"
                | "hitimer"
                | "loprog"
                | "hiprog"
                | "octave_offset"
                | "note_offset"
                | "default_path"
                | "position"
                | "output"
        ) {
        return Some(Family::Ignored);
    } else if name.starts_with("fil2") || name == "cutoff2" || name == "resonance2" {
        LeftOut::Filters
    } else if numbered_part(name, "effect").is_some()
        || name
            .strip_prefix("effect")
            .is_some_and(|n| n.parse::<u32>().is_ok())
        || name == "bus"
    {
        LeftOut::Effects
    } else if name.starts_with("oscillator") {
        LeftOut::Waves
    } else {
        return None;
    };
    Some(Family::LeftOut(part))
}

/// What a setting of an LFO or an envelope the sampler lacks does.
enum Depth {
    /// How far an LFO moves something.
    Lfo,
    /// How far a pitch or extra envelope moves something.
    Envelope,
    /// Its speed, shape or timing: nothing moves without a depth.
    Setting,
}

/// The settings of numbered LFOs that move nothing by themselves.
const LFO_SETTINGS: [&str; 13] = [
    "freq", "wave", "delay", "fade", "phase", "count", "offset", "ratio", "scale", "smooth",
    "step", "beats", "sub",
];
/// The settings of numbered envelopes that move nothing by themselves.
const ENVELOPE_SETTINGS: [&str; 10] = [
    "time", "level", "shape", "sustain", "loop", "points", "dynamic", "ampeg", "freq_lfo", "depth",
];

/// Whether `name` sets an LFO or an envelope the sampler lacks, and what.
fn depth_kind(name: &str) -> Option<Depth> {
    if matches!(name, "amplfo_depth" | "pitchlfo_depth" | "fillfo_depth") {
        return Some(Depth::Lfo);
    }
    if name == "pitcheg_depth" {
        return Some(Depth::Envelope);
    }
    if ["amplfo_", "pitchlfo_", "fillfo_", "pitcheg_"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return Some(Depth::Setting);
    }
    if let Some(part) = numbered_part(name, "lfo") {
        return Some(if LFO_SETTINGS.iter().any(|s| part.starts_with(s)) {
            Depth::Setting
        } else {
            Depth::Lfo
        });
    }
    if let Some(part) = numbered_part(name, "eg") {
        return Some(if ENVELOPE_SETTINGS.iter().any(|s| part.starts_with(s)) {
            Depth::Setting
        } else {
            Depth::Envelope
        });
    }
    None
}

/// For a numbered setting (`lfo01_pitch`, `eq2_gain`): what follows the
/// number, if `name` is `prefix`, digits, `_` and more.
fn numbered_part<'n>(name: &'n str, prefix: &str) -> Option<&'n str> {
    let rest = name.strip_prefix(prefix)?;
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    rest[digits..].strip_prefix('_')
}

/// For a setting a controller moves (`cutoff_cc1`, `ampeg_attack_oncc74`,
/// `ampeg_attackcc1`): what it moves, and the controller.
fn modulation(name: &str) -> Option<(&str, usize)> {
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if stem.len() == name.len() {
        return None;
    }
    let number = name[stem.len()..]
        .parse::<usize>()
        .ok()
        .filter(|&n| n < 128)?;
    ["_oncc", "_cc", "cc"]
        .iter()
        .find_map(|suffix| stem.strip_suffix(suffix))
        .filter(|target| !target.is_empty())
        .map(|target| (target, number))
}

/// The filter of a `fil_type` (`None` for a kind the sampler lacks).
fn filter_mode(value: &str) -> Option<FilterMode> {
    match value.to_ascii_lowercase().as_str() {
        "lpf_1p" | "lpf_2p" | "lpf_2p_sv" => Some(FilterMode::LowPass12),
        "lpf_4p" | "lpf_6p" => Some(FilterMode::LowPass24),
        "hpf_1p" | "hpf_2p" | "hpf_4p" | "hpf_6p" | "hpf_2p_sv" => Some(FilterMode::HighPass),
        "bpf_1p" | "bpf_2p" | "bpf_4p" | "bpf_6p" | "bpf_2p_sv" => Some(FilterMode::BandPass),
        _ => None,
    }
}

fn curve(value: &str) -> CrossfadeCurve {
    if value.eq_ignore_ascii_case("gain") {
        CrossfadeCurve::Gain
    } else {
        CrossfadeCurve::Power
    }
}

/// The number at the end of `name` after `prefix`, if it is a controller
/// or a curve point (0 to 127).
fn cc_number(name: &str, prefix: &str) -> Option<usize> {
    name.strip_prefix(prefix)?
        .parse::<usize>()
        .ok()
        .filter(|&n| n < 128)
}

/// A number setting, as the sampler uses it.
fn real(value: &str) -> Option<f32> {
    value.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

fn number_value(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "clamped to the range of i32"
)]
fn int(value: &str) -> Option<i32> {
    number_value(value).map(|v| v.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "clamped to a range i64 holds exactly"
)]
fn int64(value: &str) -> Option<i64> {
    number_value(value).map(|v| v.round().clamp(-1e15, 1e15) as i64)
}

/// A count (0 and up).
fn count(value: &str) -> Option<u32> {
    int(value).map(|v| u32::try_from(v.max(0)).unwrap_or(0))
}

/// A frame position (0 and up).
fn frames(value: &str) -> Option<u64> {
    int64(value).map(|v| u64::try_from(v.max(0)).unwrap_or(0))
}

/// A velocity bound (1 to 127).
fn velocity(value: &str) -> Option<u8> {
    int(value).map(|v| u8::try_from(v.clamp(1, 127)).unwrap_or(1))
}

/// A crossfade velocity bound (0 to 127).
fn velocity_or_zero(value: &str) -> u8 {
    int(value).map_or(0, |v| u8::try_from(v.clamp(0, 127)).unwrap_or(0))
}

/// A key: a number (`60`) or a note name (`c4`, `C#4`, `eb-1`), middle C
/// being `c4` = 60.
fn key_number(value: &str) -> Option<i32> {
    let value = value.trim();
    if let Some(number) = int(value)
        && value.parse::<f64>().is_ok()
    {
        return Some(number);
    }
    let mut chars = value.chars().peekable();
    let class = match chars.next()?.to_ascii_lowercase() {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (shift, octave) = if let Some(after) = rest.strip_prefix('#') {
        (1, after)
    } else if let Some(after) = rest.strip_prefix('♯') {
        (1, after)
    } else if let Some(after) = rest.strip_prefix('b')
        && after.starts_with(|c: char| c.is_ascii_digit() || c == '-')
    {
        (-1, after)
    } else if let Some(after) = rest.strip_prefix('♭') {
        (-1, after)
    } else {
        (0, rest.as_str())
    };
    let octave: i32 = octave.parse().ok()?;
    Some((octave + 1) * 12 + class + shift)
}

// ----- Sounds

/// Finds a file the instrument names, relative to `folder` unless it is a
/// full path: as written, else with the letter case of each part ignored
/// (instruments made on Windows name `Piano.WAV` what is `piano.wav`).
fn find_file(folder: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim().replace('\\', "/");
    let written = Path::new(&name);
    let path = if written.is_absolute() {
        written.to_path_buf()
    } else {
        folder.join(written)
    };
    if path.is_file() {
        return Some(path);
    }
    // Part by part, ignoring case.
    let mut found = if written.is_absolute() {
        PathBuf::new()
    } else {
        folder.to_path_buf()
    };
    for component in written.components() {
        match component {
            Component::Normal(part) => {
                let exact = found.join(part);
                if exact.exists() {
                    found = exact;
                    continue;
                }
                let wanted = part.to_string_lossy().to_lowercase();
                let entry = std::fs::read_dir(&found)
                    .ok()?
                    .flatten()
                    .find(|entry| entry.file_name().to_string_lossy().to_lowercase() == wanted)?;
                found = entry.path();
            }
            other => found.push(other.as_os_str()),
        }
    }
    found.is_file().then_some(found)
}

/// A sound file, decoded, with what it tells a sampler.
struct Decoded {
    sample: Sample,
    root_key: Option<u8>,
}

/// Reads a sound file into a sample.
fn decode(path: &Path) -> Result<Decoded, String> {
    let bytes = audio::read_file(path).map_err(|e| e.to_string())?;
    let info = audio::sampler_info(&bytes);
    let decoded = audio::decode(&bytes).map_err(|e| e.to_string())?;
    let mut channels = decoded.channels.into_iter();
    let left = channels.next().unwrap_or_default();
    let right = channels.next().unwrap_or_default();
    Ok(Decoded {
        sample: Sample {
            sample_rate: decoded.sample_rate,
            left,
            right,
            file_loop: info.loop_frames,
        },
        root_key: info.root_key,
    })
}

/// One cycle of a sine, looped, at the pitch of `key`.
fn sine(key: u8) -> Sample {
    #[allow(clippy::cast_precision_loss, reason = "a small table")]
    let left = (0..SINE_FRAMES)
        .map(|i| (std::f32::consts::TAU * i as f32 / SINE_FRAMES as f32).sin())
        .collect();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "at most 12.5 kHz × 2048"
    )]
    let sample_rate = (daw_dsp::key_to_hz(f32::from(key)) * SINE_FRAMES as f32).round() as u32;
    Sample {
        sample_rate,
        left,
        right: Vec::new(),
        file_loop: Some((0, SINE_FRAMES as u64 - 1)),
    }
}

/// Finds and decodes the sounds of a read instrument (several at once),
/// and gives the bank.
fn finish(parsed: ParsedInstrument, folder: &Path) -> Result<SamplerBank, SfzError> {
    let ParsedInstrument {
        regions,
        keyswitch,
        left_out,
        mut missing,
    } = parsed;
    if regions.is_empty() {
        return Err(SfzError::NoRegions);
    }
    // Each distinct sound once.
    let mut sounds: Vec<Sound> = regions.iter().map(|r| r.sound.clone()).collect();
    sounds.sort();
    sounds.dedup();
    let files: Vec<(usize, PathBuf)> = sounds
        .iter()
        .enumerate()
        .filter_map(|(index, sound)| {
            let Sound::File(name) = sound else {
                return None;
            };
            let found = find_file(folder, name);
            if found.is_none() {
                missing.push(name.replace('\\', "/"));
            }
            found.map(|path| (index, path))
        })
        .collect();
    let decoded = decode_all(&files)?;
    let mut samples = Vec::new();
    let mut root_keys = Vec::new();
    let mut sample_of: HashMap<usize, usize> = HashMap::new();
    let mut first_failure = None;
    for ((index, path), result) in files.iter().zip(decoded) {
        match result {
            Ok(decoded) => {
                sample_of.insert(*index, samples.len());
                samples.push(decoded.sample);
                root_keys.push(decoded.root_key);
            }
            Err(why) => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                first_failure.get_or_insert_with(|| format!("{name}: {why}"));
                missing.push(name);
            }
        }
    }
    let mut bank_regions = Vec::with_capacity(regions.len());
    let mut sines: HashMap<u8, usize> = HashMap::new();
    for parsed in regions {
        let mut region = parsed.region;
        let index = sounds.binary_search(&parsed.sound).ok();
        match &parsed.sound {
            Sound::File(_) => {
                let Some(&sample) = index.and_then(|i| sample_of.get(&i)) else {
                    continue;
                };
                if parsed.keycenter_from_sound {
                    region.pitch_keycenter = root_keys[sample].unwrap_or(60);
                }
                region.sample = Some(sample);
            }
            Sound::Sine(key) => {
                let sample = *sines.entry(*key).or_insert_with(|| {
                    samples.push(sine(*key));
                    root_keys.push(None);
                    samples.len() - 1
                });
                region.sample = Some(sample);
            }
            Sound::Silence => region.sample = None,
        }
        bank_regions.push(region);
    }
    if bank_regions.iter().all(|r| r.sample.is_none()) {
        return Err(first_failure.map_or(SfzError::NoRegions, SfzError::NoSounds));
    }
    missing.sort();
    missing.dedup();
    Ok(SamplerBank {
        regions: bank_regions,
        samples,
        keyswitch,
        left_out: left_out.into_iter().collect(),
        missing,
    })
}

/// Decodes files on several threads, in the order given; stops early if
/// they hold too much sound for one instrument.
fn decode_all(files: &[(usize, PathBuf)]) -> Result<Vec<Result<Decoded, String>>, SfzError> {
    let next = AtomicUsize::new(0);
    let total = AtomicUsize::new(0);
    let too_large = AtomicBool::new(false);
    let results: Mutex<Vec<Option<Result<Decoded, String>>>> =
        Mutex::new((0..files.len()).map(|_| None).collect());
    let workers = std::thread::available_parallelism()
        .map_or(2, std::num::NonZeroUsize::get)
        .clamp(1, 8)
        .min(files.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= files.len() || too_large.load(Ordering::Relaxed) {
                        return;
                    }
                    let result = decode(&files[index].1);
                    if let Ok(decoded) = &result {
                        let bytes = total.fetch_add(decoded.sample.bytes(), Ordering::Relaxed)
                            + decoded.sample.bytes();
                        if bytes > MAX_SOUND_BYTES {
                            too_large.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                    if let Ok(mut results) = results.lock() {
                        results[index] = Some(result);
                    }
                }
            });
        }
    });
    if too_large.load(Ordering::Relaxed) {
        return Err(SfzError::TooLarge);
    }
    Ok(results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .into_iter()
        .map(|r| r.unwrap_or_else(|| Err("it was not read".to_owned())))
        .collect())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "test code"
)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("sfz-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// A short mono WAV file, with a `smpl` chunk holding a loop and a root
    /// key when asked.
    fn write_wav(path: &Path, frames: usize, smpl: Option<(u32, u32, u32)>) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut writer = crate::wav::WavWriter::create_with_channels(
            path,
            crate::wav::WavFormat::Pcm16,
            48_000,
            1,
        )
        .unwrap();
        let samples: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        writer.write_interleaved(&samples).unwrap();
        writer.finish().unwrap();
        if let Some((key, start, end)) = smpl {
            let mut bytes = std::fs::read(path).unwrap();
            let mut chunk = Vec::new();
            chunk.extend_from_slice(b"smpl");
            chunk.extend_from_slice(&60_u32.to_le_bytes());
            let mut body = vec![0_u8; 36];
            body[12..16].copy_from_slice(&key.to_le_bytes());
            body[28..32].copy_from_slice(&1_u32.to_le_bytes());
            let mut looped = vec![0_u8; 24];
            looped[8..12].copy_from_slice(&start.to_le_bytes());
            looped[12..16].copy_from_slice(&end.to_le_bytes());
            body.extend_from_slice(&looped);
            chunk.extend_from_slice(&body);
            bytes.extend_from_slice(&chunk);
            let riff = u32::try_from(bytes.len() - 8).unwrap();
            bytes[4..8].copy_from_slice(&riff.to_le_bytes());
            std::fs::write(path, bytes).unwrap();
        }
    }

    #[test]
    fn note_names_and_numbers() {
        assert_eq!(key_number("60"), Some(60));
        assert_eq!(key_number("c4"), Some(60));
        assert_eq!(key_number("C#4"), Some(61));
        assert_eq!(key_number("db4"), Some(61));
        assert_eq!(key_number("b3"), Some(59));
        assert_eq!(key_number("bb3"), Some(58));
        assert_eq!(key_number("c-1"), Some(0));
        assert_eq!(key_number("g9"), Some(127));
        assert_eq!(key_number("-1"), Some(-1));
        assert_eq!(key_number("h2"), None);
        assert_eq!(key_number(""), None);
    }

    #[test]
    fn the_text_is_read_with_its_levels_defines_includes_and_comments() {
        let folder = scratch("text");
        std::fs::write(
            folder.join("common.sfz"),
            "<group> ampeg_release=0.5 // the group's release\n",
        )
        .unwrap();
        let text = r"
// A comment line.
/* A block
   comment */
#define $LOW 48
#define $SOFT 63
<control> default_path=Samples\Piano\ octave_offset=0
<global> volume=-6 amp_veltrack=50
#include common.sfz
<region> sample=C4 soft.wav lokey=$LOW hikey=c4 pitch_keycenter=c4 hivel=$SOFT
<region>sample=C4 loud.wav key=61 lovel=64 volume=-3 tune=+5
<group> ampeg_attack=0.01 /* inline */ trigger=release rt_decay=2
<region> sample=../Shared/Thump.wav lokey=36 hikey=96
";
        let parsed = parse(text, &folder).unwrap();
        assert_eq!(parsed.regions.len(), 3, "{parsed:?}");
        let soft = &parsed.regions[0];
        assert_eq!(
            soft.sound,
            Sound::File("Samples\\Piano\\C4 soft.wav".to_owned())
        );
        assert_eq!((soft.region.lokey, soft.region.hikey), (48, 60));
        assert_eq!((soft.region.lovel, soft.region.hivel), (1, 63));
        assert_eq!(soft.region.volume, -6.0, "from the global level");
        assert_eq!(soft.region.amp_veltrack, 0.5);
        assert_eq!(soft.region.ampeg.release, 0.5, "from the included group");
        let loud = &parsed.regions[1];
        assert_eq!((loud.region.lokey, loud.region.hikey), (61, 61));
        assert_eq!(loud.region.pitch_keycenter, 61, "`key` sets the key center");
        assert_eq!(loud.region.volume, -3.0, "the region overrides the global");
        assert_eq!(loud.region.tune, 5.0);
        let thump = &parsed.regions[2];
        assert_eq!(thump.region.trigger, Trigger::Release);
        assert_eq!(thump.region.rt_decay, 2.0);
        assert_eq!(thump.region.ampeg.attack, 0.01);
        assert_eq!(
            thump.region.ampeg.release, 0.001,
            "a new group forgets the old group's settings"
        );
        assert_eq!(thump.region.volume, -6.0, "but keeps the global's");
        assert_eq!(parsed.left_out, BTreeSet::new());
        assert_eq!(parsed.missing, Vec::<String>::new());
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn regions_that_cannot_play_are_dropped_and_what_is_not_played_is_named() {
        let text = "
<control> set_cc64=0
<region> sample=up.wav hicc64=63
<region> sample=down.wav locc64=64
<region> sample=other.wav lochan=2
<region> sample=off.wav hikey=-1
<region> sample=cut.wav end=-1
<region> sample=*sine key=69
<region> sample=*saw
<region> sample=fancy.wav amplfo_depth=3 pitcheg_depth=100 amplitude_oncc7=50 cutoff_oncc1=1200 pitchlfo_freq=4 frobnicate=3
<region> sample=switched.wav sw_last=c1 sw_lokey=24 sw_hikey=26 fil_type=lpf_4p cutoff=900 fileg_depth=1200
<region> sample=swapped.wav sw_last=25 amp_velcurve_64=0.25
";
        let parsed = parse(text, Path::new(".")).unwrap();
        let sounds: Vec<&Sound> = parsed.regions.iter().map(|r| &r.sound).collect();
        assert_eq!(
            sounds,
            [
                &Sound::File("up.wav".to_owned()),
                &Sound::Sine(69),
                &Sound::File("fancy.wav".to_owned()),
                &Sound::File("switched.wav".to_owned()),
                &Sound::File("swapped.wav".to_owned()),
            ]
        );
        let left_out: Vec<&LeftOut> = parsed.left_out.iter().collect();
        assert_eq!(
            left_out,
            [
                &LeftOut::Lfos,
                &LeftOut::Envelopes,
                &LeftOut::ControllerChanges,
                &LeftOut::Waves,
                &LeftOut::Other("frobnicate".to_owned()),
            ]
        );
        assert_eq!(
            parsed.keyswitch,
            Some(KeySwitch {
                lokey: 24,
                hikey: 26,
                default: Some(24)
            })
        );
        let switched = &parsed.regions[3].region;
        let filter = switched.filter.unwrap();
        assert_eq!(
            (filter.mode, filter.cutoff, filter.envelope.depth),
            (FilterMode::LowPass24, 900.0, 1200.0)
        );
        let curve = parsed.regions[4].region.amp_velcurve.as_ref().unwrap();
        assert_eq!((curve[0], curve[64], curve[127]), (0.0, 0.25, 1.0));
        assert!(
            (curve[32] - 0.125).abs() < 1e-6,
            "filled in a straight line"
        );
    }

    #[test]
    fn the_pitch_wheel_bends_by_the_range_the_file_says() {
        // Decision 0033: two semitones each way unless the file says.
        let text = "
<global> bend_up=1200
<region> sample=a.wav
<region> sample=b.wav bend_down=-700
<region> sample=c.wav bend_up=0 bend_down=0
";
        let parsed = parse(text, Path::new(".")).unwrap();
        let ranges: Vec<(f32, f32)> = parsed
            .regions
            .iter()
            .map(|r| (r.region.bend_up, r.region.bend_down))
            .collect();
        assert_eq!(ranges, [(1200.0, -200.0), (1200.0, -700.0), (0.0, 0.0)]);
        assert!(
            parsed.left_out.is_empty(),
            "a bend range is played, not left out"
        );
        let plain = parse("<region> sample=a.wav", Path::new(".")).unwrap();
        let region = &plain.regions[0].region;
        assert_eq!((region.bend_up, region.bend_down), (200.0, -200.0));
    }

    #[test]
    fn controllers_at_their_starting_values_set_what_they_move() {
        // As some instruments expose their envelope and filter: settings
        // moved by controllers, with the controllers' starting values.
        let text = "
<control> set_cc100=63.5 set_cc103=127 set_cc107=127
<global> ampeg_attack=0.001 ampeg_sustain=0 ampeg_attack_oncc100=2 ampeg_sustain_oncc103=100
cutoff=220 cutoff_cc107=6000 pitchlfo_depthcc105=10 pitchlfo_freqcc106=10 volume_oncc7=-6.35
<region> sample=a.wav
<region> sample=b.wav ampeg_attack=0.5
<region> sample=pedal.wav on_locc64=64
";
        let parsed = parse(text, Path::new(".")).unwrap();
        assert_eq!(
            parsed.regions.len(),
            2,
            "a region started by a controller never plays"
        );
        let a = &parsed.regions[0].region;
        assert!((a.ampeg.attack - 1.001).abs() < 1e-5, "{}", a.ampeg.attack);
        assert!(
            (a.ampeg.sustain - 1.0).abs() < 1e-6,
            "sustained by its controller"
        );
        let cutoff = a.filter.unwrap().cutoff;
        assert!((cutoff - 7040.0).abs() < 0.5, "five octaves up: {cutoff}");
        assert!(
            (a.volume + 5.0).abs() < 1e-4,
            "-6.35 dB at 100 of 127: {}",
            a.volume
        );
        let b = &parsed.regions[1].region;
        assert!(
            (b.ampeg.attack - 1.5).abs() < 1e-5,
            "the controller still moves an attack set lower down: {}",
            b.ampeg.attack
        );
        assert_eq!(
            parsed.left_out,
            BTreeSet::new(),
            "a vibrato with no depth leaves nothing out"
        );
    }

    #[test]
    fn an_instrument_loads_with_its_sounds_found_however_they_are_written() {
        let folder = scratch("load");
        write_wav(
            &folder.join("Samples/Piano C4.wav"),
            4800,
            Some((62, 1000, 3999)),
        );
        write_wav(&folder.join("Samples/kick.wav"), 2400, None);
        std::fs::write(folder.join("Samples/broken.wav"), b"RIFF....WAVEjunk").unwrap();
        let sfz = folder.join("Piano.sfz");
        std::fs::write(
            &sfz,
            "<region> sample=Samples\\Piano C4.wav lokey=48 hikey=72 pitch_keycenter=sample\n\
             <region> sample=SAMPLES/KICK.WAV key=36 loop_mode=one_shot\n\
             <region> sample=Samples/gone.wav key=37\n\
             <region> sample=Samples/broken.wav key=38\n",
        )
        .unwrap();
        let bank = load(&sfz).unwrap();
        assert_eq!(
            bank.regions.len(),
            2,
            "the missing and broken ones are left out"
        );
        assert_eq!(bank.samples.len(), 2);
        let piano = &bank.regions[0];
        assert_eq!(piano.pitch_keycenter, 62, "the root key the file holds");
        let piano_sound = &bank.samples[piano.sample.unwrap()];
        assert_eq!(piano_sound.frames(), 4800);
        assert_eq!(piano_sound.file_loop, Some((1000, 3999)));
        assert_eq!(piano_sound.sample_rate, 48_000);
        let kick = &bank.regions[1];
        assert_eq!(kick.loop_mode, Some(LoopMode::OneShot));
        assert_eq!(
            bank.samples[kick.sample.unwrap()].frames(),
            2400,
            "found ignoring case"
        );
        assert_eq!(bank.missing, ["Samples/gone.wav", "broken.wav"]);

        // An instrument none of whose sounds can be read says why.
        let nothing = folder.join("Nothing.sfz");
        std::fs::write(&nothing, "<region> sample=Samples/broken.wav").unwrap();
        let error = load(&nothing).unwrap_err().to_string();
        assert!(error.contains("broken.wav"), "{error}");
        std::fs::write(&nothing, "Just some text, no regions.").unwrap();
        assert!(matches!(load(&nothing), Err(SfzError::NoRegions)));
        assert!(matches!(
            load(&folder.join("Gone.sfz")),
            Err(SfzError::Io(_))
        ));
        let _ = std::fs::remove_dir_all(folder);
    }
}

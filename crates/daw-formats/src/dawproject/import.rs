// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading another program's DAWproject file into a song (decision 0020).

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

use daw_model::{
    AudioFile, AudioId, AutomationPoint, AutomationTarget, ClipContent, CompressorSettings,
    EffectId, EffectKind, EqSettings, Instrument, LimiterSettings, Note, NoteId, PatternId,
    PluginFormat, PluginRef, PluginSetup, PluginState, Project, TempoChange, TempoMap, Ticks,
    TimeSignature, TrackId, gain_volume,
};

use super::xml::{self, Node};
use super::{DawProjectError, LeftOut, from_vstpreset, is_own_device, vst3_id_from_canonical};
use crate::native::free_name;

/// The largest `project.xml` read.
const MAX_PROJECT_XML: u64 = 256 * 1024 * 1024;
/// The largest settings file read for one device.
const MAX_STATE: u64 = 64 * 1024 * 1024;
/// The furthest position read, in beats (a song of many hours).
const MAX_BEATS: f64 = 1_000_000.0;

/// A song read from a DAWproject file, and what could not come with it.
#[derive(Debug)]
pub struct Imported {
    /// The song.
    pub project: Project,
    /// What was left out, each kind once.
    pub left_out: Vec<LeftOut>,
}

/// Names for what the file leaves unnamed (the interface's words).
pub struct ImportNames<'a> {
    /// A track's name, from its number (counted from 1).
    pub track: &'a dyn Fn(usize) -> String,
    /// A pattern's name, from its number on its track (counted from 1).
    pub pattern: &'a dyn Fn(usize) -> String,
}

/// Reads a DAWproject file. Its audio files are copied into `audio` (the
/// folder that holds a song's sound while it is open), under new names
/// where a name is taken.
pub fn import(
    path: &Path,
    audio: &Path,
    names: &ImportNames<'_>,
) -> Result<Imported, DawProjectError> {
    let file = File::open(path)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| DawProjectError::NotAProject(format!("not a ZIP container: {e}")))?;
    let text = read_entry(&mut zip, "project.xml", MAX_PROJECT_XML)?
        .ok_or_else(|| DawProjectError::NotAProject("it contains no project.xml".to_owned()))?;
    let text = String::from_utf8(text)
        .map_err(|_| DawProjectError::NotAProject("project.xml is not UTF-8 text".to_owned()))?;
    let root = xml::parse(&text).map_err(DawProjectError::NotAProject)?;
    if root.name != "Project" {
        return Err(DawProjectError::NotAProject(
            "project.xml does not describe a project".to_owned(),
        ));
    }
    let title = read_entry(&mut zip, "metadata.xml", 1024 * 1024)
        .ok()
        .flatten()
        .and_then(|bytes| xml::parse(&String::from_utf8_lossy(&bytes)).ok())
        .and_then(|metadata| metadata.child("Title").map(|t| t.text.trim().to_owned()))
        .filter(|t| !t.is_empty());
    std::fs::create_dir_all(audio)?;
    let mut importer = Importer {
        zip,
        project: Project::empty(),
        left_out: Vec::new(),
        audio_folder: audio.to_path_buf(),
        beside: path.parent().map(Path::to_path_buf),
        names,
        tracks: HashMap::new(),
        targets: HashMap::new(),
        audio: HashMap::new(),
        patterns: HashMap::new(),
        track_count: 0,
    };
    if let Some(title) = title {
        importer.project.title = title;
    }
    importer.read(&root)?;
    let mut left_out = Vec::new();
    for item in importer.left_out {
        if !left_out.contains(&item) {
            left_out.push(item);
        }
    }
    Ok(Imported {
        project: importer.project,
        left_out,
    })
}

/// An entry of the container, if it is there, within a size limit.
fn read_entry<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, DawProjectError> {
    let name = name.trim_start_matches("./");
    let Ok(entry) = zip.by_name(name) else {
        return Ok(None);
    };
    if entry.size() > limit {
        return Err(DawProjectError::NotAProject(format!(
            "{name} is unreasonably large"
        )));
    }
    let mut bytes = Vec::new();
    entry.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(DawProjectError::NotAProject(format!(
            "{name} is unreasonably large"
        )));
    }
    Ok(Some(bytes))
}

/// What an automation lane's parameter is, as the program knows it.
#[derive(Clone, Copy, Debug)]
enum Param {
    Volume(TrackId),
    Pan(TrackId),
    /// A setting from 0 to 1.
    Normalized(TrackId, AutomationTarget),
}

/// How a timeline counts time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Beats,
    Seconds,
}

impl Unit {
    fn of(node: &Node, inherited: Self) -> Self {
        match node
            .attr("timeUnit")
            .or_else(|| node.attr("contentTimeUnit"))
        {
            Some("seconds") => Self::Seconds,
            Some("beats") => Self::Beats,
            _ => inherited,
        }
    }
}

/// What makes two clips' patterns the same: the track, the notes (start,
/// length, key, velocity bits) and the length.
type PatternKey = (TrackId, Vec<(i64, i64, u8, u32)>, i64);

struct Importer<'n, R: Read + Seek> {
    zip: zip::ZipArchive<R>,
    project: Project,
    left_out: Vec<LeftOut>,
    audio_folder: PathBuf,
    /// The folder the project file is in (for audio files it refers to).
    beside: Option<PathBuf>,
    names: &'n ImportNames<'n>,
    /// Our track for each of the file's track ids.
    tracks: HashMap<String, TrackId>,
    /// What each parameter id of the file is.
    targets: HashMap<String, Param>,
    /// The audio file each audio entry of the file became.
    audio: HashMap<String, Option<AudioId>>,
    /// Patterns made so far on each track, by their notes and length (clips
    /// with the same notes share one).
    patterns: HashMap<PatternKey, PatternId>,
    track_count: usize,
}

impl<R: Read + Seek> Importer<'_, R> {
    fn read(&mut self, root: &Node) -> Result<(), DawProjectError> {
        let transport = root.child("Transport");
        let bpm = transport
            .and_then(|t| t.child("Tempo"))
            .and_then(|t| t.number("value"))
            .unwrap_or(120.0);
        self.project.tempo = TempoMap::constant(bpm);
        if let Some(signature) = transport
            .and_then(|t| t.child("TimeSignature"))
            .and_then(signature_of)
        {
            self.project.meter.set(0, signature);
        }
        if let Some(arrangement) = root.child("Arrangement") {
            self.tempo(arrangement);
            self.signatures(arrangement);
        }
        if let Some(structure) = root.child("Structure") {
            for node in &structure.children {
                match node.name.as_str() {
                    "Track" => self.track(node)?,
                    "Channel" => self.loose_channel(node)?,
                    _ => {}
                }
            }
        }
        if let Some(lanes) = root.child("Arrangement").and_then(|a| a.child("Lanes")) {
            self.lanes(lanes, None, Unit::of(lanes, Unit::Beats))?;
        }
        if root.child("Scenes").is_some_and(|s| !s.children.is_empty()) {
            self.left_out.push(LeftOut::Scenes);
        }
        Ok(())
    }

    // ----- Time

    /// Beats (or seconds, with the tempo read so far) as ticks.
    fn ticks(&self, value: f64, unit: Unit) -> Ticks {
        let value = value.clamp(0.0, MAX_BEATS);
        match unit {
            Unit::Beats => Ticks::from_quarters_f64(value),
            Unit::Seconds => {
                #[allow(clippy::cast_possible_truncation, reason = "rounded on purpose")]
                let ticks = self.project.tempo.ticks_at(value).round() as i64;
                Ticks(ticks)
            }
        }
    }

    fn tempo(&mut self, arrangement: &Node) {
        let Some(automation) = arrangement.child("TempoAutomation") else {
            return;
        };
        let unit = Unit::of(automation, Unit::Beats);
        let mut changes: Vec<TempoChange> = Vec::new();
        for point in automation.children_named("RealPoint") {
            let (Some(time), Some(bpm)) = (point.number("time"), point.number("value")) else {
                continue;
            };
            // Seconds count through the tempo read so far.
            self.project.tempo = TempoMap::from_changes(changes.clone());
            let at = self.ticks(time, unit);
            changes.push(TempoChange {
                at,
                bpm,
                glide: point.attr("interpolation") == Some("linear"),
            });
        }
        if !changes.is_empty() {
            self.project.tempo = TempoMap::from_changes(changes);
        }
    }

    fn signatures(&mut self, arrangement: &Node) {
        let Some(automation) = arrangement.child("TimeSignatureAutomation") else {
            return;
        };
        let unit = Unit::of(automation, Unit::Beats);
        let mut points: Vec<(Ticks, TimeSignature)> = automation
            .children_named("TimeSignaturePoint")
            .filter_map(|p| Some((self.ticks(p.number("time")?, unit), signature_of(p)?)))
            .collect();
        points.sort_by_key(|(at, _)| *at);
        for (at, signature) in points {
            let meter = &self.project.meter;
            let position = meter.position(at);
            let start = meter.bar_start(position.bar);
            let next = meter.bar_start(position.bar + 1);
            let bar = if (at - start).0 * 2 < (next - start).0 {
                position.bar
            } else {
                position.bar + 1
            };
            self.project.meter.set(bar, signature);
        }
    }

    // ----- Structure

    fn track(&mut self, node: &Node) -> Result<(), DawProjectError> {
        let content: Vec<&str> = node
            .attr("contentType")
            .unwrap_or("")
            .split_whitespace()
            .collect();
        if let Some(channel) = node.child("Channel") {
            match channel.attr("role").unwrap_or("regular") {
                "master" => self.master(channel)?,
                "regular" if content.contains(&"notes") || content.contains(&"audio") => {
                    self.regular_track(node, channel, &content)?;
                }
                _ => self.left_out.push(LeftOut::Routing),
            }
        }
        for child in node.children_named("Track") {
            self.track(child)?;
        }
        Ok(())
    }

    /// A channel outside any track (an effect return, say).
    fn loose_channel(&mut self, channel: &Node) -> Result<(), DawProjectError> {
        if channel.attr("role") == Some("master") {
            self.master(channel)
        } else {
            self.left_out.push(LeftOut::Routing);
            Ok(())
        }
    }

    fn master(&mut self, channel: &Node) -> Result<(), DawProjectError> {
        if let Some(gain) = channel.child("Volume").and_then(volume_of) {
            self.project.master.gain = gain;
        }
        if let Some(devices) = channel.child("Devices") {
            for device in &devices.children {
                if device.attr("deviceRole") == Some("audioFX") {
                    if let Some(kind) = self.effect_kind(device)? {
                        let enabled = enabled_of(device);
                        if let Some(id) =
                            self.project.add_effect(daw_model::ChainPlace::Master, kind)
                        {
                            self.project.update_effect(id, |e| e.enabled = enabled);
                        }
                    }
                } else {
                    self.left_out.push(LeftOut::Device(device_name(device)));
                }
            }
        }
        Ok(())
    }

    fn regular_track(
        &mut self,
        node: &Node,
        channel: &Node,
        content: &[&str],
    ) -> Result<(), DawProjectError> {
        self.track_count += 1;
        let name = node
            .attr("name")
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(|| (self.names.track)(self.track_count), str::to_owned);
        let devices: Vec<&Node> = channel
            .child("Devices")
            .map_or_else(Vec::new, |d| d.children.iter().collect());
        let instrument_device = devices
            .iter()
            .find(|d| d.attr("deviceRole") == Some("instrument"))
            .copied();
        let instrument = if content.contains(&"notes") {
            match instrument_device {
                Some(device) => self.instrument(device)?,
                None => Instrument::builtin_piano(),
            }
        } else {
            Instrument::Audio
        };
        let track = self.project.add_track(&name, instrument);
        if let Some(id) = node.attr("id") {
            self.tracks.insert(id.to_owned(), track);
        }
        // The mixer.
        let gain = channel.child("Volume").and_then(volume_of);
        #[allow(clippy::cast_possible_truncation, reason = "a pan position")]
        let pan = channel
            .child("Pan")
            .and_then(|p| p.number("value"))
            .map(|v| (v as f32).mul_add(2.0, -1.0).clamp(-1.0, 1.0));
        let mute = channel
            .child("Mute")
            .and_then(|m| m.flag("value"))
            .unwrap_or(false);
        let solo = channel.flag("solo").unwrap_or(false);
        if let Some(t) = self.project.track_mut(track) {
            if let Some(gain) = gain {
                t.strip.gain = gain;
            }
            if let Some(pan) = pan {
                t.strip.pan = pan;
            }
            t.strip.mute = mute;
            t.strip.solo = solo;
        }
        for (part, param) in [("Volume", Param::Volume(track)), ("Pan", Param::Pan(track))] {
            if let Some(id) = channel.child(part).and_then(|p| p.attr("id")) {
                self.targets.insert(id.to_owned(), param);
            }
        }
        if channel
            .child("Sends")
            .is_some_and(|s| !s.children.is_empty())
        {
            self.left_out.push(LeftOut::Routing);
        }
        // The instrument's automatable parameters.
        if let Some(device) = instrument_device {
            let synth_or_plugin = self
                .project
                .track(track)
                .is_some_and(|t| t.instrument.plugin().is_some() || t.instrument.synth().is_some());
            if synth_or_plugin {
                self.parameters(device, |param| {
                    Param::Normalized(track, AutomationTarget::Instrument { param })
                });
            }
        }
        // The effects, in order.
        for device in devices {
            match device.attr("deviceRole") {
                Some("instrument") => {}
                Some("audioFX") => {
                    let Some(kind) = self.effect_kind(device)? else {
                        continue;
                    };
                    let enabled = enabled_of(device);
                    let Some(effect) = self
                        .project
                        .add_effect(daw_model::ChainPlace::Track(track), kind)
                    else {
                        continue;
                    };
                    self.project.update_effect(effect, |e| e.enabled = enabled);
                    self.effect_parameters(device, track, effect);
                }
                _ => self.left_out.push(LeftOut::Device(device_name(device))),
            }
        }
        Ok(())
    }

    /// Remembers a device's parameters (`parameterID` is the program's number for
    /// the setting, or the plugin's parameter id).
    fn parameters(&mut self, device: &Node, param: impl Fn(u32) -> Param) {
        let Some(parameters) = device.child("Parameters") else {
            return;
        };
        for parameter in &parameters.children {
            if let (Some(id), Some(number)) = (
                parameter.attr("id"),
                parameter
                    .attr("parameterID")
                    .and_then(|n| n.trim().parse::<u32>().ok()),
            ) {
                self.targets.insert(id.to_owned(), param(number));
            }
        }
    }

    fn effect_parameters(&mut self, device: &Node, track: TrackId, effect: EffectId) {
        self.parameters(device, |param| {
            Param::Normalized(track, AutomationTarget::Effect { effect, param })
        });
    }

    /// A device's saved settings file.
    fn state(&mut self, device: &Node) -> Result<Option<Vec<u8>>, DawProjectError> {
        let Some(state) = device.child("State") else {
            return Ok(None);
        };
        let Some(path) = state.attr("path") else {
            return Ok(None);
        };
        if state.flag("external") == Some(true) {
            return Ok(None);
        }
        read_entry(&mut self.zip, path, MAX_STATE)
    }

    fn plugin(
        &mut self,
        device: &Node,
        format: PluginFormat,
    ) -> Result<PluginSetup, DawProjectError> {
        let given = device.attr("deviceID").unwrap_or("").trim().to_owned();
        let id = match format {
            PluginFormat::Clap => given,
            PluginFormat::Vst3 => vst3_id_from_canonical(&given, cfg!(windows)),
        };
        let state = self.state(device)?.and_then(|bytes| match format {
            PluginFormat::Clap => Some(bytes),
            PluginFormat::Vst3 => from_vstpreset(&bytes).map(|(_, state)| state),
        });
        Ok(PluginSetup {
            plugin: PluginRef {
                format,
                id,
                name: device_name(device),
                vendor: device.attr("deviceVendor").unwrap_or("").to_owned(),
                path: String::new(),
            },
            state: state.map(PluginState::new),
        })
    }

    fn instrument(&mut self, device: &Node) -> Result<Instrument, DawProjectError> {
        match device.name.as_str() {
            "ClapPlugin" => {
                return Ok(Instrument::Plugin(self.plugin(device, PluginFormat::Clap)?));
            }
            "Vst3Plugin" => {
                return Ok(Instrument::Plugin(self.plugin(device, PluginFormat::Vst3)?));
            }
            _ => {}
        }
        if device.attr("deviceID").is_some_and(is_own_device)
            && let Some(bytes) = self.state(device)?
            && let Ok(instrument) = serde_json::from_slice::<Instrument>(&bytes)
            && !matches!(instrument, Instrument::Plugin(_) | Instrument::Audio)
        {
            return Ok(match instrument {
                Instrument::Synth(settings) => Instrument::Synth(settings.repaired()),
                Instrument::Kit { pads } => self.kit(pads, &bytes)?,
                other => other,
            });
        }
        self.left_out.push(LeftOut::Device(device_name(device)));
        Ok(Instrument::builtin_piano())
    }

    /// An effect device as one of the program's effects, if the program has one like
    /// it.
    fn effect_kind(&mut self, device: &Node) -> Result<Option<EffectKind>, DawProjectError> {
        match device.name.as_str() {
            "ClapPlugin" => {
                return Ok(Some(EffectKind::Plugin(
                    self.plugin(device, PluginFormat::Clap)?,
                )));
            }
            "Vst3Plugin" => {
                return Ok(Some(EffectKind::Plugin(
                    self.plugin(device, PluginFormat::Vst3)?,
                )));
            }
            _ => {}
        }
        if device.attr("deviceID").is_some_and(is_own_device)
            && let Some(bytes) = self.state(device)?
            && let Ok(kind) = serde_json::from_slice::<EffectKind>(&bytes)
            && !matches!(kind, EffectKind::Plugin(_))
        {
            return Ok(Some(kind));
        }
        let mapped = match device.name.as_str() {
            "Equalizer" => Some(EffectKind::Eq(equalizer(device))),
            "Compressor" => Some(EffectKind::Compressor(compressor(device))),
            "Limiter" => Some(EffectKind::Limiter(limiter(device))),
            _ => None,
        };
        if mapped.is_none() {
            self.left_out.push(LeftOut::Device(device_name(device)));
        }
        Ok(mapped)
    }

    // ----- The arrangement

    fn lanes(
        &mut self,
        node: &Node,
        track: Option<TrackId>,
        unit: Unit,
    ) -> Result<(), DawProjectError> {
        let track = node
            .attr("track")
            .and_then(|id| self.tracks.get(id).copied())
            .or(track);
        let unit = Unit::of(node, unit);
        for child in &node.children {
            match child.name.as_str() {
                "Lanes" => self.lanes(child, track, unit)?,
                "Clips" => {
                    if let Some(track) = track {
                        let clip_unit = Unit::of(child, unit);
                        for clip in child.children_named("Clip") {
                            self.clip(clip, track, 0.0, None, clip_unit)?;
                        }
                    }
                }
                "Points" => self.points(child, unit),
                "Notes" => {
                    if let Some(track) = track {
                        self.loose_notes(child, track, unit);
                    }
                }
                "ClipSlot" => self.left_out.push(LeftOut::Scenes),
                _ => {}
            }
        }
        Ok(())
    }

    /// A clip at `base` beats (the position of its timeline's zero),
    /// kept inside `window` (its parent clip, in song beats).
    fn clip(
        &mut self,
        clip: &Node,
        track: TrackId,
        base: f64,
        window: Option<(f64, f64)>,
        unit: Unit,
    ) -> Result<(), DawProjectError> {
        if clip.flag("enable") == Some(false) {
            return Ok(());
        }
        let Some(time) = clip.number("time") else {
            return Ok(());
        };
        let start = base + self.beats(time, unit);
        let mut end = start
            + clip
                .number("duration")
                .map_or(0.0, |d| self.beats_length(time, d, unit));
        let mut start = start;
        if let Some((from, to)) = window {
            start = start.max(from);
            end = end.min(to);
        }
        let content_unit = Unit::of(clip, unit);
        let play_start = clip.number("playStart").unwrap_or(0.0);
        for content in &clip.children {
            match content.name.as_str() {
                "Notes" | "Lanes" => {
                    let notes = collect_notes(content);
                    if !notes.is_empty() {
                        self.note_clip(clip, track, start, end, play_start, &notes, content_unit);
                    }
                }
                "Audio" | "Warps" => {
                    self.audio_clip(clip, content, track, (start, end), play_start, content_unit)?;
                }
                "Clips" => {
                    let inner_unit = Unit::of(content, content_unit);
                    let inner_base = start - self.beats(play_start, content_unit);
                    for inner in content.children_named("Clip") {
                        self.clip(inner, track, inner_base, Some((start, end)), inner_unit)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// A position in beats from a timeline's units.
    fn beats(&self, value: f64, unit: Unit) -> f64 {
        match unit {
            Unit::Beats => value.clamp(-MAX_BEATS, MAX_BEATS),
            Unit::Seconds => self.ticks(value, Unit::Seconds).as_quarters(),
        }
    }

    /// A length in beats from a timeline's units, starting at `at`.
    fn beats_length(&self, at: f64, length: f64, unit: Unit) -> f64 {
        match unit {
            Unit::Beats => length.clamp(0.0, MAX_BEATS),
            Unit::Seconds => self.beats(at + length, unit) - self.beats(at, unit),
        }
    }

    /// Notes placed straight on a track's timeline, without a clip.
    fn loose_notes(&mut self, notes: &Node, track: TrackId, unit: Unit) {
        let list = collect_notes(notes);
        let Some(end) = list.iter().map(|n| n.time + n.duration).reduce(f64::max) else {
            return;
        };
        let wrapper = Node::default();
        let end = self.beats(end, unit);
        self.note_clip(&wrapper, track, 0.0, end, 0.0, &list, unit);
    }

    #[allow(clippy::too_many_arguments, reason = "a clip's whole window")]
    fn note_clip(
        &mut self,
        clip: &Node,
        track: TrackId,
        start: f64,
        end: f64,
        play_start: f64,
        notes: &[RawNote],
        unit: Unit,
    ) {
        if end <= start
            || self
                .project
                .track(track)
                .is_none_or(|t| t.instrument.is_audio())
        {
            return;
        }
        let duration = end - start;
        let to_beats = |value: f64| match unit {
            Unit::Beats => value,
            // Note times in seconds count from the clip's own start.
            Unit::Seconds => {
                value * self.project.tempo.bpm_at(Ticks::from_quarters_f64(start)) / 60.0
            }
        };
        let play_start = to_beats(play_start);
        let loop_region = match (clip.number("loopStart"), clip.number("loopEnd")) {
            (Some(a), Some(b)) if to_beats(b) - to_beats(a) > 1e-6 => {
                Some((to_beats(a), to_beats(b)))
            }
            _ => None,
        };
        // The passes through the clip's content: (content from, content
        // to, where content zero lands in the clip).
        let mut passes = Vec::new();
        match loop_region {
            None => passes.push((play_start, play_start + duration, -play_start)),
            Some((loop_start, loop_end)) => {
                let first_end = loop_end.max(play_start);
                passes.push((play_start, first_end, -play_start));
                let mut at = first_end - play_start;
                let length = loop_end - loop_start;
                while at < duration && passes.len() < 100_000 {
                    passes.push((loop_start, loop_end, at - loop_start));
                    at += length;
                }
            }
        }
        let mut placed: Vec<Note> = Vec::new();
        for (from, to, shift) in passes {
            for note in notes {
                let time = to_beats(note.time);
                if time < from || time >= to {
                    continue;
                }
                let at = time + shift;
                if at < 0.0 || at >= duration {
                    continue;
                }
                let finish = (time + to_beats(note.duration)).min(to) + shift;
                let finish = finish.min(duration);
                let note_start = Ticks::from_quarters_f64(at);
                let length = Ticks::from_quarters_f64(finish) - note_start;
                if length <= Ticks::ZERO {
                    continue;
                }
                placed.push(Note {
                    id: NoteId(0),
                    start: note_start,
                    length,
                    key: note.key,
                    velocity: note.velocity,
                    pitch_offset: 0.0,
                });
            }
        }
        placed.sort_by_key(|n| (n.start, n.key));
        let clip_start = Ticks::from_quarters_f64(start);
        let clip_length = Ticks::from_quarters_f64(end) - clip_start;
        if clip_length <= Ticks::ZERO {
            return;
        }
        // Notes that repeat every few bars (a beat written out, as the program
        // writes its patterns) become a pattern of one repeat, which the
        // clip plays again and again.
        let bar = self.project.meter.signature_at(clip_start).bar_length();
        let (placed, length) = match repeating(&placed, clip_length, bar) {
            Some(period) => (
                placed.into_iter().filter(|n| n.start < period).collect(),
                period,
            ),
            None => (placed, clip_length),
        };
        let key: Vec<(i64, i64, u8, u32)> = placed
            .iter()
            .map(|n| (n.start.0, n.length.0, n.key, n.velocity.to_bits()))
            .collect();
        let pattern = if let Some(&pattern) = self.patterns.get(&(track, key.clone(), length.0)) {
            pattern
        } else {
            {
                let count = self.project.track_patterns(track).count() + 1;
                let name = clip
                    .attr("name")
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map_or_else(|| (self.names.pattern)(count), str::to_owned);
                let pattern = self.project.add_pattern_for(track, &name, length);
                self.project.add_notes(pattern, &placed);
                self.patterns.insert((track, key, length.0), pattern);
                pattern
            }
        };
        self.project
            .add_clip(track, clip_start, clip_length, pattern);
    }

    fn audio_clip(
        &mut self,
        clip: &Node,
        content: &Node,
        track: TrackId,
        (start, end): (f64, f64),
        play_start: f64,
        content_unit: Unit,
    ) -> Result<(), DawProjectError> {
        if end <= start
            || self
                .project
                .track(track)
                .is_none_or(|t| !t.instrument.is_audio())
        {
            return Ok(());
        }
        let (audio, offset_seconds) = if content.name == "Warps" {
            let Some(audio) = content.child("Audio") else {
                return Ok(());
            };
            // Warp points map the clip's time to seconds of the file.
            let warp_unit = Unit::of(content, content_unit);
            let warps: Vec<(f64, f64)> = content
                .children_named("Warp")
                .filter_map(|w| {
                    Some((
                        self.beats(w.number("time")?, warp_unit),
                        w.number("contentTime")?,
                    ))
                })
                .collect();
            let offset = warped(&warps, self.beats(play_start, content_unit)).unwrap_or(0.0);
            if let [(t0, c0), .., (t1, c1)] = warps.as_slice() {
                let song = self
                    .project
                    .tempo
                    .seconds_at(Ticks::from_quarters_f64(start + t1 - t0))
                    - self
                        .project
                        .tempo
                        .seconds_at(Ticks::from_quarters_f64(start));
                if song > 0.0 && ((c1 - c0) / song - 1.0).abs() > 0.01 {
                    self.left_out.push(LeftOut::StretchedAudio);
                }
            }
            (audio, offset)
        } else {
            let offset = match content_unit {
                Unit::Seconds => play_start,
                Unit::Beats => {
                    let from = Ticks::from_quarters_f64(start);
                    let to = Ticks::from_quarters_f64(start + play_start);
                    self.project.tempo.seconds_at(to) - self.project.tempo.seconds_at(from)
                }
            };
            (content, offset)
        };
        let Some(id) = self.audio_file(audio)? else {
            return Ok(());
        };
        let Some(file) = self.project.audio_file(id).cloned() else {
            return Ok(());
        };
        let clip_start = Ticks::from_quarters_f64(start);
        let length = Ticks::from_quarters_f64(end) - clip_start;
        let offset = file.frames_in(offset_seconds.max(0.0)).min(file.frames);
        let fade = |key: &str| {
            let value = clip.number(key).unwrap_or(0.0).max(0.0);
            match clip.attr("fadeTimeUnit") {
                Some("beats") => {
                    self.project
                        .tempo
                        .seconds_at(Ticks::from_quarters_f64(start + value))
                        - self.project.tempo.seconds_at(clip_start)
                }
                _ => value,
            }
        };
        #[allow(clippy::cast_possible_truncation, reason = "fades of seconds")]
        let (fade_in, fade_out) = (fade("fadeInTime") as f32, fade("fadeOutTime") as f32);
        if let Some(clip_id) = self
            .project
            .add_audio_clip(track, clip_start, length, id, offset)
        {
            self.project.update_clip(clip_id, |c| {
                if let ClipContent::Audio {
                    fade_in: a,
                    fade_out: b,
                    ..
                } = &mut c.content
                {
                    *a = fade_in;
                    *b = fade_out;
                }
            });
        }
        Ok(())
    }

    /// The audio file an `<Audio>` element names, copied into the audio
    /// folder the first time (`None` if it cannot be found or read).
    fn audio_file(&mut self, audio: &Node) -> Result<Option<AudioId>, DawProjectError> {
        let Some(path) = audio
            .child("File")
            .and_then(|f| f.attr("path"))
            .map(str::to_owned)
        else {
            return Ok(None);
        };
        let external = audio.child("File").and_then(|f| f.flag("external")) == Some(true);
        self.audio_entry(path, external)
    }

    /// An audio file of the project (an entry of the container, or a file
    /// beside it when `external`), copied into the audio folder the first
    /// time (`None` if it cannot be found or read).
    fn audio_entry(
        &mut self,
        path: String,
        external: bool,
    ) -> Result<Option<AudioId>, DawProjectError> {
        if let Some(found) = self.audio.get(&path) {
            return Ok(*found);
        }
        let shown = Path::new(&path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        let bytes = if external {
            let candidate = PathBuf::from(&path);
            let candidate = if candidate.is_absolute() {
                candidate
            } else {
                self.beside.clone().unwrap_or_default().join(candidate)
            };
            std::fs::metadata(&candidate)
                .ok()
                .filter(|m| m.len() <= crate::audio::MAX_FILE_BYTES)
                .and_then(|_| std::fs::read(&candidate).ok())
        } else {
            read_entry(&mut self.zip, &path, crate::audio::MAX_FILE_BYTES)
                .ok()
                .flatten()
        };
        let decoded = bytes.as_ref().and_then(|b| {
            crate::audio::decode(b)
                .ok()
                .map(|d| (d.frames(), d.sample_rate, d.channels.len()))
        });
        let (Some(bytes), Some((frames, rate, channels))) = (bytes, decoded) else {
            self.left_out.push(LeftOut::MissingAudio(shown));
            self.audio.insert(path, None);
            return Ok(None);
        };
        let stored = free_name(&self.audio_folder, &shown);
        std::fs::write(self.audio_folder.join(&stored), &bytes)?;
        let id = self.project.add_audio_file(AudioFile {
            id: AudioId(0),
            name: Path::new(&shown)
                .file_stem()
                .map_or_else(|| shown.clone(), |s| s.to_string_lossy().into_owned()),
            file: stored,
            frames: frames as u64,
            sample_rate: rate,
            channels: u16::try_from(channels.clamp(1, 2)).unwrap_or(2),
        });
        self.audio.insert(path, Some(id));
        Ok(Some(id))
    }

    /// A custom kit (decision 0025): its pads, each with its sound brought in
    /// from the file (the state names each pad's sound under `files`); a pad
    /// whose sound is missing is left out.
    fn kit(
        &mut self,
        pads: Vec<daw_model::KitPad>,
        state: &[u8],
    ) -> Result<Instrument, DawProjectError> {
        let files = serde_json::from_slice::<serde_json::Value>(state)
            .ok()
            .and_then(|value| value.get("files").cloned())
            .unwrap_or_default();
        let mut kept = Vec::new();
        for mut pad in pads {
            let Some(path) = files
                .get(pad.audio.0.to_string())
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            if let Some(audio) = self.audio_entry(path.to_owned(), false)? {
                pad.audio = audio;
                kept.push(pad);
            }
        }
        Ok(Instrument::Kit { pads: kept })
    }

    /// An automation lane.
    fn points(&mut self, points: &Node, unit: Unit) {
        let unit = Unit::of(points, unit);
        let Some(param) = points
            .child("Target")
            .and_then(|t| t.attr("parameter"))
            .and_then(|id| self.targets.get(id).copied())
        else {
            return;
        };
        let decibels = points.attr("unit") == Some("decibel");
        let mut read: Vec<(Ticks, f32, bool)> = points
            .children_named("RealPoint")
            .filter_map(|p| {
                let at = self.ticks(p.number("time")?, unit);
                #[allow(clippy::cast_possible_truncation, reason = "a setting's value")]
                let raw = p.number("value")? as f32;
                let value = match param {
                    Param::Volume(_) if decibels => gain_volume(10.0_f32.powf(raw / 20.0)),
                    Param::Volume(_) => gain_volume(raw),
                    Param::Pan(_) | Param::Normalized(..) => raw.clamp(0.0, 1.0),
                };
                Some((at, value, p.attr("interpolation") == Some("hold")))
            })
            .collect();
        read.sort_by_key(|(at, _, _)| *at);
        let (track, target) = match param {
            Param::Volume(track) => (track, AutomationTarget::Volume),
            Param::Pan(track) => (track, AutomationTarget::Pan),
            Param::Normalized(track, target) => (track, target),
        };
        if read.is_empty() {
            return;
        }
        self.project.add_lane(track, target);
        self.project.update_lane(track, target, |lane| {
            for (index, &(at, value, hold)) in read.iter().enumerate() {
                lane.insert(AutomationPoint { at, value });
                // A held value stays until just before the next point.
                if hold
                    && let Some(&(next, _, _)) = read.get(index + 1)
                    && next.0 - at.0 > 1
                {
                    lane.insert(AutomationPoint {
                        at: Ticks(next.0 - 1),
                        value,
                    });
                }
            }
        });
    }
}

/// The shortest whole number of bars after which notes repeat exactly
/// until the end of a clip (each repeat inside its own bars), if they do
/// and it is shorter than the clip.
fn repeating(notes: &[Note], length: Ticks, bar: Ticks) -> Option<Ticks> {
    if notes.is_empty() || bar <= Ticks::ZERO || length.0 % bar.0 != 0 {
        return None;
    }
    let bars = length.0 / bar.0;
    let fingerprint = |n: &Note| (n.length, n.key, n.velocity.to_bits());
    for count in 1..bars {
        if bars % count != 0 {
            continue;
        }
        let period = bar * count;
        let first: Vec<&Note> = notes.iter().filter(|n| n.start < period).collect();
        if first.iter().any(|n| n.end() > period) {
            continue;
        }
        let repeats = notes.len() == first.len() * usize::try_from(bars / count).unwrap_or(0)
            && notes.iter().all(|n| {
                let into = Ticks(n.start.0 % period.0);
                n.end() - n.start + into <= period
                    && first
                        .iter()
                        .any(|f| f.start == into && fingerprint(f) == fingerprint(n))
            });
        if repeats {
            return Some(period);
        }
    }
    None
}

/// A note as the file gives it: times in its timeline's units.
#[derive(Clone, Copy, Debug)]
struct RawNote {
    time: f64,
    duration: f64,
    key: u8,
    velocity: f32,
}

/// The notes of a `<Notes>` element, or of the `<Notes>` inside `<Lanes>`.
fn collect_notes(node: &Node) -> Vec<RawNote> {
    let mut notes = Vec::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        match node.name.as_str() {
            "Notes" => {
                for note in node.children_named("Note") {
                    let (Some(time), Some(duration), Some(key)) = (
                        note.number("time"),
                        note.number("duration"),
                        note.number("key"),
                    ) else {
                        continue;
                    };
                    if !(0.0..=127.0).contains(&key) || duration <= 0.0 {
                        continue;
                    }
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a key from 0 to 127"
                    )]
                    let key = key as u8;
                    #[allow(clippy::cast_possible_truncation, reason = "a velocity")]
                    let velocity = note.number("vel").unwrap_or(0.8).clamp(0.0, 1.0) as f32;
                    notes.push(RawNote {
                        time: time.clamp(0.0, MAX_BEATS),
                        duration: duration.min(MAX_BEATS),
                        key,
                        velocity,
                    });
                }
            }
            "Lanes" => stack.extend(
                node.children
                    .iter()
                    .filter(|c| matches!(c.name.as_str(), "Notes" | "Lanes")),
            ),
            _ => {}
        }
    }
    notes
}

/// Seconds of the file at a clip time, from warp points (straight lines
/// between them, carried on beyond the ends).
fn warped(warps: &[(f64, f64)], at: f64) -> Option<f64> {
    let (first, rest) = warps.split_first()?;
    let Some(second) = rest.first() else {
        return Some(first.1 + (at - first.0));
    };
    let index = warps
        .partition_point(|w| w.0 <= at)
        .clamp(1, warps.len() - 1);
    let (t0, c0) = warps[index - 1];
    let (t1, c1) = warps[index];
    let _ = second;
    if (t1 - t0).abs() < 1e-12 {
        return Some(c0);
    }
    Some(c0 + (at - t0) / (t1 - t0) * (c1 - c0))
}

#[allow(clippy::cast_possible_truncation, reason = "checked by try_from")]
fn signature_of(node: &Node) -> Option<TimeSignature> {
    let numerator = u8::try_from(node.number("numerator")? as i64).ok()?;
    let denominator = u8::try_from(node.number("denominator")? as i64).ok()?;
    TimeSignature::new(numerator, denominator)
}

/// A volume parameter as linear gain (0 to 2).
fn volume_of(volume: &Node) -> Option<f32> {
    let value = volume.number("value")?;
    let gain = match volume.attr("unit") {
        Some("decibel") => 10.0_f64.powf(value / 20.0),
        _ => value,
    };
    #[allow(clippy::cast_possible_truncation, reason = "a gain")]
    Some((gain as f32).clamp(0.0, 2.0))
}

fn enabled_of(device: &Node) -> bool {
    device
        .child("Enabled")
        .and_then(|e| e.flag("value"))
        .unwrap_or(true)
}

fn device_name(device: &Node) -> String {
    device
        .attr("deviceName")
        .or_else(|| device.attr("name"))
        .unwrap_or(&device.name)
        .to_owned()
}

/// A real parameter of a standard device, in its unit.
fn real(device: &Node, name: &str) -> Option<f32> {
    let node = device.child(name)?;
    #[allow(clippy::cast_possible_truncation, reason = "a setting's value")]
    let value = node.number("value")? as f32;
    Some(value)
}

/// Seconds (or milliseconds, if the file says so) as milliseconds.
fn milliseconds(device: &Node, name: &str) -> Option<f32> {
    let value = real(device, name)?;
    Some(match device.child(name).and_then(|n| n.attr("unit")) {
        Some("seconds") | None => value * 1000.0,
        _ => value,
    })
}

/// DAWproject's standard EQ as the program's: a high-pass for the low cut, the
/// first low shelf, bell and high shelf, a low-pass for the high cut.
fn equalizer(device: &Node) -> EqSettings {
    let mut eq = EqSettings::default();
    let mut seen = HashSet::new();
    for band in device.children_named("Band") {
        if band.child("Enabled").and_then(|e| e.flag("value")) == Some(false) {
            continue;
        }
        let kind = band.attr("type").unwrap_or("");
        if !seen.insert(kind.to_owned()) {
            continue;
        }
        let freq = real(band, "Freq");
        let gain = real(band, "Gain").unwrap_or(0.0).clamp(-15.0, 15.0);
        match kind {
            "highPass" => eq.low_cut_hz = freq.map(|f| f.clamp(20.0, 500.0)),
            "lowShelf" => {
                eq.low_hz = freq.unwrap_or(eq.low_hz).clamp(40.0, 500.0);
                eq.low_gain_db = gain;
            }
            "bell" => {
                eq.mid_hz = freq.unwrap_or(eq.mid_hz).clamp(200.0, 8000.0);
                eq.mid_gain_db = gain;
                eq.mid_q = real(band, "Q").unwrap_or(eq.mid_q).clamp(0.3, 6.0);
            }
            "highShelf" => {
                eq.high_hz = freq.unwrap_or(eq.high_hz).clamp(1500.0, 16_000.0);
                eq.high_gain_db = gain;
            }
            "lowPass" => eq.high_cut_hz = freq.map(|f| f.clamp(2000.0, 20_000.0)),
            _ => {}
        }
    }
    eq
}

fn compressor(device: &Node) -> CompressorSettings {
    let defaults = CompressorSettings::default();
    CompressorSettings {
        threshold_db: real(device, "Threshold")
            .unwrap_or(defaults.threshold_db)
            .clamp(-60.0, 0.0),
        ratio: real(device, "Ratio")
            .unwrap_or(defaults.ratio)
            .clamp(1.0, 20.0),
        attack_ms: milliseconds(device, "Attack")
            .unwrap_or(defaults.attack_ms)
            .clamp(0.1, 100.0),
        release_ms: milliseconds(device, "Release")
            .unwrap_or(defaults.release_ms)
            .clamp(10.0, 1000.0),
        knee_db: defaults.knee_db,
        makeup_db: real(device, "OutputGain").unwrap_or(0.0).clamp(0.0, 24.0),
    }
}

fn limiter(device: &Node) -> LimiterSettings {
    let defaults = LimiterSettings::default();
    LimiterSettings {
        gain_db: real(device, "InputGain").unwrap_or(0.0).clamp(0.0, 18.0),
        ceiling_db: real(device, "Threshold")
            .unwrap_or(defaults.ceiling_db)
            .clamp(-12.0, 0.0),
        release_ms: milliseconds(device, "Release")
            .unwrap_or(defaults.release_ms)
            .clamp(10.0, 1000.0),
    }
}

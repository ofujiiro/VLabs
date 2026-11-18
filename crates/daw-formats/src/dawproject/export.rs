// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing a song as a DAWproject file (decision 0020).

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufWriter, Seek, Write};
use std::path::{Path, PathBuf};

use daw_model::{
    AutomationLane, AutomationTarget, ClipContent, Effect, EffectKind, Instrument, PluginFormat,
    PluginSetup, Project, Ticks, Track, volume_gain,
};
use zip::write::SimpleFileOptions;

use super::xml::Element;
use super::{DawProjectError, own_device_id, vst3_id_to_canonical, vstpreset};
use crate::atomic::write_atomically_with;
use crate::native::is_plain_name;

/// The DAWproject version written.
const VERSION: &str = "1.0";

/// Writes a song as a DAWproject file at `path`, safely (the file is
/// complete before it replaces anything). The song's audio files are
/// copied in from `audio`, the folder that holds them while the song is
/// open.
pub fn export(project: &Project, audio: &Path, path: &Path) -> Result<(), DawProjectError> {
    let mut writer = Writer {
        project,
        next_id: 0,
        files: Vec::new(),
        audio: Vec::new(),
        audio_folder: audio.to_path_buf(),
    };
    let document = writer.project_element()?.document();
    let metadata = Element::new("MetaData")
        .child(Element::new("Title").text(&project.title))
        .document();
    let mut failure = None;
    let written = write_atomically_with(path, |file| {
        let result = writer
            .write_zip(BufWriter::new(file), &document, &metadata)
            .and_then(|mut out| out.flush().map_err(DawProjectError::Io));
        match result {
            Ok(()) => Ok(()),
            Err(DawProjectError::Io(error)) => Err(error),
            Err(other) => {
                failure = Some(other);
                Err(io::Error::other("the project could not be written"))
            }
        }
    });
    match (written, failure) {
        (Ok(()), _) => Ok(()),
        (Err(_), Some(failure)) => Err(failure),
        (Err(error), None) => Err(DawProjectError::Io(error)),
    }
}

/// A position or length in beats (quarter notes), as DAWproject counts.
fn beats(at: Ticks) -> f64 {
    at.as_quarters()
}

/// The ids of a track's parts that automation lanes point at.
#[derive(Default)]
struct Targets {
    volume: String,
    pan: String,
    /// Each automated setting's parameter id.
    params: HashMap<AutomationTarget, String>,
}

struct Writer<'a> {
    project: &'a Project,
    next_id: u64,
    /// Plugin settings and the program's own device settings: entry and bytes.
    files: Vec<(String, Vec<u8>)>,
    /// Audio files: entry in the ZIP and file on disk.
    audio: Vec<(String, PathBuf)>,
    audio_folder: PathBuf,
}

impl Writer<'_> {
    fn id(&mut self) -> String {
        let id = format!("id{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn project_element(&mut self) -> Result<Element, DawProjectError> {
        let project = self.project;
        let tempo_id = self.id();
        let signature_id = self.id();
        let first = project.meter.initial();
        let transport = Element::new("Transport")
            .child(
                Element::new("Tempo")
                    .attr("unit", "bpm")
                    .number("value", project.tempo.initial_bpm())
                    .attr("min", "20")
                    .attr("max", "999")
                    .attr("id", &tempo_id)
                    .attr("name", "Tempo"),
            )
            .child(
                Element::new("TimeSignature")
                    .attr("numerator", first.numerator)
                    .attr("denominator", first.denominator)
                    .attr("id", &signature_id),
            );

        let master_channel = self.id();
        let mut structure = Element::new("Structure");
        let mut lanes = Element::new("Lanes")
            .attr("timeUnit", "beats")
            .attr("id", self.id());
        for track in &project.tracks {
            let track_id = self.id();
            let (element, targets) = self.track_element(track, &track_id, &master_channel);
            structure.push(element);
            lanes.push(self.track_lanes(track, &track_id, &targets)?);
        }
        structure.push(self.master_element(&master_channel));

        let mut arrangement = Element::new("Arrangement").attr("id", self.id());
        arrangement.push(lanes);
        arrangement.push(self.tempo_automation(&tempo_id));
        arrangement.push(self.signature_automation(&signature_id));

        Ok(Element::new("Project")
            .attr("version", VERSION)
            .child(
                Element::new("Application")
                    .attr("name", daw_ident::PRODUCT_NAME)
                    .attr("version", env!("CARGO_PKG_VERSION")),
            )
            .child(transport)
            .child(structure)
            .child(arrangement)
            .child(Element::new("Scenes")))
    }

    /// A track of the structure, and the ids its lanes point at.
    fn track_element(
        &mut self,
        track: &Track,
        track_id: &str,
        master_channel: &str,
    ) -> (Element, Targets) {
        let content = if track.instrument.is_audio() {
            "audio"
        } else {
            "notes"
        };
        let automated: HashSet<AutomationTarget> = track
            .automation
            .iter()
            .filter(|l| !l.points.is_empty())
            .map(|l| l.target)
            .collect();
        let mut targets = Targets::default();
        let mut devices = Element::new("Devices");
        if let Some(device) = self.instrument_device(&track.instrument, &automated, &mut targets) {
            devices.push(device);
        }
        for effect in &track.effects {
            devices.push(self.effect_device(effect, &automated, &mut targets));
        }
        let channel_id = self.id();
        let (channel, volume, pan) = self.channel(
            &channel_id,
            devices,
            track.strip.gain,
            track.strip.pan,
            track.strip.mute,
        );
        targets.volume = volume;
        targets.pan = pan;
        let channel = channel
            .attr("destination", master_channel)
            .attr("role", "regular")
            .attr("solo", track.strip.solo);
        let element = Element::new("Track")
            .attr("contentType", content)
            .attr("loaded", "true")
            .attr("id", track_id)
            .attr("name", &track.name)
            .child(channel);
        (element, targets)
    }

    /// A channel with its devices, mute, pan and volume; the ids of its
    /// volume and pan.
    fn channel(
        &mut self,
        id: &str,
        devices: Element,
        gain: f32,
        pan: f32,
        mute: bool,
    ) -> (Element, String, String) {
        let mute_id = self.id();
        let pan_id = self.id();
        let volume_id = self.id();
        let channel = Element::new("Channel")
            .attr("audioChannels", 2)
            .attr("id", id)
            .child(devices)
            .child(
                Element::new("Mute")
                    .attr("value", mute)
                    .attr("id", mute_id)
                    .attr("name", "Mute"),
            )
            .child(
                Element::new("Pan")
                    .attr("unit", "normalized")
                    .number("value", f64::from(f32::midpoint(pan.clamp(-1.0, 1.0), 1.0)))
                    .attr("min", "0")
                    .attr("max", "1")
                    .attr("id", &pan_id)
                    .attr("name", "Pan"),
            )
            .child(
                Element::new("Volume")
                    .attr("unit", "linear")
                    .number("value", f64::from(gain.clamp(0.0, 2.0)))
                    .attr("min", "0")
                    .attr("max", "2")
                    .attr("id", &volume_id)
                    .attr("name", "Volume"),
            );
        (channel, volume_id, pan_id)
    }

    fn master_element(&mut self, channel_id: &str) -> Element {
        let master = &self.project.master;
        let mut devices = Element::new("Devices");
        let mut unused = Targets::default();
        for effect in &master.effects {
            devices.push(self.effect_device(effect, &HashSet::new(), &mut unused));
        }
        let (channel, _, _) = self.channel(channel_id, devices, master.gain, 0.0, false);
        let id = self.id();
        Element::new("Track")
            .attr("contentType", "audio notes")
            .attr("loaded", "true")
            .attr("id", id)
            .attr("name", "Master")
            .child(channel.attr("role", "master").attr("solo", "false"))
    }

    /// A settings file in the ZIP; returns its entry name.
    fn add_file(&mut self, extension: &str, bytes: Vec<u8>) -> String {
        let name = format!("plugins/{}.{extension}", self.files.len() + 1);
        self.files.push((name.clone(), bytes));
        name
    }

    /// The `<Parameters>` of a device: one for each automated setting of
    /// it, numbered as the program numbers them, as a fraction of its range.
    fn parameters(
        &mut self,
        wanted: impl Iterator<Item = (AutomationTarget, u32, Option<f32>)>,
        targets: &mut Targets,
    ) -> Element {
        let mut parameters = Element::new("Parameters");
        for (target, number, value) in wanted {
            let id = self.id();
            let mut parameter = Element::new("RealParameter")
                .attr("parameterID", number)
                .attr("unit", "normalized")
                .attr("min", "0")
                .attr("max", "1")
                .attr("id", &id);
            if let Some(value) = value {
                parameter = parameter.number("value", f64::from(value));
            }
            parameters.push(parameter);
            targets.params.insert(target, id);
        }
        parameters
    }

    fn instrument_device(
        &mut self,
        instrument: &Instrument,
        automated: &HashSet<AutomationTarget>,
        targets: &mut Targets,
    ) -> Option<Element> {
        let mut wanted: Vec<(AutomationTarget, u32, Option<f32>)> = automated
            .iter()
            .filter_map(|&target| match target {
                AutomationTarget::Instrument { param } => Some((
                    target,
                    param,
                    instrument.synth().and_then(|s| s.param(param as usize)),
                )),
                _ => None,
            })
            .collect();
        wanted.sort_by_key(|(_, number, _)| *number);
        let parameters = self.parameters(wanted.into_iter(), targets);
        let (kind, name) = match instrument {
            Instrument::Audio => return None,
            Instrument::Plugin(setup) => {
                return Some(self.plugin_device(setup, "instrument", parameters, true));
            }
            Instrument::SoundFont { .. } => ("piano", "Piano"),
            Instrument::Drums { .. } => ("drums", "Drums"),
            Instrument::Synth(_) => ("synth", "Synth"),
            Instrument::Sfz { .. } => ("sfz", "SFZ instrument"),
        };
        let json = serde_json::to_vec_pretty(instrument).unwrap_or_default();
        let state = self.add_file("json", json);
        Some(self.own_device(
            "BuiltinDevice",
            kind,
            name,
            "instrument",
            true,
            parameters,
            &state,
            Vec::new(),
        ))
    }

    /// One of the program's own devices, with its exact settings in `state`, and
    /// whatever extra elements a standard device carries after them.
    #[allow(clippy::too_many_arguments, reason = "one element's attributes")]
    fn own_device(
        &mut self,
        tag: &str,
        kind: &str,
        name: &str,
        role: &str,
        enabled: bool,
        parameters: Element,
        state: &str,
        extra: Vec<Element>,
    ) -> Element {
        let id = self.id();
        let enabled_id = self.id();
        let mut device = Element::new(tag)
            .attr("deviceID", own_device_id(kind))
            .attr("deviceName", format!("{} {name}", daw_ident::PRODUCT_NAME))
            .attr("deviceRole", role)
            .attr("deviceVendor", daw_ident::PRODUCT_NAME)
            .attr("loaded", "true")
            .attr("id", id)
            .attr("name", name)
            .child(parameters)
            .child(
                Element::new("Enabled")
                    .attr("value", enabled)
                    .attr("id", enabled_id)
                    .attr("name", "On/Off"),
            )
            .child(Element::new("State").attr("path", state));
        for element in extra {
            device.push(element);
        }
        device
    }

    fn plugin_device(
        &mut self,
        setup: &PluginSetup,
        role: &str,
        parameters: Element,
        enabled: bool,
    ) -> Element {
        let plugin = &setup.plugin;
        let (tag, device_id, extension) = match plugin.format {
            PluginFormat::Clap => ("ClapPlugin", plugin.id.clone(), "clap-preset"),
            PluginFormat::Vst3 => (
                "Vst3Plugin",
                vst3_id_to_canonical(&plugin.id, cfg!(windows)),
                "vstpreset",
            ),
        };
        let state = setup.state.as_ref().and_then(|state| match plugin.format {
            PluginFormat::Clap => Some(state.bytes().to_vec()),
            PluginFormat::Vst3 => vstpreset(&device_id, state.bytes()),
        });
        let id = self.id();
        let enabled_id = self.id();
        let mut device = Element::new(tag)
            .attr("deviceID", &device_id)
            .attr("deviceName", &plugin.name)
            .attr("deviceRole", role)
            .attr("deviceVendor", &plugin.vendor)
            .attr("loaded", "true")
            .attr("id", id)
            .attr("name", &plugin.name)
            .child(parameters)
            .child(
                Element::new("Enabled")
                    .attr("value", enabled)
                    .attr("id", enabled_id)
                    .attr("name", "On/Off"),
            );
        if let Some(bytes) = state {
            let path = self.add_file(extension, bytes);
            device.push(Element::new("State").attr("path", path));
        }
        device
    }

    fn effect_device(
        &mut self,
        effect: &Effect,
        automated: &HashSet<AutomationTarget>,
        targets: &mut Targets,
    ) -> Element {
        let mut wanted: Vec<(AutomationTarget, u32, Option<f32>)> = automated
            .iter()
            .filter_map(|&target| match target {
                AutomationTarget::Effect { effect: id, param } if id == effect.id => {
                    Some((target, param, effect.kind.param(param as usize)))
                }
                _ => None,
            })
            .collect();
        wanted.sort_by_key(|(_, number, _)| *number);
        let parameters = self.parameters(wanted.into_iter(), targets);
        if let EffectKind::Plugin(setup) = &effect.kind {
            return self.plugin_device(setup, "audioFX", parameters, effect.enabled);
        }
        let json = serde_json::to_vec_pretty(&effect.kind).unwrap_or_default();
        let state = self.add_file("json", json);
        let (tag, kind, name, extra) = match &effect.kind {
            EffectKind::Eq(eq) => ("Equalizer", "eq", "EQ", self.eq_bands(eq)),
            EffectKind::Compressor(c) => (
                "Compressor",
                "compressor",
                "Compressor",
                vec![
                    self.real("Attack", "seconds", f64::from(c.attack_ms) / 1000.0),
                    self.boolean("AutoMakeup", false),
                    self.real("InputGain", "decibel", 0.0),
                    self.real("OutputGain", "decibel", f64::from(c.makeup_db)),
                    self.real("Ratio", "linear", f64::from(c.ratio)),
                    self.real("Release", "seconds", f64::from(c.release_ms) / 1000.0),
                    self.real("Threshold", "decibel", f64::from(c.threshold_db)),
                ],
            ),
            EffectKind::Limiter(l) => (
                "Limiter",
                "limiter",
                "Limiter",
                vec![
                    self.real("Attack", "seconds", 0.0015),
                    self.real("InputGain", "decibel", f64::from(l.gain_db)),
                    self.real("OutputGain", "decibel", 0.0),
                    self.real("Release", "seconds", f64::from(l.release_ms) / 1000.0),
                    self.real("Threshold", "decibel", f64::from(l.ceiling_db)),
                ],
            ),
            EffectKind::Reverb(_) => ("BuiltinDevice", "reverb", "Reverb", Vec::new()),
            EffectKind::Delay(_) => ("BuiltinDevice", "delay", "Delay", Vec::new()),
            EffectKind::Plugin(_) => unreachable!("plugins are handled above"),
        };
        self.own_device(
            tag,
            kind,
            name,
            "audioFX",
            effect.enabled,
            parameters,
            &state,
            extra,
        )
    }

    fn real(&mut self, tag: &str, unit: &str, value: f64) -> Element {
        Element::new(tag)
            .attr("unit", unit)
            .number("value", value)
            .attr("id", self.id())
    }

    fn boolean(&mut self, tag: &str, value: bool) -> Element {
        Element::new(tag).attr("value", value).attr("id", self.id())
    }

    /// The EQ as DAWproject's standard bands.
    fn eq_bands(&mut self, eq: &daw_model::EqSettings) -> Vec<Element> {
        let mut bands = Vec::new();
        let mut band =
            |writer: &mut Self, kind: &str, freq: f32, gain: Option<f32>, q: Option<f32>| {
                let order = bands.len();
                let mut element = Element::new("Band")
                    .attr("type", kind)
                    .attr("order", order)
                    .child(writer.real("Freq", "hertz", f64::from(freq)));
                if let Some(gain) = gain {
                    element.push(writer.real("Gain", "decibel", f64::from(gain)));
                }
                if let Some(q) = q {
                    element.push(writer.real("Q", "linear", f64::from(q)));
                }
                element.push(writer.boolean("Enabled", true));
                bands.push(element);
            };
        if let Some(hz) = eq.low_cut_hz {
            band(self, "highPass", hz, None, None);
        }
        band(self, "lowShelf", eq.low_hz, Some(eq.low_gain_db), None);
        band(
            self,
            "bell",
            eq.mid_hz,
            Some(eq.mid_gain_db),
            Some(eq.mid_q),
        );
        band(self, "highShelf", eq.high_hz, Some(eq.high_gain_db), None);
        if let Some(hz) = eq.high_cut_hz {
            band(self, "lowPass", hz, None, None);
        }
        bands
    }

    /// A track's timeline: its clips, then its automation lanes.
    fn track_lanes(
        &mut self,
        track: &Track,
        track_id: &str,
        targets: &Targets,
    ) -> Result<Element, DawProjectError> {
        let mut clips = Element::new("Clips").attr("id", self.id());
        for clip in &track.clips {
            let element = match clip.content {
                ClipContent::Pattern { pattern, .. } => {
                    let Some(pattern) = self.project.pattern(pattern) else {
                        continue;
                    };
                    let mut notes = Element::new("Notes").attr("id", self.id());
                    // The notes the clip plays, from its start (decision
                    // 0020: written out, not as a loop).
                    for note in pattern.played_in(clip) {
                        notes.push(
                            Element::new("Note")
                                .number("time", beats(note.start - clip.start))
                                .number("duration", beats(note.end - note.start))
                                .attr("channel", 0)
                                .attr("key", note.key)
                                .number("vel", f64::from(note.velocity.clamp(0.0, 1.0))),
                        );
                    }
                    Element::new("Clip")
                        .number("time", beats(clip.start))
                        .number("duration", beats(clip.length))
                        .attr("playStart", "0")
                        .attr("name", &pattern.name)
                        .child(notes)
                }
                ClipContent::Audio {
                    audio,
                    offset,
                    fade_in,
                    fade_out,
                    ..
                } => {
                    let Some(file) = self.project.audio_file(audio) else {
                        continue;
                    };
                    if !is_plain_name(&file.file) {
                        return Err(DawProjectError::MissingAudio(file.name.clone()));
                    }
                    let entry = format!("audio/{}", file.file);
                    if !self.audio.iter().any(|(e, _)| *e == entry) {
                        self.audio
                            .push((entry.clone(), self.audio_folder.join(&file.file)));
                    }
                    Element::new("Clip")
                        .number("time", beats(clip.start))
                        .number("duration", beats(clip.length))
                        .attr("contentTimeUnit", "seconds")
                        .number("playStart", file.seconds_in(offset))
                        .attr("fadeTimeUnit", "seconds")
                        .number("fadeInTime", f64::from(fade_in))
                        .number("fadeOutTime", f64::from(fade_out))
                        .attr("name", &file.name)
                        .child(
                            Element::new("Audio")
                                .attr("channels", file.channels)
                                .attr("sampleRate", file.sample_rate)
                                .number("duration", file.seconds())
                                .attr("id", self.id())
                                .child(Element::new("File").attr("path", entry)),
                        )
                }
            };
            clips.push(element);
        }
        let mut lanes = Element::new("Lanes")
            .attr("track", track_id)
            .attr("id", self.id())
            .child(clips);
        for lane in &track.automation {
            if let Some(points) = self.points(lane, targets) {
                lanes.push(points);
            }
        }
        Ok(lanes)
    }

    /// An automation lane as points of the parameter it changes.
    fn points(&mut self, lane: &AutomationLane, targets: &Targets) -> Option<Element> {
        if lane.points.is_empty() {
            return None;
        }
        let (target, unit) = match lane.target {
            AutomationTarget::Volume => (targets.volume.clone(), "linear"),
            AutomationTarget::Pan => (targets.pan.clone(), "normalized"),
            other => (targets.params.get(&other)?.clone(), "normalized"),
        };
        let mut points = Element::new("Points")
            .attr("id", self.id())
            .attr("unit", unit)
            .child(Element::new("Target").attr("parameter", target));
        for point in &lane.points {
            let value = match lane.target {
                AutomationTarget::Volume => volume_gain(point.value),
                _ => point.value,
            };
            points.push(
                Element::new("RealPoint")
                    .number("time", beats(point.at))
                    .number("value", f64::from(value))
                    .attr("interpolation", "linear"),
            );
        }
        Some(points)
    }

    /// The tempo changes: a sudden change holds, a glide is linear.
    fn tempo_automation(&mut self, tempo_id: &str) -> Element {
        let mut automation = Element::new("TempoAutomation")
            .attr("id", self.id())
            .attr("unit", "bpm")
            .child(Element::new("Target").attr("parameter", tempo_id));
        for change in self.project.tempo.changes() {
            automation.push(
                Element::new("RealPoint")
                    .number("time", beats(change.at))
                    .number("value", change.bpm)
                    .attr(
                        "interpolation",
                        if change.glide { "linear" } else { "hold" },
                    ),
            );
        }
        automation
    }

    fn signature_automation(&mut self, signature_id: &str) -> Element {
        let mut automation = Element::new("TimeSignatureAutomation")
            .attr("id", self.id())
            .child(Element::new("Target").attr("parameter", signature_id));
        for change in self.project.meter.changes() {
            automation.push(
                Element::new("TimeSignaturePoint")
                    .number("time", beats(self.project.meter.bar_start(change.bar)))
                    .attr("numerator", change.signature.numerator)
                    .attr("denominator", change.signature.denominator),
            );
        }
        automation
    }

    fn write_zip<W: Write + Seek>(
        &self,
        out: W,
        document: &str,
        metadata: &str,
    ) -> Result<W, DawProjectError> {
        let mut zip = zip::ZipWriter::new(out);
        let deflated =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("project.xml", deflated)?;
        zip.write_all(document.as_bytes())?;
        zip.start_file("metadata.xml", deflated)?;
        zip.write_all(metadata.as_bytes())?;
        for (name, bytes) in &self.files {
            zip.start_file(name, deflated)?;
            zip.write_all(bytes)?;
        }
        let stored = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        for (entry, source) in &self.audio {
            let name = entry.trim_start_matches("audio/").to_owned();
            let mut file = File::open(source).map_err(|_| {
                let shown = self
                    .project
                    .audio
                    .iter()
                    .find(|f| f.file == name)
                    .map_or(name.clone(), |f| f.name.clone());
                DawProjectError::MissingAudio(shown)
            })?;
            zip.start_file(entry, stored)?;
            io::copy(&mut file, &mut zip)?;
        }
        Ok(zip.finish()?)
    }
}

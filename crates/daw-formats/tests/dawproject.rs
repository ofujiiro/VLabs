// SPDX-License-Identifier: GPL-3.0-or-later

//! DAWproject (decision 0020): a song of this program goes out and comes back with
//! its tracks, mixer, instruments, effects, plugins and their settings,
//! notes, sound, automation, tempo and signatures; files written in the
//! style of other programs are read, with what the program cannot play named;
//! damaged files are refused.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::similar_names,
    reason = "test code"
)]

use std::io::Write;
use std::path::{Path, PathBuf};

use daw_formats::dawproject::{self, ImportNames, LeftOut};
use daw_model::{
    ACOUSTIC_KIT, AudioFile, AudioId, AutomationPoint, AutomationTarget, ChainPlace, ClipContent,
    CompressorSettings, DEFAULT_VELOCITY, EffectKind, EqSettings, Instrument, Note, NoteId,
    PluginFormat, PluginRef, PluginSetup, PluginState, Project, ReverbSettings, Swing, SynthPreset,
    Ticks, TimeSignature,
};

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dawproject-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn note(start: Ticks, length: Ticks, key: u8) -> Note {
    Note {
        id: NoteId(0),
        start,
        length,
        key,
        velocity: DEFAULT_VELOCITY,
        pitch_offset: 0.0,
    }
}

/// A tenth of a second of a 220 Hz tone, mono, 16-bit, at 44.1 kHz.
fn write_tone(path: &Path) {
    let mut writer = daw_formats::wav::WavWriter::create_with_channels(
        path,
        daw_formats::wav::WavFormat::Pcm16,
        44_100,
        1,
    )
    .unwrap();
    let samples: Vec<f32> = (0..44_100)
        .map(|i| (i as f32 * 220.0 * std::f32::consts::TAU / 44_100.0).sin() * 0.5)
        .collect();
    writer.write_interleaved(&samples).unwrap();
    writer.finish().unwrap();
}

fn names() -> ImportNames<'static> {
    ImportNames {
        track: &|n| format!("Track {n}"),
        pattern: &|n| format!("Pattern {n}"),
    }
}

fn plugin(format: PluginFormat, id: &str, name: &str, state: Vec<u8>) -> PluginSetup {
    PluginSetup {
        plugin: PluginRef {
            format,
            id: id.to_owned(),
            name: name.to_owned(),
            vendor: "Example".to_owned(),
            path: String::new(),
        },
        state: Some(PluginState::new(state)),
    }
}

/// the program's VST3 settings: two blocks, each with its length.
fn vst3_state() -> Vec<u8> {
    let mut state = Vec::new();
    for block in [&b"component state"[..], &b"controller"[..]] {
        state.extend_from_slice(&(block.len() as u32).to_le_bytes());
        state.extend_from_slice(block);
    }
    state
}

/// A song with every kind of track and setting the program has.
fn song(audio: &Path) -> Project {
    let mut project = Project::starter("Melody", "Piano");
    "Everything".clone_into(&mut project.title);
    let (piano, melody) = project.first_pattern().unwrap();
    let triplet = Ticks::note_value(1, 12);
    for (i, key) in [60_u8, 64, 67].iter().enumerate() {
        project.add_note(melody, note(triplet * i as i64, triplet, *key));
    }
    // The beat: one swung bar, repeated over four.
    let (drums, beat) = project.add_drums_with_pattern("Drums", "Beat", ACOUSTIC_KIT, 1, 4);
    for step in 0..8 {
        project.add_note(
            beat,
            note(Ticks::note_value(step, 8), Ticks::note_value(1, 16), 42),
        );
    }
    project.add_note(beat, note(Ticks::ZERO, Ticks::note_value(1, 16), 36));
    if let Some(pattern) = project.pattern_mut(beat) {
        pattern.swing = Swing {
            percent: 67,
            unit: Ticks::note_value(1, 8),
        };
    }
    if let Some(track) = project.track_mut(drums) {
        let mut kick = track.instrument.pad(36);
        kick.tune = -3.0;
        track.instrument.set_pad(kick);
        track.strip.gain = 0.7;
        track.strip.pan = -0.25;
        track.strip.solo = true;
    }
    project.add_effect(
        ChainPlace::Track(drums),
        EffectKind::Compressor(CompressorSettings {
            threshold_db: -24.0,
            ratio: 4.0,
            ..CompressorSettings::default()
        }),
    );
    // The synth, its cutoff moving.
    let mut lead = SynthPreset::Lead.settings();
    lead.cutoff_hz = 1500.0;
    let synth = project.add_track("Lead", Instrument::Synth(lead));
    let line = project.add_pattern_for(synth, "Line", Ticks::WHOLE);
    project.add_note(line, note(Ticks::ZERO, Ticks::QUARTER, 72));
    project.add_clip(synth, Ticks::WHOLE, Ticks::WHOLE, line);
    project.add_clip(synth, Ticks::WHOLE * 3, Ticks::WHOLE, line);
    let cutoff = AutomationTarget::Instrument { param: 0 };
    project.add_lane(synth, cutoff);
    project.update_lane(synth, cutoff, |lane| {
        lane.insert(AutomationPoint {
            at: Ticks::ZERO,
            value: 0.25,
        });
        lane.insert(AutomationPoint {
            at: Ticks::WHOLE * 2,
            value: 0.75,
        });
    });
    // Plugins, with settings, an effect lane on one.
    let clap = project.add_track(
        "Surge",
        Instrument::Plugin(plugin(
            PluginFormat::Clap,
            "org.surge-synth-team.surge-xt",
            "Surge XT",
            (0..=255).collect(),
        )),
    );
    let vst = project
        .add_effect(
            ChainPlace::Track(clap),
            EffectKind::Plugin(plugin(
                PluginFormat::Vst3,
                "0123456789ABCDEF0123456789ABCDEF",
                "Space Echo",
                vst3_state(),
            )),
        )
        .unwrap();
    let echo = AutomationTarget::Effect {
        effect: vst,
        param: 12,
    };
    project.add_lane(clap, echo);
    project.update_lane(clap, echo, |lane| {
        lane.insert(AutomationPoint {
            at: Ticks::QUARTER,
            value: 0.5,
        });
    });
    let reverb = project
        .add_effect(
            ChainPlace::Track(piano),
            EffectKind::Reverb(ReverbSettings {
                mix: 0.4,
                ..ReverbSettings::default()
            }),
        )
        .unwrap();
    project.update_effect(reverb, |e| e.enabled = false);
    let amount = AutomationTarget::Effect {
        effect: reverb,
        param: 0,
    };
    project.add_lane(piano, amount);
    project.update_lane(piano, amount, |lane| {
        lane.insert(AutomationPoint {
            at: Ticks::WHOLE,
            value: 0.1,
        });
    });
    // Volume and pan lanes.
    for (target, value) in [
        (AutomationTarget::Volume, 0.6),
        (AutomationTarget::Pan, 0.3),
    ] {
        project.add_lane(piano, target);
        project.update_lane(piano, target, |lane| {
            lane.insert(AutomationPoint {
                at: Ticks::QUARTER * 2,
                value,
            });
        });
    }
    // Sound.
    write_tone(&audio.join("tone.wav"));
    let voice = project.add_audio_track("Voice");
    let tone = project.add_audio_file(AudioFile {
        id: AudioId(0),
        name: "Tone".to_owned(),
        file: "tone.wav".to_owned(),
        frames: 44_100,
        sample_rate: 44_100,
        channels: 1,
    });
    let clip = project
        .add_audio_clip(voice, Ticks::WHOLE * 2, Ticks::QUARTER, tone, 4410)
        .unwrap();
    project.update_clip(clip, |c| {
        if let ClipContent::Audio {
            fade_in, fade_out, ..
        } = &mut c.content
        {
            *fade_in = 0.01;
            *fade_out = 0.05;
        }
    });
    // An SFZ instrument of the person's own (decision 0024).
    let strings = project.add_track(
        "Strings",
        Instrument::Sfz {
            file: "/instruments/Strings/Strings.sfz".to_owned(),
        },
    );
    let chords = project.add_pattern_for(strings, "Chords", Ticks::WHOLE);
    project.add_note(chords, note(Ticks::ZERO, Ticks::QUARTER * 2, 60));
    project.add_clip(strings, Ticks::ZERO, Ticks::WHOLE, chords);
    // The master: an EQ and the limiter.
    project.add_effect(
        ChainPlace::Master,
        EffectKind::Eq(EqSettings {
            low_cut_hz: Some(30.0),
            mid_gain_db: -2.5,
            ..EqSettings::default()
        }),
    );
    project.master.gain = 0.9;
    // Tempo and signatures.
    project.tempo.set(Ticks::WHOLE * 2, 132.0);
    project.tempo.set_glide(Ticks::ZERO, true);
    project.tempo.set(Ticks::WHOLE * 4, 90.0);
    project.meter.set(4, TimeSignature::new(5, 4).unwrap());
    project
}

/// The notes a track plays, as (start, end, key, velocity bits).
fn played(project: &Project, name: &str) -> Vec<(Ticks, Ticks, u8, u32)> {
    let track = project.tracks.iter().find(|t| t.name == name).unwrap();
    let mut notes: Vec<_> = track
        .clips
        .iter()
        .filter_map(|clip| Some((clip, project.pattern(clip.pattern()?)?)))
        .flat_map(|(clip, pattern)| pattern.played_in(clip))
        .map(|n| (n.start, n.end, n.key, n.velocity.to_bits()))
        .collect();
    notes.sort_unstable();
    notes
}

#[test]
fn a_song_goes_to_another_program_and_comes_back_whole() {
    let folder = scratch("round-trip");
    let sounds = folder.join("sounds");
    std::fs::create_dir_all(&sounds).unwrap();
    let original = song(&sounds);
    let file = folder.join("song.dawproject");
    dawproject::export(&original, &sounds, &file).unwrap();

    // The container holds what DAWproject asks for.
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
    let entries: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .map(std::borrow::Cow::into_owned)
        .collect();
    for wanted in ["project.xml", "metadata.xml", "audio/tone.wav"] {
        assert!(
            entries.iter().any(|e| e == wanted),
            "{wanted} in {entries:?}"
        );
    }
    assert!(entries.iter().any(|e| e.ends_with(".clap-preset")));
    assert!(entries.iter().any(|e| e.ends_with(".vstpreset")));
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("project.xml").unwrap(), &mut xml).unwrap();
    assert!(xml.contains("<Project version=\"1.0\">"));
    assert!(xml.contains("deviceID=\"org.surge-synth-team.surge-xt\""));
    assert!(xml.contains("<Equalizer"), "the EQ as DAWproject's own");
    assert!(xml.contains("<Compressor"));
    assert!(xml.contains("interpolation=\"linear\""), "the glide");

    let opened = folder.join("opened");
    let imported = dawproject::import(&file, &opened, &names()).unwrap();
    assert!(imported.left_out.is_empty(), "{:?}", imported.left_out);
    let back = imported.project;
    assert_eq!(back.title, "Everything");
    assert_eq!(back.tracks.len(), original.tracks.len());
    for (a, b) in original.tracks.iter().zip(&back.tracks) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.instrument, b.instrument, "{}", a.name);
        assert_eq!(a.strip, b.strip, "{}", a.name);
        let kinds = |t: &daw_model::Track| -> Vec<(EffectKind, bool)> {
            t.effects
                .iter()
                .map(|e| (e.kind.clone(), e.enabled))
                .collect()
        };
        assert_eq!(kinds(a), kinds(b), "{}", a.name);
        if !a.instrument.is_audio() {
            assert_eq!(
                played(&original, &a.name),
                played(&back, &a.name),
                "{}",
                a.name
            );
        }
    }
    // The written-out beat is one bar again, repeated by its clip.
    let drums = back.tracks.iter().find(|t| t.name == "Drums").unwrap();
    assert_eq!(drums.clips.len(), 1);
    let beat = back.pattern(drums.clips[0].pattern().unwrap()).unwrap();
    assert_eq!(beat.length, Ticks::WHOLE, "one bar");
    assert_eq!(drums.clips[0].length, Ticks::WHOLE * 4, "played four times");
    // The synth's two clips share their pattern.
    let lead = back.tracks.iter().find(|t| t.name == "Lead").unwrap();
    assert_eq!(lead.clips[0].pattern(), lead.clips[1].pattern());
    // Automation.
    for track in &original.tracks {
        let other = back.tracks.iter().find(|t| t.name == track.name).unwrap();
        assert_eq!(
            track.automation.len(),
            other.automation.len(),
            "{}",
            track.name
        );
        for lane in &track.automation {
            let target = match lane.target {
                AutomationTarget::Effect { param, .. } => {
                    let index = track
                        .effects
                        .iter()
                        .position(|e| matches!(lane.target, AutomationTarget::Effect { effect, .. } if effect == e.id))
                        .unwrap();
                    AutomationTarget::Effect {
                        effect: other.effects[index].id,
                        param,
                    }
                }
                target => target,
            };
            let came = other
                .automation
                .iter()
                .find(|l| l.target == target)
                .unwrap();
            assert_eq!(lane.points.len(), came.points.len());
            for (p, q) in lane.points.iter().zip(&came.points) {
                assert_eq!(p.at, q.at);
                assert!(
                    (p.value - q.value).abs() < 1e-4,
                    "{} {:?}",
                    track.name,
                    lane.target
                );
            }
        }
    }
    // Sound.
    let voice = back.tracks.iter().find(|t| t.name == "Voice").unwrap();
    let clip = voice.clips[0];
    assert_eq!(
        (clip.start, clip.length),
        (Ticks::WHOLE * 2, Ticks::QUARTER)
    );
    let ClipContent::Audio {
        offset,
        fade_in,
        fade_out,
        ..
    } = clip.content
    else {
        panic!("an audio clip");
    };
    assert_eq!(offset, 4410);
    assert!((fade_in - 0.01).abs() < 1e-6 && (fade_out - 0.05).abs() < 1e-6);
    let tone = back.audio_file(clip.audio().unwrap()).unwrap();
    assert_eq!(
        (tone.frames, tone.sample_rate, tone.channels),
        (44_100, 44_100, 1)
    );
    assert_eq!(
        std::fs::read(opened.join(&tone.file)).unwrap(),
        std::fs::read(sounds.join("tone.wav")).unwrap()
    );
    // The master, the tempo and the signatures.
    assert_eq!(back.master.gain, 0.9);
    assert_eq!(back.master.effects.len(), original.master.effects.len());
    for (a, b) in original.master.effects.iter().zip(&back.master.effects) {
        assert_eq!(a.kind, b.kind);
    }
    assert_eq!(back.tempo, original.tempo, "tempo changes and the glide");
    assert_eq!(back.meter, original.meter);
    std::fs::remove_dir_all(folder).ok();
}

/// A DAWproject container holding `xml` (and a short WAV as
/// `audio/loop.wav`).
fn container(xml: &str, folder: &Path) -> PathBuf {
    let path = folder.join("other.dawproject");
    let tone = folder.join("loop.wav");
    write_tone(&tone);
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("project.xml", options).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    zip.start_file("audio/loop.wav", options).unwrap();
    zip.write_all(&std::fs::read(&tone).unwrap()).unwrap();
    zip.start_file("plugins/surge.clap-preset", options)
        .unwrap();
    zip.write_all(b"surge settings").unwrap();
    zip.finish().unwrap();
    path
}

/// A project in the style of the format's own example (Bitwig): a CLAP
/// instrument, a stretched audio loop inside nested clips, a looped note
/// clip, a send, a noise gate, a VST2 effect, tempo automation.
const FOREIGN: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Project version="1.0">
  <Application name="Another Studio" version="5.0"/>
  <Transport>
    <Tempo max="666.000000" min="20.000000" unit="bpm" value="149.000000" id="id0" name="Tempo"/>
    <TimeSignature denominator="4" numerator="4" id="id1"/>
  </Transport>
  <Structure>
    <Track contentType="notes" loaded="true" id="id2" name="Bass" color="#a2eabf">
      <Channel audioChannels="2" destination="id15" role="regular" solo="false" id="id3">
        <Devices>
          <ClapPlugin deviceID="org.surge-synth-team.surge-xt" deviceName="Surge XT" deviceRole="instrument" loaded="true" id="id7" name="Surge XT">
            <Parameters><RealParameter parameterID="42" id="id70" unit="normalized" min="0" max="1"/></Parameters>
            <Enabled value="true" id="id8" name="On/Off"/>
            <State path="plugins/surge.clap-preset"/>
          </ClapPlugin>
          <NoiseGate deviceName="Gate" deviceRole="audioFX" id="id71"/>
          <Vst2Plugin deviceID="1234" deviceName="Old Chorus" deviceRole="audioFX" id="id72"/>
          <Compressor deviceName="Comp" deviceRole="audioFX" id="id73">
            <Enabled value="false" id="id74"/>
            <Attack unit="seconds" value="0.005" id="id75"/>
            <Ratio unit="linear" value="6" id="id76"/>
            <Release unit="seconds" value="0.2" id="id77"/>
            <Threshold unit="decibel" value="-30" id="id78"/>
          </Compressor>
        </Devices>
        <Mute value="true" id="id6" name="Mute"/>
        <Pan max="1.000000" min="0.000000" unit="normalized" value="0.250000" id="id5" name="Pan"/>
        <Sends><Send destination="id15" id="id79"><Volume unit="linear" value="0.5" id="id80"/></Send></Sends>
        <Volume max="2.000000" min="0.000000" unit="linear" value="0.659140" id="id4" name="Volume"/>
      </Channel>
    </Track>
    <Track contentType="audio" loaded="true" id="id9" name="Drumloop" color="#b53bba">
      <Channel audioChannels="2" destination="id15" role="regular" solo="false" id="id10">
        <Mute value="false" id="id13" name="Mute"/>
        <Pan max="1.000000" min="0.000000" unit="normalized" value="0.500000" id="id12" name="Pan"/>
        <Volume max="2.000000" min="0.000000" unit="linear" value="0.177125" id="id11" name="Volume"/>
      </Channel>
    </Track>
    <Track contentType="audio notes" loaded="true" id="id14" name="Master">
      <Channel audioChannels="2" role="master" solo="false" id="id15">
        <Mute value="false" id="id18" name="Mute"/>
        <Pan max="1.000000" min="0.000000" unit="normalized" value="0.500000" id="id17" name="Pan"/>
        <Volume max="2.000000" min="0.000000" unit="linear" value="1.000000" id="id16" name="Volume"/>
      </Channel>
    </Track>
  </Structure>
  <Arrangement id="id19">
    <Lanes timeUnit="beats" id="id20">
      <Lanes track="id2" id="id21">
        <Clips id="id22">
          <Clip time="0.0" duration="8.0" playStart="0.0" loopStart="0.0" loopEnd="2.0" name="Riff">
            <Notes id="id23">
              <Note time="0.000000" duration="0.250000" channel="0" key="65" vel="0.787402" rel="0.787402"/>
              <Note time="1.000000" duration="0.500000" channel="0" key="60" vel="0.5"/>
            </Notes>
          </Clip>
        </Clips>
        <Points id="id90" unit="linear">
          <Target parameter="id4"/>
          <RealPoint time="0" value="1.0" interpolation="hold"/>
          <RealPoint time="4" value="0.5" interpolation="linear"/>
        </Points>
        <Points id="id91" unit="normalized">
          <Target parameter="id70"/>
          <RealPoint time="2" value="0.3" interpolation="linear"/>
        </Points>
      </Lanes>
      <Lanes track="id9" id="id24">
        <Clips id="id25">
          <Clip time="0.0" duration="8.00003433227539" playStart="0.0" loopStart="0.0" loopEnd="8.00003433227539" fadeTimeUnit="beats" fadeInTime="0.0" fadeOutTime="0.0" name="Drumfunk3 170bpm">
            <Clips id="id26">
              <Clip time="0.0" duration="8.00003433227539" contentTimeUnit="beats" playStart="0.0" fadeTimeUnit="beats" fadeInTime="0.0" fadeOutTime="0.0">
                <Warps contentTimeUnit="seconds" timeUnit="beats" id="id28">
                  <Audio algorithm="stretch" channels="1" duration="1.0" sampleRate="44100" id="id27">
                    <File path="audio/loop.wav"/>
                  </Audio>
                  <Warp time="0.0" contentTime="0.0"/>
                  <Warp time="8.00003433227539" contentTime="2.823541666666667"/>
                </Warps>
              </Clip>
            </Clips>
          </Clip>
          <Clip time="16" duration="4" contentTimeUnit="seconds" playStart="0.5">
            <Audio channels="2" sampleRate="48000" duration="3.0">
              <File path="audio/missing.wav"/>
            </Audio>
          </Clip>
        </Clips>
      </Lanes>
    </Lanes>
    <TempoAutomation id="id40" unit="bpm">
      <Target parameter="id0"/>
      <RealPoint time="0" value="149" interpolation="hold"/>
      <RealPoint time="8" value="120" interpolation="linear"/>
      <RealPoint time="16" value="100" interpolation="hold"/>
    </TempoAutomation>
    <TimeSignatureAutomation id="id41">
      <Target parameter="id1"/>
      <TimeSignaturePoint time="0" numerator="4" denominator="4"/>
      <TimeSignaturePoint time="8.5" numerator="6" denominator="8"/>
    </TimeSignatureAutomation>
  </Arrangement>
  <Scenes/>
</Project>"##;

#[test]
fn another_programs_project_is_read_with_what_was_left_out() {
    let folder = scratch("foreign");
    let file = container(FOREIGN, &folder);
    let opened = folder.join("opened");
    let imported = dawproject::import(&file, &opened, &names()).unwrap();
    let left = &imported.left_out;
    assert!(
        left.contains(&LeftOut::Device("Gate".to_owned())),
        "{left:?}"
    );
    assert!(left.contains(&LeftOut::Device("Old Chorus".to_owned())));
    assert!(left.contains(&LeftOut::Routing), "the send");
    assert!(left.contains(&LeftOut::StretchedAudio));
    assert!(left.contains(&LeftOut::MissingAudio("missing.wav".to_owned())));
    let project = imported.project;
    assert_eq!(project.tracks.len(), 2);

    let bass = &project.tracks[0];
    assert_eq!(bass.name, "Bass");
    let setup = bass.instrument.plugin().unwrap();
    assert_eq!(setup.plugin.id, "org.surge-synth-team.surge-xt");
    assert_eq!(setup.plugin.format, PluginFormat::Clap);
    assert_eq!(setup.state.as_ref().unwrap().bytes(), b"surge settings");
    assert!(bass.strip.mute);
    assert!((bass.strip.pan + 0.5).abs() < 1e-6);
    assert!((bass.strip.gain - 0.659_14).abs() < 1e-5);
    // The compressor comes in (switched off), the gate and the VST2 do not.
    assert_eq!(bass.effects.len(), 1);
    let EffectKind::Compressor(c) = &bass.effects[0].kind else {
        panic!("a compressor");
    };
    assert!(!bass.effects[0].enabled);
    assert_eq!((c.ratio, c.threshold_db), (6.0, -30.0));
    assert!((c.attack_ms - 5.0).abs() < 1e-3 && (c.release_ms - 200.0).abs() < 1e-3);
    // The looped two-beat riff plays over eight beats: one pattern of the
    // loop's notes... written as eight beats of notes.
    let mut starts: Vec<(Ticks, u8)> = bass
        .clips
        .iter()
        .filter_map(|clip| Some((clip, project.pattern(clip.pattern()?)?)))
        .flat_map(|(clip, p)| p.played_in(clip))
        .map(|n| (n.start, n.key))
        .collect();
    starts.sort_unstable();
    let expected: Vec<(Ticks, u8)> = (0..4)
        .flat_map(|i| {
            [
                (Ticks::QUARTER * (2 * i), 65),
                (Ticks::QUARTER * (2 * i + 1), 60),
            ]
        })
        .collect();
    assert_eq!(starts, expected);
    // Its lanes: the volume (held, then a ramp) and the plugin parameter.
    let volume = project.lane(bass.id, AutomationTarget::Volume).unwrap();
    assert_eq!(volume.points.len(), 3, "a step before the ramp");
    let param = project
        .lane(bass.id, AutomationTarget::Instrument { param: 42 })
        .unwrap();
    assert!((param.points[0].value - 0.3).abs() < 1e-6);

    let drums = &project.tracks[1];
    assert!(drums.instrument.is_audio());
    assert_eq!(drums.clips.len(), 1, "the missing file's clip is left out");
    let file = project.audio_file(drums.clips[0].audio().unwrap()).unwrap();
    assert_eq!(file.name, "loop");
    assert!(opened.join(&file.file).is_file());

    // Tempo: held, then gliding to 100 from beat 8; 6/8 from bar 3.
    assert_eq!(project.tempo.changes().len(), 3);
    assert!(!project.tempo.changes()[0].glide);
    assert!(project.tempo.changes()[1].glide);
    assert_eq!(
        project.meter.signature_at_bar(2),
        TimeSignature::new(6, 8).unwrap()
    );
    std::fs::remove_dir_all(folder).ok();
}

#[test]
fn damaged_projects_are_refused() {
    let folder = scratch("damaged");
    let opened = folder.join("opened");
    let not_zip = folder.join("not.dawproject");
    std::fs::write(&not_zip, b"hello").unwrap();
    assert!(dawproject::import(&not_zip, &opened, &names()).is_err());
    for xml in [
        "",
        "<Song/>",
        "<Project><Structure>",
        "<Project version=\"1.0\"><Arrangement><Lanes><Clips><Clip time=\"nan\"/></Clips></Lanes></Arrangement></Project>",
    ] {
        let file = container(xml, &folder);
        let result = dawproject::import(&file, &opened, &names());
        if let Ok(imported) = result {
            assert!(imported.project.tracks.is_empty(), "{xml}");
        }
    }
    std::fs::remove_dir_all(folder).ok();
}

/// The schema of DAWproject 1.0 (from github.com/bitwig/dawproject, MIT),
/// kept in `tests/data/dawproject`.
fn schema(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/dawproject")
        .join(name)
}

#[test]
fn exported_projects_match_the_dawproject_schema() {
    // Needs xmllint (libxml2); the test passes quietly without it.
    let Ok(probe) = std::process::Command::new("xmllint")
        .arg("--version")
        .output()
    else {
        return;
    };
    if !probe.status.success() {
        return;
    }
    let folder = scratch("schema");
    let sounds = folder.join("sounds");
    std::fs::create_dir_all(&sounds).unwrap();
    let file = folder.join("song.dawproject");
    dawproject::export(&song(&sounds), &sounds, &file).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
    for (entry, xsd) in [
        ("project.xml", "Project.xsd"),
        ("metadata.xml", "MetaData.xsd"),
    ] {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut zip.by_name(entry).unwrap(), &mut text).unwrap();
        let written = folder.join(entry);
        std::fs::write(&written, text).unwrap();
        let output = std::process::Command::new("xmllint")
            .arg("--noout")
            .arg("--schema")
            .arg(schema(xsd))
            .arg(&written)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{entry} does not match {xsd}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(folder).ok();
}

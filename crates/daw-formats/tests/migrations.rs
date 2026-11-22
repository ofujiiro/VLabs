// SPDX-License-Identifier: GPL-3.0-or-later

//! Every version of the project format ever released keeps sample files in
//! `tests/data`, and this test proves the current code still opens them.
//! When the format changes: bump `FORMAT_VERSION`, add a migration step, and
//! add a sample file for the new version (run this test with
//! `WRITE_FORMAT_SAMPLE=1` to write one from the current code).

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test code"
)]

use std::path::PathBuf;

use daw_formats::{FORMAT_VERSION, load_project, save_project};
use daw_model::expression::{NoteExpression, PitchPoint, PitchShape, Vibrato};
use daw_model::{
    ACOUSTIC_KIT, AudioFile, AudioId, AutomationLane, AutomationPoint, AutomationTarget,
    ChainPlace, ClipContent, DEFAULT_VELOCITY, EffectKind, EqSettings, Instrument, Note, NoteId,
    NoteShape, PluginFormat, PluginRef, PluginSetup, PluginState, Project, Scale, ScaleKind, Swing,
    SynthPreset, SynthSettings, SynthWave, Ticks, TimeSignature,
};

fn data_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
}

/// The SFZ instrument of the version 7 sample.
const SAMPLE_SFZ: &str = "/instruments/Strings/Strings.sfz";

/// The synth of the version 6 sample: the Lead preset, changed.
fn sample_synth() -> SynthSettings {
    let mut settings = SynthPreset::Lead.settings();
    settings.cutoff_hz = 1234.0;
    settings.osc2.wave = SynthWave::Triangle;
    settings.osc2.semitones = 7;
    settings
}

/// The plugin of the version 3 sample, with settings made of these bytes.
fn sample_plugin() -> PluginSetup {
    PluginSetup {
        plugin: PluginRef {
            format: PluginFormat::Clap,
            id: "org.example.synth".to_owned(),
            name: "Example Synth".to_owned(),
            vendor: "Example".to_owned(),
            path: "/plugins/Example Synth.clap".to_owned(),
        },
        state: Some(PluginState::new((0..=255).collect())),
    }
}

/// A folder of this test's own.
fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("migrations-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// The sound of the version 4 sample's audio clip: a tenth of a second of
/// mono 16-bit WAV at 48 kHz.
fn write_hum(folder: &std::path::Path) {
    let mut writer = daw_formats::wav::WavWriter::create_with_channels(
        &folder.join("hum.wav"),
        daw_formats::wav::WavFormat::Pcm16,
        48_000,
        1,
    )
    .unwrap();
    let samples: Vec<f32> = (0..4800)
        .map(|i| (f64::from(i) * 0.05).sin() as f32 * 0.5)
        .collect();
    writer.write_interleaved(&samples).unwrap();
    writer.finish().unwrap();
}

/// The project every sample file contains (version 2 added the drum
/// track, the pad setting and the effects; version 3 a plugin track with
/// saved settings, a plugin effect and automation lanes; version 4 an audio
/// track with a clip of a file kept in the container; version 5 a key with
/// a change, a swung beat, a tempo glide and a time-signature change;
/// version 6 a track of the built-in synth with a lane for its cutoff;
/// version 7 a track playing an SFZ instrument, and a custom kit whose pad
/// plays the version 4 sample's sound; version 8 a bent tempo glide;
/// version 9 a singing voice whose notes have lyrics; version 10 how those
/// notes are sung: a bent pitch with vibrato, a colour, a breath; version
/// 11 the piano's notes cut at their end, and a second audio clip played
/// backwards, a fifth up).
fn reference_project() -> Project {
    let mut project = Project::starter("Pattern 1", "Piano");
    let (_, pattern) = project.first_pattern().unwrap();
    let (drums, beat) = project.add_drums_with_pattern("Drums", "Beat 1", ACOUSTIC_KIT, 1, 4);
    project.add_note(
        beat,
        Note {
            id: NoteId(0),
            start: Ticks::ZERO,
            length: Ticks::note_value(1, 16),
            key: 36,
            velocity: 1.0,
            pitch_offset: 0.0,
        },
    );
    if let Some(track) = project.track_mut(drums) {
        let mut kick = track.instrument.pad(36);
        kick.tune = -2.0;
        track.instrument.set_pad(kick);
    }
    project.add_effect(
        ChainPlace::Track(drums),
        EffectKind::Eq(EqSettings {
            low_cut_hz: Some(40.0),
            ..EqSettings::default()
        }),
    );
    let synth = project.add_track("Synth", Instrument::Plugin(sample_plugin()));
    project.add_effect(
        ChainPlace::Track(synth),
        EffectKind::Plugin(sample_plugin()),
    );
    if let Some(track) = project.track_mut(synth) {
        let mut lane = AutomationLane::new(AutomationTarget::Instrument { param: 7 });
        lane.insert(AutomationPoint {
            at: Ticks::ZERO,
            value: 0.25,
        });
        lane.insert(AutomationPoint {
            at: Ticks::WHOLE,
            value: 0.75,
        });
        track.automation.push(lane);
    }
    let voice = project.add_audio_track("Voice");
    let hum = project.add_audio_file(AudioFile {
        id: AudioId(0),
        name: "Hum".to_owned(),
        file: "hum.wav".to_owned(),
        frames: 4800,
        sample_rate: 48_000,
        channels: 1,
    });
    let clip = project
        .add_audio_clip(voice, Ticks::WHOLE, Ticks::QUARTER / 10, hum, 1200)
        .unwrap();
    project.update_clip(clip, |c| {
        if let ClipContent::Audio { gain, fade_in, .. } = &mut c.content {
            *gain = 0.5;
            *fade_in = 0.01;
        }
    });
    "Format sample".clone_into(&mut project.title);
    project.tempo.set(Ticks::quarters(8), 90.0);
    project.tempo.set(Ticks::quarters(16), 100.0);
    project.tempo.set_glide(Ticks::quarters(8), true);
    // Version 8: a ritardando that gathers (decision 0026).
    project.tempo.set(Ticks::quarters(24), 70.0);
    project.tempo.set_glide(Ticks::quarters(16), true);
    project.tempo.set_curve(Ticks::quarters(16), 0.5);
    project.key.set(0, Some(Scale::new(9, ScaleKind::Minor)));
    project.key.set(4, Some(Scale::new(0, ScaleKind::Major)));
    project.meter.set(8, TimeSignature::new(7, 8).unwrap());
    if let Some(beat) = project.pattern_mut(beat) {
        beat.swing = Swing {
            percent: 67,
            unit: Ticks::note_value(1, 8),
        };
    }
    add_newer_instruments(&mut project);
    add_voice(&mut project);
    add_shapes(&mut project, voice, hum);
    let triplet = Ticks::note_value(1, 12);
    for (i, key) in [60_u8, 64, 67, 72].iter().enumerate() {
        project.add_note(
            pattern,
            Note {
                id: NoteId(0),
                start: triplet * i as i64,
                length: triplet,
                key: *key,
                velocity: DEFAULT_VELOCITY,
                pitch_offset: 0.0,
            },
        );
    }
    project
}

/// The version 6 sample's synth track (with a lane for its cutoff) and the
/// version 7 sample's SFZ track.
fn add_newer_instruments(project: &mut Project) {
    let lead = project.add_track("Lead", Instrument::Synth(sample_synth()));
    if let Some(track) = project.track_mut(lead) {
        let mut lane = AutomationLane::new(AutomationTarget::Instrument { param: 0 });
        lane.insert(AutomationPoint {
            at: Ticks::ZERO,
            value: 0.2,
        });
        track.automation.push(lane);
    }
    project.add_track(
        "Strings",
        Instrument::Sfz {
            file: SAMPLE_SFZ.to_owned(),
        },
    );
    let kit = project.add_track("My kit", Instrument::Kit { pads: Vec::new() });
    let hum = project.audio[0].id;
    project.add_kit_pad(kit, "Hum", hum);
}

#[test]
fn every_released_format_version_still_opens() {
    let sample = data_folder().join(format!("format-v{FORMAT_VERSION}.project-sample"));
    if std::env::var_os("WRITE_FORMAT_SAMPLE").is_some() && !sample.exists() {
        let sounds = scratch("write");
        write_hum(&sounds);
        save_project(&reference_project(), &sample, Some(&sounds)).unwrap();
        std::fs::remove_dir_all(sounds).ok();
    }
    let opened = scratch("open");
    let mut found = 0;
    for entry in std::fs::read_dir(data_folder()).unwrap().flatten() {
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".project-sample") {
            continue;
        }
        found += 1;
        let version: u32 = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("format-v"))
            .and_then(|n| n.split('.').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let project = load_project(&path, Some(&opened))
            .unwrap_or_else(|e| panic!("{} no longer opens: {e}", path.display()));
        for pattern in &project.patterns {
            assert!(
                pattern.track.is_some(),
                "every pattern has its track: {}",
                path.display()
            );
        }
        check_mixing_and_plugins(&project, version, &path);
        if version >= 4 {
            check_audio(&project, &opened, &path);
        }
        if version >= 6 {
            let lead = project
                .tracks
                .iter()
                .find(|t| t.instrument.synth().is_some())
                .unwrap_or_else(|| panic!("{} lost its synth track", path.display()));
            assert_eq!(lead.instrument.synth(), Some(&sample_synth()));
            assert_eq!(
                lead.automation[0].target,
                AutomationTarget::Instrument { param: 0 }
            );
        }
        check_newer_instruments(&project, version, &path);
        check_bent_glide(&project, version, &path);
        check_voice(&project, version, &path);
        check_shapes(&project, version, &path);
        if version >= 5 {
            check_theory_and_rhythm(&project, &path);
        } else {
            assert!(!project.key.has_any(), "no key before version 5");
            assert!(project.patterns.iter().all(|p| p.swing.is_straight()));
        }
        assert_eq!(project.title, "Format sample", "{}", path.display());
        assert_eq!(
            project.tempo.bpm_at(Ticks::quarters(8)),
            90.0,
            "{}",
            path.display()
        );
        let piano = project
            .tracks
            .iter()
            .find(|t| !t.instrument.is_drums())
            .unwrap();
        let pattern = piano.clips[0].pattern().unwrap();
        let keys: Vec<u8> = project
            .pattern(pattern)
            .unwrap()
            .notes
            .iter()
            .map(|n| n.key)
            .collect();
        assert_eq!(keys, [60, 64, 67, 72], "{}", path.display());
        assert_eq!(
            project.pattern(pattern).unwrap().notes[1].start,
            Ticks::note_value(1, 12),
            "triplets stay exact"
        );
    }
    std::fs::remove_dir_all(opened).ok();
    assert!(
        found >= FORMAT_VERSION as usize,
        "a sample file is missing for some format version (found {found})"
    );
}

/// The version 11 sample's shapes (decision 0031): the piano's notes cut
/// at their end with a soft attack, and a second clip of the hum, played
/// backwards a fifth up.
fn add_shapes(project: &mut Project, voice: daw_model::TrackId, hum: AudioId) {
    let piano = project.tracks[0].id;
    if let Some(track) = project.track_mut(piano) {
        track.shape = NoteShape {
            attack: Some(0.05),
            release: Some(NoteShape::CUT_RELEASE),
        };
    }
    let clip = project
        .add_audio_clip(voice, Ticks::WHOLE * 2, Ticks::QUARTER / 20, hum, 1200)
        .unwrap();
    project.set_clip_reverse(clip, true);
    project.set_clip_pitch(clip, 7.0);
}

/// The version 11 sample's shapes came back (older samples have none).
fn check_shapes(project: &Project, version: u32, path: &std::path::Path) {
    let name = path.display();
    let piano = &project.tracks[0];
    let backwards: Vec<(bool, f32)> = project
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .filter_map(|c| match c.content {
            ClipContent::Audio { reverse, pitch, .. } => Some((reverse, pitch)),
            ClipContent::Pattern { .. } => None,
        })
        .filter(|(reverse, pitch)| *reverse || *pitch != 0.0)
        .collect();
    if version < 11 {
        assert!(
            piano.shape.is_natural(),
            "{name}: natural before version 11"
        );
        assert!(backwards.is_empty(), "{name}: forwards before version 11");
        return;
    }
    assert!(piano.shape.is_cut(), "{name}");
    assert_eq!(piano.shape.attack, Some(0.05), "{name}");
    assert_eq!(backwards, [(true, 7.0)], "{name}");
}

/// The version 9 sample's voice: a track singing "ka sa ne".
fn add_voice(project: &mut Project) {
    let voice = project.add_track(
        "Uta",
        Instrument::Voice {
            voicebank: "Uta".to_owned(),
        },
    );
    let pattern = project.add_pattern_for(voice, "Verse", Ticks::WHOLE);
    let mut lyrics = Vec::new();
    for (i, lyric) in ["ka", "さ", "ne"].iter().enumerate() {
        let id = project
            .add_note(
                pattern,
                Note {
                    id: NoteId(0),
                    start: Ticks::QUARTER * i as i64,
                    length: Ticks::QUARTER,
                    key: 69 + i as u8,
                    velocity: DEFAULT_VELOCITY,
                    pitch_offset: 0.0,
                },
            )
            .unwrap();
        lyrics.push((id, (*lyric).to_owned()));
    }
    project.set_lyrics(pattern, &lyrics);
    // Version 10: how the notes are sung (decision 0028).
    let ids: Vec<NoteId> = lyrics.iter().map(|(id, _)| *id).collect();
    project.set_expressions(
        pattern,
        &[
            (ids[0], sample_expression()),
            (
                ids[1],
                NoteExpression {
                    colour: -0.5,
                    ..NoteExpression::default()
                },
            ),
            (
                ids[2],
                NoteExpression {
                    breath: 0.25,
                    ..NoteExpression::default()
                },
            ),
        ],
    );
    project.add_clip(voice, Ticks::ZERO, Ticks::WHOLE, pattern);
}

/// The version 10 sample's first sung note: a scoop with a straight piece,
/// and a vibrato.
fn sample_expression() -> NoteExpression {
    NoteExpression {
        pitch: vec![
            PitchPoint::new(0.0, -150.0),
            PitchPoint {
                ms: 80.0,
                cents: 20.0,
                shape: PitchShape::Straight,
            },
            PitchPoint::new(160.0, 0.0),
        ],
        vibrato: Some(Vibrato {
            depth: 40.0,
            rate: 6.0,
            start: 0.15,
            fade_in: 0.1,
        }),
        ..NoteExpression::default()
    }
}

/// The version 9 sample's voice came back, lyrics and all (and older
/// samples have none).
fn check_voice(project: &Project, version: u32, path: &std::path::Path) {
    let name = path.display();
    let voice = project
        .tracks
        .iter()
        .find(|t| t.instrument.voicebank().is_some());
    if version < 9 {
        assert!(voice.is_none(), "{name}: no voices before version 9");
        return;
    }
    let voice = voice.unwrap_or_else(|| panic!("{name} lost its voice"));
    assert_eq!(voice.instrument.voicebank(), Some("Uta"));
    let pattern = project.pattern(voice.clips[0].pattern().unwrap()).unwrap();
    let sung: Vec<&str> = pattern
        .notes
        .iter()
        .map(|n| pattern.lyric(n.id).unwrap_or(""))
        .collect();
    assert_eq!(sung, ["ka", "さ", "ne"], "{name}");
    let expressions: Vec<Option<&NoteExpression>> = pattern
        .notes
        .iter()
        .map(|n| pattern.expression(n.id))
        .collect();
    if version < 10 {
        assert!(
            expressions.iter().all(Option::is_none),
            "{name}: sung the plain way before version 10"
        );
        return;
    }
    assert_eq!(expressions[0], Some(&sample_expression()), "{name}");
    assert_eq!(expressions[1].map(|e| e.colour), Some(-0.5), "{name}");
    assert_eq!(expressions[2].map(|e| e.breath), Some(0.25), "{name}");
}

/// The version 8 sample's bent glide came back; older glides are all
/// straight.
fn check_bent_glide(project: &Project, version: u32, path: &std::path::Path) {
    let name = path.display();
    let changes = project.tempo.changes();
    if version >= 8 {
        let rit = changes
            .iter()
            .find(|c| c.at == Ticks::quarters(16))
            .unwrap_or_else(|| panic!("{name} lost its ritardando"));
        assert!(
            rit.glide && (rit.curve - 0.5).abs() < 1e-12,
            "{name}: {rit:?}"
        );
        // Bent late, the middle of the glide is at about 89.8 BPM, where a
        // straight glide would be at 85.
        let middle = project.tempo.bpm_at(Ticks::quarters(20));
        assert!((middle - 89.8).abs() < 0.1, "{name}: {middle}");
    } else {
        assert!(changes.iter().all(|c| c.curve == 0.0), "{name}: straight");
    }
}

/// The version 7 sample's SFZ track and custom kit came back (and older
/// samples have neither).
fn check_newer_instruments(project: &Project, version: u32, path: &std::path::Path) {
    let sfz = project.tracks.iter().find_map(|t| t.instrument.sfz_file());
    let kit = project.tracks.iter().find(|t| t.instrument.is_custom_kit());
    if version >= 7 {
        assert_eq!(
            sfz,
            Some(SAMPLE_SFZ),
            "{} lost its SFZ track",
            path.display()
        );
        let pads = kit
            .unwrap_or_else(|| panic!("{} lost its custom kit", path.display()))
            .instrument
            .kit_pads();
        assert_eq!(pads.len(), 1);
        assert_eq!((pads[0].name.as_str(), pads[0].setting.key), ("Hum", 36));
        let sound = project.audio_file(pads[0].audio).unwrap();
        assert_eq!(sound.name, "Hum", "the pad plays the song's sound");
    } else {
        assert_eq!(sfz, None, "no SFZ instruments before version 7");
        assert!(kit.is_none(), "no custom kits before version 7");
    }
}

/// The version 2 sample's drums and effects, and the version 3 sample's
/// plugin and automation, came back.
fn check_mixing_and_plugins(project: &Project, version: u32, path: &std::path::Path) {
    if version >= 2 {
        let drums = project
            .tracks
            .iter()
            .find(|t| t.instrument.is_drums())
            .unwrap_or_else(|| panic!("{} lost its drum track", path.display()));
        assert!(matches!(&drums.instrument, Instrument::Drums { kit, .. } if kit == ACOUSTIC_KIT));
        assert_eq!(drums.instrument.pad(36).tune, -2.0, "{}", path.display());
        assert_eq!(drums.effects.len(), 1, "{}", path.display());
        assert_eq!(
            project.master.effects.len(),
            1,
            "the master limiter: {}",
            path.display()
        );
    }
    if version >= 3 {
        let synth = project
            .tracks
            .iter()
            .find(|t| t.instrument.plugin().is_some())
            .unwrap_or_else(|| panic!("{} lost its plugin track", path.display()));
        let setup = synth.instrument.plugin().unwrap();
        assert_eq!(setup, &sample_plugin(), "the plugin and its settings");
        assert!(
            matches!(&synth.effects[0].kind, EffectKind::Plugin(p) if *p == sample_plugin()),
            "the plugin effect and its settings"
        );
        assert_eq!(synth.automation.len(), 1);
        let value = synth.automation[0].value_at(half_way());
        assert!((value.unwrap() - 0.5).abs() < 1e-6, "{}", path.display());
    }
}

/// The version 4 sample's audio track, clip and file came back.
fn check_audio(project: &Project, opened: &std::path::Path, path: &std::path::Path) {
    let voice = project
        .tracks
        .iter()
        .find(|t| t.instrument.is_audio())
        .unwrap_or_else(|| panic!("{} lost its audio track", path.display()));
    let clip = voice.clips[0];
    assert_eq!(clip.start, Ticks::WHOLE);
    let ClipContent::Audio {
        offset,
        gain,
        fade_in,
        fade_out,
        ..
    } = clip.content
    else {
        panic!("{} lost its audio clip", path.display());
    };
    assert_eq!((offset, gain, fade_in, fade_out), (1200, 0.5, 0.01, 0.0));
    let file = project.audio_file(clip.audio().unwrap()).unwrap();
    assert_eq!((file.name.as_str(), file.frames), ("Hum", 4800));
    let bytes = std::fs::read(opened.join(&file.file)).unwrap();
    let sound = daw_formats::audio::decode(&bytes).unwrap();
    assert_eq!(sound.frames(), 4800, "the sound came out of the file");
}

/// Half way between the sample lane's points (at 0 and one bar).
fn half_way() -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "a tick position")]
    let half = (Ticks::WHOLE.0 / 2) as f64;
    half
}

#[test]
fn plugin_settings_are_kept_as_entries_of_their_own() {
    let path = std::env::temp_dir().join(format!("plugin-states-{}.song", std::process::id()));
    let sounds = scratch("states");
    write_hum(&sounds);
    save_project(&reference_project(), &path, Some(&sounds)).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let names: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .map(std::borrow::Cow::into_owned)
        .collect();
    assert!(
        names.iter().filter(|n| n.starts_with("plugins/")).count() == 2,
        "one entry per plugin: {names:?}"
    );
    let mut json = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("project.json").unwrap(), &mut json).unwrap();
    assert!(
        !json.contains("$state\""),
        "no settings inside the description"
    );
    assert!(json.contains("$state_file"));
    assert!(names.iter().any(|n| n == "audio/hum.wav"), "{names:?}");
    let project = load_project(&path, None).unwrap();
    assert_eq!(project, reference_project_after_load(&project));
    std::fs::remove_file(path).ok();
    std::fs::remove_dir_all(sounds).ok();
}

/// The reference project as loading gives it back (identifiers repaired).
fn reference_project_after_load(loaded: &Project) -> Project {
    let mut expected = reference_project();
    expected.repair_id_counter();
    let _ = loaded;
    expected
}

/// The version 5 sample's key changes, swing, glide and meter change came
/// back.
fn check_theory_and_rhythm(project: &Project, path: &std::path::Path) {
    let name = path.display();
    assert_eq!(
        project.key.at_bar(0),
        Some(Scale::new(9, ScaleKind::Minor)),
        "{name}"
    );
    assert_eq!(project.key.at_bar(6).map(|s| s.tonic), Some(0), "{name}");
    assert!(project.tempo.changes()[1].glide, "{name}");
    assert!((project.tempo.bpm_at(Ticks::quarters(12)) - 95.0).abs() < 1e-9);
    assert_eq!(project.meter.signature_at_bar(8).numerator, 7, "{name}");
    let swung = project
        .patterns
        .iter()
        .find(|p| !p.swing.is_straight())
        .unwrap_or_else(|| panic!("{name} lost its swing"));
    assert_eq!(swung.swing.percent, 67);
    assert_eq!(swung.swing.unit, Ticks::note_value(1, 8));
}

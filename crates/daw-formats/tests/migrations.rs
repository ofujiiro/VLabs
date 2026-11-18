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
use daw_model::{
    ACOUSTIC_KIT, AudioFile, AudioId, AutomationLane, AutomationPoint, AutomationTarget,
    ChainPlace, ClipContent, DEFAULT_VELOCITY, EffectKind, EqSettings, Instrument, Note, NoteId,
    PluginFormat, PluginRef, PluginSetup, PluginState, Project, Scale, ScaleKind, Swing, Ticks,
    TimeSignature,
};

fn data_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
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
/// a change, a swung beat, a tempo glide and a time-signature change).
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
    project.key.set(0, Some(Scale::new(9, ScaleKind::Minor)));
    project.key.set(4, Some(Scale::new(0, ScaleKind::Major)));
    project.meter.set(8, TimeSignature::new(7, 8).unwrap());
    if let Some(beat) = project.pattern_mut(beat) {
        beat.swing = Swing {
            percent: 67,
            unit: Ticks::note_value(1, 8),
        };
    }
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

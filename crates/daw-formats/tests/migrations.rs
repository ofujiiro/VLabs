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
    ACOUSTIC_KIT, AutomationLane, AutomationPoint, AutomationTarget, ChainPlace, DEFAULT_VELOCITY,
    EffectKind, EqSettings, Instrument, Note, NoteId, PluginFormat, PluginRef, PluginSetup,
    PluginState, Project, Ticks,
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

/// The project every sample file contains (version 2 added the drum
/// track, the pad setting and the effects; version 3 a plugin track with
/// saved settings, a plugin effect and automation lanes).
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
    "Format sample".clone_into(&mut project.title);
    project.tempo.set(Ticks::quarters(8), 90.0);
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
        save_project(&reference_project(), &sample).unwrap();
    }
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
        let project = load_project(&path)
            .unwrap_or_else(|e| panic!("{} no longer opens: {e}", path.display()));
        for pattern in &project.patterns {
            assert!(
                pattern.track.is_some(),
                "every pattern has its track: {}",
                path.display()
            );
        }
        if version >= 2 {
            let drums = project
                .tracks
                .iter()
                .find(|t| t.instrument.is_drums())
                .unwrap_or_else(|| panic!("{} lost its drum track", path.display()));
            assert!(
                matches!(&drums.instrument, Instrument::Drums { kit, .. } if kit == ACOUSTIC_KIT)
            );
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
        assert_eq!(project.title, "Format sample", "{}", path.display());
        assert_eq!(
            project.tempo.bpm_at(Ticks::quarters(9)),
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
    assert!(
        found >= FORMAT_VERSION as usize,
        "a sample file is missing for some format version (found {found})"
    );
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
    save_project(&reference_project(), &path).unwrap();
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
    let project = load_project(&path).unwrap();
    assert_eq!(project, reference_project_after_load(&project));
    std::fs::remove_file(path).ok();
}

/// The reference project as loading gives it back (identifiers repaired).
fn reference_project_after_load(loaded: &Project) -> Project {
    let mut expected = reference_project();
    expected.repair_id_counter();
    let _ = loaded;
    expected
}

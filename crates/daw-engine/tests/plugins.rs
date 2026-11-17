// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 3 in the engine, measured: settings that change over the song
//! (automation), plugin instruments and effects playing songs, a song whose
//! plugin is missing, and taking a plugin out while it plays.
//!
//! The plugin tests use the reference plugins (downloaded test fixtures,
//! never linked or bundled) when `VLABS_TEST_PLUGINS` names the folder that
//! holds them, and pass quietly otherwise. Every test runs under the
//! checking allocator: an allocation on the audio thread outside a plugin's
//! own processing stops the test.

#![allow(
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap, report and convert freely"
)]

use std::path::PathBuf;

use daw_dsp::sf2_writer::sine_test_font;
use daw_engine::{
    Engine, EngineConfig, PlaybackScope, PluginInfo, PluginKind, PluginTarget, Problem,
    SoundLibrary, plugin_ref, render_offline,
};
use daw_model::{
    AutomationPoint, AutomationTarget, BUILTIN_PIANO, ChainPlace, DelaySettings, DelayTime,
    EffectKind, Instrument, Note, NoteId, PluginFormat, PluginRef, PluginSetup, Project, Ticks,
    TrackId, gain_volume,
};
use daw_plugin_host::{Format, scan};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

const RATE: u32 = 48_000;

fn library() -> SoundLibrary {
    let bytes: &'static [u8] = Box::leak(sine_test_font(RATE).to_bytes().into_boxed_slice());
    let mut library = SoundLibrary::new();
    library.register_builtin(BUILTIN_PIANO, bytes);
    library.set_soundfont_reverb(0.0);
    library
}

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        max_block: 256,
    }
}

fn note(start: Ticks, length: Ticks, key: u8) -> Note {
    Note {
        id: NoteId(0),
        start,
        length,
        key,
        velocity: 0.8,
        pitch_offset: 0.0,
    }
}

/// A song with one track playing `notes`, no loop and no master effects.
fn song(notes: &[Note]) -> (Project, TrackId) {
    let mut project = Project::starter("P", "Piano");
    project.loop_region = None;
    project.master.effects.clear();
    let (track, pattern) = project.first_pattern().unwrap();
    project.add_notes(pattern, notes);
    (project, track)
}

/// At 120 BPM: a quarter note lasts half a second.
fn at(seconds: f64) -> usize {
    (seconds * f64::from(RATE)) as usize
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

fn point(quarters: f64, value: f32) -> AutomationPoint {
    AutomationPoint {
        at: Ticks((quarters * Ticks::QUARTER.0 as f64) as i64),
        value,
    }
}

/// Draws a lane on a track.
fn draw(
    project: &mut Project,
    track: TrackId,
    target: AutomationTarget,
    points: &[AutomationPoint],
) {
    project.add_lane(track, target);
    project.update_lane(track, target, |lane| {
        for p in points {
            lane.insert(*p);
        }
    });
}

#[test]
fn a_fade_drawn_on_the_volume_lane_is_heard() {
    let (plain, track) = song(&[note(Ticks::ZERO, Ticks::QUARTER * 8, 69)]);
    let mut faded = plain.clone();
    // From full volume at the start down to silence at the end of bar 1.
    draw(
        &mut faded,
        track,
        AutomationTarget::Volume,
        &[point(0.0, gain_volume(1.0)), point(4.0, 0.0)],
    );
    let (plain, _) = render_offline(&plain, library(), config(), 2.5);
    let (faded, _) = render_offline(&faded, library(), config(), 2.5);

    let start = rms(&faded[..at(0.03)]) / rms(&plain[..at(0.03)]);
    let middle = rms(&faded[at(0.98)..at(1.02)]) / rms(&plain[at(0.98)..at(1.02)]);
    let late = rms(&faded[at(1.75)..at(1.85)]) / rms(&plain[at(1.75)..at(1.85)]);
    eprintln!("level against no fade: start {start:.3}, middle {middle:.4}, late {late:.5}");
    assert!(start > 0.9, "the fade starts at full volume");
    // The lane has the mixer fader's scale: half way down is about -30 dB.
    assert!((0.02..0.05).contains(&middle), "half way down the fader");
    assert!(late < middle / 10.0, "nearly silent near the end");
    assert!(peak(&faded[at(2.05)..at(2.5)]) < 1e-4, "silent after it");
    // No clicks: the level moves smoothly from block to block.
    let steps = faded.windows(2).map(|w| (w[1] - w[0]).abs());
    let plain_steps = plain.windows(2).map(|w| (w[1] - w[0]).abs());
    assert!(
        steps.fold(0.0, f32::max) <= plain_steps.fold(0.0, f32::max) * 1.01,
        "no jumps bigger than the note's own"
    );
}

#[test]
fn a_pan_lane_moves_the_sound_from_left_to_right() {
    let (mut project, track) = song(&[note(Ticks::ZERO, Ticks::QUARTER * 8, 69)]);
    draw(
        &mut project,
        track,
        AutomationTarget::Pan,
        &[point(0.0, 0.0), point(4.0, 1.0)],
    );
    let (left, right) = render_offline(&project, library(), config(), 2.2);
    let early = 0..at(0.03);
    let late = at(2.05)..at(2.15);
    eprintln!(
        "early: left {:.4} right {:.4}; late: left {:.4} right {:.4}",
        rms(&left[early.clone()]),
        rms(&right[early.clone()]),
        rms(&left[late.clone()]),
        rms(&right[late.clone()])
    );
    assert!(
        rms(&left[early.clone()]) > 10.0 * rms(&right[early]),
        "starts on the left"
    );
    assert!(
        rms(&right[late.clone()]) > 10.0 * rms(&left[late]),
        "ends on the right"
    );
}

#[test]
fn an_effect_setting_can_change_during_the_song() {
    // Two short notes, one at the start and one at the start of bar 2.
    let short = Ticks::note_value(1, 32);
    let (mut project, track) = song(&[
        note(Ticks::ZERO, short, 69),
        note(Ticks::QUARTER * 4, short, 69),
    ]);
    let delay = project
        .add_effect(
            ChainPlace::Track(track),
            EffectKind::Delay(DelaySettings {
                time: DelayTime::Synced(Ticks::QUARTER),
                feedback: 0.0,
                mix: 0.0,
                tone_hz: 20_000.0,
                ping_pong: false,
            }),
        )
        .unwrap();
    // The delay's mix (its second setting) stays off for bar 1, then opens.
    draw(
        &mut project,
        track,
        AutomationTarget::Effect {
            effect: delay,
            param: 1,
        },
        &[point(0.0, 0.0), point(3.9, 0.0), point(4.0, 0.5)],
    );
    let (left, _) = render_offline(&project, library(), config(), 3.0);
    let first_note = rms(&left[..at(0.05)]);
    let first_echo = rms(&left[at(0.5)..at(0.55)]);
    let second_echo = rms(&left[at(2.5)..at(2.55)]);
    eprintln!("note {first_note:.4}, echo in bar 1 {first_echo:.5}, in bar 2 {second_echo:.4}");
    assert!(
        first_echo < first_note / 100.0,
        "no echo while the mix is off"
    );
    assert!(second_echo > first_note / 4.0, "an echo once it opens");
}

#[test]
fn a_song_whose_plugin_is_missing_still_plays_its_other_tracks() {
    let (mut project, _) = song(&[note(Ticks::ZERO, Ticks::QUARTER, 69)]);
    let missing = PluginRef {
        format: PluginFormat::Clap,
        id: "com.example.not-installed".to_owned(),
        name: "Not Installed Synth".to_owned(),
        vendor: String::new(),
        path: "/nowhere/Not Installed Synth.clap".to_owned(),
    };
    let other = project.add_track("Synth", Instrument::Plugin(PluginSetup::new(missing)));
    let pattern = project.add_pattern_for(other, "S", Ticks::QUARTER * 4);
    project.add_clip(other, Ticks::ZERO, Ticks::QUARTER * 4, pattern);
    project.add_note(pattern, note(Ticks::ZERO, Ticks::QUARTER, 60));

    let (mut engine, mut processor) = Engine::new(config(), library());
    engine.sync(&project, 1, PlaybackScope::Song);
    let problems = engine.take_problems();
    assert!(
        problems
            .iter()
            .any(|p| *p == Problem::PluginMissing("Not Installed Synth".to_owned())),
        "the person is told which plugin is missing: {problems:?}"
    );
    engine.play();
    let mut buffer = vec![0.0_f32; 512];
    let mut left = Vec::new();
    while left.len() < at(0.3) {
        processor.process(&mut buffer, 2);
        left.extend(buffer.as_chunks::<2>().0.iter().map(|f| f[0]));
        engine.poll();
    }
    assert!(
        rms(&left[at(0.05)..at(0.25)]) > 0.01,
        "the piano still plays"
    );
}

// ----- With the reference plugins

fn fixtures() -> Option<PathBuf> {
    let folder = PathBuf::from(std::env::var_os("VLABS_TEST_PLUGINS")?);
    folder.is_dir().then_some(folder)
}

/// The plugin of `kind` in a fixture file.
fn find(file: &str, format: Format, kind: PluginKind) -> Option<PluginInfo> {
    let path = fixtures()?.join(file);
    scan::describe(format, &path)
        .unwrap()
        .into_iter()
        .find(|p| p.kind == kind)
}

const INSTRUMENTS: [(&str, Format); 4] = [
    ("surge/Surge XT.clap", Format::Clap),
    ("surge/Surge XT.vst3", Format::Vst3),
    ("dexed/Dexed.clap", Format::Clap),
    ("dexed/Dexed.vst3", Format::Vst3),
];

/// A song whose one track plays `info` (and the library that finds it).
fn plugin_song(info: &PluginInfo, notes: &[Note]) -> (Project, TrackId, SoundLibrary) {
    let (mut project, track) = song(notes);
    project.track_mut(track).unwrap().instrument =
        Instrument::Plugin(PluginSetup::new(plugin_ref(info)));
    let mut library = library();
    library.register_plugins(vec![info.clone()]);
    (project, track, library)
}

#[test]
fn a_plugin_instrument_plays_the_song_on_time() {
    for (file, format) in INSTRUMENTS {
        let Some(info) = find(file, format, PluginKind::Instrument) else {
            return;
        };
        // One note on the second beat, a quarter note long.
        let (project, _, library) = plugin_song(&info, &[note(Ticks::QUARTER, Ticks::QUARTER, 60)]);
        let (left, right) = render_offline(&project, library, config(), 1.5);
        let before = peak(&left[..at(0.49)]);
        let onset = left
            .iter()
            .position(|s| s.abs() > 0.01)
            .unwrap_or(left.len());
        let during = rms(&left[at(0.6)..at(0.95)]);
        eprintln!(
            "{} ({format:?}): before {before:.5}, starts at {:.3} s, level {during:.4}",
            info.name,
            onset as f64 / f64::from(RATE)
        );
        assert!(before < 1e-3, "{}: silent before the note", info.name);
        assert!(
            (at(0.5)..at(0.53)).contains(&onset),
            "{}: the note starts on the beat",
            info.name
        );
        assert!(during > 0.005, "{}: and sounds", info.name);
        assert!(
            left.iter()
                .chain(&right)
                .all(|s| s.is_finite() && s.abs() < 1.0),
            "{}: no clipping",
            info.name
        );
    }
}

#[test]
fn a_plugin_parameter_follows_its_lane() {
    for (file, format) in &INSTRUMENTS[..2] {
        let Some(info) = find(file, *format, PluginKind::Instrument) else {
            return;
        };
        let (mut project, track, library) =
            plugin_song(&info, &[note(Ticks::ZERO, Ticks::QUARTER * 4, 60)]);
        // The engine lists the plugin's parameters, as the lane chooser does.
        let (mut engine, _processor) = Engine::new(config(), library.clone());
        engine.sync(&project, 1, PlaybackScope::Song);
        let volume = engine
            .plugin_params(PluginTarget::Instrument(track))
            .into_iter()
            .find(|p| p.name.to_lowercase().contains("global volume"))
            .unwrap()
            .id;
        draw(
            &mut project,
            track,
            AutomationTarget::Instrument { param: volume },
            &[point(0.0, 0.9), point(2.0, 0.9), point(2.1, 0.1)],
        );
        let (left, _) = render_offline(&project, library, config(), 2.0);
        let loud = rms(&left[at(0.3)..at(0.9)]);
        let quiet = rms(&left[at(1.3)..at(1.9)]);
        eprintln!("{format:?}: volume 0.9 {loud:.4}, then 0.1 {quiet:.5}");
        assert!(
            quiet < loud / 4.0,
            "turning the volume down mid-note is heard"
        );
    }
}

#[test]
fn a_plugin_effect_works_in_a_track_chain() {
    for (file, format) in [
        ("surge/Surge XT Effects.clap", Format::Clap),
        ("surge/Surge XT Effects.vst3", Format::Vst3),
    ] {
        let Some(info) = find(file, format, PluginKind::Effect) else {
            return;
        };
        let (plain, track) = song(&[note(Ticks::ZERO, Ticks::QUARTER * 2, 69)]);
        let mut with_effect = plain.clone();
        with_effect.add_effect(
            ChainPlace::Track(track),
            EffectKind::Plugin(PluginSetup::new(plugin_ref(&info))),
        );
        let mut library = library();
        library.register_plugins(vec![info.clone()]);
        let (dry, _) = render_offline(&plain, library.clone(), config(), 1.0);
        let (wet, _) = render_offline(&with_effect, library, config(), 1.0);
        let window = at(0.2)..at(0.8);
        let difference = rms(&dry[window.clone()]
            .iter()
            .zip(&wet[window.clone()])
            .map(|(a, b)| a - b)
            .collect::<Vec<_>>());
        eprintln!(
            "{} ({format:?}): dry {:.4}, through the effect {:.4}, difference {difference:.4}",
            info.name,
            rms(&dry[window.clone()]),
            rms(&wet[window.clone()])
        );
        assert!(rms(&wet[window]) > 0.01, "sound comes through the effect");
        assert!(difference > 0.001, "and the effect changes it");
    }
}

#[test]
fn a_plugin_track_can_be_deleted_while_the_song_plays() {
    let Some(info) = find("surge/Surge XT.clap", Format::Clap, PluginKind::Instrument) else {
        return;
    };
    let (project, track, library) =
        plugin_song(&info, &[note(Ticks::ZERO, Ticks::QUARTER * 8, 60)]);
    let (mut engine, mut processor) = Engine::new(config(), library);
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.play();
    let target = PluginTarget::Instrument(track);
    assert!(engine.has_plugin(target));
    let mut buffer = vec![0.0_f32; 512];
    let mut run = |engine: &mut Engine, blocks: usize| {
        let mut level = 0.0_f32;
        for _ in 0..blocks {
            processor.process(&mut buffer, 2);
            level = level.max(peak(&buffer));
            engine.poll();
            engine.plugin_idle();
        }
        level
    };
    assert!(run(&mut engine, 40) > 0.01, "it plays");
    let mut without = project.clone();
    without.remove_track(track);
    engine.sync(&without, 2, PlaybackScope::Song);
    assert!(!engine.has_plugin(target), "taken out of the song");
    assert!(run(&mut engine, 40) < 1e-4, "silence, and nothing breaks");
}

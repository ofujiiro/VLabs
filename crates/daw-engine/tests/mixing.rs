// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 2 in the engine, measured: drum tracks, effect chains on tracks and
//! on the master bus, settings that change while sound plays, meters,
//! playing a pattern on its own, and offline rendering in blocks.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test code: helpers unwrap and convert freely"
)]

use daw_dsp::sf2_writer::sine_test_font;
use daw_engine::{
    Engine, EngineConfig, EngineProcessor, OfflineRender, PlaybackScope, SoundLibrary,
    render_offline,
};
use daw_model::{
    BUILTIN_PIANO, ChainPlace, DelaySettings, DelayTime, ELECTRONIC_KIT, EffectKind, Note, NoteId,
    Project, ReverbSettings, Ticks,
};

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
        max_block: 512,
    }
}

fn note(start: Ticks, length: Ticks, key: u8, velocity: f32) -> Note {
    Note {
        id: NoteId(0),
        start,
        length,
        key,
        velocity,
        pitch_offset: 0.0,
    }
}

/// A song with one short piano note at the start and no master effects.
fn song() -> Project {
    let mut project = Project::starter("P", "Piano");
    project.loop_region = None;
    project.master.effects.clear();
    let (_, pattern) = project.first_pattern().unwrap();
    project.add_note(
        pattern,
        note(Ticks::ZERO, Ticks::note_value(1, 32), 69, 0.9),
    );
    project
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

#[test]
fn a_track_delay_echoes_on_the_beat_at_the_song_tempo() {
    let mut project = song();
    let (track, _) = project.first_pattern().unwrap();
    project.add_effect(
        ChainPlace::Track(track),
        EffectKind::Delay(DelaySettings {
            time: DelayTime::Synced(Ticks::QUARTER),
            feedback: 0.0,
            mix: 0.5,
            tone_hz: 20_000.0,
            ping_pong: false,
        }),
    );
    let (left, _) = render_offline(&project, library(), config(), 1.5);
    // At 120 BPM a quarter note is half a second: the echo arrives then.
    let note_level = rms(&left[..2400]);
    let gap = rms(&left[12_000..20_000]);
    let echo = rms(&left[24_000..26_400]);
    assert!(gap < note_level / 50.0, "silence between note and echo");
    assert!(
        echo > note_level * 0.4 && echo < note_level * 1.1,
        "the echo: {echo} after {note_level}"
    );
}

#[test]
fn master_effects_and_gain_shape_the_whole_mix() {
    let mut quiet = song();
    quiet.master.gain = 0.5;
    let (loud_left, _) = render_offline(&song(), library(), config(), 0.2);
    let (quiet_left, _) = render_offline(&quiet, library(), config(), 0.2);
    assert!(
        (peak(&quiet_left) / peak(&loud_left) - 0.5).abs() < 0.01,
        "master volume halves"
    );

    let mut roomy = song();
    roomy.add_effect(
        ChainPlace::Master,
        EffectKind::Reverb(ReverbSettings {
            mix: 0.5,
            ..ReverbSettings::default()
        }),
    );
    let (dry, _) = render_offline(&song(), library(), config(), 1.0);
    let (wet, _) = render_offline(&roomy, library(), config(), 1.0);
    let tail = 24_000..48_000;
    assert!(
        rms(&wet[tail.clone()]) > rms(&dry[tail.clone()]) * 10.0,
        "the room rings on"
    );

    // A bypassed effect changes nothing.
    let id = roomy.master.effects[0].id;
    roomy.update_effect(id, |e| e.enabled = false);
    let (bypassed, _) = render_offline(&roomy, library(), config(), 1.0);
    assert_eq!(bypassed, dry);
}

#[test]
fn drum_tracks_play_their_kit() {
    let mut project = Project::starter("P", "Piano");
    project.master.effects.clear();
    project.loop_region = None;
    let (_, beat) = project.add_drums_with_pattern("Drums", "Beat", ELECTRONIC_KIT, 1, 1);
    project.add_note(beat, note(Ticks::ZERO, Ticks::note_value(1, 16), 36, 1.0));
    project.add_note(
        beat,
        note(Ticks::QUARTER, Ticks::note_value(1, 16), 42, 1.0),
    );
    let (left, _) = render_offline(&project, library(), config(), 1.0);
    assert!(peak(&left[..2400]) > 0.3, "the kick plays");
    assert!(
        peak(&left[24_000..26_400]) > 0.1,
        "the hat plays on the next beat"
    );
}

/// Runs the processor and returns the left channel.
fn run(engine: &mut Engine, processor: &mut EngineProcessor, frames: usize) -> Vec<f32> {
    let mut buffer = vec![0.0_f32; 1024];
    let mut left = Vec::new();
    while left.len() < frames {
        processor.process(&mut buffer, 2);
        left.extend(buffer.as_chunks::<2>().0.iter().map(|f| f[0]));
        engine.poll();
    }
    left
}

#[test]
fn pad_settings_change_while_the_kit_plays_without_allocating() {
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);
    let mut project = Project::starter("P", "Piano");
    project.master.effects.clear();
    let (drums, beat) = project.add_drums_with_pattern("Drums", "Beat", ELECTRONIC_KIT, 1, 4);
    project.add_note(beat, note(Ticks::ZERO, Ticks::note_value(1, 16), 36, 1.0));
    let (mut engine, mut processor) = Engine::new(config(), library());
    let scope = PlaybackScope::Pattern {
        track: drums,
        pattern: beat,
    };
    engine.sync(&project, 1, scope);
    engine.play();
    let before = run(&mut engine, &mut processor, 4800);
    let level = peak(&before);
    assert!(level > 0.3, "the kick plays in pattern mode");

    // Mute the kick: the change reaches the playing sampler (no rebuild).
    let mut kick = project.track(drums).unwrap().instrument.pad(36);
    kick.muted = true;
    project.track_mut(drums).unwrap().instrument.set_pad(kick);
    engine.sync(&project, 2, scope);
    let violations = daw_rt::violation_count();
    // The pattern is one bar (2 s): play past its loop back to the kick.
    let after = run(&mut engine, &mut processor, 2 * 48_000 + 4800);
    assert_eq!(
        daw_rt::violation_count(),
        violations,
        "allocated on the audio thread"
    );
    assert!(
        peak(&after[96_000 - 4800..]) < 1e-4,
        "the muted kick is silent on the next loop"
    );
}

#[test]
fn meters_follow_each_track() {
    let mut project = song();
    let (piano, _) = project.first_pattern().unwrap();
    let (drums, _) = project.add_drums_with_pattern("Drums", "Beat", ELECTRONIC_KIT, 1, 1);
    let (mut engine, mut processor) = Engine::new(config(), library());
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.play();
    run(&mut engine, &mut processor, 1024);
    // The test instrument's note peaks at about 0.08.
    assert!(engine.track_peak(piano) > 0.02, "the piano's meter moves");
    assert!(
        engine.track_peak(drums) < 1e-6,
        "the silent drums' meter does not"
    );
}

#[test]
fn pattern_mode_plays_one_pattern_in_a_loop() {
    let mut project = song();
    let (piano, pattern) = project.first_pattern().unwrap();
    project.pattern_mut(pattern).unwrap().length = Ticks::WHOLE;
    // Placed late in the song: in song mode the start is silent.
    project.update_clip(project.track(piano).unwrap().clips[0].id, |c| {
        c.start = Ticks::WHOLE * 8;
    });
    let (mut engine, mut processor) = Engine::new(config(), library());
    engine.sync(&project, 1, PlaybackScope::Song);
    engine.play();
    let song = run(&mut engine, &mut processor, 4800);
    assert!(peak(&song) < 1e-4, "nothing at the song's start");

    let (mut engine, mut processor) = Engine::new(config(), library());
    engine.sync(
        &project,
        1,
        PlaybackScope::Pattern {
            track: piano,
            pattern,
        },
    );
    engine.play();
    let looped = run(&mut engine, &mut processor, 2 * 96_000 + 4800);
    assert!(peak(&looped[..4800]) > 0.02, "the pattern starts at once");
    assert!(
        peak(&looped[96_000..96_000 + 4800]) > 0.02,
        "and loops after its one bar"
    );
}

#[test]
fn offline_rendering_in_any_block_size_matches() {
    let mut project = song();
    let (track, _) = project.first_pattern().unwrap();
    project.add_effect(
        ChainPlace::Track(track),
        EffectKind::Reverb(ReverbSettings::default()),
    );
    let (whole, _) = render_offline(&project, library(), config(), 0.5);
    let mut render = OfflineRender::new(&project, library(), config());
    let mut pieces = Vec::new();
    for size in [100, 1000, 333, 10_000, 12_567] {
        let mut left = vec![0.0; size];
        let mut right = vec![0.0; size];
        render.render(&mut left, &mut right);
        pieces.extend(left);
    }
    assert_eq!(render.frames(), 24_000);
    assert_eq!(
        &pieces[..],
        &whole[..],
        "the same sound however it is cut up"
    );
}

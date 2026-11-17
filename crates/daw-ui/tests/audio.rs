// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 4 in the session, driven the way the interface drives it
//! (decision 0017): importing sound files, recording takes from the input,
//! shaping clips, saving and opening songs with their sound.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::assertions_on_result_states,
    clippy::assert_is_empty,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use daw_engine::{Engine, EngineConfig, EngineProcessor, InputPort, SoundLibrary};
use daw_formats::wav::{WavFormat, WavWriter};
use daw_model::{ClipContent, Ticks, TrackId};
use daw_ui::{Session, View, text};

const BLOCK: usize = 256;

fn new_song() -> (Session, EngineProcessor) {
    let (engine, processor) = Engine::new(EngineConfig::default(), SoundLibrary::new());
    let mut session = Session::starter(engine);
    session.update();
    session.show_song();
    (session, processor)
}

fn temp_path(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "daw-ui-audio-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&folder).unwrap();
    folder.join(name)
}

/// A WAV file of `seconds` of a 440 Hz tone at half level.
fn tone_file(name: &str, seconds: f64, rate: u32, channels: u16) -> PathBuf {
    let path = temp_path(name);
    let mut writer =
        WavWriter::create_with_channels(&path, WavFormat::Pcm16, rate, channels).unwrap();
    let frames = (seconds * f64::from(rate)) as usize;
    let samples: Vec<f32> = (0..frames)
        .flat_map(|i| {
            let s = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / f64::from(rate)).sin() as f32;
            std::iter::repeat_n(s * 0.5, usize::from(channels))
        })
        .collect();
    writer.write_interleaved(&samples).unwrap();
    writer.finish().unwrap();
    path
}

/// Lets `seconds` of audio pass, feeding the input with `input` if given;
/// returns the loudest sample played.
fn run(
    session: &mut Session,
    processor: &mut EngineProcessor,
    seconds: f64,
    mut input: Option<(&mut InputPort, &mut f64)>,
) -> f32 {
    let blocks = (seconds * 48_000.0 / BLOCK as f64).ceil() as usize;
    let mut buffer = vec![0.0_f32; BLOCK * 2];
    let mut peak = 0.0_f32;
    for _ in 0..blocks.max(1) {
        if let Some((port, phase)) = input.as_mut() {
            let samples: Vec<f32> = (0..BLOCK)
                .map(|i| {
                    let t = (**phase + i as f64) / 48_000.0;
                    (2.0 * std::f64::consts::PI * 220.0 * t).sin() as f32 * 0.4
                })
                .collect();
            **phase += BLOCK as f64;
            port.push(&samples, 1, 48_000);
        }
        processor.process(&mut buffer, 2);
        peak = buffer.iter().fold(peak, |m, s| m.max(s.abs()));
        session.update();
    }
    peak
}

/// Waits (letting the background threads work) until `done` holds.
fn wait_for(session: &mut Session, done: impl Fn(&Session) -> bool) {
    let start = Instant::now();
    while !done(session) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "waited too long; status: {:?}",
            session.status()
        );
        std::thread::sleep(Duration::from_millis(5));
        session.update();
    }
}

fn audio_tracks(session: &Session) -> Vec<TrackId> {
    session
        .project()
        .tracks
        .iter()
        .filter(|t| t.instrument.is_audio())
        .map(|t| t.id)
        .collect()
}

#[test]
fn an_imported_file_goes_on_a_track_of_its_own_and_plays() {
    let (mut s, mut processor) = new_song();
    let file = tone_file("Vocals.wav", 1.0, 44_100, 2);
    s.import_audio(vec![file], None, Ticks::WHOLE);
    assert!(s.is_loading_audio());
    wait_for(&mut s, |s| !s.is_loading_audio());

    let tracks = audio_tracks(&s);
    assert_eq!(tracks.len(), 1);
    let track = s.project().track(tracks[0]).unwrap();
    assert_eq!(track.name, "Vocals", "named after the file");
    let clip = track.clips[0];
    assert_eq!(clip.start, Ticks::WHOLE);
    assert_eq!(clip.length, Ticks::quarters(2), "one second at 120 BPM");
    let file = s
        .project()
        .audio_file(clip.audio().unwrap())
        .unwrap()
        .clone();
    assert_eq!(
        (file.frames, file.sample_rate, file.channels),
        (44_100, 44_100, 2)
    );
    assert!(
        s.audio_folder().join(&file.file).is_file(),
        "kept in the folder"
    );
    assert_eq!(s.selected_clip(), Some(clip.id));
    assert_eq!(s.focused_track(), tracks[0]);
    assert_eq!(s.status(), Some(text::imported("Vocals.wav").as_str()));

    // It plays from bar 2.
    s.locate_song(Ticks::WHOLE);
    s.play();
    let peak = run(&mut s, &mut processor, 0.5, None);
    assert!(peak > 0.3, "heard: {peak}");
    s.stop();

    // One undo takes it all away.
    s.undo();
    assert!(audio_tracks(&s).is_empty());
    assert!(s.project().audio.is_empty());
}

#[test]
fn importing_onto_an_audio_track_replaces_what_it_covers() {
    let (mut s, _processor) = new_song();
    // Four seconds: two bars at 120 BPM.
    s.import_audio(vec![tone_file("a.wav", 4.0, 48_000, 1)], None, Ticks::ZERO);
    wait_for(&mut s, |s| !s.is_loading_audio());
    let track = audio_tracks(&s)[0];
    s.import_audio(
        vec![tone_file("b.wav", 1.0, 48_000, 1)],
        Some(track),
        Ticks::WHOLE,
    );
    wait_for(&mut s, |s| !s.is_loading_audio());
    let spans: Vec<(Ticks, Ticks)> = s
        .project()
        .track(track)
        .unwrap()
        .clips
        .iter()
        .map(|c| (c.start, c.end()))
        .collect();
    let bar = Ticks::WHOLE;
    assert_eq!(
        spans,
        vec![
            (Ticks::ZERO, bar),
            (bar, bar + Ticks::quarters(2)),
            (bar + Ticks::quarters(2), bar * 2)
        ],
        "the old clip is cut around the new one"
    );
    assert_eq!(audio_tracks(&s).len(), 1, "no new track");
}

#[test]
fn clips_split_trim_fade_and_change_volume_one_step_each() {
    let (mut s, mut processor) = new_song();
    s.import_audio(
        vec![tone_file("loop.wav", 2.0, 48_000, 1)],
        None,
        Ticks::ZERO,
    );
    wait_for(&mut s, |s| !s.is_loading_audio());
    let clip = s.selected_clip().unwrap();

    let second = s.split_clip(clip, Ticks::QUARTER).unwrap();
    assert_eq!(s.selected_clip(), Some(second));
    assert_eq!(s.project().clip(clip).unwrap().1.end(), Ticks::QUARTER);
    let ClipContent::Audio { offset, .. } = s.project().clip(second).unwrap().1.content else {
        panic!("audio");
    };
    assert_eq!(offset, 24_000, "half a second in");
    s.undo();
    assert!(s.project().clip(second).is_none(), "one step");

    s.trim_clip_start(clip, Ticks::QUARTER * 2);
    assert_eq!(s.project().clip(clip).unwrap().1.start, Ticks::QUARTER * 2);
    s.set_clip_fades(clip, 0.25, 9.0);
    assert_eq!(
        s.clip_fades(clip),
        Some((0.25, 0.75)),
        "together no longer than the clip"
    );
    s.set_clip_gain(clip, 0.5);
    assert_eq!(s.clip_gain(clip), Some(0.5));
    s.undo();
    s.undo();
    assert_eq!(s.clip_fades(clip), Some((0.0, 0.0)));
    s.undo();
    assert_eq!(s.project().clip(clip).unwrap().1.start, Ticks::ZERO);

    // Splitting at the play position.
    s.select_clip(Some(clip));
    s.locate_song(Ticks::QUARTER * 3);
    run(&mut s, &mut processor, 0.01, None);
    assert!(s.split_at_play_position().is_some());
}

#[test]
fn a_saved_song_opens_with_its_sound() {
    let (mut s, _processor) = new_song();
    s.import_audio(
        vec![tone_file("hum.wav", 0.5, 22_050, 1)],
        None,
        Ticks::ZERO,
    );
    wait_for(&mut s, |s| !s.is_loading_audio());
    let path = temp_path("song");
    s.save_as(&path).unwrap();
    let saved = s.path().unwrap().to_path_buf();

    let (mut other, mut processor) = new_song();
    other.open(&saved).unwrap();
    wait_for(&mut other, |s| !s.is_loading_audio());
    let file = other.project().audio[0].clone();
    assert!(other.audio_folder().join(&file.file).is_file(), "extracted");
    assert!(matches!(
        other.sound_state(&file.file),
        daw_ui::SoundState::Ready(_)
    ));
    other.play();
    assert!(run(&mut other, &mut processor, 0.3, None) > 0.3);
}

/// Records `seconds` of a tone into a new audio track (no count-in).
fn record(seconds: f64, latency: f32, feed: bool) -> (Session, TrackId) {
    let (mut s, mut processor) = new_song();
    s.set_count_in_bars(0);
    let track = s.add_audio_track();
    assert_eq!(s.focused_track(), track);
    let mut port = s.engine_mut().take_input_port().unwrap();
    s.engine_mut().set_record_latency(latency);
    s.toggle_record();
    assert!(s.is_recording_audio());
    assert!(s.wants_input(), "the input is opened for recording");
    let mut phase = 0.0;
    let input = feed.then_some((&mut port, &mut phase));
    run(&mut s, &mut processor, seconds, input);
    assert!(s.live_take().is_some() || !feed, "drawn while it records");
    s.toggle_record();
    run(&mut s, &mut processor, 0.05, None);
    (s, track)
}

#[test]
fn a_recorded_take_lands_where_recording_started() {
    let (mut s, track) = record(1.0, 0.0, true);
    wait_for(&mut s, |s| {
        !s.project().track(track).unwrap().clips.is_empty()
    });
    let clip = s.project().track(track).unwrap().clips[0];
    assert_eq!(clip.start, Ticks::ZERO);
    let file = s
        .project()
        .audio_file(clip.audio().unwrap())
        .unwrap()
        .clone();
    assert_eq!(file.name, text::recording_name(1));
    assert_eq!((file.sample_rate, file.channels), (48_000, 1));
    assert!(
        (47_000..=49_500).contains(&file.frames),
        "about a second: {}",
        file.frames
    );
    let bytes = std::fs::read(s.audio_folder().join(&file.file)).unwrap();
    let sound = daw_formats::audio::decode(&bytes).unwrap();
    assert_eq!(
        sound.frames() as u64,
        file.frames,
        "the file holds the take"
    );
    assert!(
        sound.channels[0]
            .iter()
            .fold(0.0_f32, |m, x| m.max(x.abs()))
            > 0.35
    );
    assert!(s.wants_input(), "the input stays open for the next take");

    s.undo();
    assert!(
        s.project().track(track).unwrap().clips.is_empty(),
        "one step"
    );
}

#[test]
fn a_take_is_moved_earlier_by_the_delay_out_and_back_in() {
    let (mut s, track) = record(1.0, 0.1, true);
    wait_for(&mut s, |s| {
        !s.project().track(track).unwrap().clips.is_empty()
    });
    let clip = s.project().track(track).unwrap().clips[0];
    let ClipContent::Audio { offset, .. } = clip.content else {
        panic!("audio");
    };
    assert_eq!(offset, 4_800, "a tenth of a second at 48 kHz");
    assert_eq!(
        clip.start,
        Ticks::ZERO,
        "it still starts where recording did"
    );
}

#[test]
fn when_no_sound_comes_in_nothing_is_added_and_the_person_is_told() {
    let (mut s, track) = record(0.3, 0.0, false);
    wait_for(&mut s, |s| s.status() == Some(text::NO_INPUT));
    assert!(s.project().track(track).unwrap().clips.is_empty());
}

#[test]
fn audio_tracks_never_open_as_patterns_and_an_instrument_always_stays() {
    let (mut s, _processor) = new_song();
    let audio = s.add_audio_track();
    s.open_track(audio);
    assert_eq!(s.view(), View::Song);
    assert_eq!(s.focused_track(), audio);
    assert!(
        !s.is_audio_track(s.track_id()),
        "notes still go to an instrument"
    );

    let instruments: Vec<TrackId> = s
        .project()
        .tracks
        .iter()
        .filter(|t| !t.instrument.is_audio())
        .map(|t| t.id)
        .collect();
    s.remove_track(instruments[0]);
    assert!(s.is_last_instrument(instruments[1]));
    s.remove_track(instruments[1]);
    assert!(s.project().track(instruments[1]).is_some(), "kept");
}

#[test]
fn a_tempo_change_keeps_imported_sound_in_seconds() {
    let (mut s, _processor) = new_song();
    s.import_audio(vec![tone_file("t.wav", 1.0, 48_000, 1)], None, Ticks::ZERO);
    wait_for(&mut s, |s| !s.is_loading_audio());
    let clip = s.selected_clip().unwrap();
    s.set_tempo(60.0);
    assert_eq!(s.project().clip(clip).unwrap().1.length, Ticks::QUARTER);
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Standard MIDI Files (decision 0020): a song goes out as the notes it
//! plays, with its tempo (glides as small steps), signatures and key, and
//! comes back the same; files written by other programs are read, damaged
//! ones are refused.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "test code"
)]

use daw_formats::midi::{self, EventKind, ImportedTrack, Timing};
use daw_model::{
    ACOUSTIC_KIT, DEFAULT_VELOCITY, Instrument, Note, NoteId, Project, Scale, ScaleKind, Swing,
    SynthPreset, Ticks, TimeSignature,
};

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

/// A song with a piano melody, a swung beat repeated over four bars, a
/// synth bass, a glide, a 7/8 bar and a key.
fn song() -> Project {
    let mut project = Project::starter("Melody", "Piano");
    "Round trip".clone_into(&mut project.title);
    let (_, melody) = project.first_pattern().unwrap();
    let triplet = Ticks::note_value(1, 12);
    for (i, key) in [60_u8, 64, 67].iter().enumerate() {
        project.add_note(melody, note(triplet * i as i64, triplet, *key));
    }
    project.add_note(melody, note(Ticks::WHOLE, Ticks::QUARTER * 3, 72));
    let (_, beat) = project.add_drums_with_pattern("Drums", "Beat", ACOUSTIC_KIT, 1, 4);
    for step in 0..8 {
        project.add_note(
            beat,
            note(Ticks::note_value(step, 8), Ticks::note_value(1, 16), 42),
        );
    }
    if let Some(pattern) = project.pattern_mut(beat) {
        pattern.swing = Swing {
            percent: 67,
            unit: Ticks::note_value(1, 8),
        };
    }
    let bass = project.add_track("Bass", Instrument::Synth(SynthPreset::Bass.settings()));
    let line = project.add_pattern_for(bass, "Line", Ticks::WHOLE);
    project.add_note(line, note(Ticks::ZERO, Ticks::QUARTER, 36));
    project.add_clip(bass, Ticks::WHOLE, Ticks::WHOLE * 2, line);
    project.tempo.set(Ticks::WHOLE * 2, 140.0);
    project.tempo.set_glide(Ticks::ZERO, true);
    project.meter.set(3, TimeSignature::new(7, 8).unwrap());
    project.key.set(0, Some(Scale::new(9, ScaleKind::Minor)));
    project
}

/// The notes a project's track plays, as (start, end, key).
fn played(project: &Project, name: &str) -> Vec<(Ticks, Ticks, u8)> {
    let track = project.tracks.iter().find(|t| t.name == name).unwrap();
    let mut notes: Vec<(Ticks, Ticks, u8)> = track
        .clips
        .iter()
        .filter_map(|clip| Some((clip, project.pattern(clip.pattern()?)?)))
        .flat_map(|(clip, pattern)| pattern.played_in(clip))
        .map(|n| (n.start, n.end, n.key))
        .collect();
    notes.sort();
    notes
}

/// One MIDI tick at 960 per quarter note, in our ticks.
const MIDI_TICK: i64 = 3003;

fn close(a: &[(Ticks, Ticks, u8)], b: &[(Ticks, Ticks, u8)]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            (x.0 - y.0).0.abs() <= MIDI_TICK && (x.1 - y.1).0.abs() <= MIDI_TICK && x.2 == y.2
        })
}

fn name(track: &ImportedTrack) -> String {
    track.name.clone().unwrap_or_else(|| "Track".to_owned())
}

#[test]
fn a_song_goes_out_and_comes_back_as_the_notes_it_plays() {
    let original = song();
    let file = midi::song_to_midi(&original);
    assert_eq!(file.format, 1);
    assert_eq!(file.timing, Timing::PerQuarter(midi::EXPORT_PPQ));
    assert_eq!(file.tracks.len(), 4, "the conductor and three instruments");
    let bytes = midi::write(&file);
    assert_eq!(&bytes[..4], b"MThd");
    assert_eq!(
        midi::read(&bytes).unwrap(),
        file,
        "written and read the same"
    );

    let imported = midi::import(&bytes).unwrap();
    assert_eq!(imported.title.as_deref(), Some("Round trip"));
    let back = imported.into_project(name, "Pattern");
    for track in ["Piano", "Drums", "Bass"] {
        let (a, b) = (played(&original, track), played(&back, track));
        assert!(close(&a, &b), "{track}: {a:?} came back as {b:?}");
    }
    let drums = back.tracks.iter().find(|t| t.name == "Drums").unwrap();
    assert!(matches!(&drums.instrument, Instrument::Drums { kit, .. } if kit == ACOUSTIC_KIT));
    let bass = back.tracks.iter().find(|t| t.name == "Bass").unwrap();
    assert_eq!(
        bass.instrument.synth(),
        Some(&SynthPreset::Bass.settings()),
        "a synth bass comes back as the synth's bass"
    );
    let piano = back.tracks.iter().find(|t| t.name == "Piano").unwrap();
    assert_eq!(piano.instrument, Instrument::builtin_piano());
    // The swing is in the notes: the off-beat hats are late.
    let hats = played(&back, "Drums");
    assert!(
        hats[1].0 > Ticks::note_value(1, 8),
        "swung: {:?}",
        hats[1].0
    );
    // Signatures and key at their bars, a limiter on the master.
    assert_eq!(back.meter.signature_at_bar(3).numerator, 7);
    assert_eq!(back.meter.signature_at_bar(2).numerator, 4);
    assert_eq!(back.key.at_bar(0), Some(Scale::new(9, ScaleKind::Minor)));
    assert_eq!(back.master.effects.len(), 1);
}

#[test]
fn a_glide_becomes_steps_that_keep_the_songs_timing() {
    let original = song();
    let back = midi::import(&midi::write(&midi::song_to_midi(&original)))
        .unwrap()
        .into_project(name, "Pattern");
    // Steps every sixteenth over two bars.
    assert_eq!(back.tempo.changes().len(), 32 + 1);
    for bar in 0..=4 {
        let at = Ticks::WHOLE * bar;
        let (a, b) = (original.tempo.seconds_at(at), back.tempo.seconds_at(at));
        assert!((a - b).abs() < 0.0005, "bar {bar}: {a} s became {b} s");
    }
    assert!((back.tempo.bpm_at(Ticks::WHOLE * 3) - 140.0).abs() < 0.01);
}

/// A file as another program writes it: format 0, running status, notes
/// ended by a note-on of velocity zero, an end with no start, a note still
/// held at the end, 480 ticks per quarter, a tempo of 90 and 3/4 time.
fn foreign_file() -> Vec<u8> {
    let mut track = vec![
        0x00, 0xFF, 0x03, 0x04, b'L', b'e', b'a', b'd', // name
        0x00, 0xFF, 0x51, 0x03, 0x0A, 0x2C, 0x2B, // 666667 µs: 90 BPM
        0x00, 0xFF, 0x58, 0x04, 0x03, 0x02, 0x18, 0x08, // 3/4
        0x00, 0xFF, 0x59, 0x02, 0x01, 0x00, // G major
        0x00, 0xC0, 0x51, // program 81: a saw lead
        0x00, 0x90, 0x3C, 0x64, // C4 on
        0x83, 0x60, 0x3C, 0x00, // 480 later: off (running status, velocity 0)
        0x00, 0x40, 0x50, // E4 on (running status)
        0x00, 0x80, 0x3E, 0x40, // D4 off, never started
        0x83, 0x60, 0x80, 0x40, 0x40, // 480 later: E4 off
        0x00, 0x90, 0x43, 0x7F, // G4 on, never ended
        0x87, 0x40, 0xFF, 0x2F, 0x00, // 960 later: end of track
    ];
    let mut bytes = b"MThd\0\0\0\x06\0\x00\0\x01\x01\xE0MTrk".to_vec();
    bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
    bytes.append(&mut track);
    bytes
}

#[test]
fn files_from_other_programs_are_read() {
    let imported = midi::import(&foreign_file()).unwrap();
    assert!((imported.tempo.initial_bpm() - 90.0).abs() < 0.01);
    assert_eq!(imported.meter.initial(), TimeSignature::new(3, 4).unwrap());
    assert_eq!(
        imported.key.at_bar(0),
        Some(Scale::new(7, ScaleKind::Major))
    );
    assert_eq!(imported.tracks.len(), 1);
    let track = &imported.tracks[0];
    assert_eq!(track.name.as_deref(), Some("Lead"));
    assert_eq!(
        track.instrument.synth(),
        Some(&SynthPreset::Lead.settings()),
        "a General MIDI lead plays on the synth"
    );
    let notes: Vec<(Ticks, Ticks, u8)> = track
        .notes
        .iter()
        .map(|n| (n.start, n.length, n.key))
        .collect();
    assert_eq!(
        notes,
        [
            (Ticks::ZERO, Ticks::QUARTER, 60),
            (Ticks::QUARTER, Ticks::QUARTER, 64),
            (Ticks::QUARTER * 2, Ticks::QUARTER * 2, 67),
        ],
        "the held note ends with its track; the stray end is ignored"
    );
    assert!((track.notes[0].velocity - 100.0 / 127.0).abs() < 1e-6);
}

#[test]
fn imported_notes_land_where_asked_in_whole_bars() {
    let mut project = Project::starter("P", "Piano");
    let tempo = project.tempo.clone();
    let imported = midi::import(&foreign_file()).unwrap();
    let at = Ticks::WHOLE * 2 + Ticks::QUARTER;
    let added = imported.add_to(&mut project, at, name, "Imported");
    assert_eq!(added.len(), 1);
    assert_eq!(project.tempo, tempo, "the song keeps its tempo");
    let track = project.track(added[0]).unwrap();
    let clip = track.clips[0];
    assert_eq!(clip.start, Ticks::WHOLE * 2, "the clip starts on the bar");
    assert_eq!(
        clip.length,
        Ticks::WHOLE * 2,
        "two whole bars hold the notes"
    );
    let notes = played(&project, "Lead");
    assert_eq!(notes[0].0, at, "the first note is where it was dropped");
    assert_eq!(notes.len(), 3);
}

#[test]
fn time_code_files_are_read_at_a_steady_tempo() {
    // 25 frames per second, 40 ticks per frame: 1000 ticks per second.
    let track = [
        0x00, 0x90, 0x45, 0x50, // A4 on
        0x87, 0x68, 0x80, 0x45, 0x00, // 1000 later (one second): off
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let mut bytes = b"MThd\0\0\0\x06\0\x00\0\x01\xE7\x28MTrk".to_vec();
    bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&track);
    let file = midi::read(&bytes).unwrap();
    assert_eq!(
        file.timing,
        Timing::Smpte {
            fps: 25,
            per_frame: 40
        }
    );
    let imported = midi::import(&bytes).unwrap();
    assert_eq!(imported.tempo.initial_bpm(), 120.0);
    assert_eq!(
        imported.tracks[0].notes[0].length,
        Ticks::QUARTER * 2,
        "one second at 120 BPM"
    );
}

#[test]
fn every_event_vlabs_writes_reads_back() {
    let file = midi::song_to_midi(&song());
    let conductor = &file.tracks[0].events;
    assert!(
        conductor
            .iter()
            .any(|e| matches!(e.kind, EventKind::TrackName(_)))
    );
    assert!(
        conductor.iter().any(|e| e.kind
            == EventKind::KeySignature {
                sharps: 0,
                minor: true
            }),
        "A minor has no sharps"
    );
    assert!(conductor.iter().any(|e| e.kind
        == EventKind::TimeSignature {
            numerator: 7,
            denominator_power: 3
        }));
    let drums = file
        .tracks
        .iter()
        .find(|t| t.events.first().map(|e| &e.kind) == Some(&EventKind::TrackName("Drums".into())))
        .unwrap();
    assert!(
        drums
            .events
            .iter()
            .all(|e| !matches!(e.kind, EventKind::NoteOn { channel, .. } if channel != 9)),
        "drums on channel 10"
    );
}

#[test]
fn hostile_files_are_refused() {
    assert!(midi::read(b"").is_err());
    assert!(midi::read(b"RIFF....RMIDdata").is_err());
    // A track that claims to be huge.
    let mut bytes = b"MThd\0\0\0\x06\0\x00\0\x01\x01\xE0MTrk\xFF\xFF\xFF\xFF".to_vec();
    bytes.extend_from_slice(&[0x00, 0x90, 0x3C, 0x40]);
    let _ = midi::read(&bytes);
    // A length that never ends.
    let mut bytes = b"MThd\0\0\0\x06\0\x00\0\x01\x01\xE0MTrk\0\0\0\x08".to_vec();
    bytes.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x90, 0x3C, 0x40]);
    assert!(midi::read(&bytes).is_err());
    // Random bytes after a valid header.
    let mut noise = b"MThd\0\0\0\x06\0\x01\0\x03\x00\x60".to_vec();
    let mut seed = 12_345_u32;
    for _ in 0..4096 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        noise.push(seed as u8);
    }
    let _ = midi::import(&noise);
}

#[test]
fn a_voice_s_lyrics_go_out_and_come_back_on_their_notes() {
    let mut project = Project::empty();
    let voice = project.add_track(
        "Uta",
        Instrument::Voice {
            voicebank: "Uta".to_owned(),
        },
    );
    let verse = project.add_pattern_for(voice, "Verse", Ticks::WHOLE);
    let ids = project.add_notes(
        verse,
        &[
            note(Ticks::ZERO, Ticks::QUARTER, 69),
            note(Ticks::QUARTER, Ticks::QUARTER, 71),
            note(Ticks::QUARTER * 2, Ticks::QUARTER, 72),
        ],
    );
    // Two lyrics of their own (one in kana); the last note sings "a".
    project.set_lyrics(
        verse,
        &[(ids[0], "ka".to_owned()), (ids[1], "さ".to_owned())],
    );
    project.add_clip(voice, Ticks::ZERO, Ticks::WHOLE * 2, verse);
    let file = midi::song_to_midi(&project);
    let track = &file.tracks[1];
    let lyrics: Vec<(u64, &str)> = track
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Lyric(lyric) => Some((e.tick, lyric.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        lyrics,
        [(0, "ka"), (960, "さ"), (3840, "ka"), (4800, "さ")],
        "on each repeat, just before their notes"
    );
    let bytes = midi::write(&file);
    let song = midi::import(&bytes).unwrap();
    let back = song.into_project(name, "Pattern 1");
    let pattern = back.patterns.iter().find(|p| !p.notes.is_empty()).unwrap();
    let read_back: Vec<Option<&str>> = pattern.notes.iter().map(|n| pattern.lyric(n.id)).collect();
    assert_eq!(
        read_back,
        [Some("ka"), Some("さ"), None, Some("ka"), Some("さ"), None]
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Voicebanks end to end (decision 0027), with one the test makes: two
//! recordings of synthetic vowels at 220 Hz, an `oto.ini` in Shift-JIS
//! naming VCV and CV syllables, a `character.txt` and a readme. It is
//! added from a zip whose file names are Shift-JIS, its terms are read,
//! its syllables are found from lyrics, and it sings a phrase at the notes'
//! pitches.

#![allow(
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test code"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_dsp::voice::{SungNote, VoiceSample, key_hz, sing_phrase};
use daw_formats::utau;
use daw_model::lyrics::alias_candidates;

const RATE: u32 = 22_050;

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("voicebanks-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// A vowel at `hz` for `seconds`, after `silence` seconds of silence.
fn vowel(hz: f64, silence: f64, seconds: f64) -> Vec<f32> {
    let lead = (silence * f64::from(RATE)) as usize;
    let length = (seconds * f64::from(RATE)) as usize;
    let mut out = vec![0.0; lead];
    out.extend((0..length).map(|i| {
        let t = i as f64 / f64::from(RATE);
        (0.3 * (std::f64::consts::TAU * hz * t).sin()
            + 0.15 * (std::f64::consts::TAU * 2.0 * hz * t).sin()
            + 0.08 * (std::f64::consts::TAU * 3.0 * hz * t).sin()) as f32
    }));
    out
}

fn write_wav(path: &Path, samples: &[f32]) {
    let mut writer = daw_formats::wav::WavWriter::create_with_channels(
        path,
        daw_formats::wav::WavFormat::Pcm16,
        RATE,
        1,
    )
    .unwrap();
    writer.write_interleaved(samples).unwrap();
    writer.finish().unwrap();
}

fn shift_jis(text: &str) -> Vec<u8> {
    encoding_rs::SHIFT_JIS.encode(text).0.into_owned()
}

/// The test's voicebank, as files: (name in the voicebank, contents).
fn voicebank_files(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let a = folder.join("a.wav");
    write_wav(&a, &vowel(220.0, 0.1, 1.0));
    let ka = folder.join("ka.wav");
    write_wav(&ka, &vowel(220.0, 0.05, 1.0));
    let oto = "a.wav=- あ,100,20,-900,30,10\n\
               a.wav=あ,100,20,-900,30,10\n\
               ka.wav=a か,50,40,-900,60,20\n\
               ka.wav=か,50,40,-900,60,20\n";
    vec![
        ("テト/あ.wav".to_owned(), std::fs::read(&a).unwrap()),
        ("テト/か.wav".to_owned(), std::fs::read(&ka).unwrap()),
        (
            "テト/oto.ini".to_owned(),
            shift_jis(&oto.replace("ka.wav", "か.wav").replace("a.wav", "あ.wav")),
        ),
        (
            "テト/character.txt".to_owned(),
            shift_jis("name=テスト音源\nauthor=Tester\nimage=missing.bmp\n"),
        ),
        (
            "テト/readme.txt".to_owned(),
            shift_jis("利用規約: free for tests.\n"),
        ),
    ]
}

/// CRC-32, as zip files check their contents.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A zip of the files, stored, with their names as Shift-JIS bytes and
/// without the UTF-8 flag, as Japanese systems write them.
fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) {
    let mut out: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    let u16le = |v: usize| u16::try_from(v).unwrap().to_le_bytes();
    let u32le = |v: usize| u32::try_from(v).unwrap().to_le_bytes();
    for (name, bytes) in files {
        let name = shift_jis(name);
        let crc = crc32(bytes).to_le_bytes();
        let offset = out.len();
        out.extend(0x0403_4b50_u32.to_le_bytes());
        out.extend(u16le(20));
        out.extend([0, 0, 0, 0, 0, 0, 0, 0]); // flags, stored, time, date
        out.extend(crc);
        out.extend(u32le(bytes.len()));
        out.extend(u32le(bytes.len()));
        out.extend(u16le(name.len()));
        out.extend(u16le(0));
        out.extend(&name);
        out.extend(bytes);
        directory.extend(0x0201_4b50_u32.to_le_bytes());
        directory.extend(u16le(20));
        directory.extend(u16le(20));
        directory.extend([0, 0, 0, 0, 0, 0, 0, 0]);
        directory.extend(crc);
        directory.extend(u32le(bytes.len()));
        directory.extend(u32le(bytes.len()));
        directory.extend(u16le(name.len()));
        directory.extend([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // extra, comment, disk, attributes
        directory.extend(u32le(offset));
        directory.extend(&name);
    }
    let directory_at = out.len();
    out.extend(&directory);
    out.extend(0x0605_4b50_u32.to_le_bytes());
    out.extend([0, 0, 0, 0]);
    out.extend(u16le(files.len()));
    out.extend(u16le(files.len()));
    out.extend(u32le(directory.len()));
    out.extend(u32le(directory_at));
    out.extend(u16le(0));
    std::fs::write(path, out).unwrap();
}

#[test]
fn a_voicebank_is_added_read_and_sings() {
    let folder = scratch("sing");
    let made = folder.join("made");
    std::fs::create_dir_all(&made).unwrap();
    let files = voicebank_files(&made);
    // Laid out as a folder (the zip below covers archives).
    let source = folder.join("source");
    for (name, bytes) in &files {
        let path = source.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let root = folder.join("voicebanks");
    let info = utau::install(&source, &root).unwrap();
    assert_eq!(
        info.name, "テスト音源",
        "its name from character.txt, in Shift-JIS"
    );
    assert_eq!(info.author.as_deref(), Some("Tester"));
    assert_eq!(info.image, None, "a picture that is missing is not shown");
    assert!(
        info.terms.contains("利用規約"),
        "its terms: {:?}",
        info.terms
    );
    assert_eq!(info.syllables, 4);
    assert_eq!(utau::installed(&root), std::slice::from_ref(&info));

    // Lyrics find their syllables: VCV after a vowel, "- あ" at the start.
    let bank = utau::read_voicebank(&info.folder).unwrap();
    let first = bank.find(&alias_candidates("a", None), 60).unwrap();
    assert_eq!(first.alias, "- あ");
    let second = bank.find(&alias_candidates("ka", Some("a")), 64).unwrap();
    assert_eq!(second.alias, "a か");
    assert!(bank.find(&alias_candidates("su", Some("a")), 64).is_none());

    // It sings them at the notes' pitches.
    let sample: Arc<VoiceSample> = bank.sample(&first.file).unwrap();
    let notes = [
        SungNote::new(Arc::clone(&sample), first, 1.0, 0.5, 60.0),
        SungNote::new(bank.sample(&second.file).unwrap(), second, 1.5, 0.5, 64.0),
    ];
    let phrase = sing_phrase(&notes, 48_000);
    let sung = VoiceSample::new(48_000, phrase.samples.clone());
    for (start, key) in [(1.0, 60.0), (1.5, 64.0)] {
        let from = start + 0.15 - phrase.start;
        let pitch = sung.pitch_between(from, from + 0.25).unwrap();
        assert!(
            (pitch / key_hz(key) - 1.0).abs() < 0.02,
            "{pitch} Hz for {}",
            key_hz(key)
        );
    }

    // Removed, it is gone; a folder elsewhere is refused.
    utau::remove(&info.folder, &root).unwrap();
    assert_eq!(utau::installed(&root).len(), 0);
    assert!(utau::remove(&source, &root).is_err());
    std::fs::remove_dir_all(folder).ok();
}

#[test]
fn a_zipped_voicebank_with_japanese_names_is_added() {
    let folder = scratch("zip");
    let made = folder.join("made");
    std::fs::create_dir_all(&made).unwrap();
    let files = voicebank_files(&made);
    let archive = folder.join("voicebank.zip");
    write_zip(&archive, &files);
    let root = folder.join("voicebanks");
    let info = utau::install(&archive, &root).unwrap();
    assert_eq!(info.name, "テスト音源");
    assert_eq!(info.syllables, 4);
    // Its recordings are found by the names oto.ini gives them.
    let bank = utau::read_voicebank(&info.folder).unwrap();
    let entry = bank.find(&alias_candidates("ka", Some("a")), 60).unwrap();
    assert!(
        bank.sample(&entry.file).is_ok(),
        "{} was unpacked",
        entry.file
    );
    // Something that is not a voicebank is refused, and leaves nothing.
    let other = folder.join("other.zip");
    write_zip(&other, &[("readme.txt".to_owned(), b"hello".to_vec())]);
    assert!(utau::install(&other, &root).is_err());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(folder).ok();
}

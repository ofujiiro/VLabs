// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds the bundled acoustic drum kit from the CC0 "Big Rusty Drums"
//! library by Karoryfer Samples
//! (`github.com/sfzinstruments/karoryfer.big-rusty-drums`).
//!
//! ```text
//! make-drums files                  the sample files the kit needs
//! make-drums build <library> <out>  build the kit bundle
//! make-drums verify <bundle>        decode the bundle and check it
//! ```
//!
//! For each pad, two or three velocity layers (one round-robin each) are
//! mixed from the library's microphones with the balance of the library's
//! own "basic" preset: close microphones placed in the stereo field, the
//! overhead pair as recorded. Silence before the hit is trimmed, long tails
//! are cut with a fade, one gain for the whole kit brings its loudest sound
//! to -1 dBFS (keeping the balance between pads and layers), and each sound
//! is stored as 16-bit FLAC.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a command-line tool that reports what it does"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use daw_dsp::drums::{PadSound, read_bundle, write_bundle};
use flacenc::component::BitRepr;
use flacenc::error::Verify;

/// A microphone's contribution to a pad's sound.
struct Mic {
    /// The microphone folder.
    folder: &'static str,
    /// Linear gain (the preset's control value divided by 127).
    gain: f32,
    /// For mono close microphones, the position from -1 (left) to 1.
    pan: f32,
}

/// One pad of the kit.
struct PadSpec {
    sound: PadSound,
    /// The articulation folder under `Samples/`.
    folder: &'static str,
    /// The start of each file name, before `_vl`.
    prefix: &'static str,
    mics: &'static [Mic],
    /// (velocity layer in the library, highest velocity it plays here).
    layers: &'static [(u32, u8)],
    /// Longer sounds are cut, with a fade, at this length.
    max_seconds: f32,
}

const fn mic(folder: &'static str, control: u8, pan: f32) -> Mic {
    Mic {
        folder,
        gain: control as f32 / 127.0,
        pan,
    }
}

/// The kit. Microphone levels and positions follow the library's
/// `Programs/02-basic.sfz` (drummer's point of view).
const KIT: &[PadSpec] = &[
    PadSpec {
        sound: PadSound::Kick,
        folder: "kick_24/kick",
        prefix: "k",
        mics: &[mic("kick", 100, 0.0)],
        layers: &[(4, 50), (9, 100), (14, 127)],
        max_seconds: 1.6,
    },
    PadSpec {
        sound: PadSound::Snare,
        folder: "snare_14/center",
        prefix: "sn_center",
        mics: &[
            mic("btm", 100, 0.0),
            mic("top", 40, 0.0),
            mic("oh", 60, 0.0),
        ],
        layers: &[(3, 50), (6, 100), (10, 127)],
        max_seconds: 1.6,
    },
    PadSpec {
        sound: PadSound::Rimshot,
        folder: "snare_14/rimshot",
        prefix: "sn_rims",
        mics: &[
            mic("btm", 100, 0.0),
            mic("top", 40, 0.0),
            mic("oh", 60, 0.0),
        ],
        layers: &[(3, 80), (6, 127)],
        max_seconds: 1.6,
    },
    PadSpec {
        sound: PadSound::SideStick,
        folder: "snare_14/sidestick",
        prefix: "sn_ss",
        mics: &[
            mic("btm", 100, 0.0),
            mic("top", 40, 0.0),
            mic("oh", 60, 0.0),
        ],
        layers: &[(2, 80), (4, 127)],
        max_seconds: 0.8,
    },
    PadSpec {
        sound: PadSound::ClosedHat,
        folder: "hihat_14/tc",
        prefix: "ht_tc",
        mics: &[mic("cl", 100, 0.0), mic("oh", 100, 0.0)],
        layers: &[(3, 50), (5, 100), (8, 127)],
        max_seconds: 0.8,
    },
    PadSpec {
        sound: PadSound::PedalHat,
        folder: "hihat_14/chik",
        prefix: "ht_chik",
        mics: &[mic("cl", 100, 0.0), mic("oh", 100, 0.0)],
        layers: &[(3, 80), (5, 127)],
        max_seconds: 0.8,
    },
    PadSpec {
        sound: PadSound::OpenHat,
        folder: "hihat_14/open",
        prefix: "ht_open",
        mics: &[mic("cl", 100, 0.0), mic("oh", 100, 0.0)],
        layers: &[(3, 80), (6, 127)],
        max_seconds: 2.5,
    },
    PadSpec {
        sound: PadSound::LowTom,
        folder: "tom_18/center",
        prefix: "t18",
        mics: &[mic("cl", 100, 0.37), mic("oh", 70, 0.0)],
        layers: &[(4, 80), (8, 127)],
        max_seconds: 2.2,
    },
    PadSpec {
        sound: PadSound::MidTom,
        folder: "tom_15/center",
        prefix: "t15",
        mics: &[mic("cl", 100, -0.37), mic("oh", 70, 0.0)],
        layers: &[(4, 80), (7, 127)],
        max_seconds: 2.0,
    },
    PadSpec {
        sound: PadSound::HighTom,
        folder: "tom_14/center",
        prefix: "t14",
        mics: &[mic("cl", 100, -0.69), mic("oh", 70, 0.0)],
        layers: &[(3, 80), (6, 127)],
        max_seconds: 1.8,
    },
    PadSpec {
        sound: PadSound::Crash,
        folder: "crash_17/cr",
        prefix: "cr",
        mics: &[mic("cl", 40, -0.53), mic("oh", 100, 0.0)],
        layers: &[(3, 80), (5, 127)],
        max_seconds: 4.0,
    },
    PadSpec {
        sound: PadSound::Ride,
        folder: "ride_22/rd",
        prefix: "rd",
        mics: &[mic("cl", 40, 0.53), mic("oh", 100, 0.0)],
        layers: &[(5, 80), (10, 127)],
        max_seconds: 3.5,
    },
    PadSpec {
        sound: PadSound::RideBell,
        folder: "ride_22/bl",
        prefix: "rd_bl",
        mics: &[mic("cl", 40, 0.53), mic("oh", 100, 0.0)],
        layers: &[(2, 80), (4, 127)],
        max_seconds: 3.0,
    },
];

/// The library's sample rate.
const RATE: u32 = 44_100;
/// The loudest sound of the kit peaks here.
const KIT_PEAK_DB: f32 = -1.0;
/// Sounds start this long before the first audible sample.
const PRE_ROLL: f32 = 0.0005;
/// Silence is anything this far below the sound's peak.
const SILENCE_DB: f32 = -60.0;

fn file_for(spec: &PadSpec, mic: &Mic, layer: u32) -> PathBuf {
    Path::new("Samples")
        .join(spec.folder)
        .join(mic.folder)
        .join(format!("{}_vl{layer}_rr1.flac", spec.prefix))
}

/// A stereo sound being prepared.
#[derive(Clone)]
struct Stereo {
    left: Vec<f32>,
    right: Vec<f32>,
}

fn read_flac(path: &Path) -> Result<(Vec<f32>, Vec<f32>, usize), String> {
    let mut reader =
        claxon::FlacReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let info = reader.streaminfo();
    if info.sample_rate != RATE {
        return Err(format!(
            "{}: {} Hz, expected {RATE}",
            path.display(),
            info.sample_rate
        ));
    }
    let channels = info.channels as usize;
    let scale = 1.0 / (1_u64 << (info.bits_per_sample - 1)) as f32;
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (index, sample) in reader.samples().enumerate() {
        let value = sample.map_err(|e| format!("{}: {e}", path.display()))? as f32 * scale;
        if channels == 1 || index % 2 == 0 {
            left.push(value);
        } else {
            right.push(value);
        }
    }
    Ok((left, right, channels))
}

/// Mixes a layer from its microphones.
fn mix_layer(library: &Path, spec: &PadSpec, layer: u32) -> Result<Stereo, String> {
    let mut out = Stereo {
        left: Vec::new(),
        right: Vec::new(),
    };
    for mic in spec.mics {
        let (left, right, channels) = read_flac(&library.join(file_for(spec, mic, layer)))?;
        let length = left.len();
        if out.left.len() < length {
            out.left.resize(length, 0.0);
            out.right.resize(length, 0.0);
        }
        if channels == 1 {
            // Constant-power placement of a mono close microphone.
            let angle = (mic.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
            let (gain_left, gain_right) = (
                angle.cos() * std::f32::consts::SQRT_2,
                angle.sin() * std::f32::consts::SQRT_2,
            );
            for (i, s) in left.iter().enumerate() {
                out.left[i] += s * mic.gain * gain_left;
                out.right[i] += s * mic.gain * gain_right;
            }
        } else {
            for i in 0..length {
                out.left[i] += left[i] * mic.gain;
                out.right[i] += right.get(i).copied().unwrap_or(0.0) * mic.gain;
            }
        }
    }
    Ok(out)
}

fn peak(sound: &Stereo) -> f32 {
    sound
        .left
        .iter()
        .chain(&sound.right)
        .fold(0.0, |m, s| m.max(s.abs()))
}

/// Trims silence before the hit and after the tail, cuts long tails with a
/// fade, and returns the sound.
fn trim(mut sound: Stereo, max_seconds: f32) -> Stereo {
    let loudest = peak(&sound).max(1e-9);
    let silence = loudest * 10.0_f32.powf(SILENCE_DB / 20.0);
    let audible = |i: usize| sound.left[i].abs().max(sound.right[i].abs()) > silence;
    let length = sound.left.len();
    let first = (0..length).find(|&i| audible(i)).unwrap_or(0);
    let last = (0..length)
        .rev()
        .find(|&i| audible(i))
        .unwrap_or(length - 1);
    let start = first.saturating_sub((PRE_ROLL * RATE as f32) as usize);
    let max_frames = (max_seconds * RATE as f32) as usize;
    let end = (last + 1).min(start + max_frames);
    sound.left = sound.left[start..end].to_vec();
    sound.right = sound.right[start..end].to_vec();
    // Fade the end: over the last quarter when cut short, else 20 ms.
    let cut = last + 1 > start + max_frames;
    let frames = sound.left.len();
    let fade = if cut {
        frames / 4
    } else {
        ((0.020 * RATE as f32) as usize).min(frames)
    };
    for i in 0..fade {
        let gain = (i as f32 / fade as f32).powi(2);
        sound.left[frames - 1 - i] *= gain;
        sound.right[frames - 1 - i] *= gain;
    }
    sound
}

/// Whether the two channels are the same (store the sound as mono).
fn is_mono(sound: &Stereo) -> bool {
    sound
        .left
        .iter()
        .zip(&sound.right)
        .all(|(l, r)| (l - r).abs() < 1e-6)
}

fn encode(sound: &Stereo) -> Result<Vec<u8>, String> {
    let mono = is_mono(sound);
    let to_i32 = |s: f32| (s * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i32;
    let samples: Vec<i32> = if mono {
        sound.left.iter().map(|&s| to_i32(s)).collect()
    } else {
        sound
            .left
            .iter()
            .zip(&sound.right)
            .flat_map(|(&l, &r)| [to_i32(l), to_i32(r)])
            .collect()
    };
    let channels = if mono { 1 } else { 2 };
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| format!("FLAC settings: {e:?}"))?;
    let source = flacenc::source::MemSource::from_samples(&samples, channels, 16, RATE as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| format!("FLAC encoding: {e:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| format!("FLAC writing: {e:?}"))?;
    Ok(sink.as_slice().to_vec())
}

fn files() {
    for spec in KIT {
        for mic in spec.mics {
            for (layer, _) in spec.layers {
                println!("{}", file_for(spec, mic, *layer).display());
            }
        }
    }
}

fn build(library: &Path, out: &Path) -> Result<(), String> {
    let mut sounds = Vec::new();
    for spec in KIT {
        for &(layer, top) in spec.layers {
            let mixed = mix_layer(library, spec, layer)?;
            sounds.push((spec, top, trim(mixed, spec.max_seconds)));
        }
    }
    let loudest = sounds
        .iter()
        .map(|(_, _, s)| peak(s))
        .fold(0.0, f32::max)
        .max(1e-9);
    let gain = 10.0_f32.powf(KIT_PEAK_DB / 20.0) / loudest;
    println!("kit gain {:+.1} dB", 20.0 * gain.log10());
    let mut entries = Vec::new();
    for (spec, top, mut sound) in sounds {
        sound.left.iter_mut().for_each(|s| *s *= gain);
        sound.right.iter_mut().for_each(|s| *s *= gain);
        let flac = encode(&sound)?;
        println!(
            "{:<10} up to {top:>3}: {:.2} s, peak {:>6.1} dB, {:>4} KB{}",
            format!("{:?}", spec.sound),
            sound.left.len() as f32 / RATE as f32,
            20.0 * peak(&sound).max(1e-9).log10(),
            flac.len() / 1024,
            if is_mono(&sound) { ", mono" } else { "" }
        );
        entries.push((spec.sound.key(), top, flac));
    }
    let bundle = write_bundle(&entries)?;
    std::fs::write(out, &bundle).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("wrote {} ({} KB)", out.display(), bundle.len() / 1024);
    Ok(())
}

/// Decodes the bundle and checks every sound: it decodes, starts at once,
/// does not clip, ends in silence, and the layers of a pad get louder.
fn verify(bundle: &Path) -> Result<(), String> {
    let bytes = std::fs::read(bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
    let entries = read_bundle(&bytes)?;
    let mut problems = Vec::new();
    let mut previous: Option<(u8, f32)> = None;
    for entry in &entries {
        let sample = &entry.sample;
        let name = format!("{:?} up to {}", PadSound::from_key(entry.key), entry.top);
        let loudest = sample.peak();
        let onset = sample
            .left
            .iter()
            .zip(sample.right_or_left())
            .position(|(l, r)| l.abs().max(r.abs()) > loudest * 0.1)
            .unwrap_or(0);
        let tail = sample.left.len().saturating_sub(64);
        let end_level = sample.left[tail..]
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()));
        println!(
            "{name:<24} {:.2} s, peak {:.3}, loud after {:.1} ms",
            sample.len() as f32 / sample.sample_rate as f32,
            loudest,
            onset as f32 * 1000.0 / sample.sample_rate as f32
        );
        if sample.sample_rate != RATE || sample.is_empty() {
            problems.push(format!("{name}: bad format"));
        }
        if loudest >= 0.999 {
            problems.push(format!("{name}: clips"));
        }
        if onset as f32 > 0.005 * RATE as f32 {
            problems.push(format!("{name}: starts {onset} frames late"));
        }
        if end_level > loudest * 0.01 {
            problems.push(format!("{name}: does not end in silence"));
        }
        if let Some((key, softer)) = previous
            && key == entry.key
            && loudest <= softer
        {
            problems.push(format!("{name}: not louder than the layer below"));
        }
        previous = Some((entry.key, loudest));
    }
    let pads: std::collections::BTreeSet<u8> = entries.iter().map(|e| e.key).collect();
    if pads.len() != KIT.len() {
        problems.push(format!("{} pads, expected {}", pads.len(), KIT.len()));
    }
    if problems.is_empty() {
        println!("{} sounds, {} pads: all good", entries.len(), pads.len());
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["files"] => {
            files();
            Ok(())
        }
        ["build", library, out] => build(Path::new(library), Path::new(out)),
        ["verify", bundle] => verify(Path::new(bundle)),
        _ => Err("usage: make-drums files | build <library> <out> | verify <bundle>".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(problem) => {
            eprintln!("{problem}");
            ExitCode::FAILURE
        }
    }
}

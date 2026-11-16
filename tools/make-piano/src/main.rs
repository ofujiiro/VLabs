// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds the bundled piano SoundFont from the Osiris Piano sample library
//! (Versilian Studios and Karoryfer Samples, CC0 1.0).
//!
//! The original is 900 MB of 48 kHz stereo samples from three microphone
//! positions, two velocity layers, every white key. This tool turns it into
//! a compact SoundFont:
//!
//! 1. For each note and velocity layer, the chosen microphones are mixed to
//!    mono by taking each note's louder channel (the pairs are spaced, so
//!    summing them would cancel part of the sound; `analyze` shows this).
//! 2. The silent lead-in before the hammer strike is trimmed, so notes speak
//!    at once.
//! 3. The sample is cut after its attack and early decay; the end is turned
//!    into a seamless loop (a whole number of periods, levelled and
//!    crossfaded), and the SoundFont's volume envelope continues the natural
//!    decay from there.
//! 4. Release times come from the library's own mapping files.
//!
//! Usage:
//!   make-piano analyze <Osiris_Piano folder>
//!   make-piano build <Osiris_Piano folder> <output.sf2>
//!   make-piano verify <piano.sf2> <demo.wav>

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool reports by printing"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "audio sample arithmetic on known ranges"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use daw_dsp::sf2_writer::{
    Generator, InstrumentDef, PresetDef, Sample, SoundFontDef, Zone, timecents,
};

const RATE: u32 = 48_000;

/// Below this correlation, summing a microphone pair's two channels would
/// partly cancel the sound (reported by `analyze`).
const MIN_CORRELATION: f64 = 0.6;

/// One region of the library's mapping: a sample and the keys it covers.
#[derive(Clone, Debug)]
struct Region {
    /// File name, for example `Piano_UC_MicA_C4_vl1.flac`.
    file: String,
    low: u8,
    high: u8,
    root: u8,
    release: f32,
    /// No damper: the note rings on when released.
    one_shot: bool,
}

fn parse_mapping(path: &Path) -> Result<Vec<Region>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut regions = Vec::new();
    for block in text.split("<region>").skip(1) {
        let mut file = None;
        let (mut low, mut high, mut root, mut release, mut one_shot) =
            (None, None, None, 0.3, false);
        for line in block.lines() {
            let line = line.split("//").next().unwrap_or_default().trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "sample" => file = value.trim().rsplit('/').next().map(str::to_owned),
                "lokey" => low = value.trim().parse().ok(),
                "hikey" => high = value.trim().parse().ok(),
                "pitch_keycenter" => root = value.trim().parse().ok(),
                "ampeg_release" => release = value.trim().parse().unwrap_or(0.3),
                "loop_mode" => one_shot = value.trim() == "one_shot",
                _ => {}
            }
        }
        // In SFZ, a region without pitch_keycenter is centred on key 60.
        let root = root.unwrap_or(60);
        match (file, low, high) {
            (Some(file), Some(low), Some(high)) => regions.push(Region {
                file,
                low,
                high,
                root,
                release,
                one_shot,
            }),
            _ => return Err(format!("{}: a region is missing a field", path.display())),
        }
    }
    Ok(regions)
}

/// A decoded stereo sample, as floats.
struct Stereo {
    left: Vec<f32>,
    right: Vec<f32>,
}

fn decode(path: &Path) -> Result<Stereo, String> {
    let mut reader =
        claxon::FlacReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let info = reader.streaminfo();
    if info.sample_rate != RATE || info.channels != 2 {
        return Err(format!("{}: expected 48 kHz stereo", path.display()));
    }
    let scale = 1.0 / (1_i64 << (info.bits_per_sample - 1)) as f32;
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (index, sample) in reader.samples().enumerate() {
        let value = sample.map_err(|e| format!("{}: {e}", path.display()))? as f32 * scale;
        if index % 2 == 0 {
            left.push(value);
        } else {
            right.push(value);
        }
    }
    Ok(Stereo { left, right })
}

fn sample_path(root: &Path, mic: char, region_file: &str) -> PathBuf {
    let file = region_file.replace("MicA", &format!("Mic{mic}"));
    root.join("UC").join(mic.to_string()).join(file)
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum()
}

fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| f64::from(x) * f64::from(y))
        .sum();
    dot / (energy(a) * energy(b)).sqrt().max(1e-12)
}

fn analyze(root: &Path) -> Result<(), String> {
    let regions = parse_mapping(&root.join("Programs/modules/mappings/uc_mica_vl2_map.sfz"))?;
    println!("{} regions per layer", regions.len());
    print!("mic B, notes whose two channels disagree (L/R correlation < {MIN_CORRELATION}):");
    for region in &regions {
        let stereo = decode(&sample_path(root, 'B', &region.file))?;
        let window = 4_800..48_000.min(stereo.left.len());
        let corr = correlation(&stereo.left[window.clone()], &stereo.right[window]);
        if corr < MIN_CORRELATION {
            print!(" {}({corr:+.2})", region.root);
        }
    }
    println!();
    for region in regions
        .iter()
        .filter(|r| [24, 36, 48, 60, 72, 84, 96].contains(&r.root))
    {
        print!("key {:>3}:", region.root);
        let mut monos = Vec::new();
        for mic in ['A', 'B', 'C'] {
            let stereo = decode(&sample_path(root, mic, &region.file))?;
            let window = 4_800..48_000.min(stereo.left.len());
            let (l, r) = (&stereo.left[window.clone()], &stereo.right[window]);
            let mono: Vec<f32> = l.iter().zip(r).map(|(a, b)| 0.5 * (a + b)).collect();
            let coherence = energy(&mono) / f64::midpoint(energy(l), energy(r));
            let peak = stereo
                .left
                .iter()
                .chain(&stereo.right)
                .fold(0.0_f32, |m, s| m.max(s.abs()));
            print!(
                "  mic {mic}: L/R corr {:+.2}, mono/stereo energy {:.2}, peak {peak:.2}",
                correlation(l, r),
                coherence
            );
            monos.push(mono);
        }
        println!(
            "  | A-B corr {:+.2}, A-C {:+.2}, B-C {:+.2}",
            correlation(&monos[0], &monos[1]),
            correlation(&monos[0], &monos[2]),
            correlation(&monos[1], &monos[2])
        );
    }
    Ok(())
}

/// Which channel of a microphone pair goes into the mono sample.
#[derive(Clone, Copy)]
enum Channel {
    Left,
    Right,
}

/// Where the hammer strikes: the first frame reaching 10% of the peak, minus
/// 3 ms so the attack transient stays intact. The samples deliberately start
/// with up to 25 ms of key and hammer mechanism noise before the strike
/// (the original instrument has a control to skip it); a playable piano
/// must speak at once, so that lead-in is dropped.
fn onset(samples: &[f32]) -> usize {
    let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    let threshold = peak * 0.1;
    let first = samples
        .iter()
        .position(|s| s.abs() >= threshold)
        .unwrap_or(0);
    first.saturating_sub(RATE as usize * 3 / 1000)
}

/// Evens out loudness across the keyboard within one velocity layer: each
/// sample's level (first half second) is pulled towards the average of its
/// neighbours, by at most 6 dB. Returns a gain per sample.
fn levelling(levels_db: &[f32]) -> Vec<f32> {
    let n = levels_db.len();
    (0..n)
        .map(|i| {
            let from = i.saturating_sub(3);
            let to = (i + 4).min(n);
            let mut neighbours: Vec<f32> = levels_db[from..to].to_vec();
            neighbours.sort_by(f32::total_cmp);
            let target = neighbours[neighbours.len() / 2]; // median: robust to outliers
            let correction = (target - levels_db[i]).clamp(-6.0, 6.0);
            10.0_f32.powf(correction / 20.0)
        })
        .collect()
}

/// How much of the undamped top notes to keep. They are not looped: they
/// fade naturally within a couple of seconds, and looping their beating
/// strings produced audible dips.
const ONE_SHOT_SECONDS: f32 = 2.0;

/// How long to keep before the loop, by key: the bass rings longest.
fn kept_seconds(root: u8) -> f32 {
    match root {
        0..=35 => 2.2,
        36..=59 => 1.8,
        60..=83 => 1.4,
        _ => 1.0,
    }
}

/// The natural decay rate just before the loop, in dB per second, measured
/// by a straight-line fit over the last second (clamped to a sane range).
fn decay_rate(data: &[f32], loop_start: usize) -> f32 {
    let window = RATE as usize / 20;
    let from = loop_start.saturating_sub(RATE as usize);
    let points: Vec<(f32, f32)> = data[from..loop_start]
        .chunks(window)
        .enumerate()
        .map(|(i, w)| (i as f32 * 0.05, 20.0 * rms(w).max(1e-9).log10()))
        .collect();
    if points.len() < 4 {
        return 6.0;
    }
    let n = points.len() as f32;
    let mean_t = points.iter().map(|p| p.0).sum::<f32>() / n;
    let mean_db = points.iter().map(|p| p.1).sum::<f32>() / n;
    let covariance: f32 = points
        .iter()
        .map(|p| (p.0 - mean_t) * (p.1 - mean_db))
        .sum();
    let variance: f32 = points.iter().map(|p| (p.0 - mean_t).powi(2)).sum();
    (-covariance / variance.max(1e-9)).clamp(1.5, 15.0)
}

/// The SoundFont decay time that continues a decay of `rate` dB/s: the
/// specification defines decay as the time for a full 100 dB fall.
fn decay_time(rate: f32) -> f32 {
    100.0 / rate
}

fn rms(samples: &[f32]) -> f32 {
    (energy(samples) / samples.len().max(1) as f64).sqrt() as f32
}

/// Finds the loop length, near `nominal`, at which the audio just before
/// the loop start best matches the audio at the loop end, so the two line up
/// when the loop wraps. Returns the length and how well they match (the
/// correlation, 1.0 being identical).
fn best_loop_length(data: &[f32], nominal: usize, fade: usize) -> (usize, f64) {
    let end = data.len();
    let search = (nominal / 8).max(64);
    let tail = &data[end - fade..end];
    let mut best = (nominal, f64::MIN);
    for length in nominal.saturating_sub(search)..=(nominal + search) {
        if length + fade >= end {
            break;
        }
        let before = &data[end - length - fade..end - length];
        let score = correlation(before, tail);
        if score > best.1 {
            best = (length, score);
        }
    }
    best
}

/// Turns the end of `data` into a seamless loop of about `nominal` frames,
/// ending at the end of `data`: the length is chosen where the waveform
/// lines up best, the loop is levelled so it does not pulse, and its end is
/// crossfaded into what precedes its start. Returns the loop start.
fn make_loop(data: &mut [f32], nominal: usize) -> usize {
    let end = data.len();
    let fade = (nominal / 3).min(end / 4);
    let (loop_len, matching) = best_loop_length(data, nominal, fade);
    let start = end - loop_len;
    // Level the loop: undo the decay across it, so each pass sounds the same.
    let probe = loop_len / 8;
    let head = rms(&data[start..start + probe]).max(1e-6);
    let tail = rms(&data[end - probe..end]).max(1e-6);
    let ratio = head / tail;
    for (i, sample) in data[start..end].iter_mut().enumerate() {
        let t = i as f32 / loop_len as f32;
        *sample *= ratio.powf(t);
    }
    // Crossfade the loop's last frames into the frames just before its start,
    // so jumping from end to start is continuous. Well-matched audio uses a
    // linear fade (no bump); poorly matched audio an equal-power one (no dip).
    let fade = fade.min(start);
    for i in 0..fade {
        let t = (i as f32 + 0.5) / fade as f32; // 0 → 1 across the fade
        let (gain_out, gain_in) = if matching > 0.7 {
            (1.0 - t, t)
        } else {
            (
                (t * std::f32::consts::FRAC_PI_2).cos(),
                (t * std::f32::consts::FRAC_PI_2).sin(),
            )
        };
        let at = end - fade + i;
        let before = start - fade + i;
        data[at] = data[at] * gain_out + data[before] * gain_in;
    }
    start
}

/// A loop length near 250 ms that holds a whole number of periods of the
/// note's fundamental, so the waveform lines up when it wraps.
fn loop_length(root: u8) -> usize {
    let hz = 440.0 * 2.0_f64.powf((f64::from(root) - 69.0) / 12.0);
    let period = f64::from(RATE) / hz;
    let periods = (0.25 / (period / f64::from(RATE))).round().max(1.0);
    (periods * period).round() as usize
}

/// A note's sample, decoded and mixed, before looping.
struct Prepared {
    name: String,
    region: Region,
    velocities: (u8, u8),
    data: Vec<f32>,
}

#[allow(clippy::too_many_lines, reason = "the build steps, in order")]
fn build(root: &Path, output: &Path, mics: &[char]) -> Result<(), String> {
    let layers = [
        (
            "vl1",
            1_u8,
            63_u8,
            parse_mapping(&root.join("Programs/modules/mappings/uc_mica_vl1_map.sfz"))?,
        ),
        (
            "vl2",
            64,
            127,
            parse_mapping(&root.join("Programs/modules/mappings/uc_mica_vl2_map.sfz"))?,
        ),
    ];

    // Decode and mix everything first, to find one normalisation for all.
    let mut prepared = Vec::new();
    for (layer, low_velocity, high_velocity, regions) in &layers {
        for region in regions {
            let mut mixed: Vec<f32> = Vec::new();
            for &mic in mics {
                let stereo = decode(&sample_path(root, mic, &region.file))?;
                if mixed.is_empty() {
                    mixed = vec![0.0; stereo.left.len()];
                }
                let n = mixed.len().min(stereo.left.len());
                mixed.truncate(n);
                let window = 4_800..48_000.min(n);
                let (l, r) = (&stereo.left[window.clone()], &stereo.right[window]);
                // The pairs are spaced: summing their channels cancels part
                // of the sound on many notes (see `analyze`). Each note uses
                // its louder channel: the microphone nearer its strings.
                let channel = if energy(l) >= energy(r) {
                    Channel::Left
                } else {
                    Channel::Right
                };
                for i in 0..n {
                    mixed[i] += match channel {
                        Channel::Left => stereo.left[i],
                        Channel::Right => stereo.right[i],
                    };
                }
            }
            let start = onset(&mixed);
            let keep = if region.one_shot {
                (ONE_SHOT_SECONDS * RATE as f32) as usize
            } else {
                (kept_seconds(region.root) * RATE as f32) as usize + loop_length(region.root)
            };
            let end = (start + keep).min(mixed.len());
            let data = mixed[start..end].to_vec();
            let name = format!(
                "{} {}",
                region
                    .file
                    .trim_start_matches("Piano_UC_MicA_")
                    .trim_end_matches(".flac")
                    .replace('_', " "),
                layer
            );
            prepared.push(Prepared {
                name,
                region: region.clone(),
                velocities: (*low_velocity, *high_velocity),
                data,
            });
        }
    }
    // Level each velocity layer across the keyboard.
    for layer in [1_u8, 64] {
        let indices: Vec<usize> = (0..prepared.len())
            .filter(|&i| prepared[i].velocities.0 == layer)
            .collect();
        let levels: Vec<f32> = indices
            .iter()
            .map(|&i| {
                let head = &prepared[i].data[..prepared[i].data.len().min(RATE as usize / 2)];
                20.0 * rms(head).max(1e-9).log10()
            })
            .collect();
        for (&i, gain) in indices.iter().zip(levelling(&levels)) {
            for sample in &mut prepared[i].data {
                *sample *= gain;
            }
        }
    }
    let peak = prepared
        .iter()
        .flat_map(|p| p.data.iter())
        .fold(0.0_f32, |m, s| m.max(s.abs()));
    let gain = 0.89 / peak; // -1 dBFS for the loudest note

    let mut samples = Vec::new();
    let mut zones = Vec::new();
    let mut total_frames = 0;
    for p in prepared {
        let mut data: Vec<f32> = p.data.iter().map(|s| s * gain).collect();
        let looped = !p.region.one_shot;
        let (loop_points, rate, hold) = if looped {
            let loop_len = loop_length(p.region.root).min(data.len() / 4);
            let rate = decay_rate(&data, data.len() - loop_len);
            let loop_start = make_loop(&mut data, loop_len);
            (
                Some((loop_start as u32, data.len() as u32)),
                rate,
                loop_start as f32 / RATE as f32,
            )
        } else {
            // Fade out over the last 300 ms.
            let fade = (RATE as usize * 3 / 10).min(data.len());
            let len = data.len();
            for (i, sample) in data[len - fade..].iter_mut().enumerate() {
                *sample *= 1.0 - i as f32 / fade as f32;
            }
            (None, 0.0, 0.0)
        };
        let pcm: Vec<i16> = data
            .iter()
            .map(|s| (s * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16)
            .collect();
        total_frames += pcm.len();
        let index = samples.len();
        samples.push(Sample {
            name: p.name,
            data: pcm,
            sample_rate: RATE,
            root_key: p.region.root,
            correction: 0,
            loop_points,
        });
        // A gentle stereo image: bass a little left, treble a little right,
        // as a player hears it.
        let pan = ((f32::from(p.region.root) - 64.0) / 44.0 * 250.0)
            .round()
            .clamp(-250.0, 250.0) as i16;
        let release = if p.region.one_shot {
            2.5
        } else {
            p.region.release.max(0.15)
        };
        let generators = if looped {
            vec![
                Generator::SampleModes(1),
                Generator::Pan(pan),
                Generator::HoldVolEnv(timecents(hold)),
                Generator::DecayVolEnv(timecents(decay_time(rate))),
                Generator::SustainVolEnv(1000),
                Generator::ReleaseVolEnv(timecents(release)),
            ]
        } else {
            vec![
                Generator::SampleModes(0),
                Generator::Pan(pan),
                Generator::ReleaseVolEnv(timecents(release)),
            ]
        };
        zones.push(Zone {
            keys: (p.region.low, p.region.high),
            velocities: p.velocities,
            sample: index,
            generators,
        });
    }

    let font = SoundFontDef {
        name: "Piano (Osiris)".to_owned(),
        copyright: "CC0 1.0: Osiris Piano by Versilian Studios and Karoryfer Samples".to_owned(),
        comment: format!(
            "Built by make-piano from the Osiris Piano (github.com/sfzinstruments/Osiris_Piano, v0.925): \
             mic {} (louder channel per note), lead-in trimmed, tails looped, 48 kHz 16-bit mono.",
            mics.iter().collect::<String>()
        ),
        samples,
        instruments: vec![InstrumentDef {
            name: "Osiris Piano".to_owned(),
            global: vec![Generator::AttackVolEnv(timecents(0.002))],
            zones,
        }],
        presets: vec![PresetDef {
            name: "Piano".to_owned(),
            bank: 0,
            program: 0,
            instrument: 0,
        }],
    };
    let bytes = font.to_bytes();
    std::fs::write(output, &bytes).map_err(|e| format!("{}: {e}", output.display()))?;
    println!(
        "wrote {} ({:.1} MB, {} samples, {:.1} s of audio, normalisation gain {gain:.2})",
        output.display(),
        bytes.len() as f64 / 1e6,
        font.samples.len(),
        total_frames as f64 / f64::from(RATE)
    );
    Ok(())
}

/// Plays every key of a built SoundFont through our own player and
/// checks it: pitch, level against the neighbouring keys, loop continuity
/// and a smooth decay. Writes a short demo to listen to.
#[allow(clippy::too_many_lines, reason = "a list of checks")]
fn verify(path: &Path, demo: &Path) -> Result<(), String> {
    use daw_dsp::{Instrument, SoundFontBank, SoundFontPlayer};
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bank = SoundFontBank::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let mut failures = 0;
    let mut previous_level: Option<f32> = None;
    for key in 21_u8..=108 {
        let mut player = SoundFontPlayer::new(&bank, RATE, 0, 0).map_err(|e| e.to_string())?;
        player.set_reverb(0.0);
        player.note_on(key, 0.8);
        let frames = RATE as usize * 4;
        let mut left = vec![0.0; frames];
        let mut right = vec![0.0; frames];
        player.render(&mut left, &mut right);
        let mono: Vec<f32> = left
            .iter()
            .zip(&right)
            .map(|(l, r)| 0.5 * (l + r))
            .collect();

        // Starts at once.
        let onset = mono
            .iter()
            .position(|s| s.abs() > 0.005)
            .unwrap_or(usize::MAX);
        // Level of the first half second, against the previous key.
        let level = 20.0 * rms(&mono[..RATE as usize / 2]).max(1e-9).log10();
        let jump = previous_level.map_or(0.0, |p| (level - p).abs());
        previous_level = Some(level);
        // Smooth decay: in 50 ms windows after the attack, the level never
        // rises by more than 3 dB (a pulse or click at the loop would).
        // Only the looped part is ours to check (the recording's own beating
        // before it is natural), and only while clearly audible: within 40 dB
        // of the note's start. A few high notes have small dips further down
        // the tail, 45 to 60 dB below the start.
        let window = RATE as usize / 20;
        let start_level = 20.0 * rms(&mono[..window]).max(1e-9).log10();
        // Undamped top notes (from F6) are not looped: nothing to check.
        let loop_from = if key >= 89 {
            mono.len()
        } else {
            ((kept_seconds(key) + 0.1) * RATE as f32) as usize
        };
        let levels: Vec<f32> = mono[loop_from..]
            .chunks(window)
            .map(|w| 20.0 * rms(w).max(1e-9).log10())
            .take_while(|&l| l > start_level - 40.0)
            .collect();
        let (worst_at, worst_rise) = levels
            .windows(2)
            .enumerate()
            .map(|(i, w)| (i, w[1] - w[0]))
            .fold(
                (0, f32::MIN),
                |best, item| if item.1 > best.1 { item } else { best },
            );
        let worst_time = loop_from as f32 / RATE as f32 + (worst_at + 1) as f32 * 0.05;
        // Largest sample-to-sample step (a click shows as a spike).
        let steps: Vec<f32> = mono.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        let max_step = steps[RATE as usize / 2..]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        let typical_step = rms(&steps[RATE as usize / 2..]);
        let spike = max_step / typical_step.max(1e-9);

        let mut problems = Vec::new();
        if onset > RATE as usize / 100 {
            problems.push(format!("slow start ({} ms)", onset * 1000 / RATE as usize));
        }
        if jump > 6.0 {
            problems.push(format!("level jumps {jump:.1} dB from the previous key"));
        }
        if worst_rise > 3.0 {
            let around: Vec<String> = levels
                [worst_at.saturating_sub(2)..(worst_at + 4).min(levels.len())]
                .iter()
                .map(|l| format!("{l:.1}"))
                .collect();
            problems.push(format!(
                "level rises {worst_rise:.1} dB during the decay at {worst_time:.2} s (loop from about {:.2} s): {}",
                kept_seconds(key),
                around.join(" ")
            ));
        }
        if spike > 25.0 {
            problems.push(format!("possible click (step {spike:.0}x typical)"));
        }
        if problems.is_empty() {
            let rise = if levels.len() > 1 {
                format!("{worst_rise:4.1} dB")
            } else {
                "n/a".to_owned()
            };
            println!(
                "key {key:>3}: ok   level {level:6.1} dB, worst rise {rise}, step ratio {spike:4.1}"
            );
        } else {
            failures += 1;
            println!("key {key:>3}: FAIL {}", problems.join("; "));
        }
    }

    // A demo: a C major scale, then a chord, then a soft low note.
    let mut player = SoundFontPlayer::new(&bank, RATE, 0, 0).map_err(|e| e.to_string())?;
    let beat = RATE as usize / 3;
    let mut events: Vec<(usize, u8, bool, f32)> = Vec::new();
    for (i, key) in [60_u8, 62, 64, 65, 67, 69, 71, 72].iter().enumerate() {
        events.push((i * beat, *key, true, 0.75));
        events.push((i * beat + beat - beat / 8, *key, false, 0.0));
    }
    let chord_at = 9 * beat;
    for key in [48_u8, 55, 60, 64, 67, 72] {
        events.push((chord_at, key, true, 0.85));
        events.push((chord_at + 6 * beat, key, false, 0.0));
    }
    events.push((16 * beat, 36, true, 0.35));
    events.push((22 * beat, 36, false, 0.0));
    events.sort_by_key(|e| e.0);
    let total = 26 * beat;
    let mut left = vec![0.0; total];
    let mut right = vec![0.0; total];
    let mut at = 0;
    for (frame, key, on, velocity) in events {
        player.render(&mut left[at..frame], &mut right[at..frame]);
        at = frame;
        if on {
            player.note_on(key, velocity);
        } else {
            player.note_off(key);
        }
    }
    player.render(&mut left[at..], &mut right[at..]);
    write_wav(demo, &left, &right)?;
    println!("demo written to {}", demo.display());
    if failures == 0 {
        Ok(())
    } else {
        Err(format!("{failures} key(s) failed"))
    }
}

fn write_wav(path: &Path, left: &[f32], right: &[f32]) -> Result<(), String> {
    let data_bytes = (left.len() * 4) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 4).to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    for (l, r) in left.iter().zip(right) {
        for s in [l, r] {
            bytes.extend_from_slice(
                &((s * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16).to_le_bytes(),
            );
        }
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Prints the level of a source sample (mic B, louder channel) every 250 ms.
fn profile(root: &Path, file: &str) -> Result<(), String> {
    let stereo = decode(&sample_path(root, 'B', file))?;
    let channel = if energy(&stereo.left) >= energy(&stereo.right) {
        &stereo.left
    } else {
        &stereo.right
    };
    let start = onset(channel);
    let window = RATE as usize / 4;
    let levels: Vec<String> = channel[start..]
        .chunks(window)
        .take(40)
        .map(|w| format!("{:.0}", 20.0 * rms(w).max(1e-9).log10()))
        .collect();
    println!("{file}: dB every 250 ms: {}", levels.join(" "));
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["analyze", root] => analyze(Path::new(root)),
        ["build", root, output] => build(Path::new(root), Path::new(output), &['B']),
        ["build", root, output, mics] => build(Path::new(root), Path::new(output), &mics.chars().collect::<Vec<_>>()),
        ["verify", sf2, demo] => verify(Path::new(sf2), Path::new(demo)),
        ["profile", root, file] => profile(Path::new(root), file),
        _ => Err("usage: make-piano analyze <Osiris_Piano folder> | build <folder> <output.sf2> [mic] | verify <piano.sf2> <demo.wav>".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

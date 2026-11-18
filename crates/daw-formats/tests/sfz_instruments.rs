// SPDX-License-Identifier: GPL-3.0-or-later

//! Real SFZ instruments made by others (decision 0024): every `.sfz` file
//! in the folder `VLABS_TEST_SFZ` names is read with all its sounds, and
//! notes across its range are heard through the sampler. Without that
//! variable this test passes quietly (the instruments are test fixtures,
//! never part of the program: see CLAUDE.md).

#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "test code: it reports what it read"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_dsp::Instrument;
use daw_dsp::sampler::{Sampler, Trigger};

const RATE: u32 = 48_000;

fn instruments(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            instruments(&path, found);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("sfz"))
        {
            found.push(path);
        }
    }
}

/// The loudest sample of a note held for half a second, then let go for
/// one.
fn note_peak(sampler: &mut Sampler, key: u8) -> f32 {
    let (mut left, mut right) = (vec![0.0; 24_000], vec![0.0; 24_000]);
    sampler.note_on(key, 0.8);
    sampler.render(&mut left, &mut right);
    let mut peak = left
        .iter()
        .chain(&right)
        .fold(0.0_f32, |m, s| m.max(s.abs()));
    sampler.note_off(key);
    let (mut tail_left, mut tail_right) = (vec![0.0; 48_000], vec![0.0; 48_000]);
    sampler.render(&mut tail_left, &mut tail_right);
    for sample in tail_left.iter().chain(&tail_right) {
        assert!(
            sample.is_finite(),
            "key {key} gave something that is not a number"
        );
        peak = peak.max(sample.abs());
    }
    sampler.all_notes_off(true);
    sampler.render(&mut tail_left, &mut tail_right);
    peak
}

#[test]
fn real_instruments_are_read_and_heard() {
    let Some(folder) = std::env::var_os("VLABS_TEST_SFZ") else {
        return;
    };
    let mut found = Vec::new();
    instruments(Path::new(&folder), &mut found);
    found.sort();
    assert!(
        !found.is_empty(),
        "no .sfz files in {}",
        folder.to_string_lossy()
    );
    for path in found {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let started = std::time::Instant::now();
        let bank = daw_formats::sfz::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        println!(
            "{name}: {} regions, {} sounds, {:.1} MB, read in {:.2} s; left out {:?}",
            bank.regions.len(),
            bank.samples.len(),
            bank.sound_bytes() as f64 / 1e6,
            started.elapsed().as_secs_f64(),
            bank.left_out,
        );
        assert_eq!(
            bank.missing,
            Vec::<String>::new(),
            "{name}: sounds not found"
        );
        // The lowest, middle and highest keys a key going down plays.
        let mut keys: Vec<u8> = bank
            .regions
            .iter()
            .filter(|r| r.trigger != Trigger::Release && r.sample.is_some())
            .filter(|r| r.sw_down.is_none() && r.sw_previous.is_none())
            .flat_map(|r| [r.lokey, r.hikey])
            .collect();
        keys.sort_unstable();
        keys.dedup();
        let wanted = [keys[0], keys[keys.len() / 2], keys[keys.len() - 1]];
        let mut sampler = Sampler::new(Arc::new(bank), RATE);
        for key in wanted {
            let peak = note_peak(&mut sampler, key);
            println!("  key {key}: peak {peak:.3}");
            assert!(peak > 1e-3, "{name}: key {key} is silent");
            assert!(peak < 8.0, "{name}: key {key} is far too loud ({peak})");
        }
    }
}

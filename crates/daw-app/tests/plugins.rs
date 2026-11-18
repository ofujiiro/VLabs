// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding plugins the way the program does when it starts: each plugin file
//! is described by a separate run of the program, so a broken plugin cannot
//! take it down, and what was found is remembered for the next start. Uses
//! the reference plugins when `VLABS_TEST_PLUGINS` names their folder.

#![allow(
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::assert_is_empty,
    reason = "test code: helpers unwrap and report freely"
)]

use std::path::PathBuf;
use std::time::Instant;

use daw_plugin_host::scan::{ScanProblem, Scanner, System};
use daw_plugin_host::{Format, Kind};

fn program() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daw-app"))
}

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("plugin-scan-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

#[test]
fn a_broken_plugin_file_is_reported_not_fatal() {
    let folder = scratch("broken");
    std::fs::write(folder.join("Broken.clap"), b"not a plugin at all").unwrap();
    let mut scanner = Scanner::start(program(), None, vec![folder.clone()]);
    scanner.wait();
    let findings = scanner.findings();
    assert!(findings.plugins.is_empty());
    assert_eq!(findings.problems.len(), 1, "the broken file is reported");
    eprintln!("reported: {:?}", findings.problems[0].1);
    assert!(matches!(findings.problems[0].1, ScanProblem::Failed(_)));
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_plugin_made_for_another_system_is_explained_and_mac_leftovers_are_ignored() {
    let folder = scratch("other-system");
    // A plugin built for another system than this one, as a zip from its
    // maker's site might hold it (with the leftovers a Mac adds to zips).
    let (other, start): (System, &[u8]) = if cfg!(windows) {
        (System::Linux, b"\x7fELF\x02\x01\x01\x00")
    } else {
        (System::Windows, b"MZ\x90\x00\x03\x00\x00\x00")
    };
    std::fs::write(folder.join("Synth.vst3"), start).unwrap();
    std::fs::write(folder.join("._Synth.vst3"), b"Mac attributes").unwrap();
    std::fs::create_dir_all(folder.join("__MACOSX")).unwrap();
    std::fs::write(folder.join("__MACOSX").join("._Synth.vst3"), b"more").unwrap();
    let mut scanner = Scanner::start(program(), None, vec![folder.clone()]);
    scanner.wait();
    let findings = scanner.findings();
    assert_eq!(
        findings.problems,
        vec![(folder.join("Synth.vst3"), ScanProblem::OtherSystem(other))],
        "the plugin is named as made for {other:?}; the leftovers are not plugins"
    );
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_reference_plugins_are_found_and_remembered() {
    let Some(fixtures) = std::env::var_os("VLABS_TEST_PLUGINS").map(PathBuf::from) else {
        eprintln!("VLABS_TEST_PLUGINS is not set; skipped");
        return;
    };
    let cache = scratch("cache").join("plugins.json");
    let started = Instant::now();
    let mut scanner = Scanner::start(program(), Some(cache.clone()), vec![fixtures.clone()]);
    scanner.wait();
    let first = started.elapsed();
    let findings = scanner.findings();
    let has = |name: &str, format: Format, kind: Kind| {
        findings
            .plugins
            .iter()
            .any(|p| p.name == name && p.format == format && p.kind == kind)
    };
    for format in [Format::Clap, Format::Vst3] {
        assert!(
            has("Surge XT", format, Kind::Instrument),
            "Surge XT {format:?}"
        );
        assert!(has("Dexed", format, Kind::Instrument), "Dexed {format:?}");
        assert!(
            has("Surge XT Effects", format, Kind::Effect),
            "Surge XT Effects {format:?}"
        );
    }
    assert!(findings.problems.is_empty(), "{:?}", findings.problems);

    assert_eq!(findings.opened, 6, "every file was opened once");

    // The next start reads what was found from the cache: no plugin is
    // opened again.
    let remembered = std::fs::read_to_string(&cache).unwrap_or_default();
    let started = Instant::now();
    let mut again = Scanner::start(program(), Some(cache.clone()), vec![fixtures]);
    again.wait();
    let second = started.elapsed();
    let found_again = again.findings();
    eprintln!(
        "first scan {first:?} (opened {}), from the cache {second:?} (opened {})",
        findings.opened, found_again.opened
    );
    assert_eq!(found_again.plugins, findings.plugins);
    assert_eq!(
        found_again.opened,
        0,
        "plugins already known are not opened again; the cache was:\n{remembered}\nand is now:\n{}",
        std::fs::read_to_string(&cache).unwrap_or_default()
    );
}

#[test]
fn a_reference_plugin_added_from_its_download_folder_is_found_in_the_programs_own_folder() {
    let Some(fixtures) = std::env::var_os("VLABS_TEST_PLUGINS").map(PathBuf::from) else {
        eprintln!("VLABS_TEST_PLUGINS is not set; skipped");
        return;
    };
    // Dexed as its download unpacks: a folder holding the plugin in both
    // formats (decision 0023's "Add a plugin folder…").
    let own = scratch("own").join("plugins");
    let added = daw_plugin_host::install::add(&[fixtures.join("dexed")], &own);
    assert_eq!(added.plugins.len(), 2, "{added:?}");
    assert!(added.skipped.is_empty(), "{added:?}");
    let mut scanner = Scanner::start(
        program(),
        None,
        daw_plugin_host::install::own_search_folders(&own),
    );
    scanner.wait();
    let findings = scanner.findings();
    let formats: Vec<Format> = findings
        .plugins
        .iter()
        .filter(|p| p.name == "Dexed")
        .map(|p| p.format)
        .collect();
    assert!(formats.contains(&Format::Clap), "{:?}", findings.plugins);
    assert!(formats.contains(&Format::Vst3), "{:?}", findings.plugins);
    // Removed again from the program's folder: the search no longer finds it.
    for plugin in &added.plugins {
        daw_plugin_host::install::remove(plugin, &own).unwrap();
    }
    let mut scanner = Scanner::start(
        program(),
        None,
        daw_plugin_host::install::own_search_folders(&own),
    );
    scanner.wait();
    assert!(scanner.findings().plugins.is_empty());
    let _ = std::fs::remove_dir_all(own.parent().unwrap());
}

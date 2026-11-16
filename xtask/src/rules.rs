// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that enforce repository rules the compiler cannot check.
//!
//! They run with every `cargo xtask test`, locally and in CI.

use std::fs;
use std::path::{Path, PathBuf};

use super::workspace_root;

/// The layers of the application, lowest first.
///
/// A crate may depend only on crates in layers below its own. The reasons
/// are in `docs/architecture.md`; change both together.
const LAYERS: &[&[&str]] = &[
    &["daw-ident", "daw-rt"],
    &["daw-model", "daw-dsp", "daw-plugin-host", "daw-devices"],
    &["daw-formats", "daw-engine"],
    &["daw-ui"],
    &["daw-app"],
];

/// Every crate of the application starts with this.
const INTERNAL_PREFIX: &str = "daw-";

/// The first line of every Rust source file.
const LICENCE_HEADER: &str = "// SPDX-License-Identifier: GPL-3.0-or-later";

#[test]
fn every_crate_is_placed_in_a_layer() {
    let on_disk = crate_names();
    for name in &on_disk {
        assert!(
            layer_of(name).is_some(),
            "crates/{name} is not listed in LAYERS (xtask/src/rules.rs). \
             Decide which layer it belongs to and record it in docs/architecture.md."
        );
    }
    for name in LAYERS.iter().copied().flatten() {
        assert!(
            on_disk.iter().any(|existing| existing == name),
            "LAYERS (xtask/src/rules.rs) lists `{name}`, but crates/{name} does not exist."
        );
    }
}

#[test]
fn crates_depend_only_on_lower_layers() {
    for name in crate_names() {
        let manifest = read(&crates_dir().join(&name).join("Cargo.toml"));
        assert!(
            manifest.contains(&format!("name = \"{name}\"")),
            "crates/{name}/Cargo.toml must name its package `{name}`, like its folder."
        );

        let Some(layer) = layer_of(&name) else {
            continue; // Reported by `every_crate_is_placed_in_a_layer`.
        };
        for dependency in internal_dependencies(&manifest) {
            let lower = layer_of(&dependency).is_some_and(|other| other < layer);
            assert!(
                lower,
                "`{name}` must not depend on `{dependency}`: a crate may only use \
                 crates in the layers below it. See docs/architecture.md."
            );
        }
    }
}

#[test]
fn every_source_file_starts_with_the_licence_identifier() {
    let mut files = Vec::new();
    for folder in ["crates", "spikes", "xtask"] {
        rust_files(&workspace_root().join(folder), &mut files);
    }
    assert!(!files.is_empty(), "found no Rust source files to check");

    for file in files {
        let text = read(&file);
        assert_eq!(
            text.lines().next(),
            Some(LICENCE_HEADER),
            "{} must start with the line `{LICENCE_HEADER}`",
            file.display()
        );
    }
}

#[test]
fn product_identity_is_written_only_in_daw_ident() {
    let identity = [
        daw_ident::PRODUCT_NAME,
        daw_ident::EXECUTABLE_NAME,
        daw_ident::PROJECT_FILE_EXTENSION,
        daw_ident::APPLICATION_ID,
    ];
    let mut files = Vec::new();
    for folder in ["crates", "spikes", "xtask"] {
        rust_files(&workspace_root().join(folder), &mut files);
    }
    let home = workspace_root().join("crates").join("daw-ident");
    for file in files.iter().filter(|file| !file.starts_with(&home)) {
        let text = read(file);
        for value in identity {
            assert!(
                !contains_word(&text, value),
                "{} spells out `{value}`. The product name, file extension and \
                 identifiers are written only in crates/daw-ident; read them from there \
                 (see docs/decisions/0002-product-identity-in-one-place.md).",
                file.display()
            );
        }
    }
}

#[test]
fn whole_words_are_found_and_parts_of_words_are_not() {
    assert!(contains_word("save as song.ext now", "ext"));
    assert!(contains_word("\"ext\"", "ext"));
    assert!(!contains_word("extra", "ext"));
    assert!(!contains_word("a_ext_b", "ext"));
}

/// Whether `word` appears in `text` with no letter, digit or `_` on either
/// side.
fn contains_word(text: &str, word: &str) -> bool {
    let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(word).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + word.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

#[test]
fn internal_dependencies_are_found_in_every_form() {
    let manifest = r#"
        [package]
        name = "daw-example"
        daw-not-a-dependency = true

        [dependencies]
        daw-model.workspace = true
        daw-dsp = { workspace = true }
        "daw-ident" = { path = "../daw-ident" }
        serde = "1"
        # daw-commented-out.workspace = true

        [dev-dependencies] # tests only
        daw-formats.workspace = true

        [target.'cfg(windows)'.dependencies]
        daw-plugin-host.workspace = true

        [build-dependencies.daw-engine]
        workspace = true

        [features]
        daw-feature = []
    "#;
    assert_eq!(
        internal_dependencies(manifest),
        [
            "daw-dsp",
            "daw-engine",
            "daw-formats",
            "daw-ident",
            "daw-model",
            "daw-plugin-host",
        ]
    );
}

fn layer_of(name: &str) -> Option<usize> {
    LAYERS.iter().position(|layer| layer.contains(&name))
}

fn crates_dir() -> PathBuf {
    workspace_root().join("crates")
}

/// The names of the folders in `crates/`, sorted.
fn crate_names() -> Vec<String> {
    let entries = fs::read_dir(crates_dir()).expect("the crates/ folder is readable");
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The application's own crates that a `Cargo.toml` depends on, sorted.
///
/// Understands the ways a dependency can be written: a line in any
/// `[...dependencies]` table, or a `[...dependencies.name]` table of its own.
fn internal_dependencies(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_dependency_table = false;

    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            let header = line
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or_default()
                .trim();
            if let Some((_, name)) = header.rsplit_once("dependencies.") {
                names.push(unquoted(name));
                in_dependency_table = false;
            } else {
                in_dependency_table = header.ends_with("dependencies");
            }
        } else if in_dependency_table && !line.starts_with('#') {
            let key = line.split(['=', '.']).next().unwrap_or_default();
            names.push(unquoted(key));
        }
    }

    names.retain(|name| name.starts_with(INTERNAL_PREFIX));
    names.sort();
    names.dedup();
    names
}

fn unquoted(text: &str) -> String {
    text.trim().trim_matches(['"', '\'']).to_owned()
}

/// Collects every `.rs` file below `folder`, skipping build output.
fn rust_files(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return; // The folder does not exist (yet).
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                rust_files(&path, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

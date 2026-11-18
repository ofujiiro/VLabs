// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask notices`: the licence notices every copy of the program
//! must carry (decision 0022), generated from the real dependency graph.
//!
//! Which crates are built into the program comes from `cargo tree` for
//! each system it ships for (it resolves features per system, as the build
//! does); where each crate's files are comes from `cargo metadata`. Each
//! crate's own licence and notice files are copied; for the few that ship
//! none, the standard text is written with the crate's authors. Identical
//! texts are printed once.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::json;

/// A crate built into the program.
#[derive(Clone, Debug, Default)]
struct Crate {
    name: String,
    version: String,
    license: String,
    repository: String,
    authors: Vec<String>,
    folder: Option<PathBuf>,
    license_file: Option<PathBuf>,
}

impl Crate {
    fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }
}

/// Files whose names mark licence or notice texts.
fn is_licence_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "license",
        "licence",
        "copying",
        "notice",
        "copyright",
        "unlicense",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn run(cargo: &Path, root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(cargo)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`cargo {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| "cargo wrote something unreadable".to_owned())
}

/// The outside crates of a build: (name, version) to (licence,
/// repository).
type BuiltCrates = BTreeMap<(String, String), (String, String)>;

/// The outside crates built into `package` for each target: (name,
/// version) to licence and repository.
fn built_crates(
    cargo: &Path,
    root: &Path,
    package: &str,
    targets: &[&str],
) -> Result<BuiltCrates, String> {
    let mut found = BTreeMap::new();
    for target in targets {
        let tree = run(
            cargo,
            root,
            &[
                "tree",
                "--locked",
                "--package",
                package,
                "--target",
                target,
                "--edges",
                "normal",
                "--prefix",
                "none",
                "--format",
                "{p}|{l}|{r}",
            ],
        )?;
        for line in tree.lines() {
            let mut parts = line.splitn(3, '|');
            let (Some(package), Some(license), Some(repository)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            // "name v1.2.3", "name v1.2.3 (/path)" for the workspace's own,
            // with " (proc-macro)" or " (*)" after some.
            let words: Vec<&str> = package.split_whitespace().collect();
            let (Some(name), Some(version)) = (words.first(), words.get(1)) else {
                continue;
            };
            if words
                .iter()
                .skip(2)
                .any(|w| w.starts_with("(/") || w.starts_with("(\\") || w.contains(":\\"))
            {
                continue; // One of ours.
            }
            let version = version.trim_start_matches('v').to_owned();
            found
                .entry(((*name).to_owned(), version))
                .or_insert_with(|| {
                    (
                        license.trim().to_owned(),
                        repository.trim().trim_end_matches(" (*)").to_owned(),
                    )
                });
        }
    }
    Ok(found)
}

/// Where each crate's files are, its authors and its licence file.
fn locate(cargo: &Path, root: &Path, crates: &mut [Crate]) -> Result<(), String> {
    let text = run(
        cargo,
        root,
        &["metadata", "--format-version", "1", "--locked"],
    )?;
    let metadata = json::parse(&text)?;
    let mut by_name: BTreeMap<(String, String), &json::Json> = BTreeMap::new();
    for package in metadata.get("packages").map_or(&[][..], json::Json::items) {
        let (Some(name), Some(version)) = (
            package.get("name").and_then(json::Json::text),
            package.get("version").and_then(json::Json::text),
        ) else {
            continue;
        };
        by_name.insert((name.to_owned(), version.to_owned()), package);
    }
    for krate in crates.iter_mut() {
        let Some(package) = by_name.get(&(krate.name.clone(), krate.version.clone())) else {
            continue;
        };
        let manifest = package
            .get("manifest_path")
            .and_then(json::Json::text)
            .map(PathBuf::from);
        krate.folder = manifest
            .as_ref()
            .and_then(|m| m.parent().map(Path::to_path_buf));
        krate.authors = package
            .get("authors")
            .map_or(&[][..], json::Json::items)
            .iter()
            .filter_map(json::Json::text)
            .map(str::to_owned)
            .collect();
        krate.license_file = package
            .get("license_file")
            .and_then(json::Json::text)
            .and_then(|file| krate.folder.as_ref().map(|f| f.join(file)));
    }
    Ok(())
}

/// The licence and notice texts a crate ships.
fn shipped_texts(krate: &Crate) -> Vec<String> {
    let mut paths: BTreeSet<PathBuf> = BTreeSet::new();
    if let Some(folder) = &krate.folder
        && let Ok(entries) = fs::read_dir(folder)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_file() && is_licence_file(&name) {
                paths.insert(path);
            }
        }
    }
    if let Some(file) = &krate.license_file
        && file.is_file()
    {
        paths.insert(file.clone());
    }
    paths
        .iter()
        .filter(|p| fs::metadata(p).is_ok_and(|m| m.len() < 512 * 1024))
        .filter_map(|p| fs::read(p).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().replace("\r\n", "\n"))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Copyright holders for a standard text: the crate's authors without
/// e-mail addresses, or its name.
fn holders(krate: &Crate) -> String {
    let names: Vec<String> = krate
        .authors
        .iter()
        .map(|a| a.split('<').next().unwrap_or(a).trim().to_owned())
        .filter(|a| !a.is_empty())
        .collect();
    if names.is_empty() {
        format!("the {} authors", krate.name)
    } else {
        names.join(", ")
    }
}

const MIT: &str = "Copyright (c) {holders}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.";

/// The standard text of a crate's licence, for a crate that ships none:
/// MIT where offered, otherwise the Apache text another crate ships.
fn standard_text(krate: &Crate, apache: Option<&str>) -> Result<String, String> {
    let license = &krate.license;
    if license
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .any(|w| w == "MIT")
    {
        return Ok(MIT.replace("{holders}", &holders(krate)));
    }
    if license.contains("Apache-2.0")
        && let Some(apache) = apache
    {
        return Ok(format!("Copyright {}\n\n{apache}", holders(krate)));
    }
    Err(format!(
        "{} ships no licence text and its licence ({license}) has no standard text here",
        krate.label()
    ))
}

/// Writes the notices for `package` (built for `targets`) to `out`.
/// `extra` are files of this repository added as they are (the bundled
/// fonts' and sounds' licences, the AppImage runtime's).
pub fn generate(
    cargo: &Path,
    root: &Path,
    package: &str,
    targets: &[&str],
    extra: &[(&str, PathBuf)],
    out: &Path,
) -> Result<usize, String> {
    let built = built_crates(cargo, root, package, targets)?;
    let mut crates: Vec<Crate> = built
        .into_iter()
        .map(|((name, version), (license, repository))| Crate {
            name,
            version,
            license,
            repository,
            ..Crate::default()
        })
        .collect();
    locate(cargo, root, &mut crates)?;
    let texts: Vec<Vec<String>> = crates.iter().map(shipped_texts).collect();
    // The standard Apache text, from a crate that ships it.
    let apache = texts
        .iter()
        .flatten()
        .find(|t| t.contains("Apache License") && t.contains("Version 2.0, January 2004"))
        .map(|t| {
            t.find("Apache License")
                .map_or(t.as_str(), |at| &t[at..])
                .to_owned()
        });
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (krate, shipped) in crates.iter().zip(texts) {
        let shipped = if shipped.is_empty() {
            vec![standard_text(krate, apache.as_deref())?]
        } else {
            shipped
        };
        for text in shipped {
            grouped.entry(text).or_default().push(krate.label());
        }
    }

    let product = daw_ident::PRODUCT_NAME;
    let version = env!("CARGO_PKG_VERSION");
    let mut doc = String::new();
    let rule = "=".repeat(78);
    let thin = "-".repeat(78);
    let _ = write!(
        doc,
        "{product} {version}: third-party notices\n{rule}\n\n\
         {product} is free software under the GNU General Public License, version 3\n\
         or later (see LICENSE). It includes the following work of others, each under\n\
         its own licence, whose notices follow.\n\n"
    );
    for (title, path) in extra {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let _ = write!(doc, "{title}\n{thin}\n\n{}\n\n", text.trim());
    }
    let _ = write!(
        doc,
        "Libraries (Rust crates) built into the program\n{thin}\n\n"
    );
    for krate in &crates {
        let source = if krate.repository.is_empty() {
            String::new()
        } else {
            format!(" — {}", krate.repository)
        };
        let _ = writeln!(doc, "{} ({}){source}", krate.label(), krate.license);
    }
    let _ = write!(doc, "\nTheir licence and notice texts\n{thin}\n");
    for (text, users) in &grouped {
        let _ = write!(doc, "\nUsed by: {}\n\n{text}\n\n{thin}\n", users.join(", "));
    }
    if let Some(folder) = out.parent() {
        fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    fs::write(out, doc).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(crates.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licence_files_are_recognised() {
        for name in [
            "LICENSE",
            "LICENSE-MIT",
            "licence.txt",
            "COPYING",
            "NOTICE",
            "UNLICENSE",
        ] {
            assert!(is_licence_file(name), "{name}");
        }
        assert!(!is_licence_file("README.md"));
        assert!(!is_licence_file("Cargo.toml"));
    }

    #[test]
    fn standard_texts_name_the_authors() {
        let krate = Crate {
            name: "tiny".to_owned(),
            version: "1.0.0".to_owned(),
            license: "MIT OR Apache-2.0".to_owned(),
            authors: vec!["Ada <ada@example.org>".to_owned()],
            ..Crate::default()
        };
        let text = standard_text(&krate, None).unwrap();
        assert!(text.starts_with("Copyright (c) Ada\n"));
        let apache_only = Crate {
            license: "Apache-2.0".to_owned(),
            ..krate.clone()
        };
        assert!(standard_text(&apache_only, None).is_err());
        let text = standard_text(&apache_only, Some("Apache License\nVersion 2.0")).unwrap();
        assert!(text.contains("Apache License"));
        let unknown = Crate {
            license: "BSL-1.0".to_owned(),
            ..krate
        };
        assert!(standard_text(&unknown, None).is_err());
    }
}

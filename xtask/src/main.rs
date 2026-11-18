// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool reports its progress by printing"
)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

mod guide;
mod json;
mod notices;
mod package;
#[cfg(test)]
mod rules;

const HELP: &str = "\
Developer commands. Usage: cargo xtask <command>

  ci      Run every check that CI runs: fmt, lint, build, test, deny
  fmt     Check formatting (add --fix to reformat the code instead)
  lint    Run Clippy on everything; warnings count as errors
  build   Build everything
  test    Run every test
  deny    Check the licences, security advisories and sources of dependencies
  dist    Build the release program into dist/, under its product name, with
          its licence notices, user guide and icons
  notices Write the third-party licence notices (dist/THIRD_PARTY_NOTICES.txt)
  guide   Turn docs/user-guide.md into one HTML page (dist/user-guide.html)
  installer  Make the Windows installer from dist/ (needs NSIS's makensis)
  appimage   Make the Linux AppImage from dist/ (needs appimagetool)
  help    Show this text
";

/// The cargo package that builds the program people launch.
const APP_PACKAGE: &str = "daw-app";

/// The systems the program ships for; the licence check runs once for each.
const DENY_TARGETS: [&str; 2] = ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"];

/// Files copied next to the program in `dist/`. The GPL requires its text to
/// travel with the program.
const DIST_DOCUMENTS: [&str; 2] = ["LICENSE", "THIRD_PARTY.md"];

/// The generated licence notices, in `dist/` (decision 0022).
const NOTICES: &str = "THIRD_PARTY_NOTICES.txt";
/// The user guide as a page, in `dist/`.
const GUIDE: &str = "user-guide.html";

type Outcome = Result<(), String>;

/// A named check, as run by `ci`.
type Step = (&'static str, fn() -> Outcome);

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next();
    let options: Vec<String> = args.collect();

    let outcome = match command.as_deref() {
        None | Some("help" | "--help" | "-h") => {
            print!("{HELP}");
            Ok(())
        }
        Some("ci") => ci(),
        Some("fmt") => fmt(options.iter().any(|option| option == "--fix")),
        Some("lint") => lint(),
        Some("build") => build(),
        Some("test") => test(),
        Some("deny") => deny(),
        Some("dist") => dist(),
        Some("notices") => notices(&workspace_root().join("dist").join(NOTICES)).map(|count| {
            println!("Notices for {count} crates written.");
        }),
        Some("guide") => guide(&workspace_root().join("dist").join(GUIDE)),
        Some("installer") => installer(),
        Some("appimage") => appimage(),
        Some(other) => Err(format!("unknown command `{other}`\n\n{HELP}")),
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Runs every check, keeps going after a failure, and reports all of them.
fn ci() -> Outcome {
    let steps: [Step; 6] = [
        ("fmt", || fmt(false)),
        ("lint", lint),
        ("build", build),
        ("test", test),
        ("deny", deny),
        // Every crate's licence text is found (or the release would lack
        // its notices).
        ("notices", || {
            let out = target_dir()?.join("notices-check").join(NOTICES);
            notices(&out).map(|count| println!("Notices for {count} crates."))
        }),
    ];

    let mut failed = Vec::new();
    for (name, step) in steps {
        println!("\n=== {name} ===");
        if let Err(message) = step() {
            eprintln!("error: {message}");
            failed.push(name);
        }
    }

    println!();
    if failed.is_empty() {
        println!("All checks passed.");
        Ok(())
    } else {
        Err(format!("these checks failed: {}", failed.join(", ")))
    }
}

fn fmt(fix: bool) -> Outcome {
    if fix {
        cargo(&["fmt", "--all"])
    } else {
        cargo(&["fmt", "--all", "--check"])
    }
}

fn lint() -> Outcome {
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--locked",
        "--",
        "-D",
        "warnings",
    ])
}

fn build() -> Outcome {
    cargo(&[
        "build",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--locked",
    ])
}

fn test() -> Outcome {
    cargo(&["test", "--workspace", "--all-features", "--locked"])
}

fn deny() -> Outcome {
    let installed = cargo_command(&["deny", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !installed {
        return Err("cargo-deny is not installed. Install it with:\n    \
                    cargo install cargo-deny --locked"
            .to_owned());
    }
    // One run per shipping system: cargo-deny would otherwise merge the
    // features chosen for different systems into one graph and see
    // dependencies that are never built (decision 0005).
    for target in DENY_TARGETS {
        println!("--- {target}");
        cargo(&["deny", "--locked", "--target", target, "check"])?;
    }
    Ok(())
}

/// Builds the release program and assembles `dist/`, the folder people
/// receive. The program file is named after the product here, and only here.
fn dist() -> Outcome {
    let root = workspace_root();
    let target = target_dir()?;

    let built = cargo_command(&["build", "--release", "--locked", "--package", APP_PACKAGE])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|error| format!("could not run cargo: {error}"))?;
    if !built.success() {
        return Err("the release build failed".to_owned());
    }

    let suffix = env::consts::EXE_SUFFIX;
    let dist = root.join("dist");
    if dist.exists() {
        fs::remove_dir_all(&dist).map_err(|error| describe(&dist, &error))?;
    }
    fs::create_dir_all(&dist).map_err(|error| describe(&dist, &error))?;

    let source = target
        .join("release")
        .join(format!("{APP_PACKAGE}{suffix}"));
    let program = dist.join(format!("{}{suffix}", daw_ident::EXECUTABLE_NAME));
    fs::copy(&source, &program).map_err(|error| describe(&source, &error))?;

    for document in DIST_DOCUMENTS {
        let source = root.join(document);
        fs::copy(&source, dist.join(document)).map_err(|error| describe(&source, &error))?;
    }
    let count = notices(&dist.join(NOTICES))?;
    println!("Licence notices for {count} crates written.");
    guide(&dist.join(GUIDE))?;
    // The icon files come from the program itself, so every picture of it
    // is the same drawing.
    let icons = Command::new(&program)
        .arg("--write-icons")
        .arg(dist.join("icons"))
        .status()
        .map_err(|error| describe(&program, &error))?;
    if !icons.success() {
        return Err("the program could not write its icon files".to_owned());
    }

    println!(
        "{} {} is ready: {}",
        daw_ident::PRODUCT_NAME,
        env!("CARGO_PKG_VERSION"),
        program.display(),
    );
    Ok(())
}

/// Writes the licence notices for the program to `out`; returns how many
/// crates they cover.
fn notices(out: &Path) -> Result<usize, String> {
    let root = workspace_root();
    let cargo = env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from);
    let extra = [
        (
            "Fonts (assets/fonts)",
            root.join("assets/fonts/LICENSE-Go-fonts.txt"),
        ),
        (
            "The piano (assets/sounds)",
            root.join("assets/sounds/LICENSE-piano.txt"),
        ),
        (
            "The acoustic drum kit (assets/drums)",
            root.join("assets/drums/LICENSE-drums.txt"),
        ),
        (
            "The AppImage runtime",
            root.join("packaging/linux/appimage-runtime-licences.txt"),
        ),
    ];
    notices::generate(&cargo, root, APP_PACKAGE, &DENY_TARGETS, &extra, out)
}

/// Writes the user guide as one page to `out`.
fn guide(out: &Path) -> Outcome {
    let title = format!("{} user guide", daw_ident::PRODUCT_NAME);
    guide::render_file(&workspace_root().join("docs/user-guide.md"), out, &title)?;
    println!("User guide written: {}", out.display());
    Ok(())
}

fn installer() -> Outcome {
    let root = workspace_root();
    let made = package::installer(root, &root.join("dist"), &root.join("dist-installer"))?;
    println!("Installer ready: {}", made.display());
    Ok(())
}

fn appimage() -> Outcome {
    let root = workspace_root();
    let work = target_dir()?.join("appimage");
    let made = package::appimage(
        root,
        &root.join("dist"),
        &work,
        &root.join("dist-installer"),
    )?;
    println!("AppImage ready: {}", made.display());
    Ok(())
}

/// Runs `cargo` with the given arguments from the workspace root.
fn cargo(args: &[&str]) -> Outcome {
    let status = cargo_command(args)
        .status()
        .map_err(|error| format!("could not run cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`cargo {}` failed", args.join(" ")))
    }
}

fn cargo_command(args: &[&str]) -> Command {
    // Cargo tells the programs it runs which cargo to call back.
    let program = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(program);
    command.args(args).current_dir(workspace_root());
    command
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one folder below the workspace root")
}

/// Where cargo puts what it builds, as an absolute path.
fn target_dir() -> Result<PathBuf, String> {
    let dir = env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace_root().join("target"), PathBuf::from);
    if dir.is_absolute() {
        Ok(dir)
    } else {
        let current = env::current_dir()
            .map_err(|error| format!("could not find the current folder: {error}"))?;
        Ok(current.join(dir))
    }
}

fn describe(path: &Path, error: &std::io::Error) -> String {
    format!("{}: {error}", path.display())
}

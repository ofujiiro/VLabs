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
  dist    Build the release program into dist/, under its product name
  help    Show this text
";

/// The cargo package that builds the program people launch.
const APP_PACKAGE: &str = "daw-app";

/// Files copied next to the program in `dist/`. The GPL requires its text to
/// travel with the program.
const DIST_DOCUMENTS: [&str; 2] = ["LICENSE", "THIRD_PARTY.md"];

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
    let steps: [Step; 5] = [
        ("fmt", || fmt(false)),
        ("lint", lint),
        ("build", build),
        ("test", test),
        ("deny", deny),
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
    cargo(&["deny", "--locked", "check"])
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

    println!(
        "{} {} is ready: {}",
        daw_ident::PRODUCT_NAME,
        env!("CARGO_PKG_VERSION"),
        program.display(),
    );
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

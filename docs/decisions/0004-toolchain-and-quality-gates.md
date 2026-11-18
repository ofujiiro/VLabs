# 0004. Pinned toolchain, strict lints, CI on Windows and Linux

- **Status:** accepted, November 2026

## Context

The brief requires the stable Rust toolchain, and build, test, lint and
licence checks on Windows and Linux for every change. The owner cannot review
code, so automated gates carry more weight than usual.

## Decision

- **Toolchain:** Rust 1.100.0, pinned in `rust-toolchain.toml` (rustup
  installs it automatically) and mirrored in `rust-version` in `Cargo.toml`.
  Edition 2024. Upgrading is a deliberate one-line change.
- **One command for every check:** `cargo xtask ci` runs formatting, Clippy
  (all warnings are errors), the build, every test, and `cargo deny`. CI runs
  exactly that command, so a local pass predicts a CI pass.
- **Lints** (in `Cargo.toml`): Clippy's pedantic set with a few noisy lints
  allowed; `unsafe` code denied by default and allowed per module with a
  stated reason; every unsafe block needs a `// SAFETY:` comment;
  `unwrap()` and printing are flagged outside tests.
- **CI** (`.github/workflows/ci.yml`) on every push to `main` and every pull
  request:
  - **Linux: `ubuntu-26.04`**, the current Ubuntu LTS (generally available
    on GitHub since September 2026).
  - **Windows: `windows-2025`.** GitHub offers no Windows 11 desktop image;
    Windows Server 2025 is built from the same Windows 11 24H2 code base and
    uses the same audio stack.
  - Third-party actions are pinned to a commit hash, with the tag in a
    comment, so a moved tag cannot change what runs.
- **Spike and integration jobs** that need real plugins, a virtual sound card
  or a virtual screen run in a separate workflow (`spikes.yml`), so the main
  check stays fast.

## Consequences

- A newer push cancels an older running check on the same branch, to save CI
  minutes. Windows minutes count double on GitHub's private-repository
  plans; the owner should keep an eye on the monthly allowance. (Update,
  November 2026: the repository is public now, and GitHub's standard
  runners are free for public repositories.)
- Debug builds optimise dependencies fully and our code lightly
  (`opt-level = 1`), so a debug build can still play audio in real time while
  keeping the audio-thread checks on.

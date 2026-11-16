# xtask — developer commands

Small tools for working on this repository, run through cargo:

| Command | What it does |
| --- | --- |
| `cargo xtask ci` | Runs every check that CI runs: `fmt`, `lint`, `build`, `test`, `deny` |
| `cargo xtask fmt` | Checks formatting (`--fix` reformats the code instead) |
| `cargo xtask lint` | Runs Clippy on everything; warnings count as errors |
| `cargo xtask build` | Builds everything |
| `cargo xtask test` | Runs every test |
| `cargo xtask deny` | Checks the licences, security advisories and sources of dependencies |
| `cargo xtask dist` | Builds the release program into `dist/`, under its product name |

CI calls these same commands, so a change that passes `cargo xtask ci` on
your machine passes in CI too. `deny` needs
[cargo-deny](https://github.com/EmbarkStudios/cargo-deny):
`cargo install cargo-deny --locked`.

## Rules checked by the tests

`cargo xtask test` also runs the tests in `src/rules.rs`, which enforce what
the compiler cannot:

- **Layers.** Every crate under `crates/` belongs to a layer and may depend
  only on crates in lower layers. The layers are explained in
  `docs/architecture.md`.
- **Licence header.** Every Rust source file starts with
  `// SPDX-License-Identifier: GPL-3.0-or-later`.

## Rules for this crate

- It is a tool for developers and for CI. It is never part of the program
  people receive.
- It uses the standard library only, so it builds in a second or two.
- Later phases add their automation here (fetching the reference plugins for
  tests, building installers) instead of in shell scripts, so every command
  works the same on Windows and on Linux.

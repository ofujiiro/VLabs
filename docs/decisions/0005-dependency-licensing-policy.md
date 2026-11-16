# 0005. Only permissively licensed outside code

- **Status:** accepted, November 2026

## Context

VLabs is licensed GPL-3.0-or-later, and its sole copyright holder also sells
commercial licences. Dual licensing only works if every piece of outside code
can be relicensed along with ours. The brief therefore allows only permissive
licences (MIT, Apache-2.0, BSD, ISC, Zlib), forbids GPL-only and LGPL crates,
requires CC0 for bundled sounds, and asks for `cargo-deny` to enforce it.

## Decision

- `deny.toml` allows exactly: **MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause,
  ISC, Zlib**. A crate offering a choice ("MIT OR Apache-2.0") passes when one
  choice is allowed. `cargo deny` runs in every CI build, on the Linux and
  Windows dependency graphs, with every optional feature enabled.
- Code may come from crates.io only; git dependencies are refused unless a
  decision record adds them.
- Crates that must never appear are banned by name in `deny.toml`:
  `asio-sys` (the ASIO SDK), `vst3-sys` (GPL-licensed VST3 bindings) and
  `epaint_default_fonts` (egui's bundled fonts, under the OFL and Ubuntu font
  licences).

### The one exception: Unicode data tables

**Crates that embed the Unicode Character Database may carry the
Unicode-3.0 licence.** Two do:

- `unicode-ident`, which Rust's macro tooling (`proc-macro2`, used by
  `serde`, `thiserror` and most of the ecosystem) needs to recognise
  identifiers;
- `unicode-properties`, which egui's text layout needs.

Their code is MIT or Apache-2.0; only the data tables are under the Unicode
licence, a short permissive licence (OSI-approved) whose only condition is
keeping the copyright notice. It places no restriction on relicensing or on
commercial use. Avoiding it would mean avoiding most of the Rust ecosystem.
Each crate is listed by name in `deny.toml`; no other licence is excepted.
The owner should confirm they are comfortable with this exception.

### Checked per system

`cargo xtask deny` checks the Linux and the Windows dependency graphs
separately. A single combined run merges the features chosen for each system
and reports crates that are never built for that system.

This matters for egui's **system clipboard**: on Windows it uses
`clipboard-win`, licensed under the Boost Software License (BSL-1.0), which
is permissive but not on the list. The clipboard feature is therefore
switched on for Linux only, and VLabs will implement the small Windows
clipboard itself (phase 1).

### Crates considered and rejected for their licence

| Crate | Licence | What we do instead |
| --- | --- | --- |
| `symphonia` (audio decoding) | MPL-2.0 | `hound` (WAV), `claxon` (FLAC), `lewton` (OGG), `nanomp3` (MP3) |
| `midly` (MIDI files) | Unlicense | Our own Standard MIDI File reader and writer (a small format) |
| `oxisynth` (SoundFont synth) | LGPL-2.1 | `rustysynth` (MIT) |
| `vst3-sys` (VST3 bindings) | GPL-3.0 | `vst3` (MIT OR Apache-2.0), generated from the MIT-licensed SDK headers |
| `triple_buffer` | MPL-2.0 | `rtrb` and our own lock-free code |
| `audio_thread_priority` (cpal's `realtime` feature) | MPL-2.0 | Feature left off; we raise the audio thread's priority ourselves |
| egui's default fonts | OFL-1.1, Ubuntu Font Licence | The Go fonts (BSD-3-Clause), see decision 0008 |
| `clipboard-win` (egui's clipboard on Windows) | BSL-1.0 | Clipboard feature on Linux only; our own Windows clipboard |

## How to add a dependency

1. Check its current version, maintenance and licence on crates.io (not from
   memory), including the licences of what it pulls in.
2. Add it once, in `[workspace.dependencies]` of the root `Cargo.toml`.
3. Run `cargo xtask deny`.
4. Add a line to decision 0006 with the version, licence and reason.

## Notices for distributed builds

MIT, BSD and Apache licences require their notices to travel with binary
copies. Phase 6 adds a generated third-party notices file to every installer;
until then `THIRD_PARTY.md` records bundled assets.

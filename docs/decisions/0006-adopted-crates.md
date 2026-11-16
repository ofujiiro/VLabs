# 0006. Outside crates adopted

- **Status:** accepted, November 2026; extended whenever a crate is added

Each entry was checked on crates.io (version, last release, licence,
repository) on the date shown, as the brief requires, rather than from
memory. "Planned" means checked and chosen, but not yet in `Cargo.toml`.

## In use

| Crate | Version | Licence | Checked | Why |
| --- | --- | --- | --- | --- |
| `cpal` | 0.18.2 | Apache-2.0 | 2026-11-16 | Audio output and input on WASAPI, ALSA and JACK. The standard Rust audio I/O library (RustAudio), released August 2026. |
| `rtrb` | 0.4.1 | MIT OR Apache-2.0 | 2026-11-16 | Lock-free single-producer, single-consumer queues between the audio thread and the rest. Wait-free, no allocation after creation. |
| `libc` | 0.2.191 | MIT OR Apache-2.0 | 2026-11-16 | Raising the audio thread to real-time priority on Linux. |
| `clack-host` | 0.2.0 | MIT OR Apache-2.0 | 2026-11-16 | Safe wrappers for hosting CLAP plugins, now published on crates.io (September 2026). Named in the brief. |
| `clack-extensions` | 0.2.0 | MIT OR Apache-2.0 | 2026-11-16 | The CLAP extensions (GUI, state, parameters, note and audio ports). |
| `vst3` | 0.3.0 | MIT OR Apache-2.0 | 2026-11-16 | VST3 interface bindings generated from the MIT-licensed SDK headers (coupler-rs). Low-level: the host side is our own code. |
| `x11rb` | 0.14.0 | MIT OR Apache-2.0 | 2026-11-16 | Creating the X11 windows that hold plugin editors on Linux, including under XWayland. |
| `windows-sys` | 0.61.2 | MIT OR Apache-2.0 | 2026-11-16 | Win32 windows for plugin editors on Windows. |
| `libloading` | 0.9.1 | ISC | 2026-11-16 | Loading VST3 plugin libraries. |
| `egui`, `eframe` | 0.37.0 | MIT OR Apache-2.0 | 2026-11-16 | The interface, as decided in the brief (released October 2026). Used with the wgpu renderer (wgpu 30), AccessKit for accessibility, X11 and Wayland; without the default fonts, the system fonts, or (on Windows) the clipboard feature. |
| `rustysynth` | 1.3.8 | MIT | 2026-11-16 | SoundFont playback for the bundled piano. |
| `serde`, `serde_json` | 1.0.229, 1.0.154 | MIT OR Apache-2.0 | 2026-11-16 | The human-readable project description and the settings file. |
| `zip` | 9.6.0 | MIT | 2026-11-16 | The single-file project container (deflate only). |
| `midir` | 0.11.1 | MIT | 2026-11-16 | MIDI keyboard input with hot-plugging. |
| `rfd` | 0.18.0 | MIT | 2026-11-16 | The desktop's own open and save dialogs: Windows' file dialog; on Linux the XDG portal over D-Bus (loaded at run time) or zenity. Default features. |
| `claxon` | 0.4.3 | Apache-2.0 | 2026-11-16 | FLAC decoding; for now only in `tools/make-piano`, which builds the bundled piano from the FLAC recordings. |
| `egui_kittest` (with `kittest`) | 0.37.0 (0.5.0) | MIT OR Apache-2.0 | 2026-11-16 | Tests only: drives the real interface with simulated input and inspects it through its accessibility tree, with no window. Also fails tests on any glyph missing from our fonts and on unlabelled controls. |

## Planned (checked, adopted when the phase needs them)

| Crate | Version | Licence | For |
| --- | --- | --- | --- |
| `hound` | 3.5.1 | Apache-2.0 | WAV reading and writing (phase 2) |
| `flacenc` | 0.5.1 | Apache-2.0 | FLAC export (phase 6) |
| `lewton` | 0.10.2 | MIT OR Apache-2.0 | OGG Vorbis decoding (phase 4) |
| `nanomp3` | 0.2.0 | MIT OR Apache-2.0 | MP3 decoding, pure Rust (phase 4) |
| `rubato` | 5.1.0 | MIT OR Apache-2.0 | Sample-rate conversion of imported audio (phase 4) |
| `quick-xml` | 0.42.0 | MIT | DAWproject import and export (phase 6) |

## Considered and set aside

- `assert_no_alloc` (BSD-1-Clause): fine licence, but the check is twenty
  lines and we want our own failure report and a counting mode for tests.
  Written in `daw-rt`.
- `dark-light`: eframe already follows the system's dark or light setting.
- `directories` (MIT OR Apache-2.0): its `dirs-sys` dependency pulls in
  `option-ext`, which is MPL-2.0. Finding the standard folders is a few lines
  of our own (`crates/daw-app/src/paths.rs`, decision 0013).
- `midly`, `symphonia`, `oxisynth`, `triple_buffer`, `vst3-sys`,
  `audio_thread_priority`: licences, see decision 0005.

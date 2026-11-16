# 0003. The workspace is split into layered crates

- **Status:** accepted, November 2026

## Context

The brief asks for a Cargo workspace with separate crates for the project
model, the engine, plugin hosting, built-in DSP, the UI and the application
shell, and invites a better split if there is one.

## Decision

Ten crates in five layers. A crate may only depend on crates in lower
layers; `xtask/src/rules.rs` enforces this on every test run.

| Layer | Crate | Role |
| --- | --- | --- |
| 0 | `daw-ident` | Product name, file extension, identifiers |
| 0 | `daw-rt` | Real-time safety tools (the audio-thread allocation check) |
| 1 | `daw-model` | The project as data, musical time, undo history |
| 1 | `daw-dsp` | Built-in instruments and effects |
| 1 | `daw-plugin-host` | CLAP and VST3 plugins |
| 1 | `daw-devices` | Speakers, microphones, MIDI keyboards, hot-plugging |
| 2 | `daw-formats` | Files: native projects, DAWproject, MIDI, audio |
| 2 | `daw-engine` | Turns the project into sound; offline rendering |
| 3 | `daw-ui` | Everything on screen |
| 4 | `daw-app` | The program: wires everything together |

Compared with the brief's list, three crates were added:

- **`daw-formats`** (from the earlier foundation work): file reading and
  writing is large, needs its own dependencies (compression, XML, audio
  codecs) and must never be pulled into the audio thread.
- **`daw-rt`**: the allocation check and other real-time tools are needed by
  the engine, the built-in DSP, plugin hosting and device code alike, so they
  sit at the bottom.
- **`daw-devices`**: talking to sound cards and MIDI hardware is operating
  system integration, not music. Keeping it out of `daw-engine` means the
  engine builds, runs and is tested with no audio system at all (the brief's
  "headless engine" rule is then guaranteed by the dependency graph, not by
  discipline). The application connects the two.

Spikes live in `spikes/`, one package each, outside the default build.
Developer automation lives in `xtask/`.

## Alternatives considered

- **Devices inside the engine** (the earlier skeleton): simpler to start,
  but it ties the engine's tests to the system audio libraries.
- **One crate per instrument or effect:** more compile-time isolation, but
  many tiny crates with shared building blocks; can be split later if build
  times demand it.

## Consequences

- The engine can be tested in CI with no sound card and no window.
- The interface and the engine cannot reach each other's internals; they meet
  only in `daw-app`, through the model and the engine's message queues.

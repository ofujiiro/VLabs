# Architecture

How VLabs is put together, in plain language. Decisions behind each part are
in [`docs/decisions/`](decisions/README.md). Parts marked *(planned)* are
designed but not built yet; [`ROADMAP.md`](../ROADMAP.md) says when they
arrive.

## The crates and their layers

The code is a Cargo workspace of ten crates. Each crate may only use crates
in the layers below it; a test (`xtask/src/rules.rs`) enforces this.

```text
 layer 4   daw-app ─────────── the program: starts everything, wires it up
              │
 layer 3   daw-ui ─────────── everything on screen
              │
 layer 2   daw-formats        daw-engine
           (files)            (turns the project into sound)
              │                   │
 layer 1   daw-model   daw-dsp   daw-plugin-host   daw-devices
           (the song)  (built-in (CLAP, VST3)      (sound cards,
                        sounds)                     MIDI keyboards)
              │
 layer 0   daw-ident (names)     daw-rt (real-time safety tools)
```

`daw-ui` and `daw-engine` never see each other. They meet only in
`daw-app`, through the project model and the engine's message queues.

## Threads

| Thread | Runs | Rules |
| --- | --- | --- |
| Interface | The window, drawing, user input, edits to the project | May not block for long: slow work goes to workers |
| Audio | The sound card's callback: the engine's processing | Real-time: no allocation, locks, files, network or logging |
| MIDI input | Receives notes from MIDI keyboards | Only timestamps and queues them |
| Workers *(planned)* | Saving, autosave, loading audio, drawing waveforms | Ordinary threads |
| Plugin scanner *(planned)* | A separate process that inspects plugins | A crash there cannot reach the program |

## How an edit becomes sound

```text
 user action ──▶ edit ──▶ undo history ──▶ new project snapshot
                                               │
                                     compile   ▼
                                       playback program ──(lock-free queue)──▶ audio thread
                                                                                  │
 screen ◀── position, meters, recorded notes ◀──(lock-free queue)────────────────┘
```

1. **The project model (`daw-model`) is the single source of truth.** Tracks,
   patterns, notes, clips, mixer settings and the tempo map are plain data.
   Nothing else keeps its own copy of the song.
2. **Every change is an edit recorded in the history**, so everything is
   undoable. The history keeps snapshots of the project; snapshots share
   unchanged parts (reference-counted), so keeping hundreds of them is cheap
   and undo can never get out of step with the data.
3. **The engine never reads the model directly from the audio thread.** After
   an edit, the new snapshot is *compiled* into a playback program: notes as
   sorted events, the processing graph, the tempo map turned into
   sample-accurate conversion tables. All memory for it is reserved at this
   point, off the audio thread.
4. **The program is handed to the audio thread through a lock-free queue.**
   The audio thread swaps it in between two blocks of sound and sends the old
   one back, so it is freed on another thread.
5. **Information flows back the same way:** play position, level meters,
   notes recorded live, dropout counts.

Small, frequent actions (playing a note from the keyboard, moving a fader)
travel as short messages on the same queues rather than recompiling.

## The engine *(phase 1 onwards)*

- **One processing graph** for live playback and offline export: the
  instruments, each track's effect chain, the mixer, the master output, the
  metronome. Export runs the same graph as fast as possible into a file.
- **Headless:** the engine needs no window and no sound card. Tests load a
  project, render it to a buffer and check the result: a signal is present,
  the pitch and timing are right, nothing clips.
- **Sample-accurate scheduling:** each block, the engine converts the block's
  time range to ticks through the tempo map and starts every note at its exact
  sample.
- **The allocation check:** the audio callback runs inside a real-time region
  (`daw-rt`). In debug builds, any allocation there stops the program with a
  report of where it happened.

## Musical time

Positions and lengths are integer ticks, 2,882,880 per quarter note, so every
common tuplet and polyrhythm is exact (decision 0007). A tempo map with
changes anywhere converts ticks to seconds; time-signature changes define
bars.

## Files *(phase 1 onwards)*

- **Native project (`.teto`):** a single file that is a ZIP container
  holding `project.json` (human-readable, versioned), the audio files used by
  the project, and the saved state of each plugin. Every format version keeps
  sample files in the tests, and a test proves the current code still opens
  them (migrations).
- **Saving is atomic:** write to a temporary file, then rename over the old
  one, so a crash never leaves a half-written project.
- **Autosave and crash recovery:** the program saves a recovery copy in the
  background at regular intervals and offers it after a crash.
- **Interchange:** DAWproject and Standard MIDI Files, import and export
  (phase 6). Audio import: WAV, FLAC, MP3, OGG; export: WAV, FLAC.

## Plugins *(phase 3)*

- **Scanning** happens in a separate process, one plugin at a time, with a
  timeout, so a broken plugin can neither crash the program nor block its
  start. Results are cached.
- **Hosting:** CLAP through `clack-host`, VST3 through our own host code on
  the `vst3` bindings. The engine talks to every plugin through one interface
  (activate, process audio and events, save and load state, parameters,
  editor). That interface is designed so a plugin can later run in its own
  process behind the same interface, without changing the engine.
- **Editor windows:** on Linux, plugin editors need X11; VLabs creates their
  windows through X11 directly, which also works under XWayland on Wayland
  desktops. On Windows, ordinary Win32 windows.

## The interface *(phase 1 onwards)*

Built with egui (eframe, wgpu renderer). All interface code is in `daw-ui`.
The piano roll, timeline and mixer are custom-drawn surfaces that draw only
what is visible. User-facing text lives in one module so it can be
translated later. Dark and light themes follow the system by default.

## Testing

- Unit tests next to the code they test.
- Engine tests render offline and inspect the sound.
- The allocation check runs in counting mode in tests, so a test can assert
  that processing allocated nothing.
- Repository rules (layers, licence headers, the product name in one place)
  are tests in `xtask`.
- Screenshots of the interface are rendered without a window for review.

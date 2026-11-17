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

`daw-ui` drives the engine through its control handle only (play, stop,
record, notes, the play position); the processing half belongs to
`daw-app`, which hands it to the sound output (decision 0012).

## Threads

| Thread | Runs | Rules |
| --- | --- | --- |
| Interface | The window, drawing, user input, edits to the project | May not block for long: slow work goes to workers |
| Audio | The sound card's callback: the engine's processing | Real-time: no allocation, locks, files, network or logging |
| MIDI input | Receives notes from MIDI keyboards | Only timestamps and queues them |
| Autosave | Writes recovery copies of the song in the background | Ordinary thread |
| Device watchers | Reopen the sound output and connect MIDI keyboards as they come and go | Ordinary threads |
| Workers *(planned)* | Loading audio, drawing waveforms | Ordinary threads |
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
- **Autosave and crash recovery:** while there are unsaved changes, a
  recovery copy is written in the background every few seconds; after a
  crash it is offered back (decision 0013).
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
- **Two halves** (decision 0015): loading a plugin gives a main-thread half
  (`PluginHandle`: parameters, settings, its window, housekeeping) and an
  audio-thread half (`PluginProcessor`: one call per block with the block's
  notes and parameter changes at their frames). Only plain data crosses
  between them. The engine keeps the main halves by where they sit in the
  song (`PluginTarget`: a track's instrument or an effect); the audio halves
  live in its instrument and effect tables like the built-in ones.
- **Retiring safely:** when a plugin leaves the song, its audio half is
  stopped on the audio thread and sent back with the other garbage; its
  main half is dropped only after that, on the main thread. The program's
  sound output stops before the engine when the program ends, for the same
  reason.
- **Settings** live in the plugin while it plays. They are copied into the
  song (`PluginSetup.state`, opaque bytes) before saving, autosaving,
  exporting, undoing and removing a plugin; the file keeps each plugin's
  settings as an entry of its own (`plugins/<n>.state`, format version 3).
- **Missing plugins** (a song from another computer): the track plays
  silence and the effect is left out of its chain; the person is told which
  plugin is missing, and the engine tries again when the plugin search
  finds more plugins.
- **Changes in a plugin's window** mark the song as changed. Many plugins
  also report a change just after taking their saved settings back, so
  reports in the first two seconds after loading are ignored, and on a song
  with nothing unsaved a report counts only if the settings now differ.

## Automation *(phase 3)*

A track's lanes (`AutomationLane`) each change one setting: its volume, its
pan, a setting of one of its effects (built-in settings are numbered in
`EffectKind::params`; plugin effects use their parameter ids), or a
parameter of its plugin instrument. Points hold values from 0 to 1 at ticks;
between points the value moves in a straight line, and before the first and
after the last it holds. Volume lanes use the mixer fader's scale (-60 dB
to +6 dB, silence at the bottom).

The program compiles lanes with the rest of the song; the audio thread
evaluates them once per block at the block's first tick. Volume and pan
glide across each block (no clicks) and take their values at once when
playback starts or moves; built-in effects get new settings; plugin
parameters are sent as events with the block's notes. While a lane exists,
it overrules the mixer's fader for that setting during playback.

## The interface

Built with egui (eframe, wgpu renderer). All interface code is in `daw-ui`:

- `session`: the open song and everything done to it, with no drawing
  (editing commands, the selection, playing notes, the transport,
  recording, files; in `session/arrange` the views, tracks, patterns and
  clips; in `session/mixing` the mixer, effects and drum pads). It also
  decides what Space plays, as a `PlaybackScope` given to the engine: the
  song, a part of it looping, or one pattern alone (decision 0014). Tested
  without a window.
- `workspace`: the window's layout (decisions 0011 and 0014), keyboard
  routing (note keys to the session, shortcuts to commands), menus,
  dialogs; the parts of the window ask it for dialogs and windows through
  `Intent`s.
- `song_view`, `step_grid`, `piano_roll`, `mixer`, `effect_ui`,
  `transport`, `keyboard_panel`: the views; the custom-drawn ones draw
  only what is visible.
- `export`: writes the song to a WAV file on a background thread, with a
  second engine rendering it offline through the same processing as
  playback (`daw_engine::OfflineRender`), block by block, with progress and
  cancelling.
- `keys`: note keys by physical position, the rebindable shortcut table.
- `text`: every user-facing string. `theme`: colours, fonts, spacing; dark
  and light themes follow the system by default.

## Testing

- Unit tests next to the code they test.
- Engine tests render offline and inspect the sound.
- The allocation check runs in counting mode in tests, so a test can assert
  that processing allocated nothing.
- Repository rules (layers, licence headers, the product name in one place)
  are tests in `xtask`.
- Interface tests drive the real window contents with simulated mouse and
  keyboard input and read them back through the accessibility tree
  (`egui_kittest`); they also fail on missing glyphs and unlabelled
  controls.
- The whole program is run on a virtual screen and sound card (Xvfb and a
  private PipeWire), driven with simulated input; screenshots and a
  recording of its sound output are checked (see `CLAUDE.md`).

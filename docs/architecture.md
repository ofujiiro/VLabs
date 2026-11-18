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
| Input watcher | Opens the default sound input while recording, reopens it when it fails | Ordinary thread; the input's own callback is real-time and only queues samples |
| Audio files | Reads, decodes, converts and summarises the song's audio files, one at a time | Ordinary thread |
| Recorder | Writes takes of sound to WAV files as they come, keeps their loudness summary | Ordinary thread |
| Plugin scanner | A separate run of the program that inspects plugin files | A crash there cannot reach the program |

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
changes anywhere converts ticks to seconds; a change can glide to the
next one, the tempo moving in a straight line over the ticks between
them, which makes its seconds a logarithm, worked out exactly both ways
(`daw_model::piece_seconds`, `piece_ticks`; the engine's tempo table uses
the same formulas). Time-signature changes define bars, and every grid
counts from the bar line (`MeterMap::grid_floor`, `grid_round`), so after
an odd bar the beats still fall on the bar lines. Changing the time
signatures keeps the song in its bars (`Project::change_meter`, decision
0018): every position keeps its bar and its distance from the bar line.

## Theory and rhythm *(phase 5)*

Decision 0018. The song's key is a map over bars like the time
signatures (`KeyMap`: a key, or none, from bar zero, and changes). The
model's `theory` module does the music theory on MIDI keys and pitch
classes: scales (`Scale`, `ScaleKind`), moving by steps of the scale,
chords built in the key by stacking every other note of the scale, chords
of a fixed kind (`ChordKind`), inversions, and recognising a chord to name
it. The interface draws on it for the piano roll's shading, keeping notes
in the key, chord stamps and transposing.

Swing belongs to a pattern (`Pattern::swing`): in each pair of swung units
the first is stretched and the second squeezed, a smooth warp in integer
arithmetic (`Swing::apply`); the engine's compiler applies it to each
note's start and end before placing the pattern in the song, and the
session applies the inverse (`Swing::remove`) to notes recorded into a
swung pattern.

## The built-in synth *(phase 6)*

Decision 0019. `daw-dsp::synth` is the sound: sixteen voices (or one with
glide) in a fixed array, each two band-limited oscillators and noise into
a zero-delay state-variable filter (two in a row for 24 dB) and an
amplifier, with a volume envelope, a filter envelope and one LFO. Pitches,
cutoffs and the LFO are worked out every 16 samples; envelopes and the
signal every sample. `daw-model::SynthSettings` is what the song keeps
(natural units, presets, numbered automatable settings like the built-in
effects'); the engine turns it into the DSP's `SynthParams`
(`library::synth_params`), sends changed settings to the playing synth
instead of rebuilding it, and for automation applies each lane to a copy
of the base settings on the audio thread (plain data, nothing allocated).
The window is `daw-ui::synth_ui`.

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
  crash it is offered back (decision 0013). Decision 0021 adds a note of
  the take being recorded (its unfinished WAV is mended and placed after a
  crash), a copy written at once and waited for before a plugin loads or
  opens its window (`SafetyCopy`), and details in the offer. The autosave
  thread stops on an explicit message (the session's safety writer keeps a
  sender alive).
- **Audio files** (format version 4, decision 0017) are entries
  `audio/<name>` of the container, stored as they came (recordings as
  24-bit WAV, imports in their own format). While a song is open they live
  in the audio folder of this running copy, inside its recovery folder:
  saving copies them in, opening extracts them there (never over a file
  already there), autosave copies refer to them in place.
- **Format version 5** (decision 0018) adds the song's key changes, each
  pattern's swing and tempo glides; all have defaults, so older files are
  read unchanged.
- **Format version 6** (decision 0019) adds the synth instrument.
- **Interchange** (decision 0020, `daw-formats::midi`, `dawproject`,
  `flac`): Standard MIDI Files and DAWproject, in and out; what a clip
  plays is worked out once, in `Pattern::played_in`, for the engine and
  both exporters. MIDI goes out at 960 ticks per quarter with glides as
  sixteenth-note tempo steps that keep the timing; DAWproject writes the
  program's own devices with their exact settings as JSON (and the EQ,
  compressor and limiter also as DAWproject's standard devices), plugin
  settings as `.clap-preset` and `.vstpreset`, pattern clips as the notes
  they play, and reads loops, nested clips and warps, folding repeated
  notes back into patterns and naming what it leaves out (`LeftOut`).
  Audio import: WAV, FLAC, MP3, OGG (phase 4); export: WAV and FLAC (FLAC
  encoded frame by frame as the song renders).
- **Opening and importing** (`session/files.rs`): Open takes any song
  file (another program's opens as a new unsaved song); Import and drops
  take sound and MIDI files.

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

## Audio *(phase 4)*

Decision 0017. An audio track has no instrument; its clips each play part
of an audio file. A clip starts on the timeline in ticks and plays its file
at the file's own speed from `offset` (frames of the file), with a volume
and fades in seconds; a tempo change keeps audio clips' lengths in seconds.

- **Sound in memory:** the interface's audio store reads each file of the
  song in the background (`daw-formats::audio`: WAV, FLAC, MP3, Ogg
  Vorbis), converts it to the engine's rate (`daw-dsp::audio`, our own
  windowed-sinc converter), summarises its loudness for drawing (`Peaks`),
  and gives the engine the sound (`Engine::register_audio`, kept in the
  sound library so exports have it).
- **Playing:** the compiled program holds each track's clips as spans on
  the song's frame timeline; the audio thread mixes them into the track
  before its effects, from any position (nothing to rewind), with an
  equal-power curve for fades and a 1.5 ms fade at every edge except where
  a clip continues the one before (a split).
- **Recording:** the input device's callback pushes its samples into the
  engine's input queue (`InputPort`); the audio thread empties it every
  block and, while a take runs, passes it on in chunks marked with the
  take's number and place (`RecordedAudio`) to the recorder thread, which
  writes the WAV file and the take's waveform as it grows. When the take
  ends, the session puts it where recording started, moved earlier by the
  delay out and back in that the devices report, as one undo step. The
  input device is opened only while the session wants it.

```text
 microphone ─▶ input callback ─(InputPort)─▶ audio thread ─(chunks)─▶ recorder ─▶ take.wav
                                                  │                                  │
                                       song plays on, metronome            session: clip at
                                                                            the start, one undo step
```

## The interface

Built with egui (eframe, wgpu renderer). All interface code is in `daw-ui`:

- `session`: the open song and everything done to it, with no drawing
  (editing commands, the selection, playing notes, the transport,
  recording, files; in `session/arrange` the views, tracks, patterns and
  clips; in `session/mixing` the mixer, effects and drum pads; in
  `session/audio` audio tracks, importing, recording and shaping clips,
  with the audio store and the recorder thread beside it). It also
  decides what Space plays, as a `PlaybackScope` given to the engine: the
  song, a part of it looping, or one pattern alone (decision 0014). Tested
  without a window.
- `workspace`: the window's layout (decisions 0011 and 0014), keyboard
  routing (note keys to the session, shortcuts to commands), menus,
  dialogs; the parts of the window ask it for dialogs and windows through
  `Intent`s.
  In `session/theory` the key where the pattern is, keeping notes in it,
  chord stamps, inversions, intervals, swing and any pattern length; in
  `session/conductor` the tempo, time signature and key at the play
  position and their changes along the song (decision 0018).
- `song_view`, `step_grid`, `piano_roll`, `mixer`, `effect_ui`,
  `transport`, `keyboard_panel`: the views; the custom-drawn ones draw
  only what is visible. `signature_row` is the song view's row of tempo,
  time-signature and key changes, pinned under its ruler.
- `export`: writes the song to a WAV file on a background thread, with a
  second engine rendering it offline through the same processing as
  playback (`daw_engine::OfflineRender`), block by block, with progress and
  cancelling.
- `keys`: note keys by physical position, the rebindable shortcut table.
- `text`: every user-facing string. `theme`: colours, fonts, spacing; dark
  and light themes follow the system by default.

## Distribution *(phase 6)*

Decision 0022. `cargo xtask dist` builds the release program and puts
beside it the licence notices (`xtask/src/notices.rs`: `cargo tree` per
system for which crates are built in, `cargo metadata` for where their
files are), the user guide as one HTML page (`xtask/src/guide.rs`) and the
icon files the program writes itself (`--write-icons`). `cargo xtask
installer` and `appimage` fill the templates in `packaging/` from
`daw-ident` and run NSIS and appimagetool; "Program builds" runs them on
Windows and on Ubuntu 24.04.

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

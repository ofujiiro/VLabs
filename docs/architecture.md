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
| MIDI input | Receives what MIDI controllers send | Asks the window's routing table where each message goes (decision 0033), then queues it for the engine or the window |
| Autosave | Writes recovery copies of the song in the background | Ordinary thread |
| Device watchers | Reopen the sound output and connect MIDI keyboards as they come and go | Ordinary threads |
| Input watcher | Opens the default sound input while recording, reopens it when it fails | Ordinary thread; the input's own callback is real-time and only queues samples |
| Audio files | Reads, decodes, converts and summarises the song's audio files, one at a time | Ordinary thread |
| Recorder | Writes takes of sound to WAV files as they come, keeps their loudness summary | Ordinary thread |
| Plugin scanner | A separate run of the program that inspects plugin files | A crash there cannot reach the program |
| SFZ reader | Reads an SFZ instrument and decodes its sounds (on up to eight helper threads), one thread per instrument | Ordinary thread; the engine builds the track again once it is done |
| Voicebank reader | Reads a voicebank's `oto.ini` and other files the first time a voice track needs it | Ordinary thread, one per voicebank |
| Voicebank picture reader | Decodes a voicebank's picture the first time the window shows its voice | Ordinary thread, one per picture; the window turns it into a texture |
| Voice renderer | Renders voice tracks' phrases, one at a time, keeping each by everything that shapes it | Ordinary thread; the audio thread plays rendered phrases like audio clips |

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
next one, the tempo following a curve of the family
`T0·[1 + (w^q − 1)x]^(1/q)` over the ticks between them (a straight line
for `q = 1`, bent by the glide's shape, decision 0026), whose seconds have
closed forms both ways (`daw_model::TempoPiece`; the engine's tempo table
uses the same arithmetic). Time-signature changes define bars, and every grid
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
- **Format version 6** (decision 0019) adds the synth instrument;
  **7** SFZ instruments and custom kits (decisions 0024, 0025); **8** the
  bend of a tempo glide (decision 0026); **9** voice instruments and each
  pattern's lyrics (decision 0027); **10** how each note is sung (decision
  0028); **11** note shapes and backwards or re-pitched clips (decision
  0031); **12** patterns' controller curves and the song's MIDI
  assignments (decision 0033). Each kept beside what it belongs to, with defaults, so older
  files are read unchanged.
- **Interchange** (decision 0020, `daw-formats::midi`, `dawproject`,
  `flac`): Standard MIDI Files and DAWproject, in and out; what a clip
  plays is worked out once, in `Pattern::played_in`, for the engine and
  both exporters. MIDI goes out at 960 ticks per quarter with glides as
  sixteenth-note tempo steps that keep the timing, and patterns' controller
  curves as pitch bends, controllers and pressure (and comes back with them,
  a file's sustain pedal holding its notes, decision 0033); DAWproject writes the
  program's own devices with their exact settings as JSON (and the EQ,
  compressor and limiter also as DAWproject's standard devices), plugin
  settings as `.clap-preset` and `.vstpreset`, pattern clips as the notes
  they play, and reads loops, nested clips and warps, folding repeated
  notes back into patterns and naming what it leaves out (`LeftOut`).
  Audio import: WAV, FLAC, MP3, OGG (phase 4); export: WAV and FLAC (FLAC
  encoded frame by frame as the song renders).
- **Opening and importing** (`session/files.rs`): Open takes any song
  file (another program's opens as a new unsaved song); Import and drops
  take sound and MIDI files. A MIDI file dropped on a track of notes, or
  on its open pattern, goes into that track as one pattern
  (`ImportedSong::add_into`, decision 0034); elsewhere it makes tracks.

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
  desktops. A plugin window that asks to be shown through XEmbed (its
  `_XEMBED_INFO` says "mapped") is told it is embedded and shown, as the
  XEmbed protocol has the host do (MT Power Drum Kit 2's window stayed
  blank before). On Windows, ordinary Win32 windows.
- **Whether a plugin has an editor** is asked of it once and remembered
  (`PluginHandle::has_editor`): the interface asks every frame, and a VST3
  plugin answers by making a whole editor (0.6 s for MT Power Drum Kit 2,
  and some plugins start over when another editor is made beside the open
  one).
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

## Plugins added in the program, and the sound library *(decision 0023)*

- **The program's own plugin folder** (`plugins/` in its data folder, with
  `clap/` and `vst3/` inside) is always searched, with the usual folders
  and the person's extra folders (settings). `daw-plugin-host::install`
  puts plugins there from files, bundles, folders and `.zip` archives
  (entry names checked so nothing lands outside, sizes limited), tells
  VST2 plugins, installers and plugins for other systems apart by their
  endings and first bytes, and removes only what is inside that folder.
- **Windows plugins on Linux** go to `plugins/windows/`, which the search
  does not look in; when Wine and yabridge are installed, `yabridgectl add`
  (once) and `sync` set them up in `~/.vst3/yabridge` and
  `~/.clap/yabridge`, which the search finds. Installers run through Wine
  (Linux) or are opened by the shell (Windows).
- **Off the interface thread:** `daw-app`'s plugin worker does the
  copying, unpacking, installing and yabridge calls, one job at a time,
  then the search runs again (queued if one is under way). The Plugins
  window only asks (`Request::AddPlugins`, `RemovePlugin`,
  `SetUpWindowsPlugins`) and shows what each job reports.
- **The sound library:** `daw-formats::library` lists a folder once, in the
  background (hidden files skipped, up to half a million files), sorted
  case-blind; browsing a folder and searching (every word, anywhere in the
  path) then work on that listing in memory. The drawer (`library_panel`)
  keeps one listing per library folder and lists again on request.
- **Previews** play through the engine: the file is decoded and resampled
  on a thread of its own, then handed to the audio thread, which mixes it
  over everything after the master effects (like the metronome) at about
  -3 dB, and hands the sound back through the garbage queue when it ends
  or is replaced. A number tells previews apart, so the interface knows
  whether the one it asked for is still playing.
- **Dragging from the library** uses egui's drag-and-drop payload
  (`LibraryDrag`); the song view takes it when the pointer is let go over
  the tracks, exactly as it takes files dropped from the desktop.
- **SoundFont files** use the model's existing `SoundSource::File`: the
  engine's sound library loads them (once per file) with the same player
  as the bundled piano; a file that is gone gives `Problem::MissingFile`
  and a plain sound in its place.

## SFZ instruments *(decision 0024)*

- **Three layers.** `daw-formats::sfz` reads the text (headers and their
  levels, `#define`, `#include`, note names, sound paths found whatever
  their letter case) and decodes the sounds, on several threads, into a
  `daw_dsp::sampler::SamplerBank`: regions and decoded sounds, with what
  the file uses that is not played (`LeftOut`) and the sounds it could not
  find. `daw-formats` depends on `daw-dsp` (a lower layer) for that bank.
  `daw_dsp::sampler::Sampler` plays a bank like the other built-in
  instruments: voices with four-point interpolation, envelopes, loops
  (also the loop a WAV file holds), filters, round robins, keyswitches,
  release sounds, choke groups and voice limits, with nothing allocated on
  the audio thread.
- **Controllers at their starting values.** The program sends instruments
  no controllers, so what controllers would decide is decided once, at
  their starting values (`set_ccN`, or MIDI's usual 100 for volume, 64 for
  pan, 127 for expression, 0 for the rest): which regions play
  (`loccN`/`hiccN`), crossfades by controller, and the settings they move
  (envelope times and levels, cutoff, resonance, volume, pan, tuning,
  offsets). LFOs and pitch envelopes are reported as left out only when
  they move something.
- **Read in the background.** The engine cannot read sound files (decision
  0003), so the program hands its `SoundLibrary` the reader as a plain
  function (`set_sfz_reader`). With `set_background_loading(true)` (the
  program, the interface tests) the library starts one thread per
  instrument file and builds a silent stand-in meanwhile; the engine keeps
  the tracks waiting and builds them again at the first `sync` after the
  reading finished, then reports once what the instrument plays without
  (`Problem::PartlyRead`). Offline rendering waits for every instrument.
  Instruments read are shared by file between the live engine and
  exports, and forgotten when another song opens.
- **In the interface:** `Instrument::Sfz { file }` (file format version
  7) is added from the library, by dropping or importing a `.sfz` file, or
  from the instrument chooser; the status line says which instruments are
  being read while it has nothing else to say, and the track's line says
  "reading…" until its instrument is ready.

## Custom kits *(decision 0025)*

- **Model:** `Instrument::Kit { pads }`; each `KitPad` has the drum pads'
  settings (`PadSetting`, with its key), a name and the song's audio file
  it plays (`AudioId`). Pads take free keys from C1 (36) up, at most 128;
  `Project::add_kit_pad` and `remove_kit_pad` (which also takes the pad's
  notes out of the track's patterns). An audio file stays in the song while
  a clip or a pad uses it. A custom kit counts as drums (`is_drums`).
- **Sound:** the engine builds a `daw_dsp::sampler::Sampler` from the pads
  (`library::kit_sampler`): one one-shot region per pad, its volume, pan
  and tuning from the pad, a shortened length fading out by its end. A pad
  whose sound is still being decoded is added when `register_audio` brings
  it (the kit is built again then).
- **Adding sounds** goes through the audio import path (`ImportPlace` with
  `pad_of: Some(track)`): copied and decoded in the background, then made a
  pad, one undo step each. The step grid works from rows (a key and a name):
  the bundled kits give their fifteen, a custom kit its pads.
- **Files:** file format version 7; DAWproject carries the kit as one of
  the program's own devices with its sounds in the archive.

## Singing voices *(decisions 0027 and 0028)*

A voice track's instrument names a voicebank; its patterns keep, beside
their notes and by note, each note's lyric (`Pattern::lyrics`) and how it
is sung (`Pattern::expression`: pitch points, vibrato, colour, breath), so
a note stays a small value that is cheap to copy.

```text
 notes + lyrics + expression ──▶ phrases (model) ──▶ per phrase: aliases (model)
                                                     pitch curve (model)
                                                     recordings (formats::utau)
                                                          │ worker thread
                                                          ▼
                              audio thread ◀── rendered phrase (dsp::voice), kept by key
```

- **`daw_model::lyrics`** turns romaji and kana into the names a classic
  UTAU voicebank gives its recordings (VCV, then CV).
- **`daw_model::expression`** says how a voice's notes fall into phrases
  (one note at a time, a rest of 50 ms or more starts a phrase) and works
  out the pitch sung through a phrase (`PhrasePitch`): each note takes over
  at its first pitch point, gliding from wherever the voice is then, follows
  its points' shapes and its vibrato. Notes without points glide 80 ms
  around their start, as in 7a. The engine renders with it and the piano
  roll draws with it, so what is drawn is what is sung.
- **`daw_formats::utau`** reads and installs voicebanks (`oto.ini`,
  `prefix.map`, `character.txt`, the readme; UTF-8 or Shift-JIS), and
  their pictures: `character.txt`'s small picture and `character.yaml`'s
  portrait (OpenUtau's), BMP, PNG or JPEG, scaled down to 1024 points when
  larger (decision 0029).
- **`daw_dsp::voice`** analyses a recording (YIN pitch, a mark on every
  period) and sings a phrase by pitch-synchronous overlap-add at the pitch
  it is given as a function of time: each syllable starts its
  preutterance before its note and crossfades over the note before, as
  UTAU's wavtool joins them. Per note: a gain (velocity, on the sampler's
  square curve), a formant ratio (colour: grains read faster or slower)
  and breath (a granular whisper of the same syllable, above 1 kHz).
- **`daw_engine::voices`** places a track's notes in seconds through the
  tempo map, cuts them into phrases, renders each on the voice renderer
  thread (at once when rendering offline) and keeps it by a hash of
  everything that shapes it, so an edit renders again only what it
  changed; a phrase still rendering is silent.
- **In the interface:** the Voicebanks window (`voicebanks`), voice tracks,
  lyrics and the expression edits (`session/voice`), lyrics typed in the
  piano roll, and the sung pitch line with its Pitch mode, points, vibrato
  handles and panels (`pitch_line`); the lane under a voice's notes shows
  velocity, colour, breath or tempo. The voicebank's picture is drawn
  faintly on its track's header and clips and behind its piano roll's
  notes (`voice_picture`); the session reads each picture once, in the
  background, and keeps it as a texture (`session/voice`, decision 0029).

## MIDI controllers *(decision 0033)*

```text
 controller ─▶ daw-devices (parse) ─▶ daw-app ─▶ daw_ui::midi::MidiRouter ── route(table) ──┐
                                                         │                                    │
                     keys, wheels, pedals, pressure ◀────┘ (engine's live queue)      window ◀┘
                     → the instrument in focus                                 (learning, knobs and
                                                                               buttons with a job,
                                                                               transport buttons)
```

- **Where a message goes** is decided on the MIDI thread by
  `daw_ui::midi::route`, from a table the window rewrites each frame
  (`RouteTable`: what it waits for, and the controls with a job). Keys,
  pads, the wheels, pressure and the pedals go straight into the engine's
  live queue (`MidiSender::note_on`, `sustain`, `control`); transport
  messages (MIDI Machine Control, Start, Continue, Stop) and the controls
  with a job go to the window; bank select and channel mode messages go
  nowhere.
- **Instruments take controllers** through `Instrument::control`
  (`daw_dsp::Control`: the pitch wheel from -1 to 1, a numbered controller
  or pressure from 0 to 1). Each included instrument keeps a `Performance`
  (bend, modulation, pressure, expression) and answers what it can: the
  synth bends two semitones and deepens its LFO's vibrato, SoundFonts pass
  the wheel, modulation and expression to rustysynth, the SFZ sampler bends
  each voice by its region's `bend_up`/`bend_down` and scales its output by
  expression; custom kits do not answer. Plugins get everything: CLAP as
  MIDI on their MIDI note port, VST3 as the parameters their
  `IMidiMapping` names (asked once, at load).
- **On the audio thread** a live key comes up on the slot it went down on
  (`live_keys`), the pedal comes up wherever it held notes, and when the
  focus moves the slot left behind gets `Control::REST`.
- **Curves in patterns** (`Pattern::controls`, `daw_model::controls`): one
  `ControlLane` per controller, points in pattern ticks, a straight line
  between. `Pattern::controls_played_in` says what a clip plays (repeated,
  swung, pinned at the clip's edges), for the engine and the MIDI
  exporter. The engine compiles them per track (`CompiledControl`) and
  looks at them every 64 frames of a block, sending moves as block events
  at their frames; outside a clip, and when playback stops or moves, the
  wheel, modulation, pressure and expression go back to rest.
- **Recording** sends the hand's moves of the recording slot as
  `EngineEvent::RecordedControl` (one per controller per block), starting
  with the controllers where they stood; the pattern's old curve for a
  controller the hand moved is not played meanwhile. The session thins each
  pass (`controls::thin`) and writes it into the take's undo step.
- **Knobs given settings** are `Project::midi_links` (`MidiLink`: a
  `MidiControl` and a `LinkTarget`, a track's or the master's setting as
  automation names it); a move is `Project::set_linked`, merged into one
  undo step per burst. **Buttons given commands** live in the settings
  (`midi_buttons`, by command), beside the keyboard shortcuts.
- **Learning** a control waits in the session (a setting: `learn_midi`) or
  the workspace (a command: the shortcuts window's Learn); the first
  matching control gets the job.

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
  only what is visible. `pitch_line` is the sung pitch of a voice track,
  drawn over the piano roll's notes (decision 0028); `voice_picture` places
  and draws a voicebank's faint picture (decision 0029). `signature_row` is the song view's row of tempo,
  time-signature and key changes, pinned under its ruler. `tempo_lane` is
  the tempo line itself (points, glides with their shapes and handles,
  their panels; decision 0026), drawn in the signature row and in the lane
  under the piano roll.
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

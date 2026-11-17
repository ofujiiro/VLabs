# 0015. Hosting plugins: one boundary, two halves, a scanner process

- **Status:** accepted, November 2026 (builds on decision 0009's spike)

## Context

Phase 3 brings third-party instruments and effects in the two v1 formats,
CLAP and VST3 (brief). The phase 0 spike proved both formats load, play,
save and restore state and show their editors on Windows and Linux. Turning
that into the product raises five questions: what the engine sees of a
plugin, which thread does what, how plugins are found without risking the
program, where their settings are kept, and how their parameters change over
the song (automation).

## Decision

**One boundary, in plain data.** `daw-plugin-host` offers every plugin, of
either format, as two halves:

- a **plugin handle** for the main thread: describe, parameters (names,
  ranges, current values, display text), save and load state, open and close
  the editor, and *idle* (the housekeeping a plugin asks of its host:
  callbacks, timers, file descriptors, window events);
- a **plugin processor** for the audio thread: process one block of stereo
  audio with the block's note and parameter events at frame offsets, stop.

No CLAP or VST3 type crosses the boundary. Parameter values cross it as
fractions of their range (0 to 1); each format converts. Running a plugin in
its own process later means implementing the same two halves over shared
memory; the engine does not change.

**Threads.** Loading, activating, state, parameter information, editors and
idle happen on the thread that created the plugin: the window's thread for
live playback, the export thread for an export (which creates its own
instances). Processing happens on the audio thread, inside
`daw_rt::permit_alloc`, because third-party code may allocate; our code
around it does not (event lists and parameter queues are reserved when a
plugin loads). To remove a plugin, the audio thread stops its processor and
hands it back through the garbage queue; the main thread then drops both
halves. Our editor windows are serviced every frame while one is open.

**Formats.** CLAP through `clack-host` 0.2. VST3 through our own host layer
on the `vst3` 0.3 bindings (the spike's thousand lines, hardened: fixed
pools for events and parameter changes, no allocation while processing).

**Scanning in a separate process.** The program runs itself with
`--describe-plugin <format> <file>` for each plugin file, one at a time, with
a time limit; the child loads the file, lists its plugins (from the
factory's descriptions, without creating instances) and prints them. A
plugin that crashes or hangs only ends its child. Results are cached in the
program's data folder, keyed by path, size and modification time; failures
are remembered until the file changes. Folders: on Linux `~/.clap`,
`/usr/lib/clap`, `/usr/local/lib/clap`, `~/.vst3`, `/usr/lib/vst3`,
`/usr/local/lib/vst3` (so yabridge's bridged Windows plugins appear); on
Windows the common and per-user `CLAP` and `VST3` folders; plus `CLAP_PATH`.
Scanning runs in the background when the program starts.

**State.** A plugin's settings are what its own state call returns. They are
captured when the song is saved, autosaved or exported, and just before a
plugin is removed, and kept in the song file as separate entries of the
`.teto` container (`plugins/…`), referenced from `project.json`. A plugin
starts from its saved state when it is created. Tweaking a plugin's editor
is not an undo step (the plugin owns those settings), but it does mark the
song as having unsaved changes.

**Automation.** A track can have automation lanes: a target and points
(musical time, value 0 to 1), straight lines between points. Targets: the
track's volume and pan, any setting of a built-in effect on the track, any
automatable parameter of its plugins. The engine evaluates the lanes at each
block's song position while the song plays: plugin parameters as parameter
events, built-in effects through their settings, volume and pan with a short
ramp to avoid clicks. Lanes are edited and undone like everything else.

## Alternatives

- **Scanning inside the program, with crash protection** (signal handlers,
  structured exceptions). Rejected: a plugin that corrupts memory or hangs
  cannot be recovered from in-process; the brief asks for a separate process.
- **A separate scanner executable.** Rejected for now: the program running
  itself needs no second file to ship and keeps one build.
- **Plugin states inside `project.json` as text (base64).** Rejected: states
  of sample-based plugins reach megabytes and would make the song description
  slow to read and unpleasant to inspect; the brief puts them in the
  container.
- **Plugin edits as undo steps.** Rejected: the host cannot see inside a
  plugin's state reliably and saving it after every knob turn would be slow;
  hosts generally leave plugin editors outside their undo history.

## Consequences

- Third-party code runs in the program's process once scanned; a plugin that
  crashes while playing still takes the program down. Out-of-process hosting
  stays possible behind the same boundary (recorded for later).
- Latency that plugins report is not compensated in phase 3: a plugin with
  look-ahead plays slightly late against other tracks.
- Exports instantiate the song's plugins again on the export thread, from
  freshly captured states.

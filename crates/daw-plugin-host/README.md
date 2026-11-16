# daw-plugin-host — third-party plugins

Finds, loads and runs instruments and effects made by others, in the two
plugin formats of v1: **CLAP** and **VST3**.

## What belongs here

- **Scanning** the plugin folders, in a separate process, so one broken plugin
  can neither crash the application nor block its start.
- **Loading** a plugin, sending it notes and audio, getting audio back.
- **Editor windows:** opening and closing a plugin's own window.
- **State:** saving and restoring a plugin's settings with the project.
- **Parameters:** listing them, reading them, automating them.

## Rules

- **One boundary for every plugin.** The engine talks to plugins only through
  the interface this crate defines, designed so that a plugin can later run
  in its own process without changing the engine.
- **Unsafe code is confined and explained.** Talking to plugins means calling
  foreign code. Each module that does so opts in to `unsafe` explicitly, and
  every unsafe block carries a `// SAFETY:` comment.
- **Never link or bundle plugin code.** Reference plugins (Surge XT, a second
  instrument, one effect) are downloaded test fixtures only.
- **Not in v1, but not designed out:** VST2, LV2, a 32-bit plugin bridge.
  Nothing here should make them impossible later. FL Studio's native plugins
  cannot be loaded by any other host; do not attempt it.
- On Linux, scan the standard folders, including `~/.vst3` and `~/.clap`.
  Windows plugins bridged by yabridge then appear as native ones with no
  work on our side; this is documented for users, not rebuilt by us.
- This crate depends on no project data, no engine and no interface code.

## Roadmap

Phase 0 (risk spike: load a CLAP and a VST3 instrument on both systems),
phase 3 (scanner, hosting, editor windows, state, automation).

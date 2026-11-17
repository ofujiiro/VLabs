# Handover — where VLabs stands (November 2026)

Read `CLAUDE.md` first; it points to the brief, roadmap and decisions.

## Done

- Phases 0–3 of the roadmap: foundations, first sound, making a beat,
  plugins and automation. The owner confirmed the phase 1 and 2 builds.
- Phase 3 (all on `main`): CLAP and VST3 hosting (decision 0015), the plugin
  search in a separate run of the program with a cache, the instrument
  chooser, plugin effects in the mixer, plugin windows (X11/Win32),
  automation lanes in the song view (decision 0016), plugin settings saved
  in file format v3, typed engine problems worded in `daw-ui`, a sound-device
  watchdog, and explanations for plugin files made for another system.
- CI is green: `ci.yml`, and `spikes.yml` runs the plugin tests with the
  reference plugins (Surge XT, Surge XT Effects, Dexed) on Linux and Windows.
- Milestone build: "Program builds" run 44097772066 (before the last commit,
  which only improves plugin-search messages).

## Waiting on the owner

- The hands-on check of the phase 3 build (ROADMAP.md, "Phase 3 milestone:
  things to try"). Untick → tick in ROADMAP.md when confirmed.
- Whether to trigger a fresh "Program builds" run with the latest commit.

## Not verified (say so if asked)

- Closing a plugin window with its own close button on a real desktop (the
  test screen has no window manager).
- The program with plugins on Windows by hand (automated tests cover it).
- nyasynth's window: on Xvfb its toolkit (baseview) wants an sRGB GL config
  that llvmpipe-on-Xvfb lacks, and its panic aborts the whole program. Its
  sound works (Linux build, VST3 and CLAP).

## Known gaps carried forward (ROADMAP.md lists them)

- No setting for extra plugin folders; automation is drawn, not recorded; no
  master-bus lanes; no instrument swap on a track; plugins run in-process (a
  crashing plugin takes the program down); plugin latency not compensated.

## Next: phase 4 — Audio

Recording from a microphone or interface, audio clips on the timeline, cut,
trim, fades, waveforms, file import (WAV, FLAC, MP3, OGG). Start with a
design record for the audio-clip screens (two or three concepts, as before).

## Practical notes

- Disk allowance is tight: `target/debug` grows past 20 GB; delete
  `target/debug/incremental` and old `target/debug/build/daw-*` hash folders
  when space runs low.
- The Linux plugin CI job recompiles from scratch after a failure (its cache
  is only saved on success): expect ~15 minutes.
- Reference plugins for local tests live outside the repo; see CLAUDE.md.

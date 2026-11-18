# Handover — where VLabs stands (November 2026)

Read `CLAUDE.md` first; it points to the brief, roadmap and decisions.

## Done

- Phases 0–4 of the roadmap: foundations, first sound, making a beat,
  plugins and automation, audio. The owner confirmed the phase 1 and 2
  builds; phases 3 and 4 await their hands-on check.
- Phase 4 (decision 0017), on branch `claude/relaxed-einstein-0sb43g`:
  audio tracks and clips in the model (split, trim, clear a span, move
  between audio tracks, tempo changes keep audio in seconds); decoding WAV
  (our own reader), FLAC (claxon), MP3 (nanomp3), Ogg Vorbis (lewton);
  our own windowed-sinc rate converter and waveform summaries in
  `daw-dsp::audio`; file format v4 with `audio/<name>` entries; the engine
  plays audio spans and records through `InputPort` → audio thread →
  `RecordedAudio` chunks → the recorder thread; `AudioInput` in
  `daw-devices` (opened only while recording); the session's audio store
  (background decoding), importing, recording, clip editing; the song
  view's waveforms, trim and fade handles, split (menu, Ctrl+B), import
  (Ctrl+I, dropped files), the live take and input meter; the program
  keeps audio in the recovery folder and recovers it.
- A fix for the owner's report of a stacking noise when clicking a drum
  sound's name: the name now plays on the press event, once (it also
  played nothing on very quick clicks). The stacking itself did not
  reproduce on Linux; the owner was asked to retry.
- The roadmap has the owner's later request: singing voices with open
  voicebanks (OpenUtau-style; OpenUtau is MIT), after v1.
- `cargo xtask ci` passed locally on the final commit (fmt, clippy, build,
  all tests, cargo-deny on both targets).

## Waiting on the owner

- The hands-on checks of the phase 3 and phase 4 builds (ROADMAP.md, "things
  to try"), and whether the drum-name noise still happens (and on which
  system). No downloadable build of phase 4 exists yet: start the
  "Program builds" workflow (by hand, or with a `milestone-*` tag).

## First thing for the next session

- All phase 4 work is pushed to `claude/relaxed-einstein-0sb43g` (not
  merged into `main`, no pull request). `ci.yml` runs only on `main` and
  pull requests, so CI was started by hand on the branch: CI run
  44213391156 and Spikes run 44213410087 (plugin tests). Both were still
  running when this session ended: check their results first (Windows has
  not been tried by hand); fix anything they found.

## Not verified (say so if asked)

- Recording from a real microphone (tested with a virtual one fed by the
  program's own output: takes land within about 5 ms of the beat there).
- The import file dialog (no dialog program on the test screen) and
  dragging files from a real file manager (simulated drops are tested).
- Phase 4 on Windows by hand; by CI, see the runs above.
- Closing a plugin window with its own close button on a real desktop; the
  program with plugins on Windows by hand.

## Known gaps carried forward (ROADMAP.md lists them)

- Audio: no time-stretching, no input monitoring, no by-hand latency
  adjustment, no takes per loop pass, no multi-clip selection or clip copy
  and paste, sound kept in memory, recordings stored as WAV (FLAC later),
  a crash mid-take loses that take.
- Earlier: no extra plugin folders setting; automation drawn, not recorded;
  no master-bus lanes; no instrument swap; plugins in-process; plugin
  latency not compensated.

## Next: phase 5 — Theory and rhythm

Key and scale highlighting, snap-to-scale, chord stamps, inversions,
transposition by interval, tempo and time-signature track, tuplets, swing.
Start with a design record for the tempo track and the scale tools (two or
three concepts, as before). The model's `set_tempo` already keeps audio
clips in seconds when the tempo changes anywhere.

## Practical notes

- Disk allowance is tight: `target/` grows past 20 GB; delete
  `target/debug/incremental` and old `target/debug/build/daw-*` hash
  folders when space runs low.
- The virtual screen needs `mesa-vulkan-drivers` and `libegl1`; recording
  tests need the virtual microphone (CLAUDE.md, "Testing without
  hardware").
- The Linux plugin CI job recompiles from scratch after a failure (its cache
  is only saved on success): expect ~15 minutes.
- Reference plugins for local tests live outside the repo; see CLAUDE.md.

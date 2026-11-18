# Handover — where VLabs stands (November 2026)

Read `CLAUDE.md` first; it points to the brief, roadmap and decisions.

## Done

- Phases 0–4 of the roadmap: foundations, first sound, making a beat,
  plugins and automation, audio. The owner confirmed the phase 1 and 2
  builds; phases 3 and 4 await their hands-on check. Phase 4 is merged
  into `main` (pull request 1) and its CI runs passed on both systems.
- Phase 5, theory and rhythm (decision 0018), on branch
  `phase-5-theory-rhythm`, **not pushed yet** (the session had no push
  access; the owner gives it):
  - model: `theory` (scales, the key map, chords, inversions, steps of the
    key), tempo glides with exact formulas, `Swing` (integer warp and its
    inverse), `Project::change_tempo`, `Project::change_meter` (keeps the
    song in its bars), grids per bar (`MeterMap::grid_floor`,
    `grid_round`), file format v5 and its sample;
  - engine: the tempo table follows glides; patterns are swung as they are
    compiled; offline renders check a glide and swing;
  - session: `session/theory` (key where the pattern is, keeping notes in
    it, chord stamps, inversions, intervals, swing, any pattern length) and
    `session/conductor` (tempo, signature and key at the play position;
    changes along the song); `pattern_meter` gives the pattern editors the
    song's bars from where the pattern starts; recording into a swung
    pattern writes notes where they play back as heard;
  - interface: key button; Draw, Snap (tuplets, "Keep notes in the key"),
    Swing and Transpose menus; Alt+Up/Down inverts; the piano roll's key
    shading, chord preview and end handle; the step grid's swung hits and
    end handle; the song view's signature row (`signature_row.rs`).
- Checked on this setup: `cargo xtask ci` (all checks), the reference
  plugins' tests as the Spikes job runs them, and the program itself on a
  virtual screen and sound card: screenshots of every new screen, and swing
  measured in its recorded output (hits alternate 167/83 ms at 120 BPM,
  triplet feel).

## Waiting on the owner

- Push access for the phase 5 branch (then CI runs on Windows as well).
- The hands-on checks of phases 3, 4 and 5 (ROADMAP.md, "things to
  try"), and whether the drum-name noise of phase 4 still happens.

## First thing for the next session

- Once the branch is pushed: check its CI runs (Windows has not built
  phase 5 yet), and start the "Program builds" workflow for the owner.

## Not verified (say so if asked)

- Phase 5 on Windows (by hand or by CI).
- Listening: swing and glides were measured, not heard.
- From earlier phases: recording from a real microphone, the import file
  dialog, dragging files from a real file manager, phase 4 on Windows by
  hand, closing a plugin window with its own close button on a real
  desktop.

## Known gaps carried forward (ROADMAP.md lists them)

- Phase 5: no chord track, no beat groupings for odd meters, no "make a
  tuplet" command, swing per pattern only, the delay keeps the first
  tempo, Escape does not close the location line's menus.
- Audio: no time-stretching, no input monitoring, no by-hand latency
  adjustment, no takes per loop pass, no multi-clip selection or clip copy
  and paste, sound kept in memory, recordings stored as WAV (FLAC later),
  a crash mid-take loses that take.
- Earlier: no extra plugin folders setting; automation drawn, not
  recorded; no master-bus lanes; no instrument swap; plugins in-process;
  plugin latency not compensated.

## Phase 6 in progress (branch `phase-6-completing-v1`, local, NOT pushed)

Done and committed (each with tests that pass in the dev container):

- Decisions 0019 (synth), 0020 (songs in and out), 0021 (autosave
  polish), 0022 (installers, notices, guide); version 0.6.0.
- The synth: DSP (`daw-dsp::synth`), model (`SynthSettings`, presets,
  numbered automatable settings), engine (live settings, automation on
  the audio thread), file format v6 with its sample, the window
  (`daw-ui::synth_ui`, three columns, fits 960x600), chooser entry,
  window buttons, automation lanes. `Pattern::played_in` is the one place
  that says what a clip plays.
- `daw-formats::midi` (read, write, song to MIDI with glides as steps,
  import with instruments by channel and program) and
  `daw-formats::dawproject` (export with VLabs's own devices plus
  standard EQ/compressor/limiter, `.clap-preset`/`.vstpreset`; import with
  loops expanded, repeats folded back into patterns, `LeftOut` notices).

Left to do, in order:

1. Interface for files (decision 0020): Open accepts `.dawproject` and
   `.mid` (new unsaved song; the host's open dialog filter), Import (Ctrl+I,
   was "Import audio") takes MIDI files at the play position and drops,
   one Export dialog (sound file WAV/FLAC + quality, DAWproject, MIDI),
   status line naming what was left out; `Host` gains the file choosers.
2. FLAC export: `daw-formats::flac` writer streaming frames with
   `flacenc::encode_fixed_size_frame` (rewrite STREAMINFO at the end),
   move `flacenc` from dev- to normal dependencies, `ExportFormat` gains
   the container; test by decoding with claxon.
3. Autosave polish (decision 0021): take-in-progress note and WAV repair
   on recovery, safety copy before plugin loads/windows, clearer dialog,
   several orphans in a row.
4. Distribution (decision 0022): `daw-app --write-icons`, `cargo xtask
   notices` (cargo metadata, licence texts, run in `ci`), `cargo xtask
   installer` (NSIS script from daw-ident), `cargo xtask appimage`
   (appimagetool 1.9.1, sha256 ed4ce84f…), `build.yml` jobs (AppImage on
   ubuntu-24.04), user guide `docs/user-guide.md` to HTML (pulldown-cmark
   in xtask), menu "User guide" (F1).
5. Validate an exported `project.xml` against the official `Project.xsd`
   (github.com/bitwig/dawproject) with `xmllint` (now in the dev image).
6. Run the program on the virtual screen (synth window screenshots, record
   a synth note and measure its pitch), `cargo xtask ci`, the plugin
   tests, update ROADMAP/README/architecture/CLAUDE.md, scan every
   commit's patch for server details, push, write the PR text.

## Next: phase 6 — Completing v1

Subtractive synth, DAWproject and Standard MIDI File import and export,
FLAC export, autosave polish, installers (Windows installer, Linux
AppImage), generated third-party notices, user guide. Start with a design
record for the interchange formats: tempo glides map to DAWproject's tempo
automation (linear), swing is written into the notes on export, MIDI files
get glides as small steps (decision 0018).

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

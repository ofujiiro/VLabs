# Handover — where VLabs stands (November 2026)

Read `CLAUDE.md` first; it points to the brief, roadmap and decisions.

## Done

- Phases 0–5, merged into `main` (phase 5 is pull request 2, CI green on
  Windows and Linux). The owner confirmed the phase 1 and 2 builds;
  phases 3, 4 and 5 await their hands-on check.
- Phase 6, completing v1, on branch `phase-6-completing-v1` (pushed, pull
  request to be opened by the owner): decisions 0019–0022; the synth
  (`daw-dsp::synth`, `daw-model::SynthSettings`, engine settings and
  automation, `daw-ui::synth_ui`, file format v6); MIDI files and
  DAWproject in and out, FLAC export, the Open/Import/Export menus;
  autosave polish (takes survive a crash, safety copies before plugins,
  a clearer offer); right-click menus to place and swap a track's
  patterns (the owner's report); the NSIS installer and the AppImage
  (`packaging/`, `cargo xtask installer` / `appimage`, "Program builds"),
  generated licence notices (`cargo xtask notices`, a CI step), the user
  guide (`docs/user-guide.md`, F1); version 0.6.0.
- Checked on this setup: `cargo xtask ci` (all checks), the reference
  plugins' tests as the Spikes job runs them, exported DAWproject files
  against the official schema (`xmllint`), and the program on a virtual
  screen and sound card: the pattern menus, the chooser, the synth window,
  the export dialog and the recovery dialog in screenshots; a synth note
  recorded from the program measured 261.66 Hz for middle C.

## Waiting on the owner

- Open the pull request for `phase-6-completing-v1` (CI then runs on
  Windows), and start "Program builds" to get the installer and AppImage.
- The hands-on checks of phases 3 to 6 (ROADMAP.md, "things to try").
- A decision on VST2 (`.dll`) plugins: the brief keeps them out of v1 for
  licensing reasons (Steinberg no longer licenses VST2; clean-room headers
  carry legal risk).
- A decision on bundling Wine and yabridge with the Linux build (their
  licences, LGPL and GPL, and their size), versus setting up the person's
  own Wine and yabridge.

## Next: plugins and sound libraries (the owner's request)

A plugin manager inside the program (add a plugin file, folder or archive;
see every plugin and its state; remove; extra folders), Windows plugins on
Linux through Wine and yabridge, sound libraries (folders of samples to
browse, preview and drag in), SoundFont and SFZ instruments. See
ROADMAP.md.

## Not verified (say so if asked)

- The installer and the AppImage on real machines: CI builds them; running
  the downloaded build tools (appimagetool) was not permitted in the
  development session, so neither was built locally.
- Opening our DAWproject files in Bitwig Studio, Studio One or Cubase.
- Listening: the synth was measured, not heard.
- Phase 6 on Windows by hand (CI runs every test on Windows).
- From earlier phases: recording from a real microphone, the import file
  dialog, dragging files from a real file manager, phase 4 on Windows by
  hand, closing a plugin window with its own close button on a real
  desktop.

## Known gaps carried forward (ROADMAP.md lists them)

- Phase 6: synth without unison or its own effects; MIDI without
  controllers; DAWproject without key or clip volume; unsigned installer.
- Phase 5: no chord track, no beat groupings for odd meters, no "make a
  tuplet" command, swing per pattern only, the delay keeps the first
  tempo, Escape does not close the location line's menus.
- Audio: no time-stretching, no input monitoring, no by-hand latency
  adjustment, no takes per loop pass, no multi-clip selection or clip copy
  and paste, sound kept in memory, recordings stored as WAV.
- Earlier: no extra plugin folders setting (the plugin manager adds it);
  automation drawn, not recorded; no master-bus lanes; no instrument swap;
  plugins in-process; plugin latency not compensated.
- On Windows, VST3 class ids are kept in the order the plugin reports them
  (COM order); DAWproject converts them at its edge, but a song saved on
  Windows and opened on Linux would not find the same VST3 plugin by id.

## Practical notes

- `target/` grows past 20 GB and `dist/` holds a 55 MB program: keep both
  out of folders that are backed up or synced.
- The reference plugins' tests and the schema check need extra tools
  (Xvfb, `xmllint`); NSIS's `makensis` checks the installer script on
  Linux too.
- The Linux plugin CI job recompiles from scratch after a failure: expect
  ~15 minutes.

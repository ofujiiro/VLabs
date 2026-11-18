# Handover — where VLabs stands (November 2026)

Read `CLAUDE.md` first; it points to the brief, roadmap and decisions.

## Done

- Phases 0–5, merged into `main` (phase 5 is pull request 2, CI green on
  Windows and Linux). The owner confirmed the phase 1 and 2 builds;
  phases 3, 4 and 5 await their hands-on check.
- Phase 6, completing v1, merged into `main` (pull request 3): decisions
  0019–0022; the synth
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

- Plugins and sound libraries (the owner's request after phase 6), merged
  into `main` (pull request 4): decision 0023;
  `daw-plugin-host::install` (plugins put in place from files, folders and
  archives; yabridge), the Plugins window (`daw-ui::plugin_manager`) and
  `daw-app`'s plugin worker; AIFF reading; `daw-formats::library`
  (listing folders), the engine's preview voice, the library drawer
  (`daw-ui::library_panel`, F6); SoundFont file tracks with their presets
  in the track menu. Two crash dumps a test had left in the tree in phase
  5 were removed (they held only the build container's environment) and
  core files are ignored.
- Checked for that branch: `cargo xtask ci`, the reference plugins' tests
  (with a new one: Dexed added from its folder is found by the real
  search, and removed), and the program on the virtual screen: the
  library listed a folder, a click played the kick (recorded: exactly its
  0.6 s, at the expected level), a loop dragged from the library landed
  on bar 2, an AIFF loop double-clicked landed at the play position, a
  SoundFont track played middle C at 262 Hz with piano overtones, and the
  Plugins window removed a plugin added to the program's folder.
- The owner decided (November 2026) to keep VST2 (`.dll`) plugins out of
  v1, as the brief says (decision 0023); a `.dll` added in the Plugins
  window is explained.

- After those merges, CI failed on Windows; the fixes were merged in pull
  request 5: two install tests wrote Linux binary headers into their test
  plugins, and the installer script pieced file paths together with `/`,
  which NSIS's `File` cannot follow on Windows (the script now gets each
  file's whole path).
- The owner decided (November 2026) not to bundle Wine and yabridge with
  the Linux build (decision 0023), and to drop the ".mse" question.

## Waiting on the owner

- The hands-on checks of phases 3 to 6 and of the plugins round
  (ROADMAP.md, "things to try").

## Next

- SFZ instruments (a sampler of their own), then the singing voices
  (ROADMAP.md).

## Not verified (say so if asked)

- The installer and the AppImage on real machines. CI made the AppImage;
  the Windows installer's fix was merged without my checking its CI run. Running the
  downloaded build tools (appimagetool) or making a stand-in program for
  makensis was not permitted in the development session, so neither was
  built locally.
- Opening our DAWproject files in Bitwig Studio, Studio One or Cubase.
- Listening: the synth was measured, not heard.
- Phase 6 on Windows by hand (CI runs every test on Windows).
- Plugins and libraries: Windows plugins through a real Wine and yabridge
  (no Wine on the test computer; yabridge is tested through a stand-in);
  installers on Windows and through Wine; the Plugins window's file and
  folder dialogs, and dropping files from a real file manager.
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
- Plugins and libraries: no VST2, no SFZ, no preview of MIDI files or
  SoundFonts, no waveforms in the library, large SoundFonts load on the
  interface thread.
- Earlier: automation drawn, not recorded; no master-bus lanes; no instrument swap;
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

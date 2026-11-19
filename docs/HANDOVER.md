# Handover — where VLabs stands (November 2026)

Read `CLAUDE.md` first; it points to the brief, roadmap and decisions.

## Done

- Phases 0–5, merged into `main` (phase 5 is pull request 2, CI green on
  Windows and Linux). The owner has tried every phase's
  build: all works.
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

- SFZ instruments (decision 0024), merged into `main`
  (pull request 7; CI green on the pull
  request and on `main`): `daw-dsp::sampler` (the sampler and its
  bank), `daw-formats::sfz` (the reader: levels, `#define`/`#include`,
  sound paths found whatever their case, controllers at their starting
  values, sounds decoded on several threads, at most 8 GB),
  `Instrument::Sfz` (file format 7, with its sample file), the engine's
  background reading (`SoundLibrary::set_sfz_reader`,
  `set_background_loading`; `Problem::PartlyRead`), and in the window:
  `.sfz` in the library, dropped or imported, **SoundFont or SFZ…** in the
  chooser, a "Reading the instrument…" notice.
- Checked for SFZ: `cargo xtask ci`; the reader against 18 free
  instruments made by others (the CC0 Ganjo and Karoryfer Cowsynth from
  the sfzinstruments collection, run with `VLABS_TEST_SFZ`): all read,
  nothing missing, notes heard across each one's range; and the program on
  the virtual screen: an instrument double-clicked and another dragged
  from the library became tracks; middle C on Ganjo recorded at the pitch
  of its own recording (267 Hz: that banjo was tuned a little sharp) and
  A4 at 444 Hz; a Cowsynth organ, whose sustain is set by a controller's
  starting value, held steady for 2.5 s then faded in its release; the
  "plays without LFOs" notice for an instrument with a vibrato; and a song
  with SFZ tracks recovered after the program was killed.
- Custom kits (decision 0025, a beta tester's idea), pull request 7:
  `Instrument::Kit { pads }` with `KitPad` (file format 7), the engine's
  `kit_sampler`, sounds added through the audio import path
  (`ImportPlace::pad_of`), the step grid built from rows, and in the
  window: **Custom kit** in **+ Instrument…**, **Add sounds…**, drops and
  library drags on the grid, **Make a kit of these sounds** on a library
  folder, **Remove from the kit**. Checked: tests (model, engine renders,
  DAWproject and file round trips, interface), and the program on the
  virtual screen: a library folder made a kit of four rows, a beat clicked
  in, and the recording had its hits every quarter of a second.
- Plugins the owner tried (MT Power Drum Kit 2, Decent Sampler, DW-303,
  Sienna Bass), pull request 7: plugin windows asking to be shown through
  XEmbed are shown (MT Power Drum Kit 2's was blank); whether a plugin has
  a window is asked once (the interface asked every frame, and a VST3
  plugin answers by making a whole window: 0.6 s for MT Power Drum Kit 2,
  which also undid its START button); VST3 bus layouts set, output silence
  flags cleared, output parameter and event lists given; a plugin missing a
  system library says which (`ScanProblem::MissingLibrary`); an opt-in
  probe test (`VLABS_PROBE_PLUGINS`) plays reported plugins. Checked: MT
  Power Drum Kit 2 in the program on its first run (language, START, then
  three hits recorded) and the same in the probe; DW-303 plays once GTK and
  WebKitGTK are installed. Decent Sampler stays silent until a library is
  loaded in its window. Sienna Bass is a Windows VST2 plugin, which VLabs
  does not support.

## Checked by the owner

- The owner tried every milestone build (phases 1 to 6), the plugins
  round, SFZ instruments, custom kits and the plugin fixes (November
  2026): everything works. That closes v1.

## Merged after v1, not yet tried by the owner

All on `main` (November 19, 2026; the two feature branches were
fast-forwarded into `main` at the owner's request, without pull requests).

- **New songs open on the whole song** (the owner's request):
  `Session::starter` and `Session::new_song` no longer open the beat; the
  interface tests use a `beat()` fixture for the step grid.
- **Expressive tempo** (decision 0026, the owner's request "make the tempo
  able to change gradually, for piano compositions"): glides have a bend
  (`TempoChange::curve`, file format 8) following
  `T0·[1 + (w^q − 1)x]^(1/q)` with closed forms both ways, shared by the
  model and the engine (`daw_model::TempoPiece`, `curve_through`); Early,
  Even and Late in a glide's panel and a handle to drag; *rit.*/*accel.*
  labels; "Arrive gradually" in a point's panel; the tempo line is a widget
  of its own (`daw-ui::tempo_lane`), used by the signature row and by the
  piano roll, whose lane switches between Velocity and Tempo; a point added
  inside a glide glides on with the same bend; DAWproject writes a bent
  glide as straight sixteenth-note pieces.
- **Singing voices, milestone 7a** (decision 0027): `daw_model::lyrics`
  (kana, romaji, VCV/CV alias candidates), `Pattern::lyrics` (by note id)
  and `Instrument::Voice` (file format 9), `daw_formats::utau` (reading
  and installing voicebanks, Shift-JIS via `encoding_rs`), `daw_dsp::voice`
  (YIN pitch, pitch marks, TD-PSOLA resampler, UTAU-style joins in
  `sing_phrase`), `daw_engine::voices` (background voicebank reading and
  phrase rendering on a worker thread, cached by a hash of everything that
  shapes a phrase, played as audio spans instead of the track's notes; a
  live "a" preview sampler in `library::voice_preview`), `daw-ui::voicebanks`
  (the Voicebanks window), `session::voice` (voice tracks, `sing_from`,
  `lyric_found`), lyric typing and "?" marks in the piano roll, Voices in
  the instrument chooser, a Japanese font of the computer as fallback
  (`theme::use_fallback_font`, found in `daw-app`'s `japanese_font`).
- Checked by the development session: `cargo xtask ci` (Linux); for tempo,
  the closed forms against the tempo they integrate, an offline render of
  a bent ritardando, file and DAWproject round trips, interface tests, and
  the program on the virtual screen; for singing, DSP tests (pitch found,
  shifts of an octave within 1.5 %, levels, no clicks, notes on their
  beats), voicebank tests (folder and Shift-JIS zip, terms, aliases, sung
  pitches), an offline engine render, interface tests (lyrics typed
  across notes, the Voicebanks window), and the real program played
  through a virtual sound card: each note at its pitch (329, 393, 440 and
  390 Hz against 330, 392, 440 and 392) with a harmonic-rich synthetic
  voice.

## Next session: start here

1. Look at `main`'s CI run for the merge (commit `8392189` and the
   handover commit after it): these changes have not run on Windows yet.
   Fix anything it finds first.
2. Ask the owner how the hands-on checks went (ROADMAP.md: "Expressive
   tempo" and "7a: things to try", with a real Kasane Teto UTAU voicebank).
   Real recordings are the big unknown: the renderer has only met
   synthetic voices.
3. Build **singing voices 7b, expression** (decision 0027) on a new
   branch: pitch curves drawn over a voice track's notes and vibrato
   (depth, rate, delay per note), loudness envelopes, UTAU-style flags
   (breathiness, gender). Pointers:
   - pitch today comes from `phrase_key` in `daw_dsp::voice` (each note's
     key, an 80 ms glide between notes), handed to `resample` as an `hz`
     closure: a curve or vibrato is another term in that closure;
   - keep per-note expression beside the notes in `Pattern`, by note id,
     like `lyrics` (a note stays `Copy`); file format 10 with its sample
     (`WRITE_FORMAT_SAMPLE=1`) and migration check;
   - include the new fields in `phrase_key`'s hash in `daw_engine::voices`
     so edits render again;
   - draw and edit them in the piano roll only on voice tracks (the lane
     under the notes could gain a "Pitch" choice beside Velocity and
     Tempo); a decision record for the screen, with rejected concepts.
4. Small 7a gaps worth closing on the way: copying and pasting notes loses
   their lyrics (the session clipboard holds `Note`s only); MIDI export
   could write lyrics as lyric meta events; a fermata command for tempo.

## Not verified by the development session (say so if asked)

- Expressive tempo and singing: listening (everything was measured, not
  heard); Windows (no CI run yet); real UTAU voicebanks (none could be
  downloaded in the development container); the Japanese font fallback on
  a real Windows or Linux desktop; the Voicebanks window's file dialogs
  (the test host answers them); typing a lyric by double-click (Enter is
  tested; double-clicks cannot happen in the interface tests).
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
- SFZ: the instrument chooser's file dialog (no dialog program on the
  virtual screen; the chooser's path is tested), listening (pitch, levels
  and envelopes were measured), very large commercial libraries (the
  largest tried held 415 MB of sound), SFZ on Windows by hand (CI runs
  every test on Windows).
- Custom kits: the sound chooser's file dialog (the chooser's path is
  tested), listening (hits and timing were measured).
- Plugins the owner tried: Decent Sampler playing a library (its window and
  file chooser work); the plugins on Windows.
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
- Plugins and libraries: no VST2, no preview of MIDI files, SoundFonts or
  SFZ instruments, no waveforms in the library, large SoundFonts load on
  the interface thread.
- SFZ: no LFOs, pitch envelopes or effects; controllers fixed at their
  starting values; all sound in memory (8 GB per instrument at most); an
  instrument edited on the disk is read again only after a restart.
- Custom kits: changing a pad's settings stops the kit's sounding hits; at
  most 128 sounds; no waveforms.
- Earlier: automation drawn, not recorded; no master-bus lanes; no instrument swap;
  plugins in-process; plugin latency not compensated.
- On Windows, VST3 class ids are kept in the order the plugin reports them
  (COM order); DAWproject converts them at its edge, but a song saved on
  Windows and opened on Linux would not find the same VST3 plugin by id.

- Expressive tempo: no fermatas, no tempo from a free performance, no
  freehand drawing.
- Singing 7a: no pitch curves, vibrato, envelopes or flags; no UST/USTX;
  no DiffSinger/ENUNU; Japanese phonemes only; lyrics not carried by copy
  and paste, nor exported to DAWproject or MIDI.

## Practical notes

- `target/` grows past 20 GB and `dist/` holds a 55 MB program: keep both
  out of folders that are backed up or synced.
- The reference plugins' tests and the schema check need extra tools
  (Xvfb, `xmllint`); NSIS's `makensis` checks the installer script on
  Linux too.
- The Linux plugin CI job recompiles from scratch after a failure: expect
  ~15 minutes.
- Interface tests: a dialog that grows settles for a few frames; clicks
  sent meanwhile are lost (run `run_steps(8)` first). See CLAUDE.md.
- The project file extension is a word the identity rule refuses in any
  Rust file (it is also Kasane Teto's name): use other names in code and
  tests.
- Do not put AI model identifiers in commits (CLAUDE.md): no
  Co-Authored-By trailer naming a model. Some earlier commits on `main`
  carry one by mistake; history was left as it is.

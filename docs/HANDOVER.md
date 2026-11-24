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
- The owner sang with a real voicebank on the build from `main` with 7a
  (November 20, 2026): "everything works great with the vb, still tuning
  is missing". The tuning (7b) was not on `main` yet; it was merged that
  day at the owner's request.

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

## Merged into `main` on November 20, 2026, not yet tried by the owner

Built on the branch `claude/admiring-dirac-yzpv2t` and fast-forwarded into
`main` at the owner's request, without a pull request, as 7a was. CI and
the Spikes workflow, started by hand on the branch (they run on `main` and
pull requests only), passed on Windows and Linux, CI for its last code
commit too.

- **Singing voices, milestone 7b: expression** (decision 0028).
  `daw_model::expression` (each note's `NoteExpression`: pitch points in
  ms and cents with `PitchShape`, `Vibrato`, colour, breath; `phrases`,
  the one rule for cutting a voice's notes into phrases; `PhrasePitch`, the
  pitch sung through a phrase, each note taking over at its first point
  from wherever the voice is; `sung_gain`), `Pattern::expression` by note
  id (file format 10, sample and migration check), `daw_dsp::voice`
  (`sing_phrase` takes the pitch as a function of time; `Colour` per note:
  formant ratio by reading grains faster or slower, breath by a granular
  whisper above 1 kHz that also softens the voice's highs; per-note gain),
  `daw_engine::voices` (velocity and expression in each job and its key),
  and in the window: `daw-ui::pitch_line` (the sung line, Pitch mode
  handles and panels), `session/voice` (`SungLine`, the expression edits),
  the location line's **Notes | Pitch** switch in place of Draw on voice
  tracks (`Command::PitchMode`, Ctrl+P), **Add vibrato / Remove vibrato /
  Plain pitch** in the notes menu, the lane's **Velocity / Colour /
  Breath / Tempo** on voice tracks.
- **7a gaps closed:** copy, cut, paste and duplicate carry lyrics and
  expression (`session::CopiedNote`, also in the clipboard text, which
  stays readable both ways with older copies); MIDI files carry lyrics
  (`EventKind::Lyric`, out and in).
- Checked by the development session: `cargo xtask ci` (Linux); model
  tests (the plain glide equals 7a's arithmetic to 1e-9; shapes,
  snapping, vibrato's start, growth and hand-over, no jumps; ranges); DSP
  tests (a 5.5 Hz vibrato of 50 cents sang at 5.51 Hz and 54 cents; colour
  moved the formants' centre ×1.255 for a ratio of 1.26 and ×0.812 for
  0.794 with the pitch held; breath took the highs from 99 % to 80 % to
  40 % periodic, pitch and level kept; gain exact); an engine render
  (vibrato swings both ways, half velocity sings at a quarter level, the
  untouched note is sample-identical, an edit renders again); interface
  tests (bend by click, drag, a leaving point in time only, right-click,
  the point's panel with slow clicks, Ctrl+P, vibrato from the menu, its
  handle and panel, the Colour and Breath lanes, copy/paste through the
  clipboard text, old clipboard text, duplicate); MIDI lyrics round trip.
  **The real program** on the virtual screen and sound card, with a
  synthetic voicebank: the Pitch mode, a vibrato added from the menu and
  dragged deeper, a bend added by a click, the lanes; its recorded output
  sang A3 at 219.9 Hz, C4's vibrato from 0.25 s in at ±131/134 cents and
  5.56 Hz, E4 rising to F#4 at 369.9 Hz where clicked; breath took note
  1's highs from 0.990 to 0.668 periodic at nearly the same level; colour
  moved note 4's harmonics' centre from 999 to 1196 Hz. That run found two
  faults, fixed with a test: the voice lane's Tempo button fell below the
  lane, and lyrics vanished in the Pitch mode.

## Built on November 20, 2026, not yet tried by the owner

- **Voicebank pictures** (decision 0029, the owner's request: "have the
  avatar display on the song visualisation, on the instrument, and in the
  pattern editor, as a low opacity image"). `daw_formats::utau` reads
  `character.yaml`'s `portrait` beside `character.txt`'s `image`
  (`VoicebankInfo::picture` prefers the portrait, `icon` the small
  picture) and decodes BMP, PNG and JPEG with the `image` crate
  (`read_picture`: size limits, scaled to 1024 points at most);
  `session/voice`'s `VoicePictures` decodes each on a thread of its own
  the first time it is needed and keeps it as a texture, forgotten when
  the voicebank is removed or added again (`voice_portrait`,
  `voice_icon`); `daw-ui::voice_picture` places and draws a picture with a
  low opacity (never more than 2.5 times its size), named "Picture of …"
  for screen readers and tests. Drawn in the song view's track header
  (left of the buttons) and clips (`voice_icon`, 30 %), behind the piano
  roll's notes (`voice_portrait`, 14 %), and clear in the instrument
  chooser.
- Checked by the development session: `cargo xtask ci` (Linux); a
  voicebank test (a BMP icon, a 2000 × 1000 PNG portrait scaled to
  1024 × 512, a nested `portrait:` in `character.yaml` ignored, JPEG, a
  file that is no picture refused); an interface test (the picture on the
  header and the clip, none for a voicebank without one, the portrait at
  the piano roll's right edge, not blown up); and the real program on the
  virtual screen with a synthetic voicebank carrying both pictures: the
  header, the clip, the chooser and the piano roll in screenshots. That
  look moved the header's picture out from under the track's buttons and
  gave the small places the small picture (a tall portrait shrank to a
  sliver there).

- **Release 1.0** (decision 0030, the owner's request: "create the
  release of V1, if a server is needed tell me": none is). The workspace's
  version is 1.0.0; `build.yml` publishes a GitHub release when a tag
  `v<version>` is pushed (or when run by hand with the tag as its
  `release` input, which makes the tag): a last job checks the tag against
  the version, gathers the installer, the AppImage, the plain folders
  (each packed with one folder inside) and `SHA256SUMS.txt`, and runs
  `gh release create` with the notes in `docs/releases/<tag>.md` (first
  line: the title). The README has a Download section; CLAUDE.md says how
  to release.
- Checked by the development session: the packing script on stand-in
  files (one folder inside each archive, Linux's executable permission
  kept, the checksums), the version check, the workflow's YAML.
- **Published** (November 20, 2026):
  [VLabs 1.0](https://github.com/ofujiiro/VLabs/releases/tag/v1.0.0), tag
  `v1.0.0` on `a21b09f`. Pushing the tag was refused (HTTP 403 from the
  session's git access), so "Program builds" was run by hand on `main`
  with `release: v1.0.0` and made the tag. Before it, CI on `main` failed
  on Windows: the new picture test looked for the piano roll's picture
  before its thread had decoded it (fixed: it waits, checked with a reader
  slowed by 300 ms). After it, the published files were downloaded and
  checked: the checksums match; the AppImage starts on this container's
  glibc 2.39 and its About says 1.0.0; but the plain Linux folder was built
  on Ubuntu 26.04 and needs glibc 2.43. Both Linux builds are now made on
  24.04 and the release job refreshes an existing release's files; 1.0's
  files were built again that way, from `261f181` (the tag's program
  sources; only the workflow and decision 0030 changed). Checked again:
  all five checksums match, the plain Linux program needs glibc 2.39 at
  most, and both Linux downloads start here and say 1.0.0. The Windows
  installer and zip were built and packed on Windows by the workflow but
  not run (no Windows here).

## Built on November 24, 2026 as 1.3.0, not yet merged or tried by the owner

On the branch `claude/zen-noether-qvaw3z` (the owner's request: "add MIDI
controller compatibility"), decision 0033, version 1.3.0 with its notes.

- **Playing** (`daw_dsp::Control`, `Instrument::control`, `Performance`):
  the synth bends ±2 semitones, deepens its LFO vibrato by the modulation
  wheel and pressure (`WHEEL_VIBRATO`, 50 cents) and follows expression;
  `SoundFontPlayer` passes bend, CC1 and CC11 to rustysynth; the SFZ
  `Sampler` bends each voice by its region's `bend_up`/`bend_down` (now
  read by `daw-formats::sfz`) and scales by expression; custom kits are
  `set_drum_kit(true)` (no bend, no expression); `SineSynth` bends. Block
  events (`BlockEvent`, `BlockEventKind`) carry controllers at their frame.
- **Plugins** (`daw_plugin_host::Event::{PitchBend, Controller, Pressure}`):
  CLAP as `MidiEvent` on the first note port supporting the MIDI dialect
  (`midi_port`, host dialects now CLAP | MIDI); VST3 through `midi_map`
  (`IMidiMapping` asked for all 130 controller numbers at load) as
  parameter changes, not echoed to the controller.
- **Engine**: `MidiSender::control`; `LiveNote::Control`; `live_keys` (a
  key comes up on the slot it went down on), the pedal comes up on every
  slot it holds, `rest_live` when the focus moves; curves compiled per
  track (`CompiledControl`, `ControlSpan`), scheduled every 64 frames
  (`schedule_controls`), rested on stop, locate and panic
  (`rest_song_controls`) and when a curve leaves a track
  (`install_program`); recording: `live_values`, `live_override`,
  `recorded_controls`, `EngineEvent::RecordedControl`, `records()`.
- **Model**: `daw_model::controls` (`ControlKind`, `ControlLane`,
  `ControlPoint`, `thin`, `MidiControl`, `MidiSource`, `LinkTarget`,
  `MidiLink`), `Pattern::controls`, `controls_played_in`, `set_control`,
  `Instrument::takes_controls`, `Project::midi_links`, `link_midi`,
  `forget_midi`, `set_linked`, `linked_value`, `has_link_target`. File
  format 12 (`repair_midi` on load), sample `format-v12.project-sample`.
- **MIDI files**: `EventKind::{Controller, PitchBend, Pressure}`;
  `control_events` (export, rest after a clip); `curves` (import: steps
  after pauses, sweeps as lines, thinned); the file's sustain pedal
  lengthens notes.
- **Interface**: `daw-ui::midi` (`ControllerInput`, `route`, `RouteTable`,
  `Learning`, `can_learn`, `MidiRouter`), `midi_ui::assign_menu` (opened
  with `container_context_menu`: a click sense would change how sliders
  start a drag), `widgets::right_pressed` (sliders ignore the right
  button), `Gesture::sliders`/`link` and `slider_row(…, param)`,
  `control_lane` (the curve editor), the piano roll's `LaneKind::Control`
  (Bend, Mod, the pattern's other curves; short button names
  `text::control_button`), `session/controls.rs` (curves, recorded moves),
  `session/midi.rs` (learning, links, merged undo per burst), the
  workspace's `midi_buttons`, `button_learning`, `update_midi_table`,
  `midi_window`, the shortcuts window's MIDI column, `Settings::{midi_buttons,
  midi_off}`.
- **Devices and app**: `MidiMessage::{Pressure, Transport}` (MMC, Start,
  Continue, Stop), `MidiInputs::start(name, off, …)`, `seen`, `set_off`;
  `daw-app` turns device messages into `ControllerInput` and hands them to
  the router.
- Checked: `cargo xtask ci`; the reference plugins' tests (Surge XT and
  Dexed bend as CLAP and VST3: a quarter of the wheel is ±0.5 semitones on
  Surge, ±0.75 on Dexed whose range is 3); the program on the virtual
  screen: the lane's choices and a drawn bend, the fader's menu and the
  waiting message, the MIDI window (it says MIDI is unavailable there: no
  ALSA sequencer in the container), the shortcuts window's MIDI column.
  That run found and fixed four faults (a right-click moved sliders, the
  notice hid the waiting message, a lane button too wide, the shortcuts
  table too wide).

## Released as 1.2 on November 23, 2026, not yet tried by the owner

- **The export's end** (the owner: "the sound loops back to the start
  and it is heard"). `OfflineRender::new` played the song with its loop
  region (`PlaybackScope::Song`); it is now unlooped, and
  `OfflineRender::with_scope` keeps the loop for `render_offline` (tests
  of looping). `daw-ui::export` holds the quiet frames after the song's
  end and writes them only if sound follows, so the file ends with the
  sound. `tests/song.rs`: a four-bar song looping two bars exported 23 s
  before, now 8 s plus the kick's ring.
- **Selecting and moving several** (decision 0032). `Project::move_tracks`;
  `session/selection.rs` (`Pick`, `pick_track`, `move_selected_tracks`,
  `toggle_clip`, `select_clips`, `select_all_clips`, `move_clips`,
  `remove_selected_clips`, `duplicate_selected_clips`,
  `nudge_selected_clips`); the session's `selected_clip` became
  `selected_clips` (the last is the one single-clip commands use) and
  `selected_tracks`. `song_view`: `header_input` (click, Ctrl/Shift+click,
  drag a header), `Drag::MoveGroup`, `Drag::Band` (`clips_in`,
  `group_overlaps`), selected tracks outlined; the headers' labels are not
  selectable (a drag on a name moves the track). `run_in_song`: Delete,
  Duplicate, the nudges and Ctrl+A act on the selection.
- Checked: `cargo xtask ci`; the export test (failed on the old code);
  interface tests for tracks (select, Ctrl, Shift, drag up and down, undo)
  and clips (Ctrl+click, group move, Delete, undo, Ctrl+A, Ctrl+D, Escape,
  the box); the real program on the virtual screen: a track dragged above
  another, a clip moved. Ctrl and Shift could not be sent to the real
  program there.

## Released as 1.1 on November 22, 2026, tried by the owner: everything works

- **Second library folder crash** (the owner's report: "adding more than
  one folder in the folders tab crashes the program"). Adding a folder
  selected it before the library had taken it in, so the files column
  indexed past the end of its roots (`library_panel`); it now takes the
  folder in at once, and removing a folder keeps the one shown selected.
  An interface test adds three folders, switches and removes two (it
  panicked exactly so before the fix).
- **Shaping sounds** (decision 0031, the owner's request: "no way to
  change a sound in the program, for instance to cut it earlier; add
  ways to change imported sounds and the instruments, for instance a
  release cut piano").
  - Model: `Track::shape` (`NoteShape`), and `reverse` and `pitch` on
    audio clips (`Project::set_clip_reverse`, `set_clip_pitch`; trimming,
    splitting and `audio_end` follow direction and speed); format 11, with
    its sample file.
  - DSP: `daw_dsp::sf2_shape` rewrites a SoundFont's instrument zones with
    the new attack and release (`SoundFontBank::from_bytes_shaped`);
    `Sampler::set_shape` and `DrumSampler::set_shape` (with a release,
    one-shots follow their key); `InstrumentSettings::Shape`.
  - Engine: `Engine::shapes` per slot; a shape change goes to a playing
    sampler or kit as a setting, and rebuilds a SoundFont
    (`SoundLibrary::shaped_soundfont`, the last three kept); with a
    release, the SoundFont's own reverb is off. Audio spans play backwards
    and faster or slower (`step`).
  - Interface: `shape_ui` (the Note shape button and panel, in the track
    header and the location line), `sound_editor` (the window; opened by
    double-clicking an audio clip or Edit sound… in its menu),
    `session/shape.rs`, `session/sound_edit.rs`.
- Checked by the development session: `cargo xtask ci`; offline renders
  (`daw-dsp/tests/shapes.rs` plays the bundled piano: a cut note is 60 dB
  down 50 ms after its end with no click, a long release rings, a slow
  attack swells; the kits and the sampler; `daw-engine/tests/shapes.rs`
  and `audio.rs`: in songs, with the piano's reverb on, a reversed ramp,
  a clip a fifth up at 659 Hz); interface tests for both panels; the real
  program on the virtual screen (screenshots of the panel and the editor,
  every control of the editor used with the mouse) and the virtual sound
  card: a song of four piano chords recorded from the program, natural
  and cut: with the cut each chord is silent 50 ms after its end (-150
  dB), naturally it rings at -9 to -16 dB; no click at the cuts. That
  recording is what found the SoundFont reverb ringing on after a cut.
- **A MIDI file into a track** (the owner's next request, in French: "put
  a MIDI file into the synth, it automatically creates a piano"), decision
  0034, part of 1.3: `ImportedSong::add_into` (one pattern, drums or the
  rest, curves for instruments that take them, `clear_span` before the
  clip) and the shared `place`; `Session::import_midi_into`;
  `import_files` sends MIDI files to a dropped-on track of notes; the song
  view takes any track row as the drop's track; the pattern view sends
  MIDI files dropped on it to its track and opens the new pattern.
  Checked: format and session tests, and on the virtual screen a file
  dragged from the library onto the piano's row became a pattern there.

## Next session: start here

0. MIDI controllers (1.3.0, on `claude/zen-noether-qvaw3z`): ask the owner
   whether to merge it into `main` and release it, and to try it with
   their controller (ROADMAP.md: "MIDI controllers: things to try"): keys,
   pads, both wheels, pedals, knobs given settings, buttons given
   commands, transport buttons. Nothing was tried with a real device.
   Also ask them to drop a MIDI file on the synth's row (ROADMAP.md: "A
   MIDI file into a track: things to try").
1. Ask the owner how 1.2 goes (ROADMAP.md: "Moving several at once:
   things to try"): selecting and moving several tracks and clips, and an
   export with a loop set.
1. Look at the release (the repository's Releases page, VLabs 1.0) and the
   "Program builds" run that published it; ask the owner whether the
   downloads install and start on their computers.
2. Ask the owner how 7b's checks went (ROADMAP.md: "7b: things to try",
   with their real voicebank) and whether it is the tuning they found
   missing on the 7a build. If they want more, what OpenUtau and UTAU
   users tune that VLabs does not yet: phoneme timing (each note's
   preutterance and overlap, dragged), consonant speed (UTAU's velocity),
   modulation (how much of the recording's own pitch wobble is kept),
   an envelope inside a note, pitch drawn freehand, more flags. Expressive
   tempo's check was not mentioned: ask.
3. Otherwise build **singing voices 7c, exchange** (decision 0027): UST (UTAU) and
   USTX (OpenUtau) projects in and out. Pointers:
   - 7b's model is OpenUtau's: pitch points in ms from the note's start
     (USTX `pitch.data` x), cents (USTX y is in tenths of a semitone),
     shapes io/l/i/o = Smooth/Straight/SlowStart/SlowEnd, a first point
     that snaps to the note before (`snap_first`); OpenUtau's vibrato
     length is a share of the note from its end (ours: a start in seconds),
     period in ms (ours: a rate in Hz), in/out in % (ours: a growth in
     seconds, a fixed 80 ms fade-out);
   - UST is Shift-JIS text (`encoding_rs` is in); its Mode2 pitch is
     PBS/PBW/PBY/PBM, its vibrato VBR, its flags g and B (ours: colour
     with the sign turned round, breath);
   - a decision record first (where import and export sit in the menus,
     what is left out and named, as DAWproject import does).
4. Small gaps worth closing on the way: a fermata command for tempo; the
   pitch line follows the first placement's tempo (see known gaps).

## Not verified by the development session (say so if asked)

- MIDI controllers: no real controller (the container has no MIDI system:
  `/dev/snd/seq` is missing, so midir cannot open the sequencer); device
  messages were tested from the parse onwards and the program's screens on
  the virtual screen. Windows' MIDI (winmm) passes MMC as it does on Linux
  by midir's defaults, unchecked. How the bends and vibrato sound (measured
  only).

- Shaping sounds: how the shapes sound to a person (measured only); the
  panels in the light theme and on Windows; a shaped SoundFont other than
  the bundled piano (the rewrite follows the SoundFont 2 specification
  and is checked on the piano and the generated test fonts).

- Release 1.0: the Windows installer and zip were not run (built by the
  workflow on Windows; the installer step there succeeded).

- Voicebank pictures: real voicebanks' pictures (checked with pictures
  made for the tests), the light theme, Windows by hand.

- Singing expression (7b): listening (everything was measured, in tests
  and in the program's output); real voicebanks; the program on Windows by
  hand (CI runs every test there); shortcuts on the virtual screen
  (Ctrl+P, Ctrl+A did not reach the window there; tests press them).

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

- MIDI controllers: no relative knobs, Mackie Control or controller
  profiles; knob moves of VLabs settings are not recorded as automation;
  mute and solo cannot be given buttons; SFZ ignores the modulation wheel
  and pressure; copying notes leaves their curves; DAWproject drops curves.

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
- Singing 7a: no UST/USTX; no DiffSinger/ENUNU; Japanese phonemes only;
  lyrics not exported to DAWproject (MIDI carries them since 7b).
- Singing 7b: no freehand pitch, no envelope inside a note (volume
  automation does crescendos), no other UTAU flags, no vocoder (a
  milestone of its own); the line follows the tempo of the pattern's first
  placement and is drawn unswung; expression stays out of MIDI and
  DAWproject.

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
- The development session cannot push tags (HTTP 403): releases are made
  by running "Program builds" by hand with the `release` input
  (CLAUDE.md, "Releasing").
- Do not put AI model identifiers in commits (CLAUDE.md): no
  Co-Authored-By trailer naming a model. Some earlier commits on `main`
  carry one by mistake; history was left as it is.

# CLAUDE.md — working on VLabs

Guidance for any session (human or AI) picking up this repository.

## Start here

1. **Read [`docs/PROJECT_BRIEF.md`](docs/PROJECT_BRIEF.md).** It is the
   owner's brief: product vision, settled decisions, engineering rules and
   the roadmap. It overrides everything else except a later decision record.
2. **Read [`ROADMAP.md`](ROADMAP.md)** for where the work stands and what
   comes next.
3. Read [`docs/HANDOVER.md`](docs/HANDOVER.md) for the latest state.
4. Skim [`docs/architecture.md`](docs/architecture.md) and the index of
   [`docs/decisions/`](docs/decisions/README.md) before changing anything
   structural.

The repository is public (since November 2026): everything committed is
published once pushed.

## Who is who

The **project owner** directs the product and does not read or write Rust.
You write all the code and are responsible for its quality, its tests, and
for explaining what you did in plain language. Decide technical and design
questions yourself and record each significant one in `docs/decisions/`.
Stop and ask only when blocked by something only the owner can do or decide:
an account or credential, spending money, a legal choice, or a finding that
makes a settled decision in the brief unworkable.

## Repository map

| Path | What |
| --- | --- |
| `crates/daw-*` | The application, one crate per concern; each has a README with its rules |
| `spikes/` | Phase 0 throwaway experiments (not part of the product) |
| `xtask/` | Developer commands (`cargo xtask ...`) and repository-rule tests |
| `docs/` | Brief, architecture, decision records |
| `assets/` *(from phase 1)* | Bundled fonts, piano, drum kit (all recorded in `THIRD_PARTY.md`) |
| `packaging/` | Installer (NSIS) and AppImage templates; names only through placeholders filled from `daw-ident` |
| `.github/workflows/` | CI (`ci.yml`) and the spike/integration jobs (`spikes.yml`) |

## Commands

```bash
cargo xtask ci        # everything CI checks: fmt, clippy, build, test, cargo-deny
cargo xtask fmt --fix # reformat
cargo test -p daw-model
cargo run -p daw-app  # the program
cargo xtask dist      # release build in dist/: program, notices, guide, icons
cargo xtask notices   # third-party licence notices (also a CI step)
cargo xtask installer # Windows installer from dist/ (NSIS's makensis)
cargo xtask appimage  # Linux AppImage from dist/ (appimagetool)
```

**Releasing** (decision 0030): set `version` in the workspace's
`Cargo.toml` (1.N.0 adds features, 1.N.M only fixes), write
`docs/releases/v<version>.md` for the people downloading it (its first line
is the title), bring it to `main` with CI green, then push the tag
`v<version>` on that commit. "Program builds" builds everything and
publishes the release; it refuses a tag that is not the version. Without
the right to push tags, run "Program builds" by hand on `main` with the tag
as its `release` input: the job makes the tag (the development session's
git access refused tags in November 2026, HTTP 403, so 1.0 was made this
way). Run by hand again for an existing release, it replaces its files and
notes. Then check the published files: download them, compare
`SHA256SUMS.txt`, unpack the AppImage (`--appimage-extract`) and start
`squashfs-root/AppRun --silent` on the virtual screen.

`cargo-deny` must be installed: `cargo install cargo-deny --locked`.

On Linux, building needs: `libasound2-dev libjack-jackd2-dev libudev-dev
libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev
libxkbcommon-x11-dev libwayland-dev`.

## Non-negotiable rules

These come from the brief; tests and CI enforce most of them.

- **The audio thread is sacred.** No allocation, locks, file or network
  access, or logging on the real-time thread. UI and engine talk through
  lock-free queues (`rtrb`). Wrap real-time code in `daw_rt::RtGuard` /
  `rt_region`; debug builds abort on allocation inside it. Use
  `daw_rt::permit_alloc` only around third-party plugin calls.
- **Licences.** Outside code must be MIT, Apache-2.0, BSD, ISC or Zlib
  (`deny.toml`, decision 0005). Check a crate's current licence, version and
  maintenance on crates.io before adding it, then record it in decision 0006.
  Bundled sounds must be CC0 and listed in `THIRD_PARTY.md`. Never use the
  ASIO SDK.
- **Layers.** A crate depends only on lower layers (decision 0003,
  `xtask/src/rules.rs`).
- **Names in one place.** The product name, file extension and app
  identifiers come from `daw-ident`; never write them as literals elsewhere
  (a test checks).
- **User-facing text in one place** (in `daw-ui`), English only for now.
- **Everything the user does is undoable**; the project model is the single
  source of truth for UI and engine.
- **Musical time is integer ticks** (`2_882_880` per quarter note, decision
  0007). Never floating-point beats.
- **The engine runs headless**; offline export uses the same graph as live
  playback.
- Every Rust file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- `unsafe` is denied by default; a module that needs it opts in with a reason,
  and every unsafe block has a `// SAFETY:` comment.
- Do not put any AI model identifier in commits, code or docs.

## How to work

- Small commits with clear messages. Run `cargo xtask ci` before pushing.
- Before building a major screen, think through two or three distinct
  concepts, pick the strongest, and record the rejected ones and why.
- You cannot hear or see the way the owner does. Verify with automated tests
  and offline renders (signal present, pitch, timing, no clipping) and with
  screenshots, and say exactly what you could not verify. **Never report
  something as working if you have not run it.**
- End every milestone with a build the owner can run and a short checklist of
  things to try. Explain progress in plain language: what works, what to try,
  what is still rough.
- Keep this file, `ROADMAP.md` and `README.md` current.

## Testing without hardware (Linux containers)

- **Virtual screen:** `xvfb-run -a -s "-screen 0 1600x1000x24" <command>`;
  screenshots with `import -window root shot.png` (ImageMagick). The
  window's renderer needs Mesa's software drivers (`mesa-vulkan-drivers`,
  `libegl1`); without them the program says it "could not start" (no
  surface for any backend).
- **Virtual sound card:** start a private PipeWire with a null sink, then use
  the normal default device:

  ```bash
  export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
  dbus-daemon --session --fork --address=unix:path=$XDG_RUNTIME_DIR/bus
  export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
  pipewire & sleep 1; wireplumber & sleep 2
  pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=test-speakers media.class=Audio/Sink object.linger=true audio.position=[FL FR] }'
  ```

  (packages: `pipewire pipewire-alsa pipewire-jack wireplumber dbus`).
- **Virtual microphone:** to test recording, make a virtual source and feed
  it the virtual speakers' sound, so the program records what it plays
  (its metronome falls on the beats, which shows how well takes line up):

  ```bash
  pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=test-mic media.class=Audio/Source/Virtual object.linger=true audio.position=[FL FR] }'
  pw-link test-speakers:monitor_FL test-mic:input_FL
  pw-link test-speakers:monitor_FR test-mic:input_FR
  ```

  The program's audio files, and its autosave copy (`song.teto`, which says
  each clip's offset), are in the scratch home's
  `.local/share/<product>/recovery/<run>/`. A program that was killed
  offers its work back at the next start: answer the dialog before
  clicking anything else.
- **Driving the program:** with the virtual sound card running, start
  `target/debug/daw-app` under `xvfb-run`, find the window with
  `xdotool search --name <product name>`, then click and type with
  `xdotool mousemove X Y click 1` / `xdotool key ctrl+r`, take screenshots
  with `import -window root`, and record what it plays with
  `pw-record --target test-speakers -P '{ stream.capture.sink=true }' out.wav`.
  Point `HOME`, `XDG_CONFIG_HOME` and `XDG_DATA_HOME` at a scratch folder so
  settings and recovery copies do not leak between runs. `--silent` starts
  the program without opening a sound device.
- **Audio test files:** `crates/daw-formats/tests/data/sine-440.*` were made
  for the tests from a generated tone (LAME and oggenc, from the Ubuntu
  archive: `lame vorbis-tools`); the interface and session tests write
  their own WAV files.
- **Interface tests** (`crates/daw-ui/tests/interface.rs`) use
  `egui_kittest`: find controls by their accessible label, click them, send
  keys. Dialogs re-centre for a few frames after opening; run a few frames
  before clicking inside one. The test clock moves a quarter of a second per
  frame, so a double-click cannot happen in a test (open clips through their
  menu there, and check double-clicks in the real program). Each square of
  the step grid is labelled ("Kick, step 1"). Test what a person does; a
  new test should be a step someone takes in the program.
- **Playing notes in the real program:** synthetic key events from
  `xdotool keydown --window` do not reach the note keys; hold a key of the
  on-screen keyboard with `xdotool mousemove X Y mousedown 1` /
  `mouseup 1` instead (middle C is the key labelled Q in the lower half).
  The window's title is "<song> — <product>", so find it with
  `xdotool search --name <product>`, not `^<product>`.
- **Dragging in the real program** (the library onto the song, say):
  `xdotool mousemove X Y mousedown 1`, then several `mousemove` steps with
  short pauses, then `mouseup 1`; one jump does not start a drag.
- **Crash dumps:** the allocation check aborts a child process on purpose;
  where the system writes core files into the current folder, they land in
  `crates/daw-rt/`. They are ignored by git; turn core files off in the
  test container (`ulimit -c 0`).
- **Driving the program:** `xdotool windowfocus` can fail on a window that
  has just appeared; click the program's own buttons (Play, Stop) rather
  than relying on keys when focus is uncertain, or send keys to the window
  itself (`xdotool key --window <id> Escape`).
- **Clicks in tests:** `harness.get_by_label(..).click()` presses and
  releases in one frame; the fixture's `click` and `drag` take several
  frames, as a person does. Test panels and menus with both: a press that
  closes a panel before the release only shows up the slow way.
- **Dialogs that change size settle for a few frames** (they re-centre):
  a click sent while one settles is lost, whether through the
  accessibility tree or the pointer. After a dialog grows (more rows, a
  panel of terms appearing), run several frames (`run_steps(8)`) before
  clicking inside it.
- **Singing (decision 0027):** tests make their own voicebanks (WAV files
  written by the test and an `oto.ini`); give them plain-letter aliases
  when the interface draws them (our fonts have no kana). Synthetic voices
  must be rich in harmonics (a vowel of many harmonics under formants):
  a near-pure sine is shifted badly by pitch-synchronous overlap-add and
  measures wrong. The project file extension is a word the identity rule
  refuses anywhere in Rust code, and it is also Kasane Teto's name: use
  other names in code and tests.
- **Singing expression (decision 0028):** in interface tests, find the sung
  pitch's handles by their labels (`text::pitch_point(key, index)`,
  `text::vibrato_handle(key)`) and click relative to them: the piano
  roll's zoom is not fixed. Accessible names must be unique on screen: the
  swing button can say "Straight" while a point's panel offers the
  "Straight" curve, which is why the shapes are labelled
  `text::curve_shape(..)`. A clipped widget is still in the accessibility
  tree: check that a control you rely on lies inside its area (the voice
  lane's Tempo button once fell below the lane unnoticed). Breathy voices
  hide their pitch from YIN in the full band; measure pitch below 500 Hz.
  To check the real program, a song made with the crates themselves (a
  small Cargo project in the scratchpad depending on `daw-model` and
  `daw-formats` by path, with its own copy of `rust-toolchain.toml`) and a
  synthetic voicebank in the scratch home's `.local/share/vlabs/voicebanks`
  save a lot of clicking; `pw-record` the output and measure it.
- **Shaping sounds (decision 0031):** the bundled piano's SoundFont
  reverb rings on past a note's end: measure a note's tail with the
  reverb off (`set_soundfont_reverb(0.0)`, `SoundFontPlayer::set_reverb`)
  or with a release set (which leaves it out). The piano's own release is
  short (about 0.15 s); hold notes for a second to compare tails. In
  interface tests the sound editor's ends and fade squares are sliders
  labelled `text::SOUND_START_HANDLE`, `SOUND_END_HANDLE`, `CLIP_FADE_IN`
  and `CLIP_FADE_OUT`; the note shape's sliders are labelled by their
  ticks (`get_by_role_and_label(Role::Slider, text::SHAPE_ATTACK)`). To
  check them in the real program, a song made with the crates (a clip of
  a generated sound, piano chords) opens from the command line:
  `daw-app <song>`; double-click its clip to open the editor.
- **MIDI controllers (decision 0033):** the development container has no
  MIDI system (no `/dev/snd/seq`): midir cannot open the sequencer, the
  log fills with ALSA's "seq_hw" lines and the MIDI window says MIDI is
  unavailable. Test from the message onwards: `daw_ui::midi::route` is
  plain data; in interface tests `workspace_mut().midi_router()` gives a
  `MidiRouter` whose `handle(ControllerInput)` reaches the engine at once
  and the window at its next frame (`f.tick()`); engine tests use
  `engine.midi_sender()` (`note_on`, `sustain`, `control`). A slider's MIDI
  menu opens with `container_context_menu`: never give a slider a click
  sense (its drag would start late and split into two undo steps), and
  egui's sliders follow any button, so `widgets::right_pressed` puts the
  value back on a right-click. In the piano roll's lane, curves are found
  by `text::curve_lane(..)` (the button), `text::control_curve(..)` (the
  area) and `text::control_point(.., index)`. The shortcuts window needs
  `run_steps(8)` after opening before its Learn buttons take clicks. To
  measure a plugin's bend, compare its pitch with the unbent sound
  (`reference.rs`'s `shift`): FM patches fool plain pitch finders by an
  octave.
- **Panels and popups:** give an `egui::Area` panel fixed column widths and
  no separators that stretch across it. A widget that takes "all the room
  there is" grows the panel to the window's edge, the window nudges it
  back, and it slides frame after frame from under the pointer (decision
  0018's signature row did this once). The location line must fit a
  1280-point window with the longest labels shown.
- **Reference plugins** (test fixtures only, never linked or bundled): Surge
  XT 1.3.4 (an instrument) and Surge XT Effects (an effect) from
  `github.com/surge-synthesizer/releases-xt` releases
  (`surge-xt-linux-1.3.4-pluginsonly.tar.gz`, `surge-xt-win64-1.3.4-pluginsonly.zip`),
  and Dexed 0.9.8 (a second instrument) from `github.com/asb2m10/dexed`
  releases (`dexed-0.9.8-lnx.zip`, `dexed-0.9.8-win.zip`), each in CLAP and
  VST3. CI downloads them in `spikes.yml`; the plugin host's, the engine's,
  the interface's and the program's plugin tests use them when
  `VLABS_TEST_PLUGINS` names the folder that holds them (`surge/` and
  `dexed/` inside), and pass quietly otherwise.
- **Plugins in the real program:** link the fixture plugins into the scratch
  home's `.clap` and `.vst3` folders (symbolic links work); the program
  finds them at start (its cache is `plugins.json` in the data folder). The
  virtual screen has no window manager: plugin windows open at 80,80 on top
  of the program, so move them aside with `xdotool windowmove <id> X Y`
  (`xdotool search --name "^Surge"` finds them), and close them with the
  program's own window buttons. Start the program only once PipeWire and
  WirePlumber are up; a sound device that never starts is reopened after
  three seconds.
- **Run the reference plugins' tests one at a time** (`-- --test-threads 1`),
  as CI does: plugin window toolkits (JUCE, VSTGUI) tie themselves to one
  main thread, and in parallel the editor test can wait forever.
- **Plugins someone reports:** put them in a folder and run
  `VLABS_PROBE_PLUGINS=<folder> cargo test -p daw-plugin-host --test probe -- --nocapture`
  under a virtual screen: each instrument is played at once, after a few
  seconds, and with its window open. Some plugins are silent until a start
  screen in their window is clicked (MT Power Drum Kit 2's free version);
  `VLABS_PROBE_HOLD=<seconds>` keeps the window open to click through it.
  Linux plugins often need system libraries the container lacks (MT Power
  Drum Kit: xcb-util, xcb-cursor, cairo, pango; plugins whose windows are
  web views: GTK and WebKitGTK): install them in the container.
- **SFZ test instruments** (fixtures only, never bundled): free
  instruments from the `sfzinstruments` collection on GitHub, for instance
  `ganjo` and `karoryfer.cowsynth` (CC0; download the repository's zip).
  `VLABS_TEST_SFZ=<folder>` makes `crates/daw-formats/tests/sfz_instruments.rs`
  read every `.sfz` inside and play notes across each one's range.
- **Typing in the real program:** `xdotool type` does not reach text
  fields on the virtual screen (no window manager); scroll lists with the
  mouse wheel (`xdotool click 5`) instead of searching. Shortcuts sent with
  `xdotool key` (Ctrl+A, F5, Ctrl+P) did not reach the window either in
  November 2026: use the program's buttons and menus (double-click a clip
  to open it), and screenshot a menu before clicking in it: its items move
  (the notes menu loses its "Select notes first" line once notes are
  selected).
- Network access in the development container is restricted: crates.io,
  GitHub (git, raw files and release downloads) and the Ubuntu archive work;
  many other sites do not.

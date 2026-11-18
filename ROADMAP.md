# Roadmap

The phases come from the project brief. Each ends with a build the owner can
run and a checklist of things to try. Status is updated at every milestone.

**Now:** phase 4 (audio: recording, importing, shaping clips) is built
and its milestone build is ready for the owner to try; the phase 3 build
(plugins and automation) also still awaits the owner's check. Phase 2 is
complete and the owner confirmed its milestone build works. Phase 1 is
complete and the owner confirmed its build works on Linux. Phase 0 is
complete: all three risk spikes passed on Windows and Linux (decision
0009). The repository has been public since November 2026 (the owner's
decision).

## Phase 0 — Foundations and risk spikes

- [x] Repository, licence (GPL-3.0-or-later), contributor licence agreement,
      README, this roadmap, `CLAUDE.md`, decision records
- [x] Cargo workspace with layered crates and repository-rule tests
- [x] CI on Windows and Linux: format, lint, build, test, licence check
- [x] Real-time allocation check (`daw-rt`), tested
- [x] Spike (a): low-latency audio output on Windows and Linux
- [x] Spike (b): host a CLAP and a VST3 instrument (Surge XT): notes in,
      audio out, editor window, save and restore state, on both systems
- [x] Spike (c): a piano roll with 10,000 notes that scrolls and zooms
      smoothly
- [x] Spike results recorded (decision 0009): go for phase 1

## Phase 1 — First sound

- [x] Project model: exact musical time, tracks, patterns, notes, tempo and
      time signature; undo and redo for everything (decision 0007)
- [x] Native project files, with migration tests from the first version
- [x] Engine core: transport, sample-accurate scheduling, metronome,
      count-in, offline rendering through the same processing as playback
- [x] Bundled piano: a CC0 SoundFont built from the Osiris Piano
- [x] Sound output with no setup, a silent stand-in when there is no sound
      card, MIDI keyboards connected automatically
- [x] The main window (decision 0011): transport bar, piano roll with mouse
      editing and a velocity lane, on-screen keyboard, menus, dark and light
      themes
- [x] Playing from the computer keyboard, by key position on every layout;
      labels follow the keyboard's layout
- [x] Live note recording with metronome and count-in; one take is one undo
      step; optional snapping of recorded notes
- [x] Our own shortcut scheme, rebindable, shown in tooltips and menus, never
      clashing with note keys
- [x] Autosave and crash recovery, settings remembered (decision 0013)
- [x] The owner's hands-on check of the milestone build on Linux (Windows
      still to be tried)

### Phase 1 milestone: things to try

1. Start the program: an empty piano roll says "Click anywhere to place a
   note". Click a few spots: each click places a note and you hear it.
2. Press Space: the pattern plays in a loop. Press Space again to stop.
3. Play the computer keyboard: the bottom row from Z and the row from Q are
   two piano octaves. Page Up and Page Down move the octave. The on-screen
   keyboard shows which key plays what (on an AZERTY keyboard, check the
   letters match your keys).
4. Drag a note to move it, drag its right edge to make it longer,
   right-click it to delete it. Drag in the velocity lane to change how hard
   it plays. Ctrl+Z undoes, Ctrl+Shift+Z redoes.
5. Press Ctrl+R: one bar of clicks counts you in, then play something on
   the keyboard. Press Ctrl+R or Space to stop. One Ctrl+Z removes the whole
   take.
6. Save with Ctrl+S, close, and open the song again from the menu.
7. Plug in a MIDI keyboard while the program is running: a message says it
   is connected, and it plays the piano. The sustain pedal holds notes, and
   recorded notes last as long as you heard them.
8. Try the light theme (menu › Theme) and change a shortcut (menu ›
   Keyboard shortcuts).

### Known gaps in this build

- On Windows, copy and paste of notes works inside the program but not
  between two copies of it (the system clipboard is used on Linux only;
  decision 0005).
- The pitch wheel and other controllers of a MIDI keyboard are not used
  yet (the sustain pedal is).
- The sound was on the quiet side (fixed in phase 2: the piano is 6 dB
  louder and every new song has a limiter on the master).
- Open and save dialogs are not attached to the window, so on some desktops
  they may open behind it.

## Phase 2 — Making a beat

- [x] Drum sampler with two bundled kits: a recorded acoustic kit (Big
      Rusty Drums, CC0) and a synthesised electronic kit (decision 0008)
- [x] Screens for the song, a pattern and the mixer (decision 0014): one
      large surface showing the song or one pattern, the location line as
      the way between them, the keyboard or the mixer at the bottom
- [x] Step sequencer: click or drag to add hits, a loudness strip, each
      sound's volume, pan, tune and length; drums from the computer keyboard
- [x] Patterns: several per track; new, copy, rename, delete
- [x] Arrangement timeline: tracks with mute and solo, clips with a picture
      of their notes; place, move, repeat, copy and remove clips; loop a part
- [x] What Space plays follows the screen: the song, or the part of the song
      where the open pattern is placed, or a pattern on its own
- [x] Mixer: volume, pan, mute, solo, level meters and effect chains for
      every track and the master
- [x] Built-in effects: EQ (it draws its curve), compressor, reverb, delay
      in time with the song, limiter; a new song's master has a limiter
- [x] WAV export in the background, CD or studio quality, with progress and
      Cancel; the echoes and reverb fade out at the end
- [x] File format version 2, with migration of phase 1 songs
- [x] The owner's hands-on check of the milestone build

### Phase 2 milestone: things to try

1. Start the program: a new song opens on the drum step grid. Click the
   Kick squares on steps 1, 5, 9 and 13 and the Snare squares on 5 and 13,
   then press Space. Drag along the Closed hi-hat row to add a hi-hat on
   every step; Ctrl+Z removes the whole drag.
2. Play the drums from the computer keyboard: the key caps on the left say
   which key plays each sound (Z is the kick, X the snare, G the hi-hat).
3. Click a sound's name: you hear it, and a panel changes its volume, pan,
   tune and length, or silences it. Try Kit › Electronic at the top right.
4. Click "Untitled song" at the left of the location line (or press F5):
   the song view. The beat repeats over four bars. Drag the clip's right end
   to make it last longer, click an empty spot on a track to place its
   pattern again, double-click a clip to open it, right-click it for more.
5. Open the piano (location line: Drums › Piano, or double-click the piano
   clip) and add a melody. In a pattern, Space plays the part of the song
   where that pattern is, with the other tracks.
6. Press F9 (or the fader button at the right of the location line): the
   mixer. Move the faders and pans, mute and solo tracks. On the drums,
   "+ Add effect" › Reverb, then raise its Amount; on the piano, try the EQ
   and watch its curve.
7. In the song view, drag along the ruler to loop a part of the song.
8. Menu › Export as WAV… (Ctrl+E): choose the quality and where; a progress
   bar, then the file plays in any music player.
9. Save, close and open the song again: patterns, mixer and effects come
   back. A phase 1 song opens too.

### Known gaps in this build

- Clips cannot be moved to another track, several clips cannot be selected
  at once, and clips have no copy and paste (Duplicate places a copy right
  after a clip).
- Effect settings cannot change during the song yet (automation is phase
  3), and the compressor does not show how much it turns down.
- The tempo stays the same for the whole song (a tempo track is phase 5).
- The on-screen keyboard stays a piano keyboard on drum tracks; the step
  grid shows which computer key plays each drum.
- I cannot listen: drum timing and levels were checked by measuring
  rendered and recorded sound, not by ear. The balance between the piano
  and the kits deserves your ears.

## Phase 3 — Plugins

- [x] How plugins are hosted (decision 0015) and their screens (decision
      0016); reference plugins for tests: Surge XT, Surge XT Effects and
      Dexed, each as CLAP and VST3
- [x] One plugin host for CLAP and VST3: instruments and effects play in
      the song, their settings are saved with it, their own windows open
      (X11 on Linux, Win32 on Windows)
- [x] Scanner in a separate run of the program, so a broken plugin cannot
      take the program down while looking; what it learns is remembered
- [x] Instrument chooser ("+ Instrument…"): the included instruments, then
      every plugin instrument found, with a search field and "Look again"
- [x] Plugin effects in the mixer's "+ Add effect" menu
- [x] Plugin windows open from the track header, the mixer strip and the
      location line
- [x] Automation: lanes under each track in the song view for its volume,
      pan, its effects' settings and its plugin's parameters; the setting
      last moved in a plugin's window is offered first; click to add a
      point, drag to move, right-click to remove, all undoable
- [x] Songs whose plugins are not installed still open and play their
      other tracks; the missing plugin is named
- [x] File format version 3 (plugin settings as entries of their own), with
      migration of phase 1 and 2 songs
- [ ] The owner's hands-on check of the milestone build

### Phase 3 milestone: things to try

You need a plugin or two first. Two good free ones: **Surge XT** (a
synthesiser, with **Surge XT Effects**) from surge-synthesizer.github.io,
and **Dexed** (a classic FM synthesiser) from asb2m10.github.io/dexed. Their
installers put them in the usual folders.

1. Start the program and show the song (click the song's name at the left
   of the location line, or press F5). Click "+ Instrument…" under the
   tracks: the piano and drums, then your plugins, each with its maker and
   format. Type a few letters in Search to narrow the list.
2. Choose Surge XT (CLAP): a "Surge XT" track appears with a pattern. Click
   the small window button on its header (left of the zigzag): Surge's own
   window opens. Play the computer keyboard: you hear Surge. Turn something
   in its window, then close it with its close button.
3. Open the Surge pattern and draw a few long notes; press Space.
4. Back in the song, click the zigzag (Automation) button on the Surge
   track and choose Volume: a lane appears under the track with a flat line
   at the present volume. Click low in the lane a few bars later: a fade.
   Drag the point around; right-click it to remove it; Ctrl+Z undoes. Play:
   the sound fades as drawn.
5. Move a knob in Surge's window, then click Automation again: that knob
   is offered first ("Last moved in the plugin"). Draw it changing over the
   song.
6. Press F9 for the mixer. On a track, "+ Add effect" › Plugins › Surge XT
   Effects: its window opens. Built-in effects can change over the song
   too: Automation › "Reverb · Amount", for instance.
7. Save, close the program and open the song again: the plugins come back
   with their settings, and the lanes with their points. The window title
   shows no unsaved change until you change something (in a plugin's window
   too).
8. Export as WAV (Ctrl+E): the plugins and automation are in the file.
9. Try a VST3 one too (Dexed › VST3 in the chooser), and an effect plugin
   on the master strip.

### Known gaps in this build

- Plugins are only looked for in the usual folders (and `CLAP_PATH`);
  there is no setting for other folders yet.
- Automation is drawn, not recorded: moving a knob while the song plays
  does not write points. Lanes are per track (none on the master), points
  move one at a time, and the line between points is always straight.
- While a lane changes a track's volume or pan, the mixer's fader for it
  is overruled during playback (its tooltip says so).
- A track's instrument cannot be swapped for another; add a new track.
- Plugins run inside the program: one that crashes takes the program with
  it (your work is autosaved). Only a plugin's main stereo input and output
  are used.
- What I could not check myself: closing a plugin window with its own close
  button on a real desktop (my virtual screen has no window manager; the
  program's own buttons and its tests close them), and using the program
  with plugins on Windows (the automated tests load, play, save and open
  windows for all three reference plugins on Windows too). I cannot listen:
  levels and fades were checked by measuring recorded sound.

## Phase 4 — Audio

- [x] How sound is kept and shown (decision 0017): audio tracks, clips that
      start on the beat and play their file at its own speed, the song
      file keeping its audio files (format version 4)
- [x] Recording from the default input (the built-in microphone or an
      interface): Record on an audio track counts in and records from the
      play position; the take grows on screen with its waveform and an
      input meter; it is lined up by the delays the system reports; one
      undo step
- [x] The input opens only while recording (and half a minute after), and
      the person is told when no sound came in or no input exists
- [x] Importing WAV, FLAC, MP3 and Ogg Vorbis: "Import audio…" (Ctrl+I) or
      dropping files on the song; several files get a track each, lined up
- [x] Shaping clips on the timeline: trim either end (pattern clips too),
      fade in and out with handles, split (at the mouse or the play
      position, Ctrl+B), volume, move to another audio track; a new take or
      import replaces what it covers
- [x] Waveforms drawn from loudness summaries at any zoom; files decoded
      and converted to the engine's rate in the background (our own
      converter)
- [x] Audio clips through the same mixer, effects, automation and export
      as everything else; cuts never click; a tempo change keeps sound at
      its own speed
- [x] Songs open and save with their sound; autosave keeps it; a recovered
      song comes back with it
- [ ] The owner's hands-on check of the milestone build

### Phase 4 milestone: things to try

Headphones are best for recording: the program does not play the
microphone back to you, and speakers would be recorded too.

1. Show the song (click the song's name at the left of the location line,
   or press F5) and click "+ Audio" under the tracks: an "Audio" track
   appears, in focus. Its empty row says what to do.
2. Press Ctrl+R (or the red button, whose tooltip now says "Record
   sound"): one bar of clicks, then sing or play into the computer's
   microphone. A red clip grows with your waveform and a meter in the
   track's header shows how loud you are. Press Space to stop: the status
   line says how many seconds were recorded. One Ctrl+Z takes it away.
3. Press Space to hear it with the rest of the song. Click an empty spot of
   the audio track at bar 3: the play position goes there; record again:
   the new take starts there and replaces what it covers.
4. Bring in a sound file: menu › Import audio… (Ctrl+I), or drag WAV,
   FLAC, MP3 or Ogg files from your file manager onto the song view: onto
   an audio track (it lands on the beat under the pointer), or elsewhere
   for a new track. Several files at once get a track each, lined up.
5. Shape a clip: drag its left or right end to trim it (it stops on beats;
   hold Alt to place it exactly), drag the small squares at its top
   corners to fade it in or out, drag the clip to move it, also onto
   another audio track. Right-click it: Split here, and Volume. Or select
   it, put the play position inside it and press Ctrl+B.
6. Press F9: audio tracks have mixer strips and effects like any track;
   their automation lanes work too.
7. Save, close the program and open the song again: the sound is inside
   the song file. Change the tempo: recordings keep their own speed.
8. Export as WAV (Ctrl+E): the audio clips are in the file.
9. A drum sound's name (left of the step grid) plays the sound once per
   click again, even on a quick tap of a touchpad (see the known gaps).

### Known gaps in this build

- No time-stretching: after a tempo change, recordings and imported loops
  play at their own speed and drift from the beat.
- The program does not play the microphone back through itself (no input
  monitoring); use headphones plugged into the computer or the interface.
- Takes are lined up using the delays the system reports. On the test
  setup (a virtual PipeWire sound card) they landed within about 5 ms of
  the beat, varying by one audio block from one opening of the devices to
  the next. There is no setting to adjust this by hand yet.
- While recording sound the song plays on through its loop: one take is
  one clip (no takes per loop pass).
- Several clips cannot be selected at once, and clips have no copy and
  paste (Duplicate places a copy right after a clip).
- A song's sound is kept in memory while it is open (about 23 MB per minute
  of stereo sound); recordings are stored as 24-bit WAV in the song file
  (FLAC later). A crash in the middle of a take loses that take.
- The stacking noise the owner heard when clicking a drum sound's name did
  not happen on the test setup; the click now plays its sound on the press
  itself, once, which also fixed quick clicks that played nothing. Please
  try it again and say which system it was on if it still happens.
- What I could not check myself: a real microphone (recording was tested
  with a virtual one fed by the program's own output), the import file
  dialog (the test screen has no dialog program; importing itself is
  tested), dragging files from a real file manager (tested by simulating
  the drop), and phase 4 on Windows by hand. I cannot listen: timing and
  levels were checked by measuring recorded sound.

## Phase 5 — Theory and rhythm

Key and scale highlighting, snap-to-scale, chord stamps, inversions,
transposition by interval, tempo and time-signature track, tuplets, swing.

## Phase 6 — Completing v1

Subtractive synth, DAWproject and MIDI import and export, FLAC export,
autosave polish, installers (Windows installer, Linux AppImage), generated
third-party notices, user guide.

## After v1 — Singing voices (the owner's request, November 2026)

Sing from the piano roll: give notes lyrics and have a voicebank sing
them, with the voice managed and tuned inside VLabs, in the spirit of
OpenUtau (MIT-licensed, the open successor of UTAU). Not started; planned
for after v1.

- **Voicebanks the person installs**, in the open formats that matter
  most: classic UTAU voicebanks (WAV recordings with `oto.ini` timing, in
  CV, VCV or CVVC style, with `character.txt`), such as the many Kasane
  Teto voicebanks, and OpenUtau's machine-learning formats, DiffSinger
  (the one OpenUtau recommends) and ENUNU. Other open formats are judged
  when the work starts, by how open and how widely used they are.
- **Managed and tuned inside VLabs:** a list of installed voicebanks
  (add, read its terms, remove); lyrics on notes; phoneme timing; pitch
  curves and vibrato drawn over the notes; voice colour and the usual
  UTAU flags; rendering in the background, the result kept and played
  like an audio clip.
- **To decide in its own decision record:** a renderer of our own in Rust
  following OpenUtau's design, or OpenUtau's engine running in its own
  process; the vocoder (WORLD) and, for DiffSinger, an ONNX runtime, each
  checked against the licence policy (decision 0005); importing and
  exporting USTX (OpenUtau) and UST (UTAU) projects.
- **Voicebanks are not bundled** unless their terms allow it like CC0
  (decision 0008): each has its own terms of use, set by its creators
  (Kasane Teto voicebanks differ: some allow commercial use, some do not),
  and VLabs shows them when a voicebank is added.
- **Already leaves room:** notes keep space for per-note pitch offsets;
  lyrics and pitch curves will be new optional fields of a note (a new
  file format version, with its migration test).

## Later (not in v1, not designed out)

VST2 (clean-room headers and a licensing check first), a 32-bit plugin
bridge, LV2, plugins running in their own process, a staff-notation view,
microtonal tunings (the note model already leaves room for per-note pitch
offsets), lower-latency Windows audio modes (low-latency shared mode,
optional exclusive mode), cpal's native PipeWire backend, native Wayland
plugin editors as plugins adopt them.

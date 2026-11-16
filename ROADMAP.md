# Roadmap

The phases come from the project brief. Each ends with a build the owner can
run and a checklist of things to try. Status is updated at every milestone.

**Now:** phase 1 (first sound) is built and waiting for the owner's
hands-on check (checklist below). Phase 0 is complete: all three risk spikes
passed on Windows and Linux (decision 0009).

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
- [ ] The owner's hands-on check of the milestone build

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
- The sound is on the quiet side (a three-note chord peaks around −16 dBFS);
  a master limiter in phase 2 will let the level go up safely.
- Open and save dialogs are not attached to the window, so on some desktops
  they may open behind it.

## Phase 2 — Making a beat

Step sequencer, drum sampler with the starter kit, patterns, arrangement
timeline, mixer, built-in effects (EQ, compressor, reverb, delay, limiter),
WAV export.

## Phase 3 — Plugins

Scanner (separate process), plugin browser, VST3 and CLAP instruments and
effects, editor windows, state saving, parameter automation.

## Phase 4 — Audio

Recording from the built-in microphone or an interface, audio clips on the
timeline, cut, trim, fades, waveform display, file import (WAV, FLAC, MP3,
OGG).

## Phase 5 — Theory and rhythm

Key and scale highlighting, snap-to-scale, chord stamps, inversions,
transposition by interval, tempo and time-signature track, tuplets, swing.

## Phase 6 — Completing v1

Subtractive synth, DAWproject and MIDI import and export, FLAC export,
autosave polish, installers (Windows installer, Linux AppImage), generated
third-party notices, user guide.

## Later (not in v1, not designed out)

VST2 (clean-room headers and a licensing check first), a 32-bit plugin
bridge, LV2, plugins running in their own process, a staff-notation view,
microtonal tunings (the note model already leaves room for per-note pitch
offsets), lower-latency Windows audio modes (low-latency shared mode,
optional exclusive mode), cpal's native PipeWire backend, native Wayland
plugin editors as plugins adopt them.

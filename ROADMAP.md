# Roadmap

The phases come from the project brief. Each ends with a build the owner can
run and a checklist of things to try. Status is updated at every milestone.

**Now:** phase 3 (plugins). Phase 2 is complete and the owner confirmed
its milestone build works. Phase 1 is complete and the owner confirmed its
build works on Linux. Phase 0 is complete: all three risk spikes passed on
Windows and Linux (decision 0009).

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

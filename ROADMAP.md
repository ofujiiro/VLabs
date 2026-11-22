# Roadmap

The phases come from the project brief. Each ends with a build the owner can
run and a checklist of things to try. Status is updated at every milestone.

**Now:** phases 0 to 6 and the plugins and sound libraries round are
built and merged (pull requests 1 to 7), including SFZ instruments
(decision 0024), custom kits (a beta tester's idea, decision 0025) and
fixes for plugins the owner tried (MT Power Drum Kit 2, Decent Sampler),
merged in pull request 7. The owner has tried every milestone build and
the plugins round (November 2026): everything works, which closes v1.
A new or opened song now starts on the whole song, not an instrument
(the owner's request). Glides now have shapes, and the tempo shows under
the piano roll too (decision 0026). Singing voices have begun: milestone
7a, a first voice sung from classic UTAU voicebanks, is built (decision
0027). Both are merged into `main`; the owner has sung with a real
voicebank on that build: everything works (November 2026). Milestone 7b,
expression, the voice's tuning (decision 0028: the sung pitch drawn
through the notes and shaped in a Pitch mode, vibrato, and each note's
loudness, colour and breath), is merged into `main` too (November 20,
2026) and awaits the owner's hands-on check. A voicebank's picture now
shows faintly on its track, its clips and behind its notes (decision 0029,
the owner's request). **VLabs 1.0 is released** (decision 0030): version
1.0.0, published on the repository's Releases page with a Windows
installer and a Linux AppImage. After 1.0, the owner's reports: adding a
second library folder crashed the program (fixed), and sounds could not
be shaped in the program: **shaping sounds** (decision 0031) adds note
shapes for the instruments (a one-click release cut) and a sound editor
for audio clips; built on a development branch, awaiting the owner's
check and a 1.1 release. Next: 7c, UST and USTX projects in and out,
unless the owner's check of 7b asks for more tuning first.
The repository has been public since November 2026 (the owner's
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
- [x] The owner's hands-on check of the milestone build

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

1. Start the program: a new song opens on the whole song (since November
   2026; it opened on the drum step grid before). Double-click the
   **Beat 1** clip to open the step grid. Click the
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
- [x] The owner's hands-on check of the milestone build

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
- [x] The owner's hands-on check of the milestone build

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

- [x] How it is shown and edited (decision 0018): the song has a key, with
      changes at bars; chord stamps; a signature row in the song view for
      tempo, time-signature and key changes; swing per pattern
- [x] The song's key: a button in the transport bar for the key at the
      play position; the piano roll shades the notes outside the key and
      marks the tonic; "Keep notes in the key" (Snap menu) puts new and
      dragged notes on notes of the key, and the arrows move by its steps
- [x] Chord stamps ("Draw"): a click places a chord built in the key on
      the clicked note (triads or seventh chords: in C major, D gives D
      minor) or a chord of a chosen kind (18 kinds), in any inversion; the
      pointer shows the chord and its name before the click
- [x] Inversions (Alt+Up, Alt+Down) and transposition by any interval, or
      by steps of the key ("Transpose" menu)
- [x] Tempo changes anywhere, and glides (a gradual speeding up or slowing
      down) worked out exactly, on the audio thread too
- [x] Time-signature changes at any bar, any meter (1 to 64 beats over
      1/2 to 1/32); changing a signature keeps the song in its bars: clips,
      automation, tempo changes and the loop keep their bar and beat
- [x] Tuplets: triplet, quintuplet and septuplet grids and note lengths,
      dotted lengths, quintuplet and septuplet steps; every grid counts from
      the bar line
- [x] Swing per pattern (0 to 100 %, on eighths or sixteenths), applied as
      it plays; notes recorded into a swung pattern are written where they
      play back as heard
- [x] Patterns of any length (drag the end of the pattern): patterns of
      different lengths turn against each other
- [x] File format version 5, with migration of phase 1 to 4 songs
- [x] The owner's hands-on check of the milestone build

### Phase 5 milestone: things to try

1. Choose the song's key: in the top bar, click "No key" (right of the
   time signature), then A and minor. Open the piano (location line:
   Drums › Piano): the rows outside A minor are shaded, the A rows are
   marked, and the keys on the left have a dot for each note of the key.
2. Open Snap and tick "Keep notes in the key". Click a shaded row: the
   note lands on the nearest note of the key. With a note selected, the
   up arrow moves it to the next note of the key.
3. Draw › "Triads in the key". Move the pointer over the grid: the chord
   a click would place is shown, with its name ("D minor"). Click: the
   whole chord is placed and heard; drag to make it longer. Try a chord
   of one kind (Major seventh) and an inversion.
4. Select a chord, then Transpose › Transpose up › "Up a perfect fifth",
   and Transpose › Invert up (or Alt+Up).
5. Snap now has triplets, quintuplets and septuplets; New notes has
   dotted lengths. Draw a line of quintuplets.
6. On the drum beat, click "Straight" and choose "Triplet feel": the
   hi-hats swing, and the swung hits are drawn late in their squares.
7. Drag the small tab at the right end of the step grid's ruler to 12
   steps: the beat now repeats every three beats against the piano's
   four bars.
8. Show the song (F5) and click the arrow next to "Tempo and
   signatures" under the ruler. Click bar 9 in the Signatures strip and
   choose 7/8: everything after it moves along with its bars. Click the
   tempo line at bar 5: a tempo change; drag its point up. Click the first
   point and tick "Glide to the next tempo": the song speeds up gradually
   until bar 5.
9. The tempo, time signature and key in the top bar show what is in
   effect at the play position, and change that part of the song.
10. Save, close and open the song again: the key, the changes, the swing
    and the pattern lengths come back. A phase 4 song opens too.

### Known gaps in this build

- No chord track (chords per bar shown over the song), and no beat
  grouping for odd meters: 7/8 is counted in seven, and the metronome
  accents only the first beat.
- Tuplets come from the grid; there is no command that turns existing
  notes into a tuplet.
- Swing is per pattern (no groove for the whole song), and the piano roll
  shows notes where they are written (the step grid shows the swing).
- The delay effect keeps the song's first tempo when the tempo changes.
- Note names use sharps ("C#4") even in flat keys; key names follow the
  usual spelling ("Bb minor", "C# minor").
- Escape does not close the menus of the location line (Draw, Snap,
  Swing); clicking elsewhere does.
- What I could not check myself: listening (swing was measured in the
  program's recorded output: hits alternate 167 and 83 ms at 120 BPM with
  the triplet feel, as they should; glides and swing are also measured in
  offline renders), and phase 5 on Windows: CI runs every test on Windows
  once the branch is pushed.

## Phase 6 — Completing v1

- [x] How each part works and looks (decisions 0019 to 0022)
- [x] The built-in synth (decision 0019): two oscillators (sine,
      triangle, saw, square; band-limited) and noise, a filter (low-pass
      steep or gentle, high-pass, band-pass) with its own envelope, a
      volume envelope, an LFO for vibrato, wobble and tremolo, 16 voices
      or one with glide, eight presets; its own window (presets, then
      oscillators, filter with its curve, envelopes with their shapes,
      movement and playing); its knobs can be automated; file format
      version 6
- [x] Standard MIDI Files in and out (our own reader and writer): a song
      goes out as the notes it plays (swing, repeats), with tempo changes
      (glides as small steps), time signatures and keys; a file comes in
      as tracks by channel (drums on channel 10, General MIDI synth sounds
      on the synth)
- [x] DAWproject in and out: tracks, mixer, instruments, effects (the EQ,
      compressor and limiter also as DAWproject's own), plugins with their
      settings, notes, audio with fades, automation, tempo (glides as
      linear tempo automation), time signatures; another program's file is
      read with what was left out named; exported files match the official
      schema
- [x] FLAC export (16 or 24 bits), written as the song renders
- [x] Open takes VLabs songs, DAWproject and MIDI files; Import (Ctrl+I)
      takes sound and MIDI files; Export is one dialog (a sound file, a
      project for another program, the notes as a MIDI file)
- [x] Autosave polish (decision 0021): a take being recorded survives a
      crash, a copy is written right before a plugin loads or opens its
      window, the recovery offer names the song and says how old the copy
      is
- [x] Patterns: a right-click on an empty spot of a track places any of
      its patterns (or a new one); a clip's menu swaps the pattern it plays
      (the owner's report)
- [x] The Windows installer (NSIS, per user, file type, uninstaller) and
      the Linux AppImage, built by "Program builds"; generated licence
      notices in every release; the user guide (F1 opens it)
- [x] Version 0.6.0
- [x] The owner's hands-on check of the milestone build

### Phase 6 milestone: things to try

1. Show the song (F5), click **+ Instrument…** and choose **Synth**. Click
   the small window button on the synth track's header: the synth opens.
   Play the computer keyboard while you try the presets (Bass, Lead, Warm
   pad…) and move the Cutoff, Resonance and Envelope sliders.
2. On the synth track, click the zigzag (Automation) and choose
   "Synth · Cutoff"; draw the cutoff rising over a few bars and play.
3. In a drum pattern, duplicate the beat (location line menu), change the
   copy, go back to the song and right-click an empty spot on the drum
   track: choose the copy. Right-click the first clip: "Play another
   pattern" swaps it.
4. Menu › **Export…**: try a FLAC file, a DAWproject file and a MIDI file.
   Open the DAWproject in another program if you have one (Bitwig Studio,
   Studio One, Cubase), and the MIDI file in any music program.
5. Menu › **Open…** a MIDI file from the internet: it opens as a new song
   with its tracks. Or **Import…** (Ctrl+I) a MIDI file into your song.
6. Record a take of sound on an audio track and, while it records, end the
   program from the system (or pull the plug): start it again and recover
   your work; the take is there.
7. Menu › **User guide** (F1).
8. Install from the downloads of "Program builds": the Windows installer
   (Windows warns it is from an unknown publisher: More info › Run anyway)
   and the Linux AppImage (make it executable, double-click it).

### Known gaps in this build

- The synth has two oscillators and one LFO; no unison, no effects of its
  own (use the track's effect chain).
- MIDI export leaves out audio tracks and plugin settings (MIDI files hold
  notes); MIDI import ignores controllers and pitch bends.
- DAWproject cannot carry the song's key or an audio clip's own volume;
  reading another program's file leaves out group tracks and sends,
  time-stretching and the noise gate (the status line names what).
- The installer is not signed (a certificate costs money: the owner's
  call), so Windows SmartScreen warns once.
- What I could not check myself: the installer and the AppImage on real
  machines (CI builds them; running the downloaded build tools locally was
  not permitted in my session), opening our DAWproject files in Bitwig,
  Studio One or Cubase (they match the official schema), listening (the
  synth's pitch, envelopes, filter and levels were measured), phase 6 on
  Windows by hand (CI runs every test on Windows).

## Plugins and sound libraries (the owner's request, November 2026)

Built after phase 6 (decision 0023), merged in November 2026 (pull
request 4).

- [x] A Plugins window (menu, and the instrument chooser): every plugin
      found, the files that cannot be used and why, a search field.
- [x] Add plugins by dropping a `.vst3`, `.clap`, a folder or a downloaded
      `.zip` on the window, or with the window's buttons: copied into the
      program's own plugin folder, always searched.
- [x] Remove plugins added this way; add and remove folders to search.
- [x] Installers: opened on Windows; run through Wine on Linux.
- [x] Windows plugins on Linux: set up through yabridge automatically when
      Wine and yabridge are installed; the window shows how to install them.
- [x] VST2 plugins (`.dll`) and plugins for other systems are recognised
      and explained.
- [x] A sound library in place of the keyboard (F6): folders of sounds, MIDI
      files and SoundFonts, however large; browse, search, click to hear,
      drag onto the song or double-click to add.
- [x] AIFF files read (import, library).
- [x] SoundFont (`.sf2`) instrument tracks, with their presets in the track
      menu.
- [x] SFZ instruments (decision 0024; merged in pull request 7): `.sfz`
      files in the library, dropped, imported or chosen in
      **+ Instrument…** (now **SoundFont or SFZ…**) become tracks played
      by a new built-in sampler; read in the background, however large;
      what an instrument uses that VLabs does not play is said once;
      file format version 7.
- [x] Custom kits (decision 0025; pull request 7): **Custom kit** in
      **+ Instrument…**, or **Make a kit of these sounds** on a library
      folder, makes a drum track of your own sounds, one pad per sound, with
      beats on the step grid; more sounds by **Add sounds…**, dropping or
      dragging from the library; the sounds are kept in the song.
- [x] Plugins the owner tried (pull request 7): plugin windows that ask to be
      shown on Linux are shown (MT Power Drum Kit 2's window was blank);
      the program no longer builds a plugin's window on every frame to ask
      whether it has one (it crawled with MT Power Drum Kit 2, whose START
      button was undone); a plugin missing a system library says which.
- VST2 plugins: kept out of v1 (the owner's decision, November 2026, as
  the brief says).

### Things to try

1. Open the menu, choose **Plugins…**: your plugins are listed, with who
   made them; files that cannot be used are explained.
2. Download a free plugin (for instance Surge XT or Dexed) and drop the
   `.zip` itself on the VLabs window. The Plugins window says "Added: …",
   and **+ Instrument…** offers it. Click **Remove** next to it: it goes.
3. Drop a `.dll` (a VST2 plugin) or a Mac plugin: VLabs says why it cannot
   use it and what to look for instead.
4. On Linux with a Windows `.vst3`: install Wine and yabridge as the
   Plugins window says, click **Check again**, then drop the Windows
   plugin. It should appear as "Set up with yabridge" and be playable.
5. Press **F6**, click **Add a folder…** and choose a folder of samples
   (thousands of files is fine). Click sounds to hear them, type in
   **Search**, drag a sound onto the song, double-click another.
6. Drop a whole folder of samples on the window: it joins the library.
7. Drop a SoundFont (`.sf2`) on the window, or choose **SoundFont or
   SFZ…** in **+ Instrument…**: a track plays it; pick its sounds in the
   track menu (the track's name at the top left of its pattern).
8. **SFZ instruments:** download a free one (the sfzinstruments collection
   on GitHub has many, such as Ganjo or Karoryfer's Cowsynth), unzip it,
   and add its folder to the library (F6). Double-click the `.sfz` file, or
   drag it onto the song: a track named after it appears ("SFZ · …" under
   its name). Play it from the keyboard. A very large instrument says
   "Reading the instrument…" for a moment first.
9. The pattern menus from phase 6: right-click an empty spot of a track to
   place any of its patterns by name; right-click a clip, **Play another
   pattern**, to swap it.
10. **Custom kits:** **+ Instrument…**, **Custom kit**, choose a few of your
    own sounds at once: the step grid opens with a row per sound; click
    squares and press Space. Click **Add sounds…** for more, or drag a
    sound from the library onto the grid. Click a row's name to tune it or
    shorten it. Then right-click a folder of sounds in the library, **Make
    a kit of these sounds**. Save, close, open the song again: the kit is
    all there.
11. **MT Power Drum Kit 2:** add it with **+ Instrument…**, open its window
    (the window button on its track): the window shows (it was blank), and
    the program stays quick. Click **START** at the bottom right (needed
    every session in its free version), then play: it sounds. **Decent
    Sampler:** open its window, load a sample library (**FILE…**,
    **Load…**), then play.

### Known gaps in this build

- **Windows plugins on Linux were not run end to end here**: the test
  computer has no Wine. The yabridge calls are tested with a stand-in for
  yabridge, and the Windows plugin files are set aside correctly; the rest
  needs a try on a real Linux desktop (item 4 above).
- **Installers were not run here**: opening one on Windows and running one
  through Wine on Linux are untested. A plugin's installer may also put it
  somewhere VLabs does not look; **Search another folder…** covers that.
- **VST2 plugins** are not supported (kept out of v1 by the owner).
- **SFZ instruments** play without LFOs (vibrato, tremolo), pitch
  envelopes and the instrument's own effects (the status line says so when
  an instrument uses them); settings moved by controllers (the modulation
  wheel…) take the controllers' starting values and do not move while
  playing. Their sounds are all kept in memory (at most 8 GB per
  instrument): very large libraries need as much memory as their sound.
  An instrument edited on the disk is read again only after the program
  restarts. The library does not preview them (double-click or drag them
  in). Songs with SFZ tracks are file format 7: older copies of VLabs
  refuse them.
- **Custom kits:** changing a pad's settings stops that kit's sounds
  already playing; a kit holds at most 128 sounds; the grid shows no
  waveforms. Songs with custom kits are file format 7.
- **Plugins the owner tried:** MT Power Drum Kit 2 was played in the
  program after its **START** button (Linux build). Decent Sampler's window
  and its file chooser work; playing it with a library loaded was not
  verified here. Sienna Bass is a Windows VST2 plugin
  (`.dll`), which VLabs does not support. Each plugin's own system
  libraries (GTK and WebKitGTK for DW-303) must be installed on Linux.
- The library cannot play MIDI files or SoundFonts to preview them (drag or
  double-click them in), shows no waveforms, and does not play sounds
  larger than 256 MB (drag those in).
- A large SoundFont (hundreds of megabytes) pauses the window for a moment
  when it is added or when a song using it opens.
- Wine and yabridge are not bundled with the Linux version (the owner's
  decision, November 2026): the Plugins window says how to install them.

### Checked

- [x] The owner's hands-on check of the plugins round, SFZ instruments,
      custom kits and the plugin fixes: everything tried works
      (November 2026).

## After v1 — Opening on the whole song (the owner's request, November 2026)

- [x] A new song, and a song opened from a file or recovered, starts on
      the whole song (the song view), not on an instrument's piano roll
      or step grid. Opened and recovered songs already did; a new song
      opened on the drum step grid and now opens on the song too.

## After v1 — Expressive tempo (the owner's request, November 2026)

Gradual tempo changes for piano writing (decision 0026), merged into
`main` in November 2026. Glides themselves came in phase 5; this round shapes
them and brings the tempo to the piano roll.

- [x] A glide has a shape: **Early** (most of the change first), **Even**
      (a straight line, as before) or **Late** (holds back, then changes
      more and more: a ritardando that gathers), or anything in between by
      dragging the handle in the middle of the glide. Worked out exactly,
      in the model and on the audio thread (file format version 8)
- [x] Glides are labelled *rit.* or *accel.*; a tempo point's panel can
      make the tempo **arrive there gradually**; right-clicking a glide's
      handle makes the change sudden again
- [x] The lane under the piano roll shows **Velocity** or **Tempo**: the
      song's tempo where the pattern plays, edited there as in the song view
- [x] A tempo change added inside a glide glides on with the same shape,
      so nothing changes until it is moved (before, the tempo after it
      jumped)
- [x] DAWproject: a bent glide goes out as straight pieces a sixteenth
      note long; MIDI files already follow any shape

### Things to try

1. Open a song with a melody, show the song (F5) and open **Tempo and
   signatures** under the ruler. Click the tempo line at bar 5 and drag
   the new point down to about 60.
2. Click that point and tick **Arrive gradually**: the line slopes from
   the start down to bar 5, marked *rit.*, with a small diamond in the
   middle. Play: the song slows down gradually.
3. Click the diamond and choose **Late**: the tempo now holds on, then
   slows more and more before bar 5. Try **Early**, then drag the diamond
   up and down and listen to the difference. Ctrl+Z undoes a whole drag.
4. Open the melody (double-click its clip). Under the notes, click
   **Tempo** (left of the lane): the same tempo line, under the phrase it
   shapes. Add a change there, open it, and tick **Glide to the next
   tempo**.
5. Save, close and open the song again: the shapes come back.

### Known gaps in this round

- No fermatas yet (a slower tempo for one beat does the job meanwhile).
- The tempo cannot follow a free performance yet (playing without the
  metronome and having the bars follow): on the "Later" list.
- The tempo lane under the piano roll is as tall as the velocity lane, so
  small tempo changes are drawn small; the song view's tempo strip is a
  little taller now.
- Not heard by ear: the shapes were measured (beat times against the
  formula, offline renders), not listened to.

## After v1 — Singing voices (the owner's request, November 2026)

Sing from the piano roll: give notes lyrics and have a voicebank sing
them, with the voice managed and tuned inside VLabs, in the spirit of
OpenUtau (MIT-licensed, the open successor of UTAU). Designed in decision
0027 as four milestones; **7a, a first voice, is built** and merged
into `main` (November 2026); **7b, expression, is built** (decision 0028)
and merged into `main` (November 20, 2026).

### 7a. A first voice (built)

- [x] Classic UTAU voicebanks added from a folder or a `.zip` (Japanese
      file names in Shift-JIS handled) into the program's own folder, in a
      **Voicebanks** window that shows each one's name, author and its
      creator's terms, and removes it
- [x] `oto.ini` (in the voicebank's folder and its subfolders),
      `prefix.map`, `character.txt` and the readme, in UTF-8 or Shift-JIS
- [x] Voice tracks from **+ Instrument…** › **Voices**; file format
      version 9 (a voice instrument, lyrics kept beside the notes)
- [x] Lyrics on notes in the piano roll: Enter or a double-click types
      one, words after a space go to the next notes, Tab moves on; a lyric
      the voicebank lacks is marked "?" and sung as "a"; `R` is a rest
- [x] Japanese lyrics in romaji or kana; VCV ("a か", "- か" at a phrase
      start), then CV, then romaji names, with `prefix.map` affixes
- [x] Our own renderer: each syllable shifted and stretched by
      pitch-synchronous overlap-add (formants kept), joined with UTAU's
      preutterance and overlap, a glide of 80 ms between notes; rendered
      phrase by phrase in the background, only what changed rendered
      again, played like an audio clip (mixer, effects, export)
- [x] Played live, a voice sounds its "a" at the key's pitch at once
- [x] Japanese text drawn with a Japanese font of the computer when it
      has one (none is bundled), otherwise lyrics in romaji
- [x] The owner's hands-on check with a real voicebank (November 2026):
      everything works

### 7a: things to try

1. Download a classic UTAU voicebank (a Kasane Teto voicebank, say, from
   its official site; read its terms). Open the menu, **Voicebanks…**,
   **Add a .zip…** (or **Add a folder…** for one already unpacked): it is
   listed with its terms.
2. **+ Instrument…**: the voicebank is under **Voices**. Choose it.
3. Open the voice track's pattern and click a short melody of four notes.
   Select the first, press Enter, type `ka sa ne te` and press Enter: the
   notes show their lyrics.
4. Play. The first time, the status line says the voice is being rendered
   for a moment; then it sings. Change a note's pitch or lyric and play
   again: only that phrase is rendered again.
5. Save, close and open the song: the voice and its lyrics come back.

### 7a: known gaps

- (Pitch curves, vibrato, loudness, colour and breath came in 7b.) The
  sound is UTAU-like; large shifts (an octave or more from where a
  syllable was recorded) sound buzzier.
- No UST or USTX import or export yet (7c); DAWproject keeps the voice
  track (without lyrics), MIDI files keep the notes.
- No DiffSinger or ENUNU voices yet (7d); no English or other languages'
  phonemizers yet.
- (Copying and pasting notes now carries their lyrics, and MIDI files
  carry them: closed in 7b.)
- Not heard by ear: the singing was measured (pitch, timing, no clicks)
  with synthetic voices made by the tests, not listened to, and not tried
  with a real voicebank here (none can be downloaded in the development
  container).

### 7b. Expression (built, decision 0028)

- [x] The pitch the voice sings drawn as a line through a voice track's
      notes, from the same arithmetic the engine sings with
      (`daw_model::expression`): faint while notes are edited
- [x] **Notes | Pitch** on a voice track's location line, in the place of
      Draw (Ctrl+P): in Pitch the notes stay put and the line's handles
      come forward
- [x] Pitch points per note, in milliseconds and cents: a click over a
      note bends the voice there, points are dragged (a glide's first
      point, which leaves from wherever the voice is, in time only),
      clicked for their exact values and the curve's shape after them
      (smooth, straight, slow start, slow end), right-clicked away; notes
      without points glide exactly as in 7a
- [x] Vibrato per note: **Transpose › Add vibrato** for the selected notes
      (±35 cents at 5.5 Hz, from 0.25 s, growing over 0.2 s: short notes
      stay plain); a diamond dragged for its start and depth, clicked for
      its depth, rate, start and growth
- [x] The lane under a voice's notes: **Velocity** (now how loud each note
      is sung), **Colour** (formants moved up to four semitones, the pitch
      kept), **Breath** (a whisper of the syllable mixed in above 1 kHz)
      and **Tempo**
- [x] Rendered again only where an edit changed something; file format
      version 10 (each note's expression kept beside it), with its sample
      and migration check
- [x] Copying, pasting and duplicating notes carry their lyrics and
      expression; MIDI files carry lyrics, out and in (7a gaps)
- The vocoder planned for 7b in decision 0027 waits: colour and breath
  work in the resampler we have, and whether real voicebanks need a
  vocoder is for the owner's first real voicebank to show (decision 0028)

### 7b: things to try

1. Open a voice track's pattern with a few notes (7a's steps above), and
   click **Pitch** at the top right of the window (or press Ctrl+P): the
   notes dim and a white line shows the pitch the voice sings, gliding from
   note to note.
2. Click above the middle of a long note: the line now rises to that spot
   and stays there. Play: the voice bends up. Drag the new point around;
   right-click it to remove it. Ctrl+Z undoes.
3. Click the small hollow circle where a glide leaves the note before and
   drag it to the left: the voice leaves earlier and glides more slowly.
   Click it (without dragging) and try **Straight** or **Slow end**.
4. Select the notes (Ctrl+A in **Notes**, or Ctrl-drag a box around them)
   and choose **Transpose › Add vibrato**: the long notes start to swing a
   moment in. Drag a vibrato's diamond up for a wider vibrato; click it and
   change its **Rate**.
5. Under the notes, click **Breath** and drag one note's bar up; then
   **Colour** and drag another's above or below the middle line. Play and
   compare with the notes left plain.
6. Save, close and open the song: the bends, vibratos, colours and breath
   come back. Copy a phrase (Ctrl+C) and paste it (Ctrl+V): the copy sings
   the same lyrics the same way.

### 7b: known gaps

- No shapes drawn freehand (points and their curves only), no envelope
  inside a note (the track's volume automation does crescendos), no other
  UTAU flags, no vocoder yet.
- In a pattern placed several times at different tempos, the line is drawn
  at the tempo where the pattern is first placed (the glides last as long
  in seconds everywhere, so they cover a little more or less of a note).
- A swung voice pattern is drawn where its notes are written; the voice
  sings them swung.
- Colour moves the formants by reading grains faster or slower: very large
  colours with very low notes can sound grainy. Breath is a whisper of the
  same syllable, not a model of the breath itself.
- Not heard by ear: pitch, vibrato rate and depth, formant movement, the
  noise of breath and levels were measured, in tests and in the program's
  own output on a virtual sound card, with a synthetic voice made for the
  tests; no real voicebank was tried.

### Voicebank pictures (built, decision 0029)

The owner's request: "for a voicebank, have the avatar display on the song
visualisation, on the instrument, and in the pattern editor, as a low
opacity image."

- [x] A voicebank's picture, faint, where its voice sings: in its track's
      header (just left of the track's buttons), at the start of each of
      its clips, and large behind its piano roll's notes (at the bottom
      right, still while the notes scroll); small and clear beside its
      name in **+ Instrument…**
- [x] Both pictures voicebanks carry: the small one named in
      `character.txt` (usually a 100 × 100 BMP) in the small places, and
      OpenUtau's portrait named in `character.yaml` (usually a large PNG)
      behind the notes, each standing in for the other; BMP, PNG and JPEG,
      read once, in the background

### Voicebank pictures: things to try

1. Add a voicebank that has a picture (most do; voicebanks made for
   OpenUtau often have a large portrait as well) and add its voice track:
   the picture shows faintly in the track's header and at the start of its
   clips.
2. Open one of its clips: the picture (the portrait, if it has one) stands
   faintly behind the notes at the bottom right and stays put while you
   scroll; the notes and the pitch line stay readable over it.
3. **+ Instrument…**: the voicebank's small picture is beside its name.

### Voicebank pictures: known gaps

- The Voicebanks window lists voicebanks without their pictures.
- A picture changed on the disk shows after the voicebank is added again
  or the program starts again.
- Not tried with real voicebanks' pictures (none can be downloaded in the
  development container): checked with a 100 × 100 BMP and a large PNG
  portrait made for the tests.

### Later milestones (decision 0027)

- **Next, 7c:** UST (UTAU) and USTX (OpenUtau) projects in and out; 7b's
  pitch points, shapes and vibrato follow OpenUtau's, so they map across.
  **Then 7d:** DiffSinger voices, after a spike on licences, model sizes
  and speed. **A vocoder** (WORLD-style, for larger shifts and more flags)
  is a milestone of its own, judged once real voicebanks have been heard
  (decision 0028).
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
  lyrics (7a) and expression (7b) are kept beside the notes, by note.

## Release 1.0 (the owner's request, November 2026)

Decision 0030. No server is needed: GitHub builds the files and hosts
them.

- [x] Version 1.0.0: everything on `main`, v1 as the brief defines it and
      the rounds after it (opening on the whole song, expressive tempo,
      singing 7a and 7b, voicebank pictures)
- [x] Published by "Program builds" when a tag `v<version>` is pushed: the
      Windows installer, the Linux AppImage, the plain program folders
      (`.zip` for Windows, `.tar.gz` for Linux), `SHA256SUMS.txt`, and the
      notes written in `docs/releases/`; the tag must name the version
- [x] VLabs 1.0 published on the repository's
      [Releases page](https://github.com/ofujiiro/VLabs/releases/tag/v1.0.0)
      (November 20, 2026); the published Linux downloads checked: the
      checksums match, both start on Ubuntu 24.04's glibc and say 1.0.0

### Release 1.0: things to try

1. Open the [Releases page](https://github.com/ofujiiro/VLabs/releases):
   **VLabs 1.0** shows its notes and files.
2. On Windows, download `VLabs-1.0.0-setup.exe` and install it (Windows
   warns about an unknown publisher: **More info**, **Run anyway**); on
   Linux, the AppImage. Start VLabs: the menu's **About** says 1.0.0.
3. Open a song saved with an earlier build: it opens as it was.

### Release 1.0: known gaps

- The installer is not signed: a code-signing certificate is a yearly cost,
  the owner's decision whenever they want it.
- Every known gap listed above, for each round, is still there; the
  release notes name the ones a newcomer meets first.

## After 1.0 — Shaping sounds (the owner's request, November 2026)

Decision 0031. "There is no way to change a sound in the program, for
instance to cut it off earlier; add ways to change imported sounds and the
instruments, for instance to make a release cut piano."

- [x] A library folder added after another no longer crashes the program
      (the library showed the new folder before it had taken it in)
- [x] Note shapes: an attack and a release for every track whose
      instrument plays recorded sounds (the piano, SoundFonts, SFZ
      instruments, drum kits, custom kits), each the instrument's own or
      a time; the piano reads its SoundFont again with the new times; the
      samplers take them while playing; with a release, kit hits stop at
      their note's end, and a SoundFont's own reverb is left out
- [x] The Note shape panel, from the track's header and the pattern's
      location line: a picture of the shape, the two times, and Cut at
      note end, Soft start, Long tail, Natural
- [x] Audio clips play backwards or at another pitch (tape-style), in the
      model, the engine, the timeline's waveforms; format version 11
- [x] The sound editor: double-click an audio clip (or Edit sound… in its
      menu): its whole sound with the played part lit, ends that drag
      exactly, fade handles, volume, pitch, Backwards, As loud as it can be
- [x] Checked: offline renders of the bundled piano (a cut note is 60 dB
      down 50 ms after its end; the natural one still rings; no click), the
      kits and the sampler; the format sample for version 11; interface
      tests for both panels; the real program on the virtual screen and
      sound card (chords recorded from it: silent 50 ms after each chord
      with the cut, -9 to -16 dB without), screenshots of both windows
- [ ] The owner's hands-on check, then release 1.1

### Shaping sounds: things to try

1. In a song with the piano, click the small rise-and-fall button in the
   piano track's header, then **Cut at note end**. Play: each chord stops
   dead when it ends. **Natural** brings the ringing back.
2. Tick **Attack** and drag it right: notes swell in. **Long tail**: they
   ring on after their key comes up.
3. On a drum track (or a custom kit made of your sounds), choose **Cut at
   note end**, then lengthen and shorten the notes in the pattern: each
   hit stops where its note ends.
4. Import a sound, double-click its clip: the sound editor opens. Drag the
   right end to cut the clip shorter, the small squares to fade it, then
   try **Backwards**, **Pitch** and **As loud as it can be**. Ctrl+Z undoes
   each step.
5. Save, close and open the song again: the shapes and edits are kept.

### Shaping sounds: known gaps

- A sound I could not hear: the cut, swell and long tail were measured, not
  listened to; how they feel on the bundled piano is for the owner's ears.
- The bundled piano's own release is short already (about 0.15 s), so its
  cut is a short tail removed; a release cut is most striking on sustained
  sounds (organs, pads, strings from SoundFonts or SFZ instruments).
- A shaped SoundFont keeps a second copy of its file in memory while it
  plays (19 MB for the piano).
- DAWproject export keeps neither note shapes nor backwards or re-pitched
  clips (such clips go out forwards at their own pitch).
- Pitch changes speed with it (as a tape); changing pitch while keeping the
  length (time-stretching) is for later.

## Later (not in v1, not designed out)

VST2 (clean-room headers and a licensing check first), a 32-bit plugin
bridge, LV2, plugins running in their own process, a staff-notation view,
microtonal tunings (the note model already leaves room for per-note pitch
offsets), lower-latency Windows audio modes (low-latency shared mode,
optional exclusive mode), cpal's native PipeWire backend, native Wayland
plugin editors as plugins adopt them, the tempo following a free
performance (play without the metronome, tap or find the beats, and the
bars follow; decision 0026), fermatas, drawing the tempo freehand.

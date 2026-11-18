# Roadmap

The phases come from the project brief. Each ends with a build the owner can
run and a checklist of things to try. Status is updated at every milestone.

**Now:** phases 0 to 6 and the plugins and sound libraries round are
built and merged (pull requests 1 to 5). Next: SFZ instruments, then the
owner's hands-on checks close v1; the singing voices come after v1. The
owner confirmed the phase 1 and 2 builds; the phase 3 to 6 builds and the
plugins round await their hands-on check. The repository has been public
since November 2026 (the owner's decision).

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
- [ ] The owner's hands-on check of the milestone build

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
- [ ] The owner's hands-on check of the milestone build

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
- [ ] SFZ instruments: planned next.
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
7. Drop a SoundFont (`.sf2`) on the window, or choose **SoundFont…** in
   **+ Instrument…**: a track plays it; pick its sounds in the track menu
   (the track's name at the top left of its pattern).
8. The pattern menus from phase 6: right-click an empty spot of a track to
   place any of its patterns by name; right-click a clip, **Play another
   pattern**, to swap it.

### Known gaps in this build

- **Windows plugins on Linux were not run end to end here**: the test
  computer has no Wine. The yabridge calls are tested with a stand-in for
  yabridge, and the Windows plugin files are set aside correctly; the rest
  needs a try on a real Linux desktop (item 4 above).
- **Installers were not run here**: opening one on Windows and running one
  through Wine on Linux are untested. A plugin's installer may also put it
  somewhere VLabs does not look; **Search another folder…** covers that.
- **VST2 plugins** are not supported (kept out of v1 by the owner), and
  **SFZ instruments** are not there yet.
- The library cannot play MIDI files or SoundFonts to preview them (drag or
  double-click them in), shows no waveforms, and does not play sounds
  larger than 256 MB (drag those in).
- A large SoundFont (hundreds of megabytes) pauses the window for a moment
  when it is added or when a song using it opens.
- Wine and yabridge are not bundled with the Linux version (the owner's
  decision, November 2026): the Plugins window says how to install them.

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

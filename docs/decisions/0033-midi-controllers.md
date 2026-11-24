# 0033. MIDI controllers: wheels, pedals and pads that play, takes that keep them, knobs and buttons given a job

- **Status:** accepted, November 2026 (extends decisions 0012, 0016 and
  0020)

## The task

The owner: "add MIDI controller compatibility."

A MIDI controller is the box on the desk: keys, often pads, a pitch wheel
and a modulation wheel (or a joystick), a socket for a sustain or
expression pedal, knobs and faders, and on many of them transport buttons.
Since phase 1, VLabs connects every controller on its own, plugged in
before or while it runs, and plays the track in focus from its keys; the
sustain pedal holds notes. Everything else the controller sends was thrown
away: the wheels did nothing (a known gap since phase 1), knobs and
faders did nothing, the transport buttons did nothing, and a part played
with bends came back without them.

"Compatible" therefore means: whatever a controller sends does what a
musician expects, with no setup where an expectation exists, and with one
gesture where it does not.

## Concepts considered

**A. The controller plays what you work on; takes keep what you played;
anything else is given a job by moving it (chosen).**

- *Playing, no setup.* Keys, pads, both wheels, pressure (aftertouch) and
  the pedals act on the instrument of the track in focus, as the keys
  already did. The included instruments answer the way hardware does: the
  pitch wheel bends two semitones each way (General MIDI's default; an SFZ
  instrument's own `bend_up` and `bend_down`); the modulation wheel, and
  pressure, add vibrato; the expression pedal shapes loudness. Drum kits
  are not bent. Plugins receive everything the controller sends, so a
  plugin's own MIDI learn (Surge XT, Dexed, Vital…) works on its knobs.
- *Transport, no setup.* The standard transport messages (MIDI Machine
  Control, and Start, Continue and Stop) play, stop, record and go back to
  the start.
- *Takes keep the performance.* While notes are recorded, what the wheels,
  pressure, the expression pedal and the instrument's other controllers do
  is recorded beside them, in the same undo step, as curves in the
  pattern. The piano roll's lane (Velocity, Tempo) gains **Bend** and
  **Modulation**, and a choice for each other curve the pattern holds;
  points are added, dragged and removed as in automation lanes.
- *A job for a knob, fader or button, by moving it.* Right-click a slider
  (a mixer fader or pan, an effect's or the synth's settings), choose
  **Assign a MIDI control…**, move a knob: the slider now follows that
  knob. Buttons and pads are given commands (play, record, loop, undo…) in
  the Keyboard shortcuts window, beside each command's keys. A control
  with a job in VLabs no longer reaches the instrument.
- *One window to see it all.* **MIDI controllers…** in the menu lists the
  connected controllers, each with a switch, and what the song's knobs
  are assigned to.

**B. Profiles for popular controllers**, shipped with the program, that
give knobs, faders and buttons their jobs as soon as a known model is
plugged in. *Rejected for now:* a database of devices nobody here can
test, kept up to date by hand, and still needing a way to assign controls
on every other device. Profiles can come later as ready-made assignments
on top of A.

**C. Mackie Control / HUI**, the protocols of "DAW mode" control
surfaces, with moving faders and displays answering back. *Rejected for
now:* a large two-way protocol that only some surfaces speak, and nothing
here to test it with. Their transport buttons often also send MIDI
Machine Control, which A handles.

**D. Knobs that follow the focus** (eight knobs always moving the eight
main settings of whatever is in focus). *Rejected for now:* it needs the
knobs learnt once and a choice of "main settings" for every instrument
and effect; explicit assignment explains itself ("this knob moves that
slider") and works for faders and buttons too.

## Decisions in detail

**What the included instruments answer.**

| Controller | Synth | Piano, SoundFonts | SFZ instruments | Kits | Plugins |
| --- | --- | --- | --- | --- | --- |
| Pitch wheel | ±2 semitones | ±2 semitones | the file's bend range (±2 by default) | — | sent |
| Modulation wheel (1) | vibrato, up to 50 cents | vibrato (the SoundFont's own) | — (no LFOs, decision 0024) | — | sent |
| Pressure (aftertouch) | vibrato, with the wheel | — | — | — | sent |
| Expression (11) | loudness | loudness | loudness | — | sent |
| Sustain (64) | holds notes (since phase 1) | ← | ← | ← | ← |
| Other controllers | — | — | — | — | sent |

Bank select and program changes are not passed on: a stray button must
not swap the instrument's sound.

**The focus.** Controllers act on the track in focus; when the focus
moves, the track left behind goes back to rest (wheel centred, no
vibrato, full expression), and a key held while the focus moved still
ends on the instrument it started on (before, it could sound for ever).

**Curves in patterns** (`Pattern::controls`). A pattern keeps one curve
per controller it moves (`ControlLane`: the pitch wheel, a numbered
controller, or pressure), points in the pattern's ticks with a value
(−1 to 1 for the wheel, 0 to 1 for the others), a straight line between
them. They repeat and swing with the pattern's notes. They play inside
the pattern's clips; when a clip that moved the wheel, modulation,
pressure or expression ends, and when playback stops, those go back to
rest; other controllers keep their last value, as MIDI does. Recorded
curves are thinned (points a straight line predicts are dropped) so a
gesture keeps a handful of points. While a take records a curve, the
pattern's old curve for it is not played (the hand overrides it), and the
take replaces it where the take ran.

**Assignments.** A control is a channel and either a controller number
(a knob, a fader, most buttons) or a key (pads, some buttons). Knobs and
faders send absolute values (0–127): the setting jumps to the knob's
value. Endless knobs must be set to absolute mode on the controller.
Assignments of the song's settings (a track's volume, pan, an effect's
setting, the synth's settings, the master volume) are kept in the song
(`Project::midi_links`): they name its tracks and effects, and assigning
or forgetting is an undoable edit. Each burst of knob moves is one undo
step. Buttons for commands are kept in the program's settings, like
keyboard shortcuts: they are the same for every song. One control has one
job; assigning it again moves it.

**Devices.** A controller switched off in the MIDI window is not
connected at all (another program may use it); the switch is remembered
by the controller's name.

**Files.** Format version 12 adds `Pattern::controls` and
`Project::midi_links`. MIDI files carry the curves both ways (pitch bend,
controllers, channel pressure); a file's sustain pedal now holds its notes
when it comes in, as a live take does. DAWproject leaves the curves out
for now.

**Where the decisions are made.** The MIDI thread asks a small table,
shared with the window, where each message goes
(`daw_ui::midi::RouteTable`): to the instrument (straight into the
engine's queue, as before), or to the window (assigned controls, learning,
transport). The table is plain data, tested without a device.

## Consequences

- A part played with bends, vibrato and swells now sounds as played, and
  can be redrawn afterwards.
- A controller's faders, knobs and buttons can run the mixer, the effects,
  the synth and the transport; plugins keep their own MIDI learn.
- Relative (endless) knobs, Mackie Control surfaces, controller profiles,
  sending MIDI back to controllers (lights, motor faders), several
  controllers playing several tracks at once, and recording a knob's moves
  of a VLabs setting as automation are left for later.
- Copying notes does not copy the curves under them yet; DAWproject does
  not carry them.
- Songs with curves or assignments are format 12: older copies of VLabs
  refuse them.

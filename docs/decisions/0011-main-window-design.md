# 0011. The main window: one focus surface, with context and drawers

- **Status:** accepted, November 2026 (phase 1; refined as later phases add
  the timeline, mixer and plugins)

## The tasks the window serves

Designed from what a musician is doing, not from the panels other programs
have: **sketch** an idea, **hear** it, **shape** it, **arrange** it,
**finish** it. In the first minute a newcomer must place a note and hear it
with no setup and no manual.

## Concepts considered

**A. The studio desk.** Browser on the left, arrangement in the middle, an
editor docked below, a mixer docked somewhere else, everything visible at
once. Familiar from large DAWs. *Rejected:* dense by construction. The first
action (place a note) competes with dozens of controls (Hick's law), small
panels mean small targets (Fitts's law), and it contradicts "few things on
screen at once".

**B. Workspaces.** A switch between full-screen task screens (Sketch,
Arrange, Mix), with a shared transport bar. Each screen is calm and focused.
*Rejected as the main structure:* switching screens loses context (you
cannot see where a pattern sits in the song while editing it), and full
screen modes invite "where am I?" mistakes.

**C. Focus and context, with drawers (chosen).** The window is built around
**one large focus surface**: the thing being worked on right now. In phase
1 that is the pattern, in the piano roll. Around it:

- a **transport bar** at the top, always the same, with large targets for the
  most frequent actions (play, stop, record) and the tempo and metre;
- a **location line** (Song › Piano › Pattern 1) that names what is in focus
  and lets you step out to the song (from phase 2, where the timeline
  becomes a focus of its own and a slim song strip keeps context visible);
- **drawers** that slide in on request, never covering the focus by
  default: instrument and sound settings, the track's channel strip, later the
  sound browser and plugin list. Advanced controls are therefore one click
  away, behind a visible handle, never hidden and never in the way;
- a **play surface** at the bottom: an on-screen keyboard labelled with the
  computer keys that play each note, which can be collapsed.

It keeps B's calm (one main thing on screen) without B's loss of context,
and keeps A's power one step away.

## What a newcomer sees first

An empty piano roll with a short hint in the grid: *click to place a note,
press Space to hear it*. A click places a note **and plays it at once**.
Space plays the pattern in a loop. Typing on the computer keyboard plays
the piano, and the on-screen keyboard lights up to show which keys do
what. The first sound takes seconds.

## Conventions kept on purpose

Where the familiar answer is the best one, it is kept: keys down the left
and time across the piano roll; the transport at the top; Space for play
and stop; Ctrl+Z, Ctrl+S, Ctrl+C/V; right-click to delete a note (fast,
and forgiving because undo always works).

## Keyboard scheme

Our own, with one rule that makes it easy to guess: **plain letter and
digit keys play notes; everything else is a command.** Commands use Ctrl
(save, undo, record, loop, metronome, duplicate), or keys that never play
notes (Space, Delete, the arrows, Page Up/Down for the keyboard's octave,
Escape). So a shortcut can never collide with playing, and playing never
triggers a command. The note keys follow the physical positions of a
QWERTY keyboard (two piano-like rows), so the layout works the same on
AZERTY, QWERTZ and other layouts. Every shortcut appears in its tooltip
and menu entry; the bindings are a table the user can change.

## Visual identity

Our own: calm, low-saturation surfaces, one accent colour for notes, the
playhead and focus, red reserved for recording. Generous spacing on an
8-point grid, rounded corners, clear hierarchy by size and weight rather
than borders. Dark and light themes from the same colour tokens; the
system setting is followed by default and can be overridden. Icons are
drawn by our own code from simple shapes. Text is set in the Go fonts
(decision 0008).

# 0014. Phase 2 screens: song, step sequencer, mixer

- **Status:** accepted, November 2026 (extends decision 0011)

## The tasks

Phase 2 adds four things a musician does: **make a beat** (drums on a step
grid), **build the song** from patterns, **balance and shape** the sound
(levels, effects), and **share it** (a WAV file). Decision 0011 set the
frame: one large focus surface, a transport bar, a location line, drawers,
and a play surface at the bottom.

## Concepts considered

**A. Zoom levels on one focus surface (chosen).** The focus surface shows
either the **song** (tracks and their clips on a timeline) or **one
pattern** (a step grid for drums, the piano roll for the piano). The
location line is the way between them: *Song › Drums › Beat 1*; click
"Song" to step out, double-click a clip to step in. The bottom of the window
holds either the **keyboard** (to play) or the **mixer** (to balance), one
at a time, chosen with two buttons. Effects open in a small window from the
mixer.

**B. Split view.** The timeline always on top, the editor of the selected
clip always below. Everything visible at once. *Rejected* for the reason
decision 0011 rejected the "studio desk": two half-size surfaces, each
cramped, and twice the controls in view. The location line and the
pattern previews in the song view keep the context that B was meant to
give.

**C. Expanding lanes.** One vertical list of tracks; opening a track grows
its row into its editor, inside the timeline. Context and editing together,
no switching. *Rejected:* rows of very different heights make the timeline
jump around, the editor only gets the room left between other rows, and
two open tracks fight over the screen.

## What the choice means

- **Step sequencer.** Rows of pads (named, with the computer key that plays
  each one), columns of steps; click a square to add a hit and hear it,
  click again to remove it, drag to paint. Loudness per hit in a strip
  underneath. Pad sound settings (volume, position, tune, length) are one
  click away on the pad's name. Drum tracks play the kit from the computer
  keyboard on the General MIDI drum keys (Z is the kick).
- **Song view.** Track names, mute and solo on the left; clips with a
  preview of their notes on the right. Click an empty spot on a track to
  place its pattern, drag to move, drag the end to repeat it, double-click
  to open it. Tracks are added with a Drums or Piano button under the list.
- **What Space plays** follows the screen, so it is never surprising: in
  the song view, the song; in a pattern, *the part of the song where that
  pattern is placed*, looping, so the other tracks are heard with it (a
  pattern not placed anywhere plays alone). Recording records into the
  pattern being edited.
- **Mixer.** A strip per track and one for the master: meter, volume,
  position, mute, solo, and the effect chain with an Add button. Effects
  open in a small window with a few named controls each (the EQ draws its
  curve). The master bus starts with a limiter, so a loud mix never
  distorts.
- **Export** lives in the menu: choose CD or studio quality, choose where;
  the song renders in the background with progress and a Cancel button.

## Consequences

- A new song opens on its drum pattern: four clicks and Space give a beat
  in seconds; the piano is one click away in the location line.
- The pattern being edited and the song are never shown at once; the
  location line, the clip previews and the looped playback carry the
  context instead.

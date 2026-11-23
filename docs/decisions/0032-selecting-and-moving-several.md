# 0032. Selecting several tracks or clips, and moving them; the export plays once

- **Status:** accepted, November 2026 (extends decisions 0014 and 0017)

## The requests

The owner, after trying 1.1: "be able to select several tracks at once
(to move them), and to change where each instrument is", and "when
exporting, the sound loops back to the start and it is heard: make the
export start at the start and end as soon as there is no more beat on
the timeline."

"Tracks" can mean the rows (instruments) or what is on them (clips); both
are built, since both answer "move several things at once" and both were
missing: tracks moved one row at a time through a menu, and the song view
selected one clip at a time.

## Concepts considered

**A. Selection by click, as in file managers, and moving by dragging
(chosen).** In the track headers: a click selects a track, Ctrl+click
adds or removes one, Shift+click selects a range; dragging a header up or
down moves every selected track there, in their order, as one undo step
(the rows reorder under the pointer as it moves, as clips do on the
timeline). On the timeline: Ctrl+click puts a clip in or out of the
selection, Shift+click adds it, Ctrl or Shift and a drag from an empty
spot draws a selection box, Ctrl+A selects every clip; dragging one of
several selected clips by its middle moves them all by the same amount
(snapped as one clip is, none before the song's start, none onto a clip
that stays); Delete, Ctrl+D and the bar nudges act on all of them.
Everyone knows these gestures from their file manager; nothing new to
learn, and a plain click or drag does exactly what it did before.

**B. A selection tool (a mode).** *Rejected* as decision 0017 rejected
tool palettes: a mode left on causes mistakes, and the box needs only a
modifier.

**C. Move up / Move down for several tracks, in the menu.** *Kept* for
one track and keyboards, but too slow for reordering a song; dragging is
the direct way.

A plain drag from an empty spot keeps placing a pattern (decision 0014):
the selection box needs Ctrl or Shift, so the quickest way to make music
is unchanged. Several selected clips move in time only: a pattern clip
belongs to its track's row, and moving a mixed group across rows would
have no clear meaning.

## The export

The export played the song as the Play button does, the loop region with
it: at the loop's end it went back, the music never fell quiet, and the
export wrote up to fifteen more seconds of looped music (a four-bar song
looping two bars measured 23 seconds). It also wrote a second of silence
at the end, which was how it knew the tail was over.

- The export plays the song **once from its start**: the loop region is
  for playing, not exporting (`OfflineRender::new` is unlooped;
  `OfflineRender::with_scope` keeps the loop for tests of looping).
- The file **ends where the sound does**: up to the end of the last clip,
  then on only while notes and effects still ring (above -80 dBFS); the
  silence that tells the tail is over is not written. A song whose last
  clip ends in silence still lasts to that clip's end, so a loop exported
  bar for bar keeps its length.

## Consequences

- Reordering a song's instruments is one drag; so is moving a whole
  section of clips later or earlier.
- The selection lives in the session, not the song file; undo keeps it to
  clips and tracks that still exist.
- Exports no longer depend on whether a loop is set.

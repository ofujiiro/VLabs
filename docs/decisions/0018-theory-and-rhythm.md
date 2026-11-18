# 0018. Theory and rhythm: the song's key, chord stamps, the tempo and signature row, swing

- **Status:** accepted, November 2026 (extends decisions 0007, 0011, 0014
  and 0016)

## The tasks

Phase 5 serves a person who knows music theory and wants the editor to
know it too. They **set the frame** of a song (its key, its tempo and time
signature, and where those change), **write in the key** (see which notes
belong, keep new notes in it, place whole chords, invert them, move a line
by an interval), and **give it a feel** (swing, tuplets, odd meters,
patterns of different lengths turning against each other).

## Where the key lives

**A. The song has a key, with changes at bars (chosen).** Like the tempo
and the time signature, the key belongs to the song: one button in the
transport bar ("C major", or "No key" at first) sets the key in effect at
the play position, and key changes sit at bars in the song view's
signature row (below). The piano roll shows the key in effect where the
pattern is placed. One place to look, the same for tempo, signature and
key; a modulation is a change at a bar, as on paper.

**B. Each pattern has its own scale** (with the song's as the default for
new patterns). Modulations come for free, but there are two sources of
truth for "the key", and the transport could not say what key the song is
in. *Rejected.*

**C. A scale setting of the piano roll only** (a view option, not saved
with the song). Simple, but it is lost between sessions and means nothing
to the rest of the song (chord stamps, a future chord track, export to
other programs). *Rejected.*

## Writing in the key

- **Highlighting:** with a key, the piano roll shades the rows outside the
  key (instead of the black keys) and marks the tonic's rows; the keys on
  the left get a dot for notes in the key.
- **Keep notes in the key** (a switch in the Snap menu, off by default):
  new notes land on the nearest note of the key, notes dragged up or down
  move by steps of the scale (so a chord stays a chord of the key), and
  the up and down arrows move by a step of the scale. Shift with the arrows
  still moves by octaves.
- **Chord stamps:** the "Draw" menu next to Snap chooses what a click
  places: a note (the default), a chord built in the key on the clicked
  note (triads or seventh chords, so the quality follows the scale degree:
  in C major, D gives D minor, G gives G major), or a chord of a chosen kind
  (major, minor, diminished, augmented, suspended, sixths, sevenths, ninths,
  power chords), in root position or an inversion. While a chord is
  chosen, the pointer shows its notes before the click. One click is one
  undo step; dragging stretches every note of it.
- **Inversions:** "Invert up" (Alt+Up) moves the lowest note of each chord
  of the selection (the notes that start together) up an octave; "Invert
  down" (Alt+Down) moves the highest one down.
- **Transposition by interval:** the "Notes" menu moves the selection up or
  down by any interval up to an octave, named as musicians name them ("up a
  perfect fifth"), or, with a key, by steps of the scale ("up a third in
  the key").

Concepts rejected for the chord tools: a **chord palette panel** (a grid of
chord buttons beside the piano roll), because it takes room from the focus
surface (decision 0011) and puts dozens of choices on screen at once
(Hick's law); and **typing chord symbols** ("Am7") as the only way in, too
slow to explore and unhelpful to someone who thinks in shapes.

## Tempo and time-signature changes: the signature row

**A. A row pinned under the song view's ruler (chosen).** Two thin strips
above the tracks, folded into one summary line until opened ("Tempo and
signatures: 120 BPM · 4/4 · C major"), so a newcomer sees one calm line:

- *signatures:* time-signature and key changes as small tags at their bars.
  Click an empty bar to add a change there, click a tag to change or remove
  it (a small panel: time signature, any number of beats over 1/2, 1/4,
  1/8, 1/16 or 1/32, and key), drag a tag to another bar;
- *tempo:* the tempo drawn as a line over the song, like an automation
  lane: click to add a tempo change (on a beat), drag a point up or down
  to change its tempo and sideways to move it, click a point for its exact
  value and "Glide to the next tempo" (a gradual speeding up or slowing
  down), right-click to remove it.

**B. Markers on the ruler.** Compact, but the ruler already moves the play
position and sets the loop, the targets would be tiny (Fitts's law), and
the shape of the tempo over the song would be invisible. *Rejected.*

**C. A list of changes in a dialog** (bar, tempo, signature, key).
Precise, but indirect: no picture of the song, and every change is a trip
to a window. *Rejected* as the main way; its precision lives on in the
small panel of one change.

The transport's tempo, time signature and key show what is in effect at
the play position and change that part of the song; with no changes,
that is the whole song, as before.

**Changing a time signature keeps the song in its bars.** Clips (their
starts and ends), automation points, tempo changes and the loop keep their
bar and beat; patterns that fill whole bars keep their number of bars (and
grow to keep their notes); sound clips keep their length in seconds.
Inserting a 7/8 bar at bar 9 therefore pushes everything after it along,
as a musician expects, instead of leaving every later clip half a beat off
its bar. (Other programs keep positions in quarter notes instead; with a
pattern-based arrangement built from bars, that leaves the arrangement in
pieces.)

Tempo glides are exact: within a glide the tempo moves in a straight line
from one change to the next, and seconds are worked out with the
logarithm that this gives, both ways, in the model and on the audio
thread. Standard MIDI files have no glides; exporting them (phase 6) will
write small steps.

## Tuplets and odd meters

- The Snap menu offers triplets, quintuplets and septuplets of the usual
  values next to the straight ones; the note-length menu adds dotted values
  and the same tuplets; the step grid offers quintuplet and septuplet
  steps. Every value is exact in ticks (decision 0007).
- Every grid counts from the start of the bar the note is in, so after a
  bar of 7/8 the beats still fall where the bar lines are.
- Any time signature can be set: 1 to 64 beats over 1/2 to 1/32 notes,
  with the common ones offered first.

## Swing

**A. Swing per pattern, applied as it plays (chosen).** A pattern has a
swing amount (0 % straight, 67 % triplet feel, 100 % dotted) on eighth or
sixteenth notes. Notes stay where they are written; playback delays every
second subdivision, moving everything between the grid lines along with it
(a smooth warp, so notes off the grid keep their place in the feel). The
step grid draws the swung steps where they sound. Recording into a swung
pattern stores what was played as written positions, so it plays back as
heard.

**B. One groove for the whole song.** Simpler, but a straight bass line
and a swung hi-hat are an everyday combination. *Rejected for now* (a song
groove could be added on top later).

**C. Swing as an editing command** that moves the notes. What you see is
what you hear, but the straight positions are lost and changing the feel
means doing it again. *Rejected.*

## Patterns of different lengths

A pattern's length is no longer whole bars only: drag the end of the
pattern (the handle on the ruler of the piano roll, the end of the step
grid) to any grid step. Its clips keep repeating it for their own length,
so a pattern of three beats against one of four turns against it, and the
song view's clip pictures show each repeat. The bar buttons still add and
remove whole bars.

## Consequences

- File format version 5: the song's key changes, each pattern's swing,
  and tempo glides. Every new field has a default, so older files need no
  rewriting.
- Chord stamps, highlighting and keeping notes in the key share one small
  set of theory functions in the model (`daw_model::theory`): scales,
  chords, intervals, all on MIDI keys and pitch classes, tested on their
  own.
- The engine needs only two changes: the tempo table follows glides, and
  the compiler swings each pattern's notes before placing them in the song.
- DAWproject (phase 6) has tempo automation with straight-line steps,
  which matches glides; swing is written into the notes on export; the
  song's key travels as a key-signature hint where the target format has
  one.
- Not in this phase: a chord track, beat groupings for odd meters (7/8 as
  2+2+3, for the metronome's accents), a "make a tuplet" command for
  existing notes, swing for the whole song, and the delay effect following
  tempo changes (it follows the song's first tempo).

# 0034. A MIDI file dropped on a track plays on that track

- **Status:** accepted, November 2026 (extends decisions 0020 and 0023)

## The request

The owner, after trying 1.3 (in French): "be able to put a MIDI file into
the synth (it automatically creates a piano)". Importing a MIDI file made
new tracks of notes, each with the General MIDI instrument the file names
(often a piano), so a melody meant for an instrument already in the song
had to be copied out of the new track by hand.

## Concepts considered

**A. The drop's target decides (chosen).** A MIDI file let go on a track
of notes (an instrument, the synth, a plugin, a SoundFont, drums, a
voice), on its row in the song view, from the desktop or from the
library, goes into that track: its notes become one new pattern there,
named after the file, placed from the bar where it was let go. A MIDI
file dropped from the desktop on a pattern open in the piano roll or the
step grid goes into that pattern's track at the play position, and the
new pattern opens. Anywhere else (below the tracks, on an audio track,
through Import…) the file still makes new tracks, as before: a whole song
from the internet keeps its parts apart.

**B. A question after each MIDI import ("new tracks or into …?").**
Rejected: a dialog for every drop, and it cannot know which track was
meant without asking a second question.

**C. A menu item on the track ("Import MIDI file into this track…").**
Not rejected, but slower than a drop and invisible until looked for. The
drop is what the owner tried; a menu item can come later.

## Decision

- `daw_formats::midi::ImportedSong::add_into(project, track, at, name)`
  puts a read file's notes into one track. A drum track takes the file's
  drum tracks (channel 10), any other track the file's other tracks; if
  the file has none of the kind wanted, it takes them all. The notes of
  every track taken are merged into one pattern (in order of time, then
  pitch), with their lyrics. An instrument that takes curves (decision
  0033) also gets the file's pitch bends and controllers (the first track's
  of each kind). Audio tracks are refused.
- The pattern covers whole bars from the bar the drop lands in, and
  replaces what that track already played in that span (as a take does),
  so nothing overlaps.
- In the interface: `Session::import_midi_into` (one undo step, "Import
  MIDI file"; the status line says "Imported tune.mid into Lead"),
  `Session::import_files` sends MIDI files there when the drop's track
  holds notes, and the pattern view sends MIDI files dropped on it there.

## Consequences

- No new track appears when the target was clear; Undo removes the
  pattern and gives back what it replaced.
- A file of several instruments dropped on one track plays them all on
  that instrument; drop it below the tracks to keep them apart.
- Library drags onto the open pattern are not taken yet (the step grid
  only takes sounds for a custom kit); dragging onto the track's row in
  the song does it.

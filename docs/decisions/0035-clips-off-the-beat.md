# 0035. Clips off the beat: a switch for snapping in the song

- **Status:** accepted, November 2026 (extends decisions 0014 and 0017)

## The request

The owner: "make it so one can untick an option so that a pattern or a
sound can be put off the beat."

Until now, clips in the song always landed on the grid: a pattern clicked
in, moved or repeated landed on bars (on beats with Alt), a sound's ends
and splits on beats (exactly with Alt only), and a sound dropped from the
desktop or the library on a beat. Placing something just before a bar (a
cymbal swell, a pick-up phrase, a sample that starts a little early) meant
holding Alt, which nothing on screen told anyone about, and which some
Linux desktops take for moving windows.

## Concepts considered

**A. A checkbox "Snap to bars and beats" in the song's location line
(chosen).** It sits where the open pattern shows its own grid, on the
right of the line, ticked by default. Unticked, everything placed or
dragged in the song (a pattern clicked in or placed from the empty spot's
menu, clips moved alone or together, ends repeated or trimmed, the split
point, sounds dropped from the desktop or the library, the play position
clicked on an audio track) goes exactly where the pointer is. Alt keeps
meaning "the other way": with snapping off, holding Alt brings the beats
back. The choice is remembered between sessions with the other
preferences (`Settings::snap_clips`). It is visible, says what it does,
and leaves the quick way of building a song unchanged for everyone who
leaves it ticked.

**B. A grid menu for the song, as the pattern has (bars, beats, 1/8,
off…).** *Rejected for now:* the request is to put things off the grid,
not on a finer one; a menu of seven choices is more to read for the one
that was asked. It can come later in the same place if finer grids are
wanted.

**C. Only documenting Alt.** *Rejected:* a hidden key is what made the
request necessary, and Alt drags windows on some desktops.

## Details

- `SongView::land` is the one rule: snapping on keeps decision 0017's
  behaviour exactly; snapping off places freely, Alt snaps to beats.
- Dragged freely, a sound can be as short as one tick, a pattern as short
  as a 1/64 note (a beat while edges snap).
- The tempo and signature points, automation lanes, the ruler and the loop
  region keep their own snapping: they are not patterns or sounds.
- Nudging clips with the keyboard still moves them by whole bars, from
  wherever they are.

## Consequences

- Clips can start anywhere; the engine and the file format already took
  any tick, so nothing else changed.
- Settings files written before keep snapping (the field defaults to on).

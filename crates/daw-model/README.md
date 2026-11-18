# daw-model — the project as data

A project (a song) described as plain data: tracks, patterns, notes, audio
clips, the arrangement, mixer settings, effect chains, references to plugins,
and the tempo and time-signature map. Every change to that data is an edit
that can be undone and redone.

This is the **single source of truth**. The interface shows it and edits it;
the engine plays what it describes. Nothing else holds a competing copy of
the song.

## What belongs here

- The data structures of a project.
- Musical time, represented exactly.
- Edits, and the undo and redo history.

## Rules

- **Musical time is exact.** Positions and lengths are rational values or
  high-resolution integer ticks, never floating-point beats, so tuplets and
  polyrhythms never drift.
- **Everything the user does is undoable.** Data changes only through edits
  that the history records.
- **Pure data.** No files, no audio, no threads, nothing drawn on screen.
  This crate depends on no other crate of the application.
- Keep the shape of the data easy to map onto DAWproject, the interchange
  format.
- Leave room in the note model for per-note pitch offsets (for microtonal
  tunings later), and do nothing that would rule out a staff-notation view.

## Roadmap

Phase 1 (project, notes, undo), phase 2 (patterns, arrangement, mixer),
phase 4 (audio clips), phase 5 (tempo changes and glides, time-signature
changes that keep the song in its bars, the song's key, swing, and the
`theory` module: scales, chords, inversions, steps of the key).

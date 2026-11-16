# 0007. Musical time is counted in integer ticks

- **Status:** accepted, November 2026

## Context

The brief: represent musical time exactly (rational values or
high-resolution integer ticks, never floating-point beats) so tuplets and
polyrhythms stay precise.

## Decision

Every musical position and length is a whole number of **ticks**, stored as
a 64-bit integer. There are **2,882,880 ticks per quarter note**.

That number is 2⁶ × 3² × 5 × 7 × 11 × 13. Because it divides evenly by all
those factors, every common subdivision is a whole number of ticks:

- straight notes down to 1/256 notes;
- triplets, quintuplets, sextuplets, septuplets, nonuplets, and 11- and
  13-note groups, at any of those note values;
- one tuplet nested inside another when their factors are different (a
  triplet inside a quintuplet, for instance);
- patterns of 3, 5, 7, 9, 11, 13, 16... steps looping against each other.

A 64-bit tick count can address about 3 × 10¹² quarter notes, far beyond any
song. Arithmetic is exact, comparisons are trivial, and sorting notes is
cheap.

Tempo is a separate concern: a tempo map converts ticks to seconds and
samples, piece by piece, and rounds to the nearest audio sample only at the
moment of playback. Tempo values themselves are stored as decimal numbers
(beats per minute), as DAWproject does.

## Alternatives considered

- **Rational numbers** (numerator/denominator): exact for every tuplet,
  including unusual nestings, but every comparison needs a multiplication,
  denominators can grow, and the file format is less obvious. The tick
  resolution above covers every subdivision in practical use.
- **960 ticks per quarter** (common in MIDI software): septuplets and
  11-tuplets do not divide evenly and drift.
- **Floating-point beats:** forbidden by the brief, and rightly: rounding
  errors accumulate over a song.

## Consequences

- Rarely, a tuplet with a factor outside the list (a 17-tuplet, say) is
  placed on the nearest tick: an error of a third of a microsecond at 120 BPM,
  inaudible and non-accumulating, because positions are absolute.
- MIDI files use coarser resolutions; importing scales exactly (every common
  MIDI resolution divides 2,882,880), exporting rounds to the file's
  resolution.
- DAWproject stores beats as decimal numbers; conversion is exact for every
  position a person can draw.

# 0026. Expressive tempo: shaped glides, and the tempo under the piano roll

- **Status:** accepted, November 2026 (the owner's request; extends
  decision 0018)

## The task

"Make the tempo able to change gradually, for piano compositions for
example: an advanced feature." A pianist shapes time: a ritardando at the
end of a phrase that gathers as it goes, an accelerando into a climax, a
little rubato over a few bars, then *a tempo*.

Decision 0018 already gave the song glides: a tempo change can glide to
the next one, in a straight line over the score. Two things still stand in
a pianist's way:

1. **A straight line is one shape among several a musician hears.**
   Measured final ritardandi slow down little at first and most at the end:
   Friberg and Sundberg (1999, "Does music performance allude to
   locomotion?") found that the tempo of a final ritardando follows
   `v(x) = [1 + (w^q − 1)·x]^(1/q)`, with `q` about 2 to 3, the way a runner
   comes to a stop. An accelerando can rush early and settle, or creep up
   late.
2. **The piano part is written in the piano roll, but the tempo lives only
   in the song view**, folded under the ruler, away from the notes whose
   phrases it shapes.

## Concepts considered

**A. Shaped glides on the one tempo line, and the same line under the
piano roll (chosen).**

- A glide has a **shape**: **Even** (the straight line, as before),
  **Late** (it holds back, then changes more and more: a ritardando that
  gathers) or **Early** (most of the change comes first, then the tempo
  settles into the new one). A small handle sits on the middle of every
  glide: click it for the three shapes, or drag it up or down for any
  shape in between.
- The panel of a tempo point can make the tempo **arrive there
  gradually** (the previous change glides into it) as well as glide on to
  the next one. A ritardando to a slower tempo is then: click the tempo line
  where it should end, drag the new point down, tick "Arrive gradually".
- Glides are labelled on the line with the words a score uses: *rit.* when
  the tempo slows, *accel.* when it speeds up.
- **The lane under the piano roll shows Velocity or Tempo.** The tempo
  lane is the song's tempo line where the pattern is placed, edited exactly
  as in the song view, so a ritardando is drawn under the phrase it ends.

**B. Tempo marks as objects of their own**, as notation programs have
them: *rit.* and *accel.* spans with a final tempo as a percentage,
*a tempo*, fermatas. Familiar words, but a second description of the tempo
beside the tempo line (two sources of truth for one thing), and the shape
of a change still needs a picture. *Rejected*; its words live on as the
labels.

**C. Drawing the tempo freehand**, a pencil that leaves a point every few
pixels. One gesture, but it leaves dozens of points that are tedious to
adjust afterwards, and two points and a shape cover what a phrase needs.
*Rejected for now.*

**D. The tempo from a free performance**: play without the metronome, tap
the beats (or let the program find them), and the bars follow the playing.
The strongest tool for pianists who play rather than write, but a
different feature, with its own design: *later* (ROADMAP.md).

## The shape, exactly

Within a glide from tempo `T0` to `T1`, with `x` the fraction of the glide
passed (0 to 1, in ticks) and `w = T1 / T0`, the tempo is

    T(x) = T0 · [1 + (w^q − 1) · x]^(1/q)

the family Friberg and Sundberg measured. `q = 1` is decision 0018's
straight line. Half-way through a glide the tempo is the *power mean* of
`T0` and `T1` with exponent `q` (arithmetic for 1, geometric for 0,
harmonic for −1), so a larger `q` keeps the tempo near the faster of the
two for longer. The tempo always moves one way, from `T0` to `T1`, never
past them.

Seconds have closed forms both ways for every `q` (with their limits at
`q = 0` and `q = 1`), so a shaped glide stays exact, in the model and on
the audio thread, as decision 0018 requires: nothing is approximated by
small steps.

The song stores the glide's **bend** `b`, from −1 to 1 (0 is straight; a
positive bend means the change comes late), and

    q = 1 + 8·b   when the tempo slows down,
    q = 1 − 8·b   when it speeds up,

so a positive bend means "holds back, then changes" whichever way the tempo
goes. **Late** and **Early** are bends of +0.5 and −0.5: on a ritardando
from 100 to 60 BPM, the middle of the glide is then at 88, against 80 for
Even and 71 for Early. Friberg and Sundberg's `q` of 2 to 3 is a bend of
about 0.15 to 0.25, between Even and Late.

## Consequences

- File format version 8: a glide's bend (0 by default, so older files need
  no rewriting).
- One piece of arithmetic, `daw_model::TempoPiece`, serves the model and
  the engine's tempo table, with tests that ticks to seconds and back come
  out exact for many shapes and tempos, and that the closed forms agree
  with the tempo they integrate.
- MIDI files have no glides: export already writes small steps (a
  sixteenth note each), which follow any shape. DAWproject has straight and
  held tempo points only: a bent glide is written as straight pieces a
  sixteenth note long (at most 64 per glide), close to what VLabs plays; a
  straight glide stays one straight piece.
- The piano roll's tempo lane edits the song. In a pattern placed more than
  once, it shows and changes the tempo where the pattern is first placed,
  as the key does (decision 0018); a pattern not in the song plays at the
  song's tempo from its start, and its lane shows that.
- Not in this round: fermatas (a slower tempo for one beat does the job
  meanwhile), the tempo from a free performance, freehand drawing.

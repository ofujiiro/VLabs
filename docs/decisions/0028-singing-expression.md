# 0028. Singing expression: the sung pitch drawn over the notes, vibrato, loudness, colour and breath

- **Status:** accepted, November 2026 (milestone 7b of decision 0027;
  builds on decisions 0011, 0018 and 0026)

## The task

Decision 0027 planned singing in milestones; 7a sings a voicebank's
syllables on the notes' pitches with a fixed glide of 80 ms between notes.
7b is expression: "pitch curves and vibrato drawn over the notes,
envelopes, the usual flags (breathiness, gender), a better vocoder".

What a singer shapes, and what a person writing a voice part needs to
reach:

1. **How the voice moves between notes:** when it leaves the note before,
   how long it takes, whether it overshoots and settles, a scoop up from
   below at the start of a phrase, a fall at its end.
2. **Vibrato on held notes:** it starts a moment after the note, grows,
   has a depth and a rate.
3. **Loudness**, note by note and across a phrase.
4. **The voice's character:** lighter or deeper (its formants), clear or
   breathy.

## Concepts considered

**A. The sung pitch drawn as a line through the notes, edited in place;
note values in the lane under the notes (chosen).**

- On a voice track the piano roll draws the pitch the voice actually sings
  as a line running through the notes: the glide into each note, its
  vibrato. Pitch is the piano roll's vertical axis, so the line is drawn
  where it means something: a glide is seen leaving one note's row and
  arriving in the next.
- The location line's **Draw** menu (chord stamps, which a single voice
  cannot sing) gives way on voice tracks to a switch, **Notes | Pitch**
  (Ctrl+P). In Notes the piano roll works as everywhere else and the line
  is drawn faintly; in Pitch the line comes forward with its handles and
  the notes stay put, so a stray click never moves the melody.
- In Pitch, a click over a note puts a pitch point there, at the height
  of the pointer, and the drag goes on moving it; a point is dragged in
  time and pitch, clicked for its exact values and the shape of the curve
  after it, right-clicked to remove. A note's vibrato has a handle where it
  starts, as high above the note as the vibrato is deep: dragged sideways
  it starts earlier or later, up or down it gets deeper or shallower;
  clicked, its panel gives depth, rate, start and how long it takes to
  grow. This is the tempo line's way of working (decision 0026: click a
  point for its exact values, drag it, right-click to remove), so a person
  who has shaped a tempo already knows it.
- The Notes menu gains **Add vibrato**, **Remove vibrato** and **Plain
  pitch** for the selected notes: the usual way to give a whole phrase
  vibrato is to select it and add it.
- Loudness, colour and breath are values of each note, and the lane under
  the notes already edits a value of each note: velocity. On voice tracks
  it offers **Velocity**, **Colour**, **Breath** and **Tempo**, all edited
  as velocity is (drag a bar; the selected notes move together).

**B. Expression lanes for everything**, as automation is drawn: pitch in
cents, vibrato depth, colour and breath as curves over time under the
piano roll. One way for every setting, but pitch would be drawn away from
the notes it bends, in a lane 72 points tall, as an abstract number of
cents; and curves over time drift away from their notes when notes move,
the reason decision 0027 kept lyrics on the notes. *Rejected.*

**C. A note inspector:** select notes and a panel lists every setting in
numbers (glide time, overshoot, vibrato, colour, breath). Exact and simple
to build, but shaping a phrase becomes filling in a form, the result is not
seen, and decision 0027 asks for curves drawn over the notes. *Kept only
as the small panels* that give a point's or a vibrato's exact values.

**D. Drawing the pitch freehand** with a pencil. One gesture, but it leaves
dozens of points that are tedious to change afterwards (decision 0026
rejected freehand tempo for the same reason), and a few points per note
draw every shape a phrase needs. *Not now.*

## The pitch, exactly

Each note may have **pitch points**: a time in milliseconds from the
note's start and a pitch in cents from the note's key, with the shape of
the curve that follows the point: **smooth** (an S-curve, the default),
**straight**, **slow start** or **slow end**. Milliseconds rather than
ticks because a glide is a gesture of the voice: it lasts as long at any
tempo, as an audio clip's fades do (decision 0017).

- A note with no points of its own has the plain glide of 7a: points at
  −40 ms and +40 ms, smooth, so songs from 7a sing exactly as before.
- A note *takes over* at its first point (just before it starts, for the
  plain glide) and sings until the next note takes over. Its first point
  leaves from wherever the voice is at that moment (the note before, its
  bends and vibrato included), so a glide can never jump; at the start of
  a phrase, where no note sings before it, the first point is used as it
  is (a scoop up from below).
- Between points the pitch follows the shape; before the first and after
  the last it holds.
- **Vibrato** (per note): a depth in cents either side of the pitch, a
  rate in Hz, a start in seconds after the note's start and a time to grow
  to its depth; it fades out over its last 80 ms before the next note
  takes over (or the note ends), so it hands over smoothly. A note shorter
  than the start has none, so adding vibrato to a whole phrase leaves its
  short notes plain. **Add vibrato** gives ±35 cents at 5.5 Hz, starting
  0.25 s into the note and growing over 0.2 s.

This is OpenUtau's model (points in milliseconds and its four shapes, a
first point that snaps to the note before, vibrato per note), so the UST
and USTX exchange of milestone 7c maps onto it directly.

The pitch of a phrase is worked out in one place, `daw_model::expression`,
from notes in seconds: the engine renders with it and the piano roll draws
it, so the line on screen is what is sung.

## Loudness, colour and breath

- **Velocity is loudness** on voice tracks, as on every other instrument:
  the square curve the sampler uses, set so that the default velocity
  (100) sings at the level 7a sang. A crescendo across a phrase or within a
  long note is the track's volume automation (phase 3). Decision 0027's
  "envelopes" are these two; no new kind of envelope is added.
- **Colour**, from −100 (darker, deeper) to +100 (lighter, brighter), moves
  the voice's formants up to four semitones without changing its pitch:
  the resampler reads each grain of the recording faster or slower before
  laying it down (pitch-synchronous overlap-add sets the pitch by where
  grains are laid, the formants by what is inside them). UTAU's *gender*
  flag does this; ours goes the way the lane does (up is lighter), where a
  positive UTAU gender is deeper.
- **Breath**, from 0 (clear) to 100, mixes in a whisper of the same
  syllable: grains taken at random places of the recording, with random
  signs and spacing, keep its vowel (their spectrum is the recording's)
  but no pitch. Above 1 kHz only, as breath noise is, and level-matched to
  the voice. UTAU's *breathiness* flag in spirit.
- Colour and breath are kept per note (a breathy last note of a phrase),
  edited for many notes at once by selecting them, as velocity is.

## Not in 7b: the vocoder

Decision 0027 named a WORLD-style vocoder for 7b, "for larger shifts and
the flags". Colour and breath work within the resampler we have, and the
renderer has still met only synthetic voices: whether real voicebanks need
a vocoder, and for what, is the question the owner's first real voicebank
answers. The vocoder becomes a milestone of its own, judged after real
voicebanks have been heard (ROADMAP.md).

## Consequences

- File format version 10: a pattern keeps each note's expression beside
  it, by note identity, as it keeps lyrics (a note stays a small value
  cheap to copy). Copying, pasting and duplicating notes, and duplicating
  a pattern, carry lyrics and expression (closing a 7a gap); on Linux the
  clipboard's text carries them beside each note's own fields, so text
  from older copies of the program still pastes.
- MIDI files carry lyrics: a Lyric event just before each sung note
  starts, read back onto the note starting at its moment (another 7a gap).
  Expression has no place in MIDI and stays out of it, as it does of
  DAWproject.
- `daw-dsp::voice::sing_phrase` takes the phrase's pitch as a function of
  time, and each note's gain, colour and breath; it no longer decides the
  glide itself.
- The engine counts the expression in a phrase's key, so an edit renders
  again only the phrases it changed.
- Tested without ears: the sung pitch measured along drawn points, vibrato
  rate and depth measured, formants measured moving with colour while the
  pitch stays, noise between the harmonics growing with breath, levels
  following velocity, no clicks; the curve's arithmetic (snapping, shapes,
  the plain glide equal to 7a's) in unit tests.
- Not in 7b: shapes drawn freehand, per-note envelopes, more UTAU flags,
  the vocoder, phonemizers for other languages.

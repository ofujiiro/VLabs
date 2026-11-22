# 0031. Shaping sounds: note shapes for instruments, a sound editor for clips

- **Status:** accepted, November 2026 (extends decisions 0017, 0024 and
  0025)

## The tasks

The owner, after VLabs 1.0: "there is no way to change a sound in the
program, for instance to cut it off earlier; add ways to change imported
sounds, and the instruments that come with the program, for instance to
make a *release cut* piano." A release cut is the sound of notes stopping
dead as their key comes up, with no ringing tail: a common move in
current beat-making, usually done with an instrument's release.

Two tasks hide in this: **shaping how an instrument's notes start and
end** (the piano, SoundFonts, SFZ instruments, the kits, custom kits made
of the person's sounds), and **editing a sound the person brought in**
(cut it exactly, fade it, turn it around, change its pitch, level it).

## Concepts considered

For the instruments:

**A. A note shape per track, in a small panel (chosen).** Each track whose
instrument plays recorded sounds has a *note shape*: an attack and a
release, each either the instrument's own ("as recorded") or a time. A
button with a little envelope in the track's header and on the pattern's
location line opens a panel: a picture of the shape (rise, level, key up,
fall), the two times with a tick each, and four quick choices: **Cut at
note end** (a 10 ms release), **Soft start**, **Long tail** and
**Natural**. The release cut is one click and says what it is in its
hint. With a release, a kit's hits (one-shots, which normally play to
their end) stop at the end of their note too: the way to cut an imported
sound shorter on a custom kit, note by note.

**B. A full envelope (delay, attack, hold, decay, sustain, release) in a
window.** *Rejected for now:* six controls where two answer what people
ask for; the synth already has full envelopes for people who want them.
The shape is a struct that can grow later without a new concept.

**C. Per-note release, drawn in the piano roll like velocity.** *Rejected
for now:* a release cut is a choice for the whole part, and per-note
control can come later as a lane without changing the track's shape.

For imported sounds:

**D. A sound editor window for one clip (chosen).** Double-clicking an
audio clip (a pattern clip opens its pattern the same way) or **Edit
sound…** in its menu opens a window: the clip's whole sound as the clip
plays it, the part it plays lit, the rest shaded; its ends drag exactly
(no snapping to beats, unlike the timeline), the small squares fade it;
under it the volume, the **pitch** in semitones, **Backwards** and **As
loud as it can be**. Decision 0017 kept such an editor for later, as a
detail view beside cutting on the timeline: that is what it is, and every
change is an ordinary edit of the clip, shown on the timeline at once.

**E. Destructive edits that write new files (Audacity's way).** *Rejected:*
reversing or re-pitching would write a copy of the sound each time,
repeated pitch changes would lose quality, and the song file would grow.
Backwards and pitch are settings of the clip instead, played by the
engine as it reads the sound.

**F. More settings on the timeline's clip menu.** *Rejected:* a cramped
menu, and cutting exactly needs a larger picture of the sound.

## How it works

- **The model** (format version 11): `Track::shape` (`NoteShape { attack,
  release }`, seconds, absent = the instrument's own) and two fields of an
  audio clip, `reverse` and `pitch` (semitones, -24 to 24). A backwards
  clip plays from `offset` towards the file's start; trimming, splitting
  and the furthest a clip can reach follow the direction and the speed.
  Pitch works like tape: up an octave plays twice as fast, so the clip's
  length changes with it (never longer than its sound).
- **The piano and SoundFonts.** The SoundFont player (`rustysynth`) reads
  envelopes from the file and cannot change them while playing, so a
  shaped SoundFont is the file read again with every instrument zone
  saying the new attack and release (`daw_dsp::sf2_shape`: the zones that
  play a sample get the two generators; the presets' offsets to them are
  cleared). The engine keeps the last three shaped copies. Reading the
  piano again takes about 20 ms, so a slider can follow. The player clamps
  releases to at least 10 ms, which is also what keeps a cut click-free.
- **The SoundFont's own reverb is off when a release is set.** Measured on
  the virtual sound card, a cut piano with the SoundFont's gentle room
  still rang at -12 dB just after each chord: the cut did not sound cut.
  With a release set, the track's notes end where the shape says; a
  reverb effect on the track adds a room on purpose.
- **Samplers and kits** (SFZ instruments, custom kits, the bundled kits)
  take the shape while playing, without being rebuilt: the attack and
  release replace each region's own; with a release, one-shots follow
  their key.
- **The sound editor** keeps nothing of its own: it reads the clip and
  edits it through the session (one undo step per gesture). "As loud as
  it can be" sets the clip's volume so its loudest moment is -0.2 dB.

## Consequences

- One click makes a release-cut piano; the same panel softens or lengthens
  any recorded instrument, and cuts kit hits to their notes.
- A shaped SoundFont costs a second copy of its file in memory (19 MB for
  the piano) while it plays.
- DAWproject has no backwards or tape-speed clips: such clips go out as
  the same stretch of their sound, forwards at their own pitch. A
  SoundFont's shape is not exported either (DAWproject has no place for
  it).
- Plugins keep their own envelopes (their windows have them); the synth
  keeps its full envelopes.

# 0025. Custom kits: a drum kit of your own sounds, played from the step grid

- **Status:** accepted, November 2026 (a beta tester's idea, passed on by
  the owner; builds on decisions 0014, 0017 and 0024)

## The task

"Create a blank instrument, mass-import sounds into it (WAV, MP3…), each
sound on its own key, then build beats with those sounds on the step grid."
A person has a folder of one-shots (their own kicks, claps, vocal chops,
foley) and wants to make a beat with them as easily as with the bundled kits.

## Concepts considered

**A. A custom kit: a new kind of drum track whose pads are your sounds
(chosen).** **+ Instrument…** offers **Custom kit**: it asks for sounds
(choose many at once), and each becomes a pad on its own key, named after
its file. The kit's beats use the step grid exactly as the bundled kits do
(one row per pad, click squares, paint, loudness, swing), and each pad has
the drum pads' settings (volume, pan, tune, length, mute). More sounds come
from **Add sounds…** on the location line, by dropping files or dragging
sounds from the library onto the step grid, and **Make a kit of these
sounds** on a library folder makes a kit of every sound in it at once.

**B. Replace the sounds of a bundled kit's pads.** Simple, but limited to
the fifteen named drum sounds, and a "Kick" row playing a dog bark is
confusing.

**C. Write an SFZ file from the chosen sounds and play it as an SFZ
instrument.** Reuses decision 0024, but the instrument would live in a file
outside the song, edited outside the program, and the step grid would not
know the pads' names.

A wins: it is the bundled kits' way of working with the person's own
sounds, and the song keeps everything.

## Decision

- **Model:** `Instrument::Kit { pads }`; each pad has its key, its name,
  the song's audio file it plays (`AudioId`, as audio clips refer to their
  sound) and the drum pads' settings (`PadSetting`). A custom kit counts as
  drums (`is_drums`): step grid, drum keyboard layout, drum-style clips,
  channel 10 in MIDI files. File format version 7 (with SFZ instruments,
  decision 0024; neither has been released yet).
- **The sounds live in the song**, like imported audio (decision 0017): they
  are copied into the song's audio folder when added, saved inside the song
  file, and the song opens anywhere with its kit. An audio file stays in the
  song while a clip or a pad uses it.
- **Keys:** pads take the free keys from C1 (36) up, then below; up to 128
  pads. The computer keyboard plays them as it plays a drum kit (Z is the
  first pad).
- **Sound:** the engine plays a kit with the sampler of decision 0024: each
  pad a one-shot region on its key, its volume, position and pitch from its
  settings, a length below full fading the sound out by its end, velocity
  changing loudness half as much as a held piano. A pad's sound that is
  still being read is added as soon as it is ready.
- **Adding sounds** reuses the import path of audio files (copied and
  decoded in the background), with "becomes a pad of this kit" in place of
  "becomes a clip"; each pad is one undo step with its sound.
- **DAWproject:** a custom kit goes out as one of the program's own devices
  with its sounds in the file, and comes back whole into the program; other
  programs cannot play it (as for the other built-in instruments).

## Consequences

- The step grid works from a list of rows (a key and a name) instead of the
  fifteen fixed drum sounds; the bundled kits give their fifteen, a custom
  kit its pads.
- Changing a pad's settings rebuilds the kit's sampler (sounding hits stop):
  acceptable for now; the bundled kits keep changing settings as they play.
- Large folders: a kit holds at most 128 sounds, one per key.

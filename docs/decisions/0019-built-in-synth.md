# 0019. The built-in synth: two oscillators, a filter, two envelopes, a window of its own

- **Status:** accepted, November 2026 (extends decisions 0011, 0014 and 0016)

## The tasks

The brief asks for "a simple subtractive synth (oscillators, filter,
envelopes)" so that VLabs makes sounds of its own with no plugin
installed. A person using it wants to **find a sound quickly** (a bass, a
pad, a lead, a pluck), **shape it** while playing it (brighter, softer,
longer, wobblier) and **make it move over the song** (a filter that opens
over eight bars). Someone who knows synthesis must never hit a wall;
someone who does not must get a good sound with one click.

## The instrument

One voice of the synth is the classic subtractive chain:

```text
 oscillator 1 ─┐
 oscillator 2 ─┼─▶ filter ─▶ amplifier ─▶ out
 noise ────────┘     ▲           ▲
            filter envelope   volume envelope      (LFO: vibrato, wobble, tremolo)
```

- **Two oscillators**, each with a wave (sine, triangle, saw, square), an
  octave (−3 to +3), semitones (−12 to +12), a fine tune (±100 cents) and a
  level; one pulse width for the square waves; a noise level. Saw and
  square waves are band-limited (polyBLEP), the triangle's corners too
  (polyBLAMP), so high notes do not alias into harsh tones.
- **The filter:** low-pass 24 dB/octave (the default), low-pass
  12 dB/octave, high-pass or band-pass; cutoff (20 Hz to 20 kHz),
  resonance, how far the filter envelope moves it (±6 octaves) and how far
  the cutoff follows the key played. It is a zero-delay state-variable
  filter (two in a row for 24 dB), which stays stable however fast it is
  moved.
- **Two envelopes** (attack, decay, sustain, release): one for the volume,
  one for the filter. Attacks rise in a straight line from wherever the
  voice is, so a retriggered note never clicks.
- **One LFO** (sine, triangle, square, saw, random) with its own rate and
  three amounts at once: vibrato (pitch), wobble (filter) and tremolo
  (volume). Three amounts instead of a "destination" switch: nothing is
  hidden behind a mode.
- **Playing:** 16 voices, or one voice ("Mono") with glide between notes;
  how much velocity changes loudness and brightness; the output volume.
- **Presets** first: Soft keys (what a new synth track starts with), Warm
  pad, Bass, Sub bass, Lead, Pluck, Brass and Plain (a bare saw to start
  from). Choosing one sets every control; moving a control makes it
  "Custom".

Everything runs on the audio thread with memory reserved beforehand: the
voices are a fixed array, a note steals the quietest released voice (or
the oldest) when all are busy, and a key already sounding is retriggered
in place.

## How it is shown

**A. A window of its own, laid out as the sound flows (chosen).** The
synth's window opens from the same window button as a plugin's (track
header, location line, mixer strip): presets on top, then three columns
from left to right in signal order: *Oscillators*; the *Filter* (with its
response drawn) and the *Movement* that sways it; the *Envelopes* (each
drawn as its shape) and how notes *play*. Three columns of compact rows
(about 810 by 580 points) fit the program's smallest window; it opens to
the right of the song view's track headers, so the button that opened it
stays in reach. It is a window inside VLabs, like the effect windows, so
it can stay open while the computer keyboard plays the track: shaping a
sound is playing it. Every control has the usual slider with its value in
words ("1.2 kHz", "350 ms"); a drag is one undo step.

**B. A drawer at the bottom,** in the keyboard's place, like the mixer.
*Rejected:* the keyboard is where a sound is auditioned, so the two would
fight for the same place, and twenty-odd controls do not fit in a drawer's
height without crowding.

**C. Presets and four "macro" knobs** (brightness, attack, release,
movement) with the full synth behind a "More" button. *Rejected as the
whole design:* the brief rules out walls for people who know what they
want. Its good part is kept: presets come first, at the top.

## Settings as data, changes while playing, automation

- The track's instrument is `Instrument::Synth(SynthSettings)` in the
  model: plain numbers and choices, all with defaults (file format
  version 6).
- A changed control does not rebuild the instrument: the engine sends the
  new settings to the playing synth (as it does for drum pads), so held
  notes carry on with the new sound.
- The synth's continuous settings are numbered, with their ranges, in the
  model (`SynthSettings::params`, like the built-in effects' settings), so
  automation lanes can change them over the song: the track's Automation
  button offers "Synth · Cutoff", "Synth · Resonance" and the others. The
  audio thread applies a lane's value to a copy of the base settings and
  hands it to the synth, with no allocation.
- Exported to DAWproject (decision 0020), the synth is a device of
  VLabs's own whose settings travel as a small file; other programs keep
  its notes and name.

## Consequences

- The program makes leads, basses and pads without plugins; plugins remain
  the way to go further.
- Not in this version: more than two oscillators, unison, a second LFO,
  modulation routing beyond the fixed amounts, the synth's own effects
  (the track's effect chain does that) and per-note pitch offsets (the
  note model keeps room for them).

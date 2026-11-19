# 0024. SFZ instruments: our own reader, a sampler, sounds loaded in the background

- **Status:** accepted, November 2026 (extends decision 0023; the last part
  of the owner's "plugins and sound libraries" request)

## The task

Many free and paid sampled instruments (pianos, drum kits, orchestral
sounds) come as an `.sfz` file and a folder of sound files. SFZ is a
plain-text format: it lists *regions*, each naming a sound file, the keys
and velocities it plays for, and how it plays (tuning, volume, envelope,
loops, filters). A person who has one should be able to drop it on the
window, find it in the sound library, or choose it in **+ Instrument…**, and
play it like any other instrument, with nothing else installed.

## Decision

**A new instrument kind, `Sfz { file }`.** The song refers to the `.sfz`
file where it is, as for a SoundFont the person chose; nothing is copied.
This is file format version 7: older copies of the program refuse such
songs instead of silently dropping the instrument. A missing file plays a
plain sound and the status line says where it was (as for SoundFonts).

**Our own reader (`daw_formats::sfz`).** The format is small and text-based;
the Rust crates that read it are unmaintained or partial, and we already
write our own MIDI reader. It reads:

- headers `<control>`, `<global>`, `<master>`, `<group>`, `<region>`, with
  each level inheriting from the ones above; `#define` and `#include`
  (paths relative to the instrument's file); `//` and `/* */` comments; sound paths
  with spaces or backslashes; note names (`c4`, `f#3`, `eb2`) and
  `octave_offset` / `note_offset`; `default_path`;
- *which region plays*: `key`, `lokey`, `hikey`, `lovel`, `hivel`, round
  robins (`seq_length`, `seq_position`), random choice (`lorand`,
  `hirand`), keyswitches (`sw_lokey`, `sw_hikey`, `sw_last`, `sw_default`,
  and `sw_down`, `sw_up`, `sw_previous`, which follow the keys held and
  played), triggers (`attack`, `release` with `rt_decay`, `first`,
  `legato`), controller conditions against the controllers' starting
  values (a piano's "pedal up" samples play, its "pedal down" ones do
  not), channel 1;
- *how it plays*: `pitch_keycenter`, `pitch_keytrack`, `transpose`, `tune`,
  `pitch_veltrack`, `pitch_random`; `volume`, `amplitude`, `pan`, `width`,
  `amp_veltrack`, `amp_velcurve_N`, `amp_keytrack`, `amp_keycenter`,
  `amp_random`; velocity and key crossfades (`xfin_*`, `xfout_*`); the
  volume envelope (`ampeg_delay`, `_start`, `_attack`, `_hold`, `_decay`,
  `_sustain`, `_release`, and their `_vel2*` forms); `offset`, `end`,
  `delay`; loops (`loop_mode`, `loop_start`, `loop_end`, and the loop
  stored in a WAV file); choke groups (`group`, `off_by`, `off_mode`);
  `polyphony` and `note_polyphony`; one filter per region (`fil_type`,
  `cutoff`, `resonance`, `fil_keytrack`, `fil_keycenter`, `fil_veltrack`)
  with its envelope (`fileg_*`); the built-in `*sine` and `*silence`;
- *settings moved by controllers* (`…_onccN`, `…_ccN`), at the
  controllers' starting values (`set_ccN`, else MIDI's usual 100 for
  volume, 64 for pan, 127 for expression, 0 for the rest): envelope times
  and levels, cutoff, resonance, volume, pan, width, tuning, offset and
  delay. Some instruments expose their envelope this way (a sustain of 0
  raised to full by a controller starting at 127): without this their
  organs and pads fall silent after the attack.

The program sends instruments no controllers yet, so these values do not
move while playing.

What an instrument uses beyond this (LFOs, pitch envelopes, other
controller modulation, effects) is left out, and **the status line names
it once** when the instrument is ready, as for DAWproject files from other
programs; an LFO or envelope is named only if it moves something at the
start (a vibrato whose depth a controller at 0 sets is not).
Sound files that cannot be found or read are left out and counted in the
same message; the rest of the instrument plays. A path written with other
letter cases (instruments made on Windows) is still found on Linux.

**A sampler in `daw-dsp` (`daw_dsp::sampler`).** The reader hands back a
ready-to-play bank (regions and decoded sounds); the sampler plays it like
the other built-in instruments: it never allocates on the audio thread,
interpolates between samples (four-point), follows each region's envelope,
loops, filter and choke group, and steals the oldest released voice first
when all 96 are busy. `daw-formats` now depends on `daw-dsp` (a lower
layer) to build that bank.

**Sounds load in the background.** A large instrument holds hundreds of
megabytes of sound, decoded once and kept in memory (shared by every track
and export that uses it). The engine cannot read sound files itself
(decision 0003 keeps file formats out of it), so the program hands it the
reader as a plain function, and the engine runs it on a thread of its own:
the track is silent while its instrument loads, the window carries on (the
status line says what is loading), and the track plays as soon as it is
ready. Exports wait for every instrument to be ready. One instrument may
hold at most 8 GB of decoded sound; beyond that it is refused with a clear
message rather than filling the memory.

**In the interface:** `.sfz` files appear in the sound library as
instruments (double-click or drag to add a track); dropping one on the
window or importing it adds a track; the instrument chooser's entry becomes
**SoundFont or SFZ…**. DAWproject keeps an SFZ track as one of the
program's own devices (as for SoundFont tracks); MIDI export gives it the
piano.

## Alternatives considered

- **An existing crate.** The Rust SFZ parsers found were unmaintained, read
  only part of the format and none plays it; sfizz (the main open-source
  SFZ player, BSD-2-Clause) is C++, and linking it would add a large native
  build to every platform. Writing the reader and the sampler keeps the
  whole path in Rust, tested, and under our licence.
- **Streaming sounds from the disk** (reading each sound's start into
  memory and the rest as it plays), as large commercial samplers do: much
  less memory for very large instruments, but a disk-reading thread per
  voice with its own timing risks. Loading everything is simpler and right
  for most free instruments; streaming can come later inside the sampler
  without changing songs.
- **Loading on the interface thread, as SoundFonts do:** the window would
  freeze for seconds while a large instrument decodes.
- **Mapping SFZ onto the SoundFont player:** the two formats differ too much
  (envelopes, round robins, keyswitches, release samples).

## Consequences

- The model gains `Instrument::Sfz`, the file format becomes version 7 with
  a sample file and a migration step, and every `match` on instruments
  handles it.
- The engine's sound library takes a reader function and loads SFZ
  instruments on a thread; the engine rebuilds a track once its instrument
  is ready and drops instruments no longer used when another song opens.
- Very large libraries need as much memory as their decoded sound;
  streaming is the way out if that becomes a problem.

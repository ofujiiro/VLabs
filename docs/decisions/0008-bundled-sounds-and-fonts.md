# 0008. Where the bundled piano, drums and fonts come from

- **Status:** accepted, November 2026

## Context

VLabs must be useful with no plugin installed: a SoundFont player with a
bundled piano, and a drum sampler with a starter kit. Bundled sounds must be
CC0 (public domain dedication) and recorded in `THIRD_PARTY.md`. The
interface also needs fonts, and egui's default fonts are under the OFL and
Ubuntu font licences, which are outside the permissive list (decision 0005).

## Decision

### Piano

The **Osiris Piano** by Versilian Studios and Karoryfer Samples, a sampled
piano released under **CC0 1.0** (the licence file is in its repository,
`github.com/sfzinstruments/Osiris_Piano`, tag v0.925). The original is 900 MB
of multi-microphone samples, far too large to bundle. `tools/make-piano`
builds a 19 MB SoundFont from it; the steps and the automated checks are in
`THIRD_PARTY.md`. Notable choices, made by measurement since nobody on the
development side can listen:

- **Mono, one microphone, the louder channel per note.** All three
  microphone positions are spaced pairs: summing a pair's channels cancelled
  up to 86% of the energy on some notes, and mixing microphones cancelled
  further. A single channel cannot cancel anything. Stereo would double the
  size; a gentle pan from bass to treble gives some width back.
- **The pre-strike mechanism noise is trimmed**, otherwise every note
  started 20 to 30 ms late.
- **Looped tails continue each note's own decay rate**, measured from the
  recording, so held notes fade like the real instrument rather than at one
  fixed speed.
- The undamped top notes are not looped: they fade naturally within two
  seconds, and their beating strings did not loop cleanly.

The FreePats CC0 pianos were also suitable, but their website is not
reachable from the development environment; Osiris is also the better
instrument. The Akai "Splendid Grand Piano" is described as public domain but
carries no explicit CC0 dedication, so it was not used.

### Drum kit

Chosen in phase 2 from two CC0 libraries in the same collection:
**Virtuosity Drums** (Versilian Studios and Karoryfer Samples, an acoustic jazz
kit) and **Big Rusty Drums** (Karoryfer Samples). Both carry the CC0 1.0
licence file in their repositories. Electronic drum sounds, if added, will be
synthesised by our own code and dedicated to CC0 as well.

### Fonts

The **Go fonts** (Go, Go Medium, Go Mono) by Bigelow & Holmes, made for the Go
project and released under the **BSD-3-Clause** licence. They were designed
for legibility (clearly distinct 0/O and 1/l/I) and include a monospaced
companion with the same shapes, which suits time and tempo readouts.

## Consequences

- No bundled asset restricts relicensing; the font licence only asks that its
  notice travel with the program.
- The repository stays small: the generated piano is committed, the 900 MB
  source is not.

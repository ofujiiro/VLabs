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
grand piano released under **CC0 1.0** (the licence file is in its
repository, `github.com/sfzinstruments/Osiris_Piano`, tag v0.925). The
original is 900 MB of multi-microphone samples, far too large to bundle. A
small tool in this repository (phase 1) builds a compact SoundFont from it:
one microphone position, two velocity layers, samples trimmed with a natural
fade, stored at 16-bit. The target is under 20 MB. The source and every
processing step are recorded in `THIRD_PARTY.md`.

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

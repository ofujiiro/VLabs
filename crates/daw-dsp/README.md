# daw-dsp — built-in instruments and effects

The sounds and effects that ship with the application, so it is useful with
no plugin installed.

## What belongs here

- **Instruments:** a SoundFont (.sf2) player, a drum sampler, a simple
  subtractive synth (oscillators, filter, envelopes).
- **Effects:** EQ, compressor, reverb, delay, limiter.
- The building blocks they share: filters, envelopes, oscillators, delay
  lines, gain and level measurement.
- **Recorded and imported sound in memory** (`audio`): buffers, conversion
  between sample rates (our own windowed-sinc converter, decision 0017) and
  loudness summaries for drawing waveforms. Made in the background; the
  audio thread only reads buffers.

## Rules

- **Safe for the audio thread.** Code that produces or processes sound does
  not allocate memory, take locks, touch files or the network, or log. All
  memory is reserved before playback starts.
- **Deterministic.** The same input gives the same output, so tests can render
  sound offline and check it: a signal is present, the pitch is right, the
  timing is right, nothing clips.
- **Pure signal processing.** No devices, no files, no threads, nothing drawn
  on screen. This crate depends on no other crate of the application.

The bundled sounds themselves (the piano SoundFont, the drum kit) are files in
`assets/`, not code. They must be CC0 and are recorded in `THIRD_PARTY.md`.

## Roadmap

Phase 1 (SoundFont player), phase 2 (drum sampler, effects), phase 4
(audio buffers, rate conversion, waveforms), phase 6 (subtractive synth).

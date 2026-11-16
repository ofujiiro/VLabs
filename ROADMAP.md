# Roadmap

The phases come from the project brief. Each ends with a build the owner can
run and a checklist of things to try. Status is updated at every milestone.

**Now:** phase 0 (foundations and risk spikes).

## Phase 0 — Foundations and risk spikes

- [x] Repository, licence (GPL-3.0-or-later), contributor licence agreement,
      README, this roadmap, `CLAUDE.md`, decision records
- [x] Cargo workspace with layered crates and repository-rule tests
- [x] CI on Windows and Linux: format, lint, build, test, licence check
- [x] Real-time allocation check (`daw-rt`), tested
- [ ] Spike (a): low-latency audio output on Windows and Linux
- [ ] Spike (b): host a CLAP and a VST3 instrument (Surge XT): notes in,
      audio out, editor window, save and restore state, on both systems
- [ ] Spike (c): a piano roll with 10,000 notes that scrolls and zooms
      smoothly
- [ ] Spike results recorded (decision 0009); go or no-go for phase 1

## Phase 1 — First sound

Project model, undo, save and load, transport, engine core, SoundFont piano,
piano roll, playing notes from the computer keyboard, live note recording
with metronome and count-in.

## Phase 2 — Making a beat

Step sequencer, drum sampler with the starter kit, patterns, arrangement
timeline, mixer, built-in effects (EQ, compressor, reverb, delay, limiter),
WAV export.

## Phase 3 — Plugins

Scanner (separate process), plugin browser, VST3 and CLAP instruments and
effects, editor windows, state saving, parameter automation.

## Phase 4 — Audio

Recording from the built-in microphone or an interface, audio clips on the
timeline, cut, trim, fades, waveform display, file import (WAV, FLAC, MP3,
OGG).

## Phase 5 — Theory and rhythm

Key and scale highlighting, snap-to-scale, chord stamps, inversions,
transposition by interval, tempo and time-signature track, tuplets, swing.

## Phase 6 — Completing v1

Subtractive synth, DAWproject and MIDI import and export, FLAC export,
autosave polish, installers (Windows installer, Linux AppImage), generated
third-party notices, user guide.

## Later (not in v1, not designed out)

VST2 (clean-room headers and a licensing check first), a 32-bit plugin
bridge, LV2, plugins running in their own process, a staff-notation view,
microtonal tunings (the note model already leaves room for per-note pitch
offsets), lower-latency Windows audio modes, cpal's native PipeWire backend.

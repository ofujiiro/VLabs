# daw-engine — turning the project into sound

Plays what `daw-model` describes, and records what comes in.

## What belongs here

- **Transport:** play, stop, position, loop, tempo following the tempo map.
- **Scheduling:** every note and clip placed to the exact audio sample.
- **The processing graph:** instruments, the effect chain of each track, the
  mixer, the master output.
- **Devices:** audio output and input on the system's default devices with no
  setup, MIDI input, and devices that appear or disappear while running.
- **Recording:** notes played live, and audio from a microphone or interface.
- **Metronome and count-in.**
- **Offline rendering:** the same graph, run as fast as possible into a buffer
  or a file, for export and for tests.

## Rules

- **The audio thread is sacred.** On the real-time thread: no memory
  allocation, no locks, no file or network access, no logging. The interface
  and the engine talk through lock-free queues. A debug-build check fails
  loudly if the audio thread ever allocates.
- **The engine runs headless.** It works with no window and no sound card, so
  a test can load a project, render it offline and inspect the result. This
  crate never depends on interface code.
- **One graph.** Offline export uses exactly the same processing as live
  playback.
- **No ASIO SDK.** The standard Windows audio system is the Windows backend
  for v1. The licence check blocks the ASIO bindings.
- The engine reads the project from `daw-model`; it does not keep its own
  version of the song.

## Roadmap

Phase 0 (risk spike: low-latency output on both systems), phase 1 (engine
core, transport, live note recording, metronome), phase 2 (mixer, effect
chains, WAV export), phase 3 (plugins in the graph, automation),
phase 4 (audio recording and playback).

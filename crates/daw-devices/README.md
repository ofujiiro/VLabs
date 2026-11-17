# daw-devices — speakers, microphones and MIDI keyboards

Connects the application to the computer's sound and MIDI hardware with no
setup, and keeps it connected as devices come and go.

## What belongs here

- **Audio output** on the system's default device: WASAPI on Windows; ALSA on
  Linux, which also reaches PipeWire and PulseAudio through their ALSA
  bridges, and JACK when it is running.
- **Audio input** from the built-in microphone or an audio interface: the
  default input, opened at its own rate and format, reopened when it fails
  (decision 0017). Both directions report the delay the system gives, so
  recordings can be lined up with what was heard.
- **MIDI input** from any connected keyboard.
- **Hot-plugging:** noticing a device that appears or disappears while the
  program runs, and following a change of the system's default device.
- **No device at all:** when there is no sound card, a silent stand-in keeps
  time, so the rest of the program behaves normally.

## Rules

- **Knows nothing about music.** This crate moves blocks of samples and MIDI
  bytes between the hardware and a callback it is given. The engine decides
  what those samples are. It depends on no other crate of the application
  except `daw-rt`.
- **The callback is real-time.** Code here that runs inside the audio
  callback follows the audio-thread rules: no allocation, no locks, no files,
  no logging.
- **No ASIO SDK.** The standard Windows audio system is enough for v1.

## Roadmap

Phase 0 (risk spike: low-latency output on both systems), phase 1 (output,
MIDI input, hot-plugging), phase 4 (audio input and recording).

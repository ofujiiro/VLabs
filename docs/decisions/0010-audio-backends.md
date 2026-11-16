# 0010. Which audio systems are used on each platform

- **Status:** accepted, November 2026

## Context

Sound must work on the default output device with no configuration and no
special driver, on Windows and on Linux (PipeWire, ALSA, JACK). The ASIO SDK
may not be used.

## Decision

All audio goes through `cpal` (decision 0006).

- **Windows: WASAPI, shared mode.** The standard Windows audio system; every
  device works with it, and it follows changes of the default device.
- **Linux: ALSA by default.** On a modern desktop, ALSA's default device is
  PipeWire's (or PulseAudio's) bridge, so this reaches whatever the user's
  system uses, and plain ALSA systems work too. Measured in phase 0: 64-frame
  buffers (1.3 ms) through PipeWire's bridge.
- **Linux: JACK as an option**, for people who run a JACK (or PipeWire-JACK)
  setup. The JACK library is loaded at run time only if it is present, so it
  is never a requirement.
- **No device at all:** a silent stand-in keeps the transport running, so the
  program still opens and works (phase 1).
- **Not now:** cpal's native PipeWire and PulseAudio backends (they would make
  the program fail to start on systems without those libraries, and the ALSA
  bridge already reaches both); WASAPI exclusive mode and the low-latency
  shared mode of newer Windows versions (possible later improvements);
  ASIO (forbidden by the brief).

## Consequences

- One code path per platform for the common case, with nothing to set up.
- On Windows, shared-mode latency is set by the system (typically 10 ms
  periods). That is fine for playing and recording with monitoring; a lower
  latency mode is a later improvement, recorded in the roadmap.

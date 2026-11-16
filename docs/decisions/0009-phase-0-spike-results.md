# 0009. Results of the three phase 0 risk spikes

- **Status:** accepted, November 2026. **Verdict: all three spikes pass; go
  for phase 1.**

The brief asked for three throwaway spikes before phase 1. Each is a package
in `spikes/`, runs locally and in CI (`.github/workflows/spikes.yml`) on
Ubuntu 26.04 and Windows Server 2025, and prints `CHECK`/`RESULT` lines.

## (a) Low-latency audio output — PASS

`spikes/audio-latency` opens the default output device with no setup, asks
for the smallest buffer the system accepts, plays a beeping tone and
measures the callbacks.

| System | Path | Buffer granted | Callback interval | Reported output latency | Dropouts | Allocations in callback |
| --- | --- | --- | --- | --- | --- | --- |
| Ubuntu 24.04 container | ALSA → PipeWire bridge, virtual speaker | 64 frames (1.33 ms) | p50 1.33 ms, p99 1.53 ms | 2.5 ms | 0 | 0 |
| Ubuntu 26.04 CI, no real-time priority | ALSA → PipeWire bridge, virtual speaker | 64 frames (1.33 ms) | p50 1.33 ms, p99 1.35 ms, max 3.5 ms | 2.7 ms | 0 | 0 |
| Ubuntu container | JACK (PipeWire's), quantum 128 | 128 frames (2.67 ms) | p50 2.67 ms | 2.6 ms | 0 | 0 |
| Windows Server 2025 CI | WASAPI shared mode, VB-Cable virtual device | 480 frames (10 ms, fixed by Windows) | p50 10.02 ms, p99 10.6 ms | 42 ms (virtual cable) | 0 | 0 |

Findings:

- The default device works with no configuration on both systems, at 48 kHz
  with 32-bit float samples.
- The allocation check (`daw-rt`) ran inside every callback and counted zero
  allocations.
- **Windows is the weak spot.** In shared mode, Windows sets a 10 ms period
  and cpal does not use the newer low-latency shared mode (IAudioClient3).
  The 42 ms latency measured is mostly the virtual cable's own buffering; on
  real hardware shared mode is typically 20 to 30 ms. That is usable for
  playing and recording, but noticeable to a trained player. Recorded as a
  later improvement (low-latency shared mode, optional exclusive mode); it
  does not block v1, and the brief's "no special driver" rule is met.
- On Linux, real-time thread priority is refused for ordinary users unless
  their distribution grants it (PipeWire's packages usually do); even
  without it, 64-frame buffers ran without dropouts in CI.
- JACK clients cannot choose the buffer size; the JACK server (or PipeWire's
  quantum) decides. VLabs therefore defaults to ALSA (decision 0010).

**Not verified, owner to check:** real sound cards on both systems, and that
the tone sounds clean (run `cargo run -p spike-audio-latency -- --seconds 5`;
you should hear short beeps at 440 Hz).

## (b) Hosting a CLAP and a VST3 instrument — PASS

`spikes/plugin-host` loads one plugin and checks the full chain. Reference
plugin: **Surge XT 1.3.4** (test fixture only, downloaded, never linked or
bundled).

Every check passed for **both formats on both systems** (Linux locally under
a virtual X11 screen and under XWayland; Linux and Windows in CI):

| Check | Result (identical on Linux and Windows) |
| --- | --- |
| Load | 66 to 220 ms |
| Notes in, audio out | sound during both notes, silence before |
| Pitch | middle C measured at 261.6 Hz, E at 329.6 Hz |
| No clipping | peak 0.27 |
| Parameter change heard | "Global Volume" to 25%: level 0.101 → 0.0016 |
| State saves | about 50 KB |
| Restored into a fresh instance | same level as when saved, same parameter value |
| Editor opens in our window | 913 × 569 pixels |
| Editor draws itself | 100% of the window drawn by the plugin (screenshot) |
| Editor closes, plugin unloads | cleanly |

Findings:

- **CLAP through `clack-host` 0.2** was straightforward. Editors on Linux
  need the host's timer and file-descriptor extensions, which are small.
- **VST3 needs a real host layer of our own.** The `vst3` crate gives raw
  bindings only; the host side (application object, messages and attribute
  lists, event lists, parameter changes, memory streams, the editor frame
  and the Linux run loop) took about a thousand lines in the spike. It
  works; it is the largest piece of phase 3.
- **Plugin GUIs built with JUCE (Surge, and many others) crash on an X server
  without a window manager**, because they assume standard desktop atoms
  exist. Our host now creates those atoms itself before loading plugins.
  Normal desktops always have them; this matters for CI and unusual setups.
- **XWayland works.** Under a Wayland compositor (Weston 13 with Xwayland
  23.2), both editors opened and drew fully through XWayland.
- The VST3 SDK headers now define a Wayland window type, so native Wayland
  plugin editors become possible later as plugins adopt them.
- VST3 enum types are unsigned on Linux and signed on Windows; the bindings
  reflect that and our code must cast accordingly.

**Not verified:** clicking and dragging inside the editors (owner, phase 3),
editors on real GNOME and KDE Wayland sessions (owner), the second
instrument and the effect plugin (chosen in phase 3), scanning in a separate
process and out-of-process hosting (designed, built in phase 3). The renders
are uploaded by CI as WAV files for listening.

## (c) A piano roll with 10,000 notes — PASS

`spikes/piano-roll`: a custom-drawn roll that draws only visible rows, grid
lines and notes; notes are found by binary search and drawn as one mesh.
10,000 notes over 595 bars (bass, chords, a melody with triplets), window
1600 × 900.

Headless benchmark through egui's real input path (frame build plus
tessellation, debug build, 4-core container):

| Scenario | p50 | p95 | max | Notes visible |
| --- | --- | --- | --- | --- |
| Scroll through the song | 0.19 ms | 0.23 ms | 0.38 ms | up to 159 |
| Scroll up and down the keys | 0.19 ms | 0.23 ms | 0.28 ms | up to 167 |
| Zoom out to the whole song and back | 0.23 ms | 2.37 ms | 3.85 ms | up to 10,000 |
| Whole song on screen (worst case) | 0.67 ms | 0.80 ms | 3.48 ms | 10,000 |

A 60 Hz screen allows 16.7 ms per frame; the budget set for the piano roll
alone was 8 ms at the 95th percentile. In CI the benchmark passed on both
systems; on Windows Server 2025 the worst 95th percentile was 2.2 ms (all
10,000 notes on screen: p50 1.1 ms, max 6.5 ms).

In a real window, continuous scrolling and zooming ran at about 69 frames
per second on Linux with software rendering (Mesa, no GPU). On the Windows
CI machine, which has no graphics card at all, the window ran at only about
3 frames per second: building each frame still took about 2 ms, but drawing
it fell back to Windows' slow software rasteriser. Real PCs have a graphics
processor (even a basic integrated one), so this does not affect the
decision, but machines without graphics acceleration (some virtual machines
and remote desktops) would be sluggish. If testers hit that, eframe's OpenGL
renderer can be offered as a fallback.

Findings:

- **Every grid needs level of detail.** The first version drew one line per
  bar at any zoom; zoomed far out that meant tens of thousands of lines and
  frames up to 22 ms. Thinning lines so they are never closer than 8 points
  fixed it. The timeline and every ruler will follow the same rule.
- One mesh for all notes keeps even 10,000 visible notes cheap (65,000
  vertices, under 1 ms to build).
- egui's clipboard support pulls in a Boost-licensed crate on Windows. It is
  enabled on Linux only; phase 1 adds a small Windows clipboard of our own.
  egui's text layout uses `unicode-properties`, which joins `unicode-ident`
  under the Unicode-data exception (decision 0005).

**Not verified, owner to check:** smoothness on a real GPU and on a
high-resolution display, and how scrolling feels with a trackpad (run
`cargo run --release -p spike-piano-roll`).

## Decision

All three risks are retired: audio output, plugin hosting (both formats,
both systems, with editors and state) and the custom-drawn editing surfaces
all work with the settled technology choices. **Phase 1 starts now**, with no
change to the brief's decisions.

The spikes stay in the repository as reference code and CI checks until the
real implementations replace them (audio in phase 1, plugins in phase 3).

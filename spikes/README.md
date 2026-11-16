# spikes

Throwaway experiments for roadmap phase 0. Each answers one risk question
from the brief; the answers are recorded in
`docs/decisions/0009-phase-0-spike-results.md`. They are not part of the
product and are built only when asked for by name (CI builds and lints them).

| Spike | Question | Run |
| --- | --- | --- |
| `audio-latency` | How low can audio output latency go on the default device, with no setup? | `cargo run -p spike-audio-latency -- --seconds 5` |
| `plugin-host` | Can we load CLAP and VST3 instruments, play them, open their editors and save their state? | see its README |
| `piano-roll` | Can egui draw a 10,000-note piano roll that scrolls and zooms smoothly? | see its README |

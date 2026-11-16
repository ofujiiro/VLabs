# spike-piano-roll

Phase 0 spike (c): can egui draw a piano roll with 10,000 notes that scrolls
and zooms smoothly?

The roll is custom-drawn: only visible key rows, grid lines and notes are
drawn, notes are found by binary search, all notes go into one triangle mesh,
and grid lines thin out as you zoom out (level of detail).

```bash
cargo run -p spike-piano-roll                 # try it in a window
cargo run -p spike-piano-roll -- --bench      # measure, no window
cargo run -p spike-piano-roll -- --autopilot 6 --screenshot roll.bmp
```

In the window: mouse wheel scrolls the keys, Shift+wheel (or a horizontal
trackpad swipe) scrolls in time, right- or middle-drag moves around,
Ctrl+wheel or pinch zooms in time, Ctrl+Alt+wheel zooms the keys.

Results are recorded in `docs/decisions/0009-phase-0-spike-results.md`.

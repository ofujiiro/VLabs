# spike-plugin-host

Phase 0 spike (b): can VLabs host CLAP and VST3 instruments on Windows and
Linux? For one plugin file it checks, in order:

1. the plugin loads;
2. two notes produce sound at the right pitch (middle C and the E above,
   measured), with no clipping;
3. changing a parameter (the overall volume) is heard;
4. the state saves, and a fresh instance restored from it sounds the same and
   reports the same parameter value;
5. with `--editor SECONDS`: the plugin's editor opens inside a window we
   create, draws itself (checked from a screenshot), and closes cleanly.

Every render is written as a WAV file, and the editor screenshot as a BMP,
into the output folder (`--out`, default `spike-output/`).

```bash
cargo run -p spike-plugin-host -- "/path/to/Surge XT.clap" --editor 5
cargo run -p spike-plugin-host -- "/path/to/Surge XT.vst3" --editor 5
```

On a Linux machine without a desktop, run it under a virtual screen:
`xvfb-run -a -s "-screen 0 1600x1200x24" ...`.

The CI workflow `spikes.yml` downloads Surge XT and runs this on both
systems. Results are recorded in `docs/decisions/0009-phase-0-spike-results.md`.

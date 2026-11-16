# Third-party material

Everything in this repository that was not written for VLabs, with its
source and licence. The rules (decision 0005) are: outside code must be
permissively licensed, bundled sounds must be CC0.

## Bundled with the program

### Fonts — `assets/fonts/`

| File | What | Source | Licence |
| --- | --- | --- | --- |
| `Go-Regular.ttf`, `Go-Medium.ttf`, `Go-Mono.ttf` | The Go font family by Bigelow & Holmes Inc., made for the Go project | `github.com/golang/image`, folder `font/gofont/ttfs` (fetched November 2026, unmodified) | BSD-3-Clause; full text in `assets/fonts/LICENSE-Go-fonts.txt` |

SHA-256 of the files as fetched:

```text
197d9f3703b4c00af609178876a8d73e396f64fe438b2c871778566632374be3  Go-Regular.ttf
e687b22d748d54f9fa10dbae599d08ae31f0b305f8bea52fb2d3639c1565ae78  Go-Medium.ttf
8bc66a0154bbf69cd24e5bde41a12ec9495c8a242c5def2255e4a164900f1ed7  Go-Mono.ttf
```

### Sounds — `assets/sounds/`

#### `piano.sf2` — the bundled piano

| | |
| --- | --- |
| Source | **Osiris Piano** by Versilian Studios and Karoryfer Samples, `github.com/sfzinstruments/Osiris_Piano`, tag v0.925 (commit `18c6afccb60cff458edbf7c394571783e074e1e9`) |
| Licence | **CC0 1.0** (public domain dedication); the processed SoundFont is also CC0. See `assets/sounds/LICENSE-piano.txt`. |
| Built by | `tools/make-piano` (`cargo run --release -p make-piano -- build <Osiris_Piano folder> assets/sounds/piano.sf2`) |
| SHA-256 | `19610bf65cb2d57838431fcc65d1ff46e2a7f23f6e0ebd6c57592fc56033b7d4` |

Processing, in order:

1. Samples from the "UC" set (dampers working, low-noise versions), microphone
   position B, both velocity layers (soft 1–63, loud 64–127), every sampled key
   with the library's own key ranges and release times.
2. Each microphone pair is spaced, so summing its two channels cancels part of
   the sound on many notes (measured by `make-piano analyze`); each note uses
   its louder channel, giving a mono sample.
3. The 10 to 30 ms of key and hammer mechanism noise before each strike is
   trimmed (the original instrument offers a control for this), so notes speak
   at once.
4. Levels are evened out across the keyboard within each velocity layer (each
   note pulled towards the median of its neighbours, at most 6 dB), then all
   samples share one normalisation (the loudest note peaks at -1 dBFS).
5. Notes up to E6 keep 1.0 to 2.2 s of their recording, then a loop of about
   a quarter second, chosen where the waveform lines up best, levelled and
   crossfaded. The SoundFont's envelope continues each note's own measured
   decay rate from there. The undamped top notes (F6 up) keep 2 s of natural
   recording with a short fade, unlooped.
6. 48 kHz, 16-bit, one preset ("Piano", bank 0, program 0).

`make-piano verify` checks every key of the result: immediate onset, no level
jump above 6 dB between neighbouring keys, no click, and loops that do not
pulse by more than 3 dB while clearly audible. All 88 keys pass.

#### Planned

- **Drum kit** (phase 2): from **Virtuosity Drums** (Versilian Studios and
  Karoryfer Samples) or **Big Rusty Drums** (Karoryfer Samples), both CC0 1.0
  (`github.com/sfzinstruments`).

## Outside code (Rust crates)

Libraries are pulled from crates.io when VLabs is built; they are not stored
in this repository. Every one is permissively licensed (MIT, Apache-2.0, BSD,
ISC or Zlib, plus the Unicode licence for one data table, see decision 0005),
which `cargo deny` checks on every change. The crates chosen directly, and
why, are listed in `docs/decisions/0006-adopted-crates.md`. Installers will
include a generated file with every library's licence notice (phase 6).

## Used for testing only, never bundled or linked

- **Surge XT** (GPL-3.0), the reference CLAP and VST3 instrument, downloaded
  by CI from `github.com/surge-synthesizer/releases-xt`. VLabs loads it as a
  plugin at run time, like any user would; none of its code is part of VLabs.

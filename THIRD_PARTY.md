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

### Sounds

None yet. Planned (decision 0008):

- **Piano:** a compact SoundFont built from the **Osiris Piano** by Versilian
  Studios and Karoryfer Samples, CC0 1.0 (`github.com/sfzinstruments/Osiris_Piano`).
- **Drum kit:** from **Virtuosity Drums** (Versilian Studios and Karoryfer
  Samples) or **Big Rusty Drums** (Karoryfer Samples), both CC0 1.0
  (`github.com/sfzinstruments`).

Each will be listed here with the exact source revision and the processing
applied.

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

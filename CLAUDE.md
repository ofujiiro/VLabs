# CLAUDE.md — working on VLabs

Guidance for any session (human or AI) picking up this repository.

## Start here

1. **Read [`docs/PROJECT_BRIEF.md`](docs/PROJECT_BRIEF.md).** It is the
   owner's brief: product vision, settled decisions, engineering rules and
   the roadmap. It overrides everything else except a later decision record.
2. **Read [`ROADMAP.md`](ROADMAP.md)** for where the work stands and what
   comes next.
3. Skim [`docs/architecture.md`](docs/architecture.md) and the index of
   [`docs/decisions/`](docs/decisions/README.md) before changing anything
   structural.

## Who is who

The **project owner** directs the product and does not read or write Rust.
You write all the code and are responsible for its quality, its tests, and
for explaining what you did in plain language. Decide technical and design
questions yourself and record each significant one in `docs/decisions/`.
Stop and ask only when blocked by something only the owner can do or decide:
an account or credential, spending money, a legal choice, or a finding that
makes a settled decision in the brief unworkable.

## Repository map

| Path | What |
| --- | --- |
| `crates/daw-*` | The application, one crate per concern; each has a README with its rules |
| `spikes/` | Phase 0 throwaway experiments (not part of the product) |
| `xtask/` | Developer commands (`cargo xtask ...`) and repository-rule tests |
| `docs/` | Brief, architecture, decision records |
| `assets/` *(from phase 1)* | Bundled fonts, piano, drum kit (all recorded in `THIRD_PARTY.md`) |
| `.github/workflows/` | CI (`ci.yml`) and the spike/integration jobs (`spikes.yml`) |

## Commands

```bash
cargo xtask ci        # everything CI checks: fmt, clippy, build, test, cargo-deny
cargo xtask fmt --fix # reformat
cargo test -p daw-model
cargo run -p daw-app  # the program
cargo xtask dist      # release build in dist/, named after the product
```

`cargo-deny` must be installed: `cargo install cargo-deny --locked`.

On Linux, building needs: `libasound2-dev libjack-jackd2-dev libudev-dev
libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev
libxkbcommon-x11-dev libwayland-dev`.

## Non-negotiable rules

These come from the brief; tests and CI enforce most of them.

- **The audio thread is sacred.** No allocation, locks, file or network
  access, or logging on the real-time thread. UI and engine talk through
  lock-free queues (`rtrb`). Wrap real-time code in `daw_rt::RtGuard` /
  `rt_region`; debug builds abort on allocation inside it. Use
  `daw_rt::permit_alloc` only around third-party plugin calls.
- **Licences.** Outside code must be MIT, Apache-2.0, BSD, ISC or Zlib
  (`deny.toml`, decision 0005). Check a crate's current licence, version and
  maintenance on crates.io before adding it, then record it in decision 0006.
  Bundled sounds must be CC0 and listed in `THIRD_PARTY.md`. Never use the
  ASIO SDK.
- **Layers.** A crate depends only on lower layers (decision 0003,
  `xtask/src/rules.rs`).
- **Names in one place.** The product name, file extension and app
  identifiers come from `daw-ident`; never write them as literals elsewhere
  (a test checks).
- **User-facing text in one place** (in `daw-ui`), English only for now.
- **Everything the user does is undoable**; the project model is the single
  source of truth for UI and engine.
- **Musical time is integer ticks** (`2_882_880` per quarter note, decision
  0007). Never floating-point beats.
- **The engine runs headless**; offline export uses the same graph as live
  playback.
- Every Rust file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- `unsafe` is denied by default; a module that needs it opts in with a reason,
  and every unsafe block has a `// SAFETY:` comment.
- Do not put any AI model identifier in commits, code or docs.

## How to work

- Small commits with clear messages. Run `cargo xtask ci` before pushing.
- Before building a major screen, think through two or three distinct
  concepts, pick the strongest, and record the rejected ones and why.
- You cannot hear or see the way the owner does. Verify with automated tests
  and offline renders (signal present, pitch, timing, no clipping) and with
  screenshots, and say exactly what you could not verify. **Never report
  something as working if you have not run it.**
- End every milestone with a build the owner can run and a short checklist of
  things to try. Explain progress in plain language: what works, what to try,
  what is still rough.
- Keep this file, `ROADMAP.md` and `README.md` current.

## Testing without hardware (Linux containers)

- **Virtual screen:** `xvfb-run -a -s "-screen 0 1600x1000x24" <command>`;
  screenshots with `import -window root shot.png` (ImageMagick).
- **Virtual sound card:** start a private PipeWire with a null sink, then use
  the normal default device:

  ```bash
  export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
  dbus-daemon --session --fork --address=unix:path=$XDG_RUNTIME_DIR/bus
  export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
  pipewire & sleep 1; wireplumber & sleep 2
  pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=test-speakers media.class=Audio/Sink object.linger=true audio.position=[FL FR] }'
  ```

  (packages: `pipewire pipewire-alsa pipewire-jack wireplumber dbus`).
- **Driving the program:** with the virtual sound card running, start
  `target/debug/daw-app` under `xvfb-run`, find the window with
  `xdotool search --name <product name>`, then click and type with
  `xdotool mousemove X Y click 1` / `xdotool key ctrl+r`, take screenshots
  with `import -window root`, and record what it plays with
  `pw-record --target test-speakers -P '{ stream.capture.sink=true }' out.wav`.
  Point `HOME`, `XDG_CONFIG_HOME` and `XDG_DATA_HOME` at a scratch folder so
  settings and recovery copies do not leak between runs. `--silent` starts
  the program without opening a sound device.
- **Interface tests** (`crates/daw-ui/tests/interface.rs`) use
  `egui_kittest`: find controls by their accessible label, click them, send
  keys. Dialogs re-centre for a few frames after opening; run a few frames
  before clicking inside one.
- **Reference plugins** (test fixtures only, never linked or bundled): Surge
  XT 1.3.4 from `github.com/surge-synthesizer/releases-xt` releases
  (`surge-xt-linux-1.3.4-pluginsonly.tar.gz`, `surge-xt-win64-1.3.4-pluginsonly.zip`).
  CI downloads them in `spikes.yml`.
- Network access in the development container is restricted: crates.io,
  GitHub (git, raw files and release downloads) and the Ubuntu archive work;
  many other sites do not.

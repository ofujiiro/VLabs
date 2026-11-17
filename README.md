# VLabs

An easy, open-source music workstation for Windows and Linux.

VLabs aims to feel as approachable as a simple audio editor and as musical as
a full studio: build patterns with a step sequencer, write melodies and chords
in a piano roll, arrange patterns and audio clips on a timeline, mix with an
effect chain per track, record notes and audio. Advanced features, presented
as simply as possible.

It works on a plain computer: built-in speakers or headphones, a mouse or
trackpad and a typing keyboard. A MIDI keyboard or an audio interface is used
automatically when you plug one in, but nothing extra is ever required.

## Status

**Pre-alpha.** Roadmap phases 1 ("first sound") and 2 ("making a beat")
work: make a beat on the step grid with a recorded acoustic kit or an
electronic kit, write melodies in the piano roll with the bundled piano,
arrange patterns on a timeline, mix with volume, pan, mute, solo and an
effect chain per track (EQ, compressor, reverb, delay, limiter), and export
the song as a WAV file. Play from the computer keyboard or a MIDI keyboard,
record with a metronome and count-in, undo anything, save and open songs.
Your work is saved automatically and offered back after a crash. See
[ROADMAP.md](ROADMAP.md) for what is done, what to try and what comes next.

## Building from source

1. Install Rust with [rustup](https://rustup.rs). The right Rust version is
   installed automatically the first time you build.
2. On Linux, install the system libraries (Ubuntu/Debian names):

   ```bash
   sudo apt install build-essential pkg-config libasound2-dev libjack-jackd2-dev libudev-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev
   ```

3. Build and run:

   ```bash
   cargo run --release -p daw-app
   ```

## Plugins on Linux

VLabs will host CLAP and VST3 plugins (roadmap phase 3) and look for them in
the standard folders, including `~/.clap` and `~/.vst3`. Windows-only plugins
can be used on Linux with [yabridge](https://github.com/robbert-vdh/yabridge):
once yabridge has set them up in those folders, VLabs sees them like native
plugins. Most plugin editor windows need X11; on a Wayland desktop they run
through XWayland, which is installed by default on common distributions.

## Licence

VLabs is free software, licensed under the
[GNU General Public License, version 3 or later](LICENSE). Commercial
licences are also available from the copyright holder.

Bundled sounds and fonts, and their licences, are listed in
[THIRD_PARTY.md](THIRD_PARTY.md).

## Contributing

Contributions are welcome under the [contributor licence agreement](CLA.md),
which keeps dual licensing possible. Developer notes are in
[CLAUDE.md](CLAUDE.md) and [docs/](docs/).

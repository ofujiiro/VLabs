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

Later, after the first full version: singing voices, in the spirit of
OpenUtau. Notes with lyrics sung by open voicebanks you install (classic
UTAU voicebanks such as Kasane Teto's, and DiffSinger and ENUNU voices),
managed and tuned inside VLabs (see [ROADMAP.md](ROADMAP.md)).

## Status

**Pre-alpha, feature-complete for v1 (phase 6).** Make a beat on the step
grid with a recorded acoustic kit or an electronic kit, write melodies in
the piano roll with the bundled piano, the built-in synthesizer (two
oscillators, a filter, envelopes, an LFO and presets) or a CLAP or VST3
instrument, record your voice or guitar from the computer's microphone,
bring in WAV, FLAC, MP3, Ogg and MIDI files, cut, trim and fade audio clips
right on the timeline, arrange everything on one timeline (right-click a
track to place any of its patterns), mix with volume, pan, mute, solo and
an effect chain per track (EQ, compressor, reverb, delay, limiter, and
plugin effects), make any setting change over the song with automation
lanes, and export the song as WAV or FLAC, as a MIDI file, or as a
DAWproject file that Bitwig Studio, Studio One and Cubase open. Open
DAWproject and MIDI files from other programs. Play from the computer
keyboard or a MIDI keyboard, record notes with a metronome and count-in,
undo anything, save and open songs (their sound inside the song file).
Give the song a key and the piano roll shows its notes, keeps new notes in
it and places whole chords with one click; transpose by intervals; change
the tempo (suddenly or gradually) and the time signature anywhere; write
triplets, quintuplets and septuplets; swing a pattern. Your work is saved
automatically, even a recording in progress, and offered back after a
crash. A Windows installer and a Linux AppImage are built for each
milestone, with a user guide (F1 in the program, or
[docs/user-guide.md](docs/user-guide.md)). See [ROADMAP.md](ROADMAP.md) for
what to try and what comes next.

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

4. To make the release folder (`dist/`, with the licence notices, the user
   guide and the icons), the Windows installer (needs NSIS) or the Linux
   AppImage (needs appimagetool): `cargo xtask dist`, then
   `cargo xtask installer` or `cargo xtask appimage`.

## Plugins

VLabs hosts CLAP and VST3 instruments and effects. It looks for them in the
standard folders when it starts, remembering what it learnt so later starts
are quick:

- Linux: `~/.clap`, `~/.vst3`, `/usr/lib/clap`, `/usr/lib/vst3`,
  `/usr/local/lib/clap` and `/usr/local/lib/vst3`;
- Windows: `Common Files\CLAP` and `Common Files\VST3` (in Program Files),
  and the same two folders under `%LOCALAPPDATA%\Programs\Common`;
- anywhere listed in the `CLAP_PATH` environment variable.

A plugin must be built for the system it runs on: a plugin made for Windows
does not load on Linux, and the other way round. The instrument chooser lists
the plugin files it found but cannot use, and says why. Windows-only plugins
can be used on Linux with
[yabridge](https://github.com/robbert-vdh/yabridge): once yabridge has set
them up in those folders, VLabs sees them like native plugins. Most plugin
windows need X11; on a Wayland desktop they run through XWayland, which is
installed by default on common distributions.

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

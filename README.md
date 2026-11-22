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

After the first full version: singing voices, in the spirit of OpenUtau.
The first two parts are built: notes with lyrics sung by classic UTAU
voicebanks you install (such as Kasane Teto's), managed inside VLabs, and
expression: the sung pitch drawn through the notes and shaped by hand
(bends, scoops, overshoots, vibrato), each note's loudness, colour and
breath. UST and USTX projects and DiffSinger voices come next (see
[ROADMAP.md](ROADMAP.md)).

## Download

**[VLabs 1.0](https://github.com/ofujiiro/VLabs/releases/latest)**: a
Windows installer (Windows 10 or 11) and a Linux AppImage (most
distributions from 2024 on). Its notes say how to install it; press F1 in
the program for the user guide.

## Status

**Version 1.1.** Make a beat on the step
grid with a recorded acoustic kit or an electronic kit, write melodies in
the piano roll with the bundled piano, the built-in synthesizer (two
oscillators, a filter, envelopes, an LFO and presets) or a CLAP or VST3
instrument, record your voice or guitar from the computer's microphone,
bring in WAV, AIFF, FLAC, MP3, Ogg and MIDI files, cut, trim and fade audio clips
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
the tempo (suddenly, or gradually with the shape of the change you want)
and the time signature anywhere; write
triplets, quintuplets and septuplets; swing a pattern. Your work is saved
automatically, even a recording in progress, and offered back after a
crash. Add plugins by dropping a downloaded `.vst3`, `.clap` or `.zip` on
the window, manage them in the Plugins window (Windows plugins on Linux are
set up through Wine and yabridge), browse and hear folders of samples in
the sound library and drag them into the song, and play SoundFont (`.sf2`)
and SFZ (`.sfz`) instruments, however large (SFZ instruments are read in
the background). Make a drum kit of your own sounds (a custom kit) and build
beats with it on the step grid. Add classic UTAU voicebanks and give notes
lyrics: a voice track sings them, and in its piano roll you shape how:
bend the sung pitch where you click, add vibrato, make notes louder,
lighter, darker or breathier; the voicebank's picture shows faintly on its
track. Shape how an instrument's notes start and end (a slow attack, a long
ring, or every note cut off as its key comes up: a "release cut" piano in
one click), and open any sound clip in the sound editor to cut it exactly,
fade it, play it backwards, change its pitch or make it as loud as it can
be (new in 1.1). Each release has a Windows installer and a Linux AppImage, with a
user guide (F1 in the program, or [docs/user-guide.md](docs/user-guide.md)).
See [ROADMAP.md](ROADMAP.md) for what to try and what comes next.

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

VLabs hosts CLAP and VST3 instruments and effects. **Plugins…** in the menu
opens the Plugins window, which lists them and adds new ones: drop a plugin
file, its folder or the `.zip` it came in on the window (or use the
window's buttons) and VLabs copies it into a plugin folder of its own,
which it always searches. Plugins added this way can be removed there too,
and folders of your own can be added to the search. VLabs also looks in
the standard folders when it starts, remembering what it learnt so later
starts are quick:

- Linux: `~/.clap`, `~/.vst3`, `/usr/lib/clap`, `/usr/lib/vst3`,
  `/usr/local/lib/clap` and `/usr/local/lib/vst3`;
- Windows: `Common Files\CLAP` and `Common Files\VST3` (in Program Files),
  and the same two folders under `%LOCALAPPDATA%\Programs\Common`;
- anywhere listed in the `CLAP_PATH` environment variable.

A plugin must be built for the system it runs on: a plugin made for Windows
does not load on Linux, and the other way round. The Plugins window and the
instrument chooser list the plugin files that cannot be used, and say why.
Windows-only plugins can be used on Linux through Wine and
[yabridge](https://github.com/robbert-vdh/yabridge): with both installed,
VLabs sets up each Windows plugin added in the Plugins window itself (and
plugins yabridge set up before appear too). VST2 plugins (`.dll`) are not
supported. Most plugin
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

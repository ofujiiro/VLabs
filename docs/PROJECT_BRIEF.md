# Project brief: VLabs, an easy, open-source DAW in Rust

You are going to design and build VLabs, a desktop music-creation application (a DAW), with me. I am the project owner and I direct the product. I do not read or write Rust, so you write all the code, and you are responsible for its quality, its tests, and for explaining what you did in plain language.

Read this whole brief, then start building. Every product question I could answer in advance is answered here, so do not open with questions. Where the brief is silent, make the choice yourself, record it, and keep going.

## What we are building

A DAW for Windows and Linux that feels as approachable as Audacity and as musical as FL Studio.

The guiding idea: **advanced features, presented in the simplest way possible.** The person I am building for knows music theory well. They must never hit a wall because the editor is too basic, and they must never have to fight the interface to do something ordinary. Simplicity comes from good defaults, clear layout and progressive disclosure, never from removing capability.

The workflow is pattern-based first, with audio on the same timeline. The user must be able to:

- Build patterns quickly with a step sequencer.
- Write melodies and chords in a piano roll.
- Arrange patterns and audio clips on a timeline.
- Mix, with an effect chain per track.
- Record, import, cut, trim and fade audio clips directly on the timeline.
- Record notes live while the song plays.

This list describes capabilities, not a layout. FL Studio and Audacity are where the idea came from, but VLabs should not try to resemble them or any other DAW.

The test I care about most: a new user opens the app and hears a note they placed themselves in under a minute, with no audio setup, no plugin install and no manual.

## Design direction

- **Exercise abstract thinking.** Start from what a musician is trying to do (sketch an idea, hear it, shape it, arrange it, finish it) and design the interface from those tasks, not from the panels other DAWs happen to have. If a familiar DAW convention is the best answer, use it. If a better one exists, use that, and tell me why.
- **Spacious and calm.** Generous spacing, few things on screen at once, a clear visual hierarchy. The interface must be easy to apprehend at first glance, easy to use, and easy to understand.
- **Apply established design principles deliberately:** hierarchy and grouping (Gestalt), large and close targets for frequent actions (Fitts's law), few choices at each step (Hick's law), progressive disclosure, visible affordances, immediate feedback for every action, consistency, forgiving undo, readable type and accessible contrast.
- **Advanced features stay one step away, never hidden.** An expert should reach anything quickly; a newcomer should never be confronted with all of it at once.
- **Original identity.** Do not copy another product's visual design, icons or product-specific names.
- **Theme:** ship a dark and a light theme, follow the system setting by default, and let the user switch.
- Before building any major screen, think through two or three distinct concepts, pick the strongest yourself, and note in the decision record what you rejected and why. I will react to the running build, not to proposals.

## No hardware required

VLabs must be fully usable on a plain computer: built-in speakers or headphones, a mouse or trackpad, and a typing keyboard. Extra hardware improves the experience but is never needed.

- Sound works on the default output device with no configuration. No special driver is required.
- The computer keyboard plays notes like a piano, and an on-screen keyboard can be clicked.
- Live note recording works from the computer keyboard as well as from a MIDI keyboard, with a metronome, a count-in and optional quantization.
- Audio recording works from the built-in microphone.
- A MIDI keyboard or audio interface is detected and used automatically when plugged in, including while the app is running.
- Every action is reachable with the mouse alone, and the common ones with the keyboard alone.

## Decisions already made

I researched these in November 2026. Treat them as settled unless you find a concrete blocker, and in that case stop and tell me before changing course. Crate names below are candidates: check each one's current status, maintenance and license yourself before adopting it, because this ecosystem moves quickly and your memory of it may be out of date.

**Language: Rust (stable toolchain).** One codebase for both platforms, no garbage collector on the audio thread, and other Rust DAWs already host CLAP and VST3 plugins on Linux and Windows, so the approach is proven. The known weak spot is that plugin hosting in Rust is younger than in C++/JUCE, which is why it gets an early risk spike below.

**Platforms:** Windows 10/11 x64 and Linux x64 (PipeWire, ALSA, JACK). Use Windows 11 and a current Ubuntu LTS as the reference systems for CI and testing. macOS is not a target, but avoid choices that would make it impossible later.

**Name:** the product is called VLabs and its project files end in `.teto`. Define the product name, the file extension and the application identifiers in one place, so that any of them can be changed later with a one-line edit.

**Plugin formats in v1: VST3 and CLAP**, instruments and effects, with their own editor windows.
- CLAP: look at `clack-host` (safe wrappers over `clap-sys`).
- VST3: the SDK has been MIT-licensed since version 3.8, so bindings can be used freely. Evaluate the existing Rust bindings.
- Scan plugins in a separate process so one broken plugin cannot crash the app or block startup.
- Design the host layer so a plugin can later run out-of-process without changing the engine.
- On Linux, scan the standard `~/.vst3` and `~/.clap` folders. That makes Windows plugins bridged by yabridge appear as native ones with no work on our side. Document this for users rather than building our own Wine bridge.
- Plugin editor windows on Linux generally need X11; plan for XWayland and verify the current state.
- Use free plugins that exist as both CLAP and VST3 on Windows and Linux as the reference test plugins, starting with Surge XT, and add a second instrument and one effect of your choice. They are test fixtures only; never link or bundle their code.

**Not in v1, but do not design them out:** VST2 (needs clean-room headers and a licensing check, since Steinberg no longer issues VST2 licenses), a 32-bit plugin bridge, LV2, a staff-notation view, microtonal tunings. FL Studio's native plugins cannot be loaded by any other host; do not attempt it.

**Built-in instruments and effects in v1**, so the app is useful with zero plugins installed:
- A SoundFont (.sf2) player with a bundled piano.
- A drum sampler tied to the step sequencer, with a bundled starter kit.
- A simple subtractive synth (oscillators, filter, envelopes).
- EQ, compressor, reverb, delay and limiter.

**Music-theory and rhythm tools in v1:**
- Key and scale highlighting, optional snap-to-scale, chord stamps, inversions, transposition by interval.
- Tempo and time-signature changes anywhere in the song, odd meters, tuplets, swing, and patterns of different lengths playing against each other.
- Represent musical time exactly (rational values or high-resolution integer ticks, never floating-point beats) so tuplets and polyrhythms stay precise. Keep the note model open enough for per-note pitch offsets later.

**Project file format: our own, plus open interchange.**
- Native format, with the extension **`.teto`**: a single-file container holding a human-readable, versioned project description, the audio files and the saved plugin states. Write migration tests from the first version.
- Import and export **DAWproject** (`.dawproject`), the open exchange format supported by Bitwig, Studio One and Cubase. It is explicitly not meant to be a DAW's native format, which is why it is our interchange format and not our save format. Keep our data model easy to map onto it.
- Import and export Standard MIDI Files. Import WAV, FLAC, MP3 and OGG. Export at least WAV and FLAC.
- Autosave and crash recovery are required, not optional.

**GUI: default to `egui` (eframe + wgpu),** because the piano roll, timeline and mixer are custom-drawn surfaces and it is the most widely used Rust GUI library. Keep all UI code in its own crate behind a clear boundary. If the UI spike below fails, propose `iced` as the alternative.

**Keyboard shortcuts: our own scheme,** not another DAW's. Keep the universal conventions (Ctrl+Z, Ctrl+S, Ctrl+C/V, Space for play/stop), make the rest consistent and easy to guess, show every shortcut in tooltips and menus, and let the user rebind them. Shortcuts must not clash with playing notes on the computer keyboard.

**Interface language: English only.** Keep user-facing text in one place so translation stays possible later, but do not build translation support now.

**Hosting: GitHub, in a private repository for now,** with GitHub Actions for CI. Write the repository so it can be made public later without cleanup: no secrets in history, license files and a README present from the start.

**License: GPL-3.0-or-later, with dual licensing.** I am the sole copyright holder so that I can also sell commercial licenses. This has consequences you must enforce:
- Every dependency must be permissively licensed (MIT, Apache-2.0, BSD, ISC, Zlib). If a crate has any other license, do not use it; find an alternative or write the piece yourself. No GPL-only or LGPL crates, because I cannot relicense someone else's copyleft code. Enforce this in CI with `cargo-deny`.
- Bundled sounds (the piano SoundFont, the drum kit) must be CC0. Record each one's source and license in a `THIRD_PARTY.md` file.
- Add a contributor license agreement file to the repository from the start.
- Do not use the ASIO SDK. The standard Windows audio system is enough for v1.

## Engineering rules

**The audio thread is sacred.** No allocation, no locks, no file or network access and no logging on the real-time thread. The UI and the engine communicate through lock-free queues. Add a debug-build check that fails loudly when the audio thread allocates.

**The engine runs headless.** It must work without a window and without a sound card, so tests can load a project, render it offline to a buffer and inspect the result. Offline export uses the same processing graph as live playback.

**Everything the user does is undoable,** and the project model is the single source of truth that both the UI and the engine read from.

**Structure it as a Cargo workspace** with separate crates for the project model, the engine, plugin hosting, built-in DSP, the UI and the application shell. Adjust the split if you find a better one, and tell me why.

**CI from day one:** build, test, lint and license-check on both Windows and Linux for every change.

## How to work with me

- Your job is to build. Decide technical and design questions yourself and record each significant one in `docs/decisions/` with a short reason. I will correct course by reacting to what you built.
- Stop and ask only when you are blocked by something that only I can do or decide: an account or credential, spending money, a legal choice, or a finding that makes one of the settled decisions in this brief unworkable.
- Explain progress in plain language. I want to know what now works, what I should try, and what is still rough.
- End every milestone with a build I can run and a short checklist of things to try.
- You cannot hear audio or see the screen the way I do. Verify what you can with automated tests and offline renders (signal present, correct pitch, correct timing, no clipping), take screenshots of the UI where possible, and tell me exactly what you could not verify so I can check it by ear and eye. Never report something as working if you have not run it.
- Keep `CLAUDE.md`, `ROADMAP.md` and a user-facing `README.md` current so any future session can pick up where the last one stopped.
- Work in small commits with clear messages.

## Proposed roadmap

Follow this order. If you find a strong reason to change it, change it and record why.

0. **Foundations and risk spikes.** Repository, license files, CI on both systems, then three throwaway spikes: (a) low-latency audio output on Windows and Linux; (b) load one CLAP and one VST3 instrument, send it notes, get audio back, open its editor window and save and restore its state, on both systems; (c) a piano roll with 10,000 notes that scrolls and zooms smoothly. Write the results into a decision record. If all three pass, continue straight into phase 1. If one fails, stop and tell me what failed and what you propose instead.
1. **First sound.** Project model, undo, save and load, transport, engine core, SoundFont piano, piano roll, playing notes from the computer keyboard, live note recording with metronome and count-in.
2. **Making a beat.** Step sequencer, drum sampler, patterns, arrangement timeline, mixer, built-in effects, WAV export.
3. **Plugins.** Scanner, plugin browser, VST3 and CLAP instruments and effects, editor windows, state saving, parameter automation.
4. **Audio.** Recording from the built-in microphone or an interface, audio clips on the timeline, cut, trim, fades, waveform display, file import.
5. **Theory and rhythm.** Scale and chord tools, tempo and time-signature track, tuplets, swing.
6. **Completing v1.** Subtractive synth, DAWproject and MIDI import and export, autosave polish, installers (Windows installer, Linux AppImage), user guide.

## Where to start

Begin with phase 0 now: create the repository structure, the license files, `CLAUDE.md`, `ROADMAP.md`, the first decision records and the CI setup, then run the three spikes. As you adopt each crate, confirm its current version, maintenance status and license from its own documentation instead of relying on memory.

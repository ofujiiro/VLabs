# daw-formats — reading and writing files

Translates between files on disk and the project data in `daw-model`.

## What belongs here

- **The native project format.** One file holding a human-readable,
  versioned description of the project, its audio files and the saved state
  of its plugins. The file extension comes from `daw-ident`.
- **Migrations.** Opening a project saved by any older version.
- **DAWproject** import and export, the open exchange format.
- **Standard MIDI Files**, import and export.
- **Audio files.** Import WAV, FLAC, MP3 and OGG; export at least WAV and
  FLAC.
- **Instruments people have:** SFZ instruments read, with their sounds,
  into `daw-dsp`'s sampler banks (`sfz`, decision 0024); folders listed for
  the sound library (`library`).
- Writing files safely, so autosave and crash recovery can never leave a
  half-written project behind. Deciding *when* to autosave is the
  application's job (`daw-app`).

## Rules

- **Migration tests from the first version.** Every released version of the
  format keeps sample files in this crate's tests, and a test proves the
  current code still opens each of them.
- Reading and writing files never happens on the audio thread.
- No dependency on the engine or the interface. This crate knows the project
  data and files, and uses `daw-dsp` only for the sampler bank SFZ
  instruments are read into.
- Files from outside are untrusted: a damaged or hostile file must produce an
  error message, never a crash.

## Roadmap

Phase 1 (save and load), phase 2 (WAV export), phase 4 (audio import:
`audio` reads WAV, FLAC, MP3 and Ogg Vorbis; the song file keeps its audio
files as entries `audio/<name>`, format version 4), phase 6 (DAWproject,
MIDI files, FLAC export), then SFZ instruments (format version 7).

# 0020. Songs in and out: MIDI files, DAWproject, FLAC, and where they live in the menus

- **Status:** accepted, November 2026 (extends decisions 0013, 0017 and 0018)

## The tasks

A song made in VLabs must be able to leave it, and work made elsewhere
must be able to come in. People do this to **finish a song in another
program** (or start one there and continue here), to **bring in notes**
(a MIDI file of a chord progression, a drum groove, a melody), to **hand
notes to someone else** (any program reads MIDI files) and to **share the
result** as a sound file, uncompressed or losslessly compressed (FLAC).

The brief settles the formats: DAWproject and Standard MIDI Files in and
out, WAV and FLAC out. This record says how each maps onto the song, and
how a person reaches them.

## Where they live

**A. One way in for a whole song, one for parts, one way out (chosen).**

- **Open…** (Ctrl+O) opens a VLabs song, and also another program's song:
  a `.dawproject` file or a MIDI file. It opens as a new, unsaved song
  named after the file; Save asks where to save it as a VLabs song, so the
  original file is never overwritten.
- **Import…** (Ctrl+I, formerly "Import audio…") adds to the song: sound
  files as before, and MIDI files, whose tracks are added at the play
  position (or, dropped on the song view, where they land), keeping the
  song's tempo. Dropping a VLabs or DAWproject file on the window opens it.
- **Export…** (Ctrl+E) opens one dialog with three choices, each with a
  sentence saying what it is for: *a sound file* (WAV or FLAC, CD or
  studio quality), *a project for another music program* (DAWproject:
  tracks, notes, sound, mixer, plugins, automation) and *the notes as a
  MIDI file* (any program; notes and tempo only).

**B. One menu entry per format** ("Export as WAV…", "Export as FLAC…",
"Export as MIDI file…", "Export as DAWproject…", and as many to import).
Direct, but it doubles the menu and puts jargon in front of every
newcomer. *Rejected* (Hick's law; the dialog explains what the words
mean).

**C. The format as the file type in the system's save dialog.** Few
screens, but the file-type list is easy to miss, differs between desktops
(portal, zenity, Windows) and has no room for the quality choice or an
explanation. *Rejected.*

## Standard MIDI Files

Written and read by our own code (decision 0005 rejected `midly` for its
licence; the format is small).

**Export** writes a format 1 file at 960 ticks per quarter note, the most
widely supported resolution. Triplets and quintuplets are exact at 960;
septuplets and finer tuplets are rounded to the nearest MIDI tick (under
0.3 ms at 120 BPM). The first track holds the song's title, tempo changes,
time signatures and key signatures (the key's signature for major and
minor keys; a mode is written as its parent major key's signature). Tempo
glides become steps every sixteenth note, each step's tempo chosen so that
it lasts exactly as long as the glide over it (decision 0018), so the
song's timing at every step matches. Each instrument track becomes a MIDI
track named after it, on its own channel (drums on channel 10, with their
General MIDI keys, which our kits already use), with a program change
(piano: Acoustic Grand Piano; the synth: a General MIDI sound near its
preset). The notes are the ones the song plays: every clip's repeats,
cut at the clip's end, swung, from muted tracks too (a muted track is
still part of the song). Audio tracks and plugin settings are left out:
MIDI files hold notes.

**Import** reads format 0, 1 and 2 files (resolution in ticks per quarter
note or in frames per second), running status, and any number of tracks
and channels. Each channel of each track that has notes becomes a VLabs
track with one pattern holding its notes (as long as its notes, in whole
bars) and one clip playing it: channel 10 becomes a drum track (acoustic
kit); General MIDI synth sounds (synth leads, pads, basses) become the
synth with a matching preset; everything else becomes the piano. Opened
as a song, the file also brings its tempo changes, time signatures (at
the nearest bar line) and key signature; imported into a song, only its
notes come in. Damaged and hostile files give a message, never a crash:
sizes are checked, events are limited, unmatched note ends are ignored,
and notes still held at the end of a track end there.

## DAWproject

DAWproject 1.0 (Bitwig's open format, MIT licensed; a ZIP holding
`project.xml`, `metadata.xml`, audio files and plugin settings), read with
`quick-xml` and written by our own code.

**What maps directly:** the tempo (and its changes: a sudden change is a
"hold" point, a glide a "linear" one, exactly), time signatures,
instrument tracks (`notes`) and audio tracks (`audio`), each track's
volume, pan, mute and solo, the master's volume and effects, note clips
with their notes, audio clips (start, length, the part of the file they
play, fades in seconds, the file itself in the ZIP), CLAP and VST3 plugins
with their settings (`.clap-preset`, and `.vstpreset` built from the
plugin's two saved blocks), and automation lanes of volume, pan, built-in
effect settings, synth settings and plugin parameters.

**Pattern clips are written as the notes they play** (every repeat inside
the clip, swung, cut at the clip's end), not as a loop: every importer
handles plain notes, while support for looped clips varies. Reading, a
looped clip is expanded the same way, and clips on a track with identical
notes share one pattern again, so a round trip through another program
keeps the song's patterns where it can.

**VLabs's own instruments and effects** are written as devices of
VLabs's own (deviceVendor and deviceID from `daw-ident`), with their
exact settings in a small JSON file, so VLabs reads them back unchanged.
The EQ, compressor and limiter are also written as DAWproject's standard
Equalizer, Compressor and Limiter, so other programs can approximate them,
and those standard devices from other programs become VLabs's EQ,
compressor and limiter when read. The piano, the drums, the synth, the
reverb and the delay have no standard equivalent: other programs keep the
track and its notes and report a missing device.

**What is lost, and said so:** the song's key (DAWproject has no key);
an audio clip's own volume (DAWproject clips have none); stretched audio
read from another program (VLabs plays sound at its own speed, decision
0017); group and effect tracks, sends and the noise gate (VLabs has none
yet: their tracks' content is kept, their routing is not); VST2 and AU
plugins; note expressions. After reading, the status line says what was
left out.

VST3 class ids: on Windows the plugin's raw class id is in COM byte order,
while DAWproject (like `.vstpreset`) uses the canonical order; the
conversion happens at the format's edge.

## FLAC export

The export dialog's sound file is WAV or FLAC, each at CD quality (44.1
kHz, 16 bits, with dither) or studio quality (48 kHz, 24 bits). FLAC is
encoded with `flacenc` (decision 0006) frame by frame as the song renders,
so a long song never has to fit in memory; the file starts in a temporary
place and replaces the target only when complete, like WAV.

## Consequences

- `daw-formats` gains `midi`, `dawproject` and `flac`; the interface's
  Open, Import and Export grow as above, and the project-file filter of
  the Open dialog lists all three song formats.
- The notes a clip plays are worked out in one place in the model
  (`Pattern::played_in`), used by the engine and by both exporters, so
  what is exported is what is heard.
- Songs recorded in VLabs still keep their recordings as WAV inside the
  song file (decision 0017); FLAC inside the song file is for later.

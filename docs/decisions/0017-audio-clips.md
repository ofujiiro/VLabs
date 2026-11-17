# 0017. Audio: clips on the song timeline, recorded and imported sound

- **Status:** accepted, November 2026 (extends decisions 0011, 0014 and 0016)

## The tasks

Phase 4 adds four things people do with sound that was not made from notes:
**capture an idea** with a microphone (sing a line, play a guitar part),
**bring sound in** from elsewhere (a loop, a sample, a vocal), **shape it**
(cut away what is not wanted, trim, fade in and out, make it quieter), and
**see it** (where the loud parts and the gaps are).

## Concepts considered

**A. Audio clips on the song timeline, shaped in place (chosen).** Recorded
and imported sound becomes clips on *audio tracks* in the song view, drawn
as waveforms. Everything is done to a clip where it sits: drag it to move
it, drag either end to trim it, drag the small handles at its top corners
to fade it in or out, split it at the mouse (its menu) or at the play
position (a shortcut). Recording follows phase 1's rule: Record records into
the track in focus, so with an audio track in focus it records sound, and
the new clip grows on the timeline as it records, with its waveform.
Importing is dragging a file onto the song view, or a menu entry.

**B. An audio editor as a focus surface,** opened by double-clicking a clip,
as the piano roll is for a pattern: a large waveform with tools for cutting,
fading, gain and normalising. *Rejected as the way to work:* the brief asks
for cutting, trimming and fading directly on the timeline, and switching
screens for every cut is slow and loses the song around the clip. Zooming
the song view gives most of the precision. A detailed editor can be added
later without changing clips.

**C. A tool palette** (pointer, knife, fade, glue), the classic DAW answer.
*Rejected:* modes cause mistakes (the knife still selected when you meant
to move), and every action here has a visible handle or a menu entry, so no
mode is needed.

For recording, **arm buttons on every track** were considered and rejected:
the focused track is already highlighted and is where notes record; one
rule for both is easier to learn.

## What the choice means

- **Audio tracks.** "+ Audio" under the track list adds one. An audio track
  has no instrument: it plays its clips through its mixer strip and effect
  chain like any track. Audio clips go on audio tracks, patterns on their
  own instrument track; a clip can move to another audio track.
- **Recording.** Focus an audio track, press Record: after the count-in,
  sound is recorded from the play position until Stop; the take is one undo
  step. While recording sound the song plays on through its loop (one take
  is one clip). A new take, or an imported file, replaces whatever part of
  other clips it covers on its track. There is no listening to the input
  through the program: with a built-in microphone and speakers it howls,
  and the delay would put a singer off; headphones on the computer or the
  interface give the sound back with no delay.
- **The input device** is the system's default input, opened when recording
  starts (the count-in gives it time) and closed a while after, so the
  system's "microphone in use" sign is not lit all the time. When nothing
  comes in, the person is told.
- **Shaping.** Trimming works on pattern clips too (the left end moves into
  the pattern). Fades use an equal-power curve; every clip edge also gets a
  fade of about a millisecond, so cuts never click, except where a clip
  simply continues the sound of the one before (a split). A clip's volume
  is in its menu.
- **Import.** WAV, FLAC, MP3 and Ogg Vorbis, by dropping files on the song
  view (on a track and at a time, or below the tracks for a new track) or
  with "Import audio…" (Ctrl+I), which places the file at the play position
  on a new audio track or the audio track in focus.

## How sound is kept

- **Time.** A clip starts on the timeline in ticks, like every clip; what it
  plays is counted in the file's own frames (`offset`), and it plays at the
  file's natural speed. Changing the tempo keeps every audio clip's length
  in seconds; its start stays on its beat. There is no time-stretching in
  v1. Fades are in seconds.
- **The model** (`daw-model`) holds, per audio file, a name for people, the
  name it is stored under, its length in frames, its sample rate and its
  channels, and per clip the file, the offset, the volume and the fades.
  Sound itself never enters the model: it is data the undo history copies.
- **The song file** (format version 4) keeps each audio file as an entry
  `audio/<name>` of the container, stored as it came: recordings as 24-bit
  WAV, imported files in their own format (an MP3 stays an MP3). Entries are
  stored without compression: audio formats are compressed already, or
  barely shrink.
- **While a song is open,** its audio files live as files in a folder of
  this running copy of the program (inside its recovery folder, decision
  0013): recordings are written there as they are made, imports copied
  there, a song's files extracted there when it opens. Saving copies them
  into the song file; autosave copies leave them where they are, so a crash
  keeps them and a recovered song has its sound. A file name is never
  reused for other sound in that folder.
- **Playing** reads sound from memory: each file is decoded once in the
  background, converted to the engine's rate (our own windowed-sinc
  converter) and given to the engine, with a summary of its loudness for
  drawing waveforms. That costs about 23 MB per minute of stereo sound;
  streaming long files from disk can come later behind the same interface.
- **Recording** goes through the engine, so takes line up with the song: the
  input device hands its samples to the engine through a lock-free queue;
  while a take runs, the audio thread passes them on in fixed-size chunks,
  marked with the take's number, to a recorder thread that writes the file
  and the waveform summary. Takes are saved at the input device's own rate
  (Windows opens inputs only at their native rate) and converted like
  imports. The take is moved earlier by the delay the devices report (sound
  out plus sound in), so what was played along lines up with what was
  heard.

## Alternatives considered

- **Audio clips on any track** (as some pattern-based programs allow):
  rejected, because a track is a mixer strip with an instrument; a vocal on
  the piano's track would go through the piano's effects.
- **Clip content in ticks** (audio length following the tempo): without
  time-stretching, a faster tempo would cut the end of every recording.
- **Everything stored as FLAC:** smaller recordings, but saving would have
  to wait for encoding. Recordings can switch to FLAC later without a new
  format version, since the container may hold any supported format.
- **`rubato` for rate conversion** (planned in decision 0006): converting a
  whole file offline is a small, well-understood job; our own windowed-sinc
  converter keeps six crates out of the build. `symphonia` stays out
  (MPL-2.0, decision 0005).

## Consequences

- One timeline holds notes and sound; nothing new to learn to arrange
  either.
- Long recordings cost memory while the song is open.
- A tempo change leaves imported loops and recordings at their own speed:
  they drift from the beat. People are told so by the waveform; stretching
  is for later.
- Device delays are measured from what the system reports, which varies
  between systems; a setting to adjust it by hand may be needed.

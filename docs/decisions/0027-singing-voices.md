# 0027. Singing voices: voicebanks, lyrics on notes, a renderer of our own

- **Status:** accepted, November 2026 (the owner's request; builds on
  decisions 0005, 0014, 0017, 0023 and 0024). Milestone 7b is designed in
  decision 0028, which moves the vocoder to a milestone of its own.

## The task

"Sing from the piano roll": give notes lyrics and have a voicebank sing
them, with the voice managed and tuned inside VLabs, in the spirit of
OpenUtau. The voicebanks people have are mostly **classic UTAU
voicebanks** (WAV recordings, an `oto.ini` saying where each syllable is in
them, a `character.txt`, a readme with the creator's terms), most of them
Japanese, such as the many Kasane Teto voicebanks; newer ones are
**DiffSinger** models (machine learning, ONNX files of hundreds of
megabytes).

The whole of it (voicebanks, lyrics, expression, exchange with UTAU and
OpenUtau, machine-learning voices) is a phase of its own, built in
milestones:

- **7a. A first voice:** add classic UTAU voicebanks, lyrics on notes,
  Japanese CV and VCV voicebanks sung by a renderer of our own, rendered
  in the background and played with the song.
- **7b. Expression:** pitch curves and vibrato drawn over the notes,
  envelopes, the usual flags (breathiness, gender), a better vocoder.
- **7c. Exchange:** UST (UTAU) and USTX (OpenUtau) projects in and out.
- **7d. Machine-learning voices:** DiffSinger through an ONNX runtime,
  after a spike on licences, model sizes and speed on an ordinary
  processor; ENUNU and others judged then.

This record settles the design for all of them and the details of 7a.

## Where singing lives

**A. A voice is an instrument, and lyrics live on notes (chosen).** A
track's instrument can be a voicebank. Its patterns are written in the
piano roll like any melody; each note shows its lyric inside it. Select a
note and press Enter (or double-click it) to type its lyric; a phrase typed
with spaces ("ka sa ne te to") goes to that note and the ones after it. New
notes sing "a". The voice plays with the song, through the mixer and the
effects, and exports like any track.

**B. A singing editor of its own**, as OpenUtau has. Richer, but a second
piano roll beside the first, against the one focus surface of decision
0011, and two ways of doing the same thing. *Rejected.*

**C. Lyrics on a lane of their own under the notes.** Keeps notes
untouched, but a lyric and its note drift apart as soon as notes move.
*Rejected.*

## The renderer

**A. Our own, in Rust, the way UTAU works (chosen).** For each note the
voicebank's recording of its syllable (as `oto.ini` places it) is shifted to
the note's pitch and stretched to its length by a *resampler*, then the
notes are joined, each overlapping the one before by the time `oto.ini`
gives, with crossfades (UTAU's *wavtool*). Our resampler finds the
recording's pitch (YIN; or the voicebank's own `.frq` files), marks its
periods, and moves pitch and length by pitch-synchronous overlap-add
(TD-PSOLA), which keeps the voice's formants. Everything runs off the audio
thread, the same on Windows and Linux, with no outside program. 7b can add
a WORLD-style vocoder (WORLD is BSD-3-Clause) for larger shifts and the
flags.

**B. OpenUtau's engine in a process of its own.** OpenUtau is MIT, but
.NET: shipping its runtime and its renderer, plus the classic resampler
programs (Windows executables of unclear licences), for one feature.
Heavy, uneven between systems, and the licences cannot all be checked.
*Rejected.*

**C. DiffSinger only.** The best quality, but an ONNX runtime, models of
hundreds of megabytes, slow on an ordinary processor, and most voicebanks
people have (Teto's among them) are classic. *Later* (7d), on top of A.

## From lyrics to recordings (7a)

Japanese first, UTAU's home. A lyric is typed in hiragana or in romaji
(converted to hiragana with our own table). It is looked up in `oto.ini`
the way OpenUtau's classic phonemizers do: for a VCV voicebank, the vowel
of the note before and the syllable ("a か"), or "- か" at the start of a
phrase; then the syllable alone (CV); then the romaji spelling; each with
the prefix and suffix that `prefix.map` gives the note's pitch, when the
voicebank has one (multi-pitch voicebanks). A lyric the voicebank does not
have sings "a" and is shown in the piano roll as not found. A note starts
a phrase when the one before ended at least 50 ms earlier.

## Timing and pitch (7a)

UTAU's timing is kept as voicebanks expect it: a syllable starts sounding
*preutterance* milliseconds before its note, so its vowel lands on the beat,
and fades in over the previous note's end for *overlap* milliseconds. When
a note is too short for the next one's preutterance, both are scaled down
together, as OpenUtau does. Each note sings at its pitch, gliding into it
from the note before over about 80 ms; pitch curves come in 7b.

## Voicebanks

- Added from a folder or a `.zip` (Japanese file names inside zips are
  usually Shift-JIS and are decoded as such) into the program's own
  `voicebanks` folder, from a **Voicebanks** window like the Plugins window
  (decision 0023): name, picture, author, and the creator's terms (the
  readme), shown when a voicebank is added. Removing one is one click.
- Text files are read as UTF-8 when they are valid UTF-8, otherwise as
  Shift-JIS, which is what UTAU wrote (`encoding_rs`, recorded in decision
  0006).
- **No voicebank is bundled** (decision 0008): each has its own terms.
- A song names its voicebank by folder; on a computer without it, the track
  is silent and says which voicebank is missing.

## Rendering and playing

- The notes a voice track plays in the song (every clip's repeats, as MIDI
  export lists them) are placed in seconds through the tempo map and cut
  into phrases. Each phrase is rendered on a background thread and kept,
  keyed by everything that shapes its sound; when notes, lyrics, the tempo
  or the voicebank change, only the phrases that changed are rendered
  again.
- The audio thread plays rendered phrases exactly as it plays audio clips
  (decision 0017): sound placed at a frame of the song. Nothing new runs in
  real time; a phrase still being rendered is silent until it is ready, and
  the window says the voice is being rendered.
- Notes played live (the keyboard, a click in the piano roll) sound at once
  with the voicebank's "a" played faster or slower, as a sampler would; the
  rendered voice is what the song plays.

## Consequences

- File format version 9: a voice instrument (`Instrument::Voice`, naming
  the voicebank) and lyrics, kept in each pattern beside its notes by note
  identity (a note stays a small value that is cheap to copy; copy, paste,
  undo and splitting notes carry the lyrics). Older files have neither.
- New pieces: `daw-formats::utau` (voicebanks: `oto.ini`, `prefix.map`,
  `character.txt`, `.frq`), `daw-dsp::voice` (pitch analysis, the
  resampler, joining notes), lyric lookup in `daw-model` (kana and
  romaji), a voice renderer in `daw-engine` beside the SFZ loader, the
  Voicebanks window and lyric typing in `daw-ui`.
- Japanese text (kana lyrics, voicebank names, terms) needs a font our
  bundled Go fonts lack, and the fonts that have it are under licences the
  policy refuses (decision 0005: the OFL among them). So none is bundled:
  the program uses a Japanese font the computer already has (Yu Gothic,
  Meiryo or MS Gothic on Windows; Noto CJK, IPA or Droid fonts on Linux)
  as a fallback after its own, and without one shows lyrics in romaji.
- In the instrument chooser, Voices are listed once a voicebank is
  installed; until then "Voicebanks…" beside "Plugins…" is the way in.
- Tested without ears: synthetic voicebanks made by the tests (vowels at
  known pitches), rendered offline: the sung pitch, where each syllable
  lands, the crossfades, no clicks between notes.
- Not in 7a: pitch curves, vibrato, envelopes and flags (7b), UST and
  USTX (7c), DiffSinger and ENUNU (7d), English and other languages'
  phonemizers (judged with 7b, once the Japanese ones are proven).

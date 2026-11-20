# 0029. A voicebank's picture on its voice track

- **Status:** accepted, November 2026 (the owner's request; builds on
  decisions 0027 and 0028)

## The task

"For a voicebank, have the avatar display on the song visualisation, on
the instrument, and in the pattern editor, as a low opacity image."

A voicebank's creators give it a face. A classic UTAU voicebank names a
small picture in its `character.txt` (`image=`, usually a 100 × 100 BMP);
voicebanks made for OpenUtau often add a large portrait of the character in
a `character.yaml` (`portrait:`, usually a PNG with a transparent
background), which OpenUtau shows faintly behind its piano roll.

## Concepts considered

**A. The picture faint wherever the voice is (chosen).**

- **The piano roll** of a voice track: the picture large and faint behind
  the notes, at the bottom right of the grid, still while the notes
  scroll, as OpenUtau shows a portrait. The notes, the sung pitch and its
  handles stay on top and readable.
- **The track's header in the song view** (where its instrument is named,
  "Voice · …"): the picture faint at its right, as tall as the header.
- **The song view's timeline:** the picture faint at the start of each of
  the voice track's clips, as tall as the clip, behind its notes and name.
- **The instrument chooser:** a small, clear picture beside each voice, to
  recognise it when choosing (the only place it is not faint: it names the
  choice).
- The portrait when the voicebank has one, otherwise its picture. Large
  pictures are scaled down once, when read (at most 1024 points on their
  long side); small ones are drawn at most two and a half times their size,
  so a 100-point icon never becomes a blur filling the screen.
- Read in the background the first time a voice track needs it, and kept
  until the voicebank is removed or added again. A voicebank without a
  picture, or with one that cannot be read, simply shows none.

**B. The picture only in the Voicebanks window and the chooser.** Simple,
but the voice would have no presence where the work happens, which is what
the owner asked for. *Rejected.*

**C. The picture filling each clip, or clear behind the notes.** Striking,
but it fights the notes and the pitch line for attention, against the calm,
readable screen of decision 0011. *Rejected:* faint, and once per clip.

## Consequences

- `daw_formats::utau` reads `character.yaml`'s `portrait` beside
  `character.txt`'s `image` and decodes BMP, PNG and JPEG with the `image`
  crate (already built into the program through the clipboard library;
  adopted directly, decision 0006), scaled down when large.
- The session keeps each voicebank's picture as a texture; the views ask it
  for the picture by voicebank and draw it with a low opacity.
- Each drawn picture is named for screen readers ("Picture of …"), which is
  also how the interface tests find it.
- Nothing is stored in the song: the picture belongs to the voicebank on
  this computer.

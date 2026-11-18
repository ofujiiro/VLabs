# 0021. Autosave polish: takes survive a crash, copies before risky moments, a clearer offer

- **Status:** accepted, November 2026 (extends decision 0013)

## Context

Decision 0013 made autosave and crash recovery work: a copy of the song
every few seconds while it has unsaved changes, in a folder each running
copy of the program owns, offered back after a crash. Phases 3 and 4
showed where it still falls short:

- **A take being recorded is lost in a crash.** The recording is written
  to its file as it comes, but it only becomes a clip of the song when it
  ends, so the copy never mentions it.
- **Plugins are the likeliest cause of a crash** (they run inside the
  program, decision 0015), and the moments they are most likely to crash
  are when they load and when their window opens. Up to five seconds of
  work can be lost right then.
- **The offer says little:** "Recover your work?" without saying which
  song or how old the copy is, and without noticing that the song's file
  was saved again after the copy was made (after a crash in a second copy
  of the program, say).

## Decision

1. **The take in progress is part of the copy.** When a take of sound
   starts, the recovery folder gets a small note: the track, where the
   take starts, its file, its sample rate and channels. The note goes when
   the take ends (by then the clip is in the song and the next copy has
   it). After a crash, the take's file is repaired (a WAV file whose
   header was never finished: its sizes are worked out from the data
   written) and the take comes back as a clip on its track, where it would
   have been placed, as if recording had been stopped at the crash.
2. **A safety copy before risky moments.** Right before a plugin is loaded
   (a new plugin track or effect) and before a plugin's window opens, the
   song is written to the recovery folder at once, if it has unsaved
   changes, and the program waits (briefly, at most two seconds) until the
   copy is on disk.
3. **A clearer offer.** The dialog names the song (its file name, or
   "Untitled song"), says how long before the stop the copy was made
   ("about 3 minutes ago") and how many takes of sound it holds, and warns
   when the song's file on disk is newer than the copy ("The song's file
   was saved after this copy was made: recovering may bring back older
   work"). The choices stay "Recover it" and "Start fresh".
4. **Several pieces of work** left by several copies of the program that
   all stopped (a power cut with two windows open) are offered one after
   the other, newest first, at the same start, instead of one per start.

## Alternatives considered

- **Writing the copy every second while recording:** more disk writes for
  a song whose only change is the take; the take's file is already on disk
  and needs only a note to be found.
- **Running plugins in their own process:** the real fix for plugin
  crashes; planned after v1 (decision 0015 leaves room for it). The safety
  copy covers the gap.
- **Keeping discarded work for a while:** "Start fresh" would become a
  half-measure that fills the disk unseen. The clearer offer is the fix.

## Consequences

- The recorder already writes takes as they come, so nothing changes for
  the audio thread; the notes and repairs happen on ordinary threads.
- A crash loses at most the last five seconds of edits, never a take
  already being recorded (except the last fraction of a second the system
  had not written yet), and nothing right before a plugin loads.

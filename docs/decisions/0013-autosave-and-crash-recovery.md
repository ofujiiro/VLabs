# 0013. Autosave and crash recovery

- **Status:** accepted, November 2026

## Context

The brief requires autosave and crash recovery from the first release. It
must survive crashes, power cuts and forced quits, must not slow the
interface, and must behave when two copies of the program run at once.

## Decision

- Each running copy of the program owns a folder in the user's data folder
  (`recovery/<process>-<time>/`) and keeps a **lock file** in it locked for
  as long as it runs (`std::fs::File::lock`). The operating system releases
  the lock when the process ends, however it ends.
- While the song has **unsaved changes**, a copy is written there at most
  every 5 seconds, by a background thread: the interface hands over a
  snapshot of the project, which is cheap because snapshots share unchanged
  data. The copy is the normal project format, written atomically, with a
  small `info.json` holding the original file's path. When the song is
  saved, the copy is removed.
- A **clean exit** deletes the folder.
- At start-up, a folder whose lock can be taken belonged to a copy that did
  not exit cleanly. If it holds a song, the person is asked: "Recover your
  work?" Continuing opens it as an unsaved song (with its original file
  name, so Save writes back to it); either answer then deletes the old
  folder. Empty or unreadable leftovers are tidied away silently.
- Settings are a JSON file in the configuration folder, written atomically
  two seconds after a change and at exit. Unknown or damaged values fall
  back to defaults.

The standard folders are found with a few lines of our own (XDG variables
on Linux, `APPDATA`/`LOCALAPPDATA` on Windows): the `directories` crate was
set aside because it pulls in `option-ext`, which is MPL-2.0 (decision 0005).

## Alternatives considered

- **A single recovery file:** breaks when two copies of the program run.
- **Process-id files checked with system calls:** process ids are reused;
  locks are released exactly when the process ends.
- **Autosaving into the user's own file:** silently changes a file the
  person did not choose to save.

## Consequences

- At most about five seconds of work can be lost in a crash.
- A crash in the middle of the person's decision leaves the old folder in
  place, so the work is offered again next time.

# 0012. The interface drives the engine through its control handle

- **Status:** accepted, November 2026 (refines a consequence stated in 0003)

## Context

Decision 0003 said the interface and the engine "meet only in `daw-app`".
Building phase 1 showed that almost everything the interface does is a
conversation with the engine: play, stop, record, locate, play a note while
a key is held, read the play position, collect recorded notes. Routing all
of that through `daw-app` would mean either a copy of the engine's control
API in `daw-app`, or a trait in `daw-ui` mirroring it, plus duplicates of the
engine's event types.

## Decision

`daw-ui` (layer 3) uses `daw-engine` (layer 2) directly, through the
engine's **control handle** (`daw_engine::Engine`) only. The handle sends
messages on lock-free queues and reads values the audio thread publishes; it
cannot reach the audio thread's state. The processor half
(`EngineProcessor`) belongs to `daw-app`, which hands it to the sound
output. The layer rule in `xtask/src/rules.rs` already allows this
direction; nothing below the interface knows the interface exists.

Inside `daw-ui`, the editing logic (`Session`: the document, the selection,
playing, recording, files) is separate from drawing (`Workspace` and the
views), so it is tested without a window, with a real engine whose audio
thread is simulated by calling the processor in the test.

## Alternatives considered

- **A trait in `daw-ui`, implemented in `daw-app`:** keeps `daw-ui` free of
  the engine, at the cost of mirroring its API and event types. Nothing is
  gained: the engine is already headless and has no window or device code.
- **The engine owning the document:** would put undo, selection and editing
  into the engine and blur the model as the single source of truth.

## Consequences

- `daw-ui`'s tests run the real engine, so playing and recording are tested
  end to end without a sound card.
- `daw-app` stays thin: devices, the window, autosave and settings.
- The engine's control handle is part of the interface's contract; changes
  to it show up as compile errors in `daw-ui`, not at run time.

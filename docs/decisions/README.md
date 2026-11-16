# Decision records

Every significant technical or design decision is written down here, with
the reason, so that anyone (the project owner, a contributor or a future
working session) can see why things are the way they are and what was
considered instead.

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-record-decisions.md) | How decisions are recorded | Accepted |
| [0002](0002-product-identity-in-one-place.md) | The product's name, file extension and identifiers live in one place | Accepted |
| [0003](0003-workspace-and-crate-layers.md) | The workspace is split into layered crates | Accepted |
| [0004](0004-toolchain-and-quality-gates.md) | Pinned toolchain, strict lints, CI on Windows and Linux | Accepted |
| [0005](0005-dependency-licensing-policy.md) | Only permissively licensed outside code | Accepted |
| [0006](0006-adopted-crates.md) | Outside crates adopted, with version and licence checks | Accepted, grows over time |
| [0007](0007-musical-time.md) | Musical time is counted in integer ticks | Accepted |
| [0008](0008-bundled-sounds-and-fonts.md) | Where the bundled piano, drums and fonts come from | Accepted |
| [0009](0009-phase-0-spike-results.md) | Results of the three phase 0 risk spikes | Accepted |
| [0010](0010-audio-backends.md) | Which audio systems are used on each platform | Accepted |
| [0011](0011-main-window-design.md) | The main window: one focus surface, with context and drawers | Accepted |
| [0012](0012-interface-drives-the-engine.md) | The interface drives the engine through its control handle | Accepted |
| [0013](0013-autosave-and-crash-recovery.md) | Autosave and crash recovery | Accepted |
| [0014](0014-song-patterns-and-mixer-screens.md) | Phase 2 screens: song, step sequencer, mixer | Accepted |

## Writing a new record

Copy the shape of an existing record: **context** (what problem, what
constraints), **decision** (what we do), **alternatives** (what else was
considered and why it lost) and **consequences** (what this makes easy or
hard). Keep it short. Number records in order and never reuse a number. When a
decision changes, write a new record that supersedes the old one and mark the
old one "Superseded by NNNN" rather than rewriting history.

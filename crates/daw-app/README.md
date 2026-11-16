# daw-app — the program people launch

The application shell: it starts the engine and the interface, connects
them, and owns everything that belongs to the running program rather than to
one project.

## What belongs here

- Starting up and shutting down cleanly.
- Opening the window and handing it to `daw-ui`.
- **Autosave and crash recovery:** deciding when to save, and offering the
  recovered project after a crash. (Writing the file safely is `daw-formats`.)
- Application settings: audio device choice, theme, shortcuts.
- Command-line options.

## Rules

- **Thin.** This crate wires the others together. Anything that could be
  tested without a window belongs in a lower crate.
- No other crate depends on this one.
- Names shown to the user, and the names of files and folders the program
  creates, come from `daw-ident`.

## The program file

Cargo builds this crate as `daw-app`. The file people actually receive is
produced by `cargo xtask dist`, which names it after `daw-ident`. That keeps
the product name in a single place.

## Roadmap

Phase 1 (window, wiring, save and load, autosave), phase 6 (installers and
autosave polish).

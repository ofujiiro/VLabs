# daw-ui — everything on screen

The whole interface, built with `egui` (eframe + wgpu).

## What belongs here

- The editing surfaces: step sequencer, piano roll, arrangement timeline,
  mixer, plugin browser, on-screen keyboard.
- **Themes:** dark and light, following the system setting by default, with a
  switch for the user.
- **Shortcuts:** our own scheme, rebindable, shown in every tooltip and menu.
- **User-facing text**, all of it, kept in one place in this crate. English
  only for now; one place so that translation stays possible later.
- Playing notes from the computer keyboard.

## Rules

- **A clear boundary.** All interface code lives in this crate, and no other
  crate except `daw-app` depends on it. The engine and the project data never
  mention windows, colours or widgets.
- **The interface owns no song data.** It shows `daw-model` and changes it
  only through undoable edits.
- **Design from the musician's task,** not from the panels other DAWs happen
  to have. Before building a major screen, think through two or three distinct
  concepts, pick the strongest, and record what was rejected and why in
  `docs/decisions/`.
- **Spacious and calm.** Few things on screen at once, a clear hierarchy,
  advanced features one step away but never hidden. The design principles to
  apply deliberately are listed in `docs/PROJECT_BRIEF.md`.
- **Original identity.** Do not copy another product's visual design, icons
  or product-specific names.
- **No hardware required.** Every action is reachable with the mouse alone,
  and the common ones with the keyboard alone. Shortcuts never clash with
  playing notes on the computer keyboard.
- **Immediate feedback** for every action, and nothing slow on the interface
  thread.
- The product name and the file extension come from `daw-ident`, never from a
  literal in this crate.

## Roadmap

Phase 0 (risk spike: a piano roll with 10,000 notes that scrolls and zooms
smoothly), then a part of every later phase.

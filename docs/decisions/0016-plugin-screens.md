# 0016. Plugin screens: a chooser, separate editor windows, automation lanes

- **Status:** accepted, November 2026 (extends decisions 0011 and 0014)

## The tasks

Phase 3 adds three things people do: **pick a plugin** (an instrument for a
new track, an effect for a chain), **use its own editor**, and **make a
setting change over the song** (automation: a filter that opens, a pan that
moves, a fade).

## Concepts considered

**A. A chooser, separate editors, lanes under tracks (chosen).**
- *Picking:* "+ Instrument…" next to "+ Drums" and "+ Piano" opens a chooser
  listing every instrument, bundled ones first, plugins after, with a search
  field and each plugin's maker; the mixer's "+ Add effect" menu lists the
  built-in effects, then the plugin effects. While plugins are being found
  for the first time, the chooser says so.
- *Editing:* a plugin's editor opens in its own window (title: plugin and
  track), from the track header, the mixer, or the location line, and closes
  with its close button.
- *Automation:* in the song view, each track header gets an "Automation"
  button; it opens a lane under the track for a chosen setting (volume, pan,
  an effect's settings, a plugin's parameters, with the parameter most
  recently moved in the plugin's editor offered first). Click to add a point,
  drag to move, right-click to remove; the line between points is what the
  setting does as the song plays.

**B. A permanent browser panel** at the side of the window, listing
instruments, effects and sounds, dragged onto tracks and mixer strips.
*Rejected:* it takes room from the one focus surface (decision 0011), and
dragging from a long tree is harder to discover than a button that says
what it adds.

**C. Plugin editors inside the main window**, in a drawer. *Rejected:*
plugin editors are native windows of fixed size made by their own toolkits;
putting them inside our window means nesting native windows in the
interface's surface, which is fragile across X11, XWayland, Wayland and
Windows, and a large editor would crush the song.

For automation, **lanes inside each pattern** (the pattern's notes and its
settings move together, as some beat-making programs do) were considered
and *rejected* for now: "the filter opens over the last eight bars" belongs
to the song, not to a pattern repeated many times; pattern automation can
come later without changing song lanes.

## Consequences

- People see plugins in the same places as the bundled sounds and effects:
  no new area to learn.
- Plugin editors float over the program; moving the main window does not
  move them.
- The parameter list of a large plugin is long (Surge XT has hundreds), so
  the lane chooser searches by name and puts the last-touched parameter on
  top: move a knob in the plugin, then choose it.

## As built (phase 3)

- Track headers in the song view got two small icon buttons on their second
  line: the plugin's window (only on plugin tracks) and Automation (a line
  through points). The same window button sits by the name on the mixer
  strip and after the track's name in the location line.
- "+ Instrument…" did not fit beside "+ Drums" and "+ Piano" in the track
  column, so the add row wraps onto a second line.
- A new lane starts with one point at the setting's present value, so the
  person sees a flat line to draw on rather than an empty lane. Each lane's
  header shows its name, its value where the song is, and a button that
  removes it.
- Clicking a lane puts the point on the nearest beat; holding Alt puts it
  exactly where clicked, as Alt does for clips. A dragged point stays
  between its neighbours.

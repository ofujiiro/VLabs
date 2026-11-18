# 0023. Plugins and sound libraries: a manager, plugins added from anywhere, Windows plugins on Linux, a library of sounds

- **Status:** accepted, November 2026 (extends decisions 0015, 0016 and 0017;
  the owner's request after phase 6)

## The tasks

The owner asked for plugins and sounds to be as easy to bring in as
possible, "so people are not limited by the editor". A person **got a
plugin** (a `.vst3` or `.clap` file, a folder, a downloaded `.zip`, an
installer) and wants to use it without learning where plugins go; **has
Windows plugins on Linux**; **has a folder of samples** (often thousands of
WAV files) and wants to find, hear and use them; **has SoundFonts**.

## The plugin manager

**A. A Plugins window, reached from the menu and from the instrument
chooser (chosen).** It lists every plugin found, instruments and effects,
with its maker and format, and the files that could not be used, with
why; a search field narrows the list. Its buttons:

- **Add plugins…** takes plugin files (`.vst3`, `.clap`), downloaded
  archives (`.zip`, opened and searched), and **Add a folder…** takes a
  folder (a `.vst3` bundle is a folder) searched for plugins. What is found
  is **copied into the program's own plugin folder** (in its data folder,
  always searched), so the person never needs to know where plugins go,
  and nothing is changed in system folders. The search for plugins then
  runs again.
- **Remove** takes a plugin the program copied out of its folder (plugins
  installed elsewhere are left alone: their own uninstallers own them).
  For a Windows plugin set up by yabridge, Remove takes the Windows plugin
  and yabridge then removes its stand-in.
- **Dropping** plugin files, folders, downloads or installers anywhere on
  the window adds them the same way and opens the Plugins window to say
  what happened. (A dropped folder with no plugins inside joins the sound
  library instead.)
- **Folders to search**: the usual folders are listed; the person can add
  others (a known gap since phase 3) and remove them again.
- **Installers** (`.exe`, `.msi`) are run: on Windows they are opened as a
  double-click would (they may ask for permission); the person finishes
  them, then clicks Look again. On Linux they run through Wine, when it is
  installed; the program waits for them, then has yabridge set up the
  plugins they put in Wine's usual plugin folders, and looks again.
- Copying, unpacking and installing run on a thread of their own; the
  window says what each file became ("Added: …", "Old.dll is a VST2
  plugin…", "made for Mac: get its Linux version").

**B. Only a list of folders to search.** Simpler, but it leaves the person
to learn where plugins go and to unpack archives themselves. *Rejected.*

**C. Installing into the system's plugin folders.** Other programs would
see the plugins too, but on Windows that needs administrator rights, and a
music program writing into system folders surprises people. *Rejected.*

## Windows plugins on Linux

The brief settled this: VLabs searches `~/.vst3` and `~/.clap`, where
[yabridge](https://github.com/robbert-vdh/yabridge) puts the Linux
"bridges" of Windows plugins, and "documents this rather than building our
own Wine bridge". The owner now asks for Windows plugins to "just work".

**Chosen: VLabs sets yabridge up itself, when Wine and yabridge are
installed.** A Windows plugin added in the Plugins window goes into a
folder of its own (`windows/` in the program's plugin folder); VLabs runs
`yabridgectl add` for that folder (once) and `yabridgectl sync`, and looks
again: the bridged plugin appears like any other. When Wine or yabridge is
missing, the Plugins window says so and shows the two steps to install
them (Wine from the distribution, yabridge from its releases), with a
"Check again" button.

**Bundling Wine and yabridge with VLabs** would make it work with nothing
else installed, but Wine is large (hundreds of megabytes) and both are
copyleft (Wine LGPL, yabridge GPL). **Decided by the owner (November
2026): not bundled.** VLabs uses the person's own Wine and yabridge, and
the Plugins window says how to install them.

## VST2 plugins (`.dll`)

The brief keeps VST2 out of v1 and asks for clean-room headers and a
licensing check first: Steinberg no longer licenses VST2, and the open
headers in circulation either carry copyleft licences or an uncertain
history. A `.dll` added in the Plugins window is recognised and explained
(a VST2 plugin, not supported: look for its VST3 or CLAP version).

**Decided by the owner (November 2026): VST2 stays out of v1**, as the
brief says. It may be looked at again after v1, with clean-room headers
and a licensing check first.

## The sound library

**A. A Library drawer, in place of the keyboard like the mixer (chosen).**
The person adds folders ("Add a folder…", or by dropping a folder on the
window); VLabs lists their sound files (WAV, AIFF, FLAC, MP3, Ogg), MIDI
files and SoundFonts in the background, however many there are (up to half
a million per folder), and shows them by folder with a search field (a
search looks through every folder at once, for every word typed). A click
plays a sound once, over the song, at about -3 dB after the master effects
(like the metronome); a second click stops it. Dragging an item onto the
song puts it there (a sound as an audio clip on the beat it is dropped on,
a MIDI file as tracks of notes, a SoundFont as an instrument track), and
double-clicking (or "Add to the song" in its menu) adds it at the play
position. The library points at the person's folders and copies nothing;
a sound used in a song is copied into the song like any import. F6 shows
and hides it. AIFF files, common in sample collections, are read from now
on.

**B. A side panel always beside the song.** Quicker to reach, but it takes
room from the focus surface all the time (decision 0011). *Rejected.*

**C. A file dialog with a preview.** The system's dialogs cannot play
sounds or search thousands of files quickly. *Rejected.*

**SoundFonts** (`.sf2`) become instruments: "SoundFont…" in the instrument
chooser, SoundFonts in the library, and `.sf2` files dropped on the window
make a track playing the file (its first preset), whose presets are chosen
from the location line. The bundled piano already played a SoundFont
through `rustysynth`; the same player plays these. The song refers to the
file where it is; when it is missing (a song from another computer) the
track plays a plain sound and the person is told where the file was.

**SFZ instruments** (the open format of many free sampled instruments)
need a sampler of their own: planned next, not in this round.

## Consequences

- `daw-plugin-host::install` puts plugins in place (copying bundles,
  opening archives safely, telling Windows binaries apart, yabridge); the
  program runs it off the interface thread and looks for plugins again.
- The settings gain extra plugin folders and the library's folders.
- The engine gains a preview voice that plays one sound at a time over the
  song, for the library; the audio thread neither allocates nor frees for
  it (the sound comes in, and goes back to be freed, through the queues).
- The interface's `Host` gains dialogs for plugin files, folders and
  SoundFonts, and a way to show a folder in the file manager.

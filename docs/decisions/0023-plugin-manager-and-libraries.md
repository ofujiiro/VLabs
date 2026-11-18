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
- **Folders to search**: the usual folders are listed; the person can add
  others (a known gap since phase 3) and remove them again.
- **Installers** (`.exe`, `.msi`) are run: on Windows as they are, then the
  plugins they install are found by looking again; on Linux through Wine,
  when it is installed, inside the Wine setup yabridge uses (below).

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
copyleft (Wine LGPL, yabridge GPL). They would run as separate programs,
not linked with VLabs, but shipping them inside VLabs's own downloads is
a licensing choice for the owner, with the dual-licensing question of
decision 0005 in mind. *Not done until the owner decides.*

## VST2 plugins (`.dll`)

The brief keeps VST2 out of v1 and asks for clean-room headers and a
licensing check first: Steinberg no longer licenses VST2, and the open
headers in circulation either carry copyleft licences or an uncertain
history. A `.dll` added in the Plugins window is recognised and explained
("VST2 plugins are not supported yet: look for a VST3 or CLAP version").
*Waiting for the owner's decision.*

## The sound library

**A. A Library drawer, beside the keyboard and the mixer (chosen).** The
person adds folders ("Add a folder…"); VLabs lists their sound files (WAV,
FLAC, MP3, Ogg), MIDI files and SoundFonts in the background, however many
there are, and shows them by folder with a search field (a search looks
through every folder at once). A click plays a sound once (it stops on the
next click); dragging an item onto the song puts it there (a sound as an
audio clip, a MIDI file as tracks of notes), and double-clicking adds it at
the play position. Nothing is copied: the library points at the person's
folders.

**B. A side panel always beside the song.** Quicker to reach, but it takes
room from the focus surface all the time (decision 0011). *Rejected.*

**C. A file dialog with a preview.** The system's dialogs cannot play
sounds or search thousands of files quickly. *Rejected.*

**SoundFonts** (`.sf2`) become instruments: "SoundFont…" in the instrument
chooser (and SoundFonts in the library) make a track playing the file,
whose presets are chosen from the location line. The song refers to the
file where it is; when it is missing (a song from another computer) the
track plays a plain sound and the person is told, as for missing plugins.

**SFZ instruments** (the open format of many free sampled instruments)
need a sampler of their own: planned next, not in this round.

## Consequences

- `daw-plugin-host::install` puts plugins in place (copying bundles,
  opening archives safely, telling Windows binaries apart, yabridge); the
  program runs it off the interface thread and looks for plugins again.
- The settings gain extra plugin folders and the library's folders.
- The engine gains a preview voice that plays one sound at a time over the
  song, for the library.

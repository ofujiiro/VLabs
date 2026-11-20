# 0030. Releases: version 1.0, published on GitHub from a tag

- **Status:** accepted, November 2026 (the owner's request; builds on
  decision 0022)

## The task

"Create the release of V1. If a server is needed, tell me."

Until now each milestone's build was a set of downloads attached to a run
of the "Program builds" workflow (decision 0022). Those downloads are made
for testing: they expire after 30 days and only people signed in to GitHub
can fetch them. A release is for everyone, and stays.

Decision 0022 left version 1.0.0 to the owner's call once the hands-on
checks were done. They were (every phase, the plugins round, SFZ
instruments, custom kits, singing 7a with a real voicebank), and the owner
has now asked for the release.

## Decision

- **Version 1.0.0**, everything on `main`: v1 as the brief defines it, and
  what came after it, singing included (decisions 0026 to 0029).
- **A GitHub release** on the repository, made by the "Program builds"
  workflow when a tag `v<version>` is pushed. It builds exactly as for a
  milestone, on clean machines, then a last job publishes:
  - the Windows installer (`…-setup.exe`) and the Linux AppImage;
  - the plain program folders, as a `.zip` for Windows and a `.tar.gz` for
    Linux (which keeps the program's "executable" permission);
  - `SHA256SUMS.txt`, to check the downloads;
  - the release notes, from `docs/releases/v<version>.md` (its first line
    is the release's title), written for the people who will download it.
- The job refuses a tag that is not the workspace's version, so a release
  cannot carry files that say another version.
- The workflow can also be run by hand with a version: it then creates the
  tag itself, on the commit it built, for when a tag cannot be pushed. Run
  by hand for a release that exists, it replaces the release's files and
  notes (a packaging fix): the files then come from the commit that run
  built.
- Both Linux downloads are built on the oldest current Ubuntu LTS (24.04),
  so they run on the same systems.
- **After 1.0:** `1.N.0` adds features (a singing milestone, say), `1.N.M`
  only fixes. Songs saved by an earlier version always open in a later one
  (the file format's migrations); a song saved by a later version may not
  open in an earlier one, as now.

**No server is needed.** GitHub hosts a public repository's release files
and builds them with GitHub Actions, both free; nothing of ours runs
anywhere.

## Alternatives

- **A server of our own** (a website with the downloads): it costs money
  and upkeep and gives nothing GitHub's release page does not, for a
  public repository. *Rejected*; a website can link to the releases later.
- **Uploading the files by hand** on GitHub's page: files from a
  development machine instead of a clean build, and easy to get wrong
  (an old installer beside a new AppImage). *Rejected.*
- **A release for every change on `main`** (nightly builds): useful for
  testers one day; CI already builds and tests every change, and "Program
  builds" makes a test build on demand. *Not now.*
- **A draft release** checked before publishing: the owner asked for the
  release itself; the notes are reviewed in the repository before the tag
  is pushed, as everything else is.

## Consequences

- To release: set `version` in the workspace's `Cargo.toml`, write
  `docs/releases/v<version>.md`, bring it to `main`, and push the tag
  `v<version>` on that commit (CLAUDE.md, "Commands").
- The installer is **not signed**: Windows SmartScreen warns about an
  unknown publisher, and the release notes say how to run it anyway.
  Signing needs a code-signing certificate, a yearly cost: the owner's
  decision, whenever they want it. The AppImage is not signed either
  (rarely checked on Linux).
- The Linux downloads need glibc 2.39 or newer (Ubuntu 24.04, Fedora 40,
  Debian 13 and later), as decision 0022 built the AppImage. The first
  1.0 files had the plain Linux folder built on Ubuntu 26.04 (glibc 2.43);
  the development session's check of the published files found it, and
  the release's files were built again with it on 24.04.
- The README links to the latest release.

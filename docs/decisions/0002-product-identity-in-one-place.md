# 0002. The product's name, file extension and identifiers live in one place

- **Status:** accepted, November 2026

## Context

The brief asks that the product name (VLabs), the project file extension
(`.teto`) and the application identifiers be defined in one place, so any of
them can be changed later with a one-line edit.

## Decision

- `crates/daw-ident/src/lib.rs` holds four constants: `PRODUCT_NAME`,
  `EXECUTABLE_NAME`, `PROJECT_FILE_EXTENSION` and `APPLICATION_ID`. Each is a
  single line.
- No other Rust source file may spell out any of these values. A repository
  test (`xtask/src/rules.rs`) fails the build if one does.
- The internal crates are named `daw-*`, not after the product, so a rename
  never touches them.
- The program file is built by cargo as `daw-app` and renamed to
  `EXECUTABLE_NAME` by `cargo xtask dist`, which reads it from `daw-ident`.
  Later, the installers (phase 6) will take every name from the same place.
- User-facing text that contains the product name builds it from
  `PRODUCT_NAME` at run time.

## What a one-line edit cannot reach

These mention the product by name and would need a search-and-replace on a
rename: the prose of the documentation (`README.md`, `docs/`), the GitHub
repository name, and the CI workflow names. They are words for humans, not
identifiers a program depends on.

## Consequences

- Renaming the product or changing the extension is one edit plus a rebuild.
- Changing `APPLICATION_ID` after a public release makes existing
  installations look like a different application to the desktop; the
  constant's documentation says so.

# daw-ident — the product's identity, in one place

Everything that names the product is written exactly once, in
[`src/lib.rs`](src/lib.rs):

| What | Used for |
| --- | --- |
| Product name | Window titles, menus, the About box, installers |
| Program file name | The file people launch (`.exe` is added on Windows) |
| Project file extension | The ending of saved projects |
| Application identifier | How Windows and Linux desktops recognise the app |

To rename the product, change the file extension or change an identifier,
edit the matching line in that file. Nothing else in the program spells
these out.

## Rules

- No other crate may contain the product name, the file extension or an
  application identifier as a literal. Read them from here.
- This crate contains constants only, and depends on nothing.
- The tests check that each value has a usable shape (for instance, that the
  extension has no dot and the program name has no spaces), so a one-line edit
  cannot quietly break installers or file associations.

The same values also appear in places a program cannot reach: the prose of
the documentation, and the name of the GitHub repository. Those are listed in
`docs/decisions/0002-product-identity-in-one-place.md`.

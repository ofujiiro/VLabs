# 0001. How decisions are recorded

- **Status:** accepted, November 2026

## Context

The project owner directs the product but does not read the code. Decisions
are taken by the developer (often an AI working session) and must be visible,
explained in plain language, and easy to revisit. Work happens across many
sessions that do not remember each other.

## Decision

Each significant decision gets a short numbered Markdown file in
`docs/decisions/`, listed in the index (`README.md` there). "Significant"
means: choosing or rejecting a dependency, a file format detail, an
architectural boundary, a user-facing design direction, or any departure from
the project brief.

Design decisions for a major screen record the two or three concepts that
were considered, which one was chosen and why the others were rejected, as
the brief asks.

## Consequences

- The owner can react to decisions after the fact by reading one file.
- A future session reads the index before changing anything structural.
- Records are never rewritten to change their meaning; a new record
  supersedes an old one.

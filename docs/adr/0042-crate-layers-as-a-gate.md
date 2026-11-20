# 0042. The crate layers are a gate

Date: 2026-11-20
Status: Accepted

## Context

The workspace has eleven crates, and the apps pin them by tag. Their dependencies form layers:
`omap`, `georef`, `fs`, `rules`, `text` and `web` are leaves; `course`, `contract`, `render` and
`update` sit on one or two of them; `printrules` is the top. The shape held by habit only.
One careless `use` would give `omap` a path to the PDF stack, or put a second HTTP or hashing
crate beside the ones `update` owns, and every app would pay for it.

## Decision

- **`scripts/check.sh architecture`** reads the workspace with `cargo metadata --no-deps` and
  fails when
  - a crate is in no layer (a new crate is a decision, made in the table `ALLOWED`),
  - a crate depends on a sibling that its row does not list (every edge is listed, so a new edge
    shows in the diff),
  - a third-party crate that carries one concern is used by a crate that does not own it: the web
    stack (`axum`, `tokio`, `rust-embed`) belongs to `web`, the PDF and font stack (`pdf-writer`,
    `subsetter`, `skrifa`, `image`) to `render`, the update's archives, signature and hashes
    (`flate2`, `minisign-verify`, `sha2`) to `update`, localisation (`fluent-*`, `unic-langid`) to
    `text`.
  Development dependencies are not counted.
- **Measured:** under a second, passes on today's tree. A probe that gave `freemapper-omap` a
  dependency on `freemapper-render` fails it with a line naming both crates; it was reverted.
- **Cheap gate:** it runs in `hygiene` and before a commit; the badge for the number of checks
  counts it.
- **Rejected: a separate test crate.** The sibling projects with many crates use one; here the
  check needs no build and no compiled code, so the gate is cheaper and runs on every commit.

## Consequences

- A new edge between crates, or a new owner of a concern, is an edit of `scripts/check.sh` with a
  reason; a drive-by dependency is a failing gate, not a review remark.
- The table is the one place that shows the layers.

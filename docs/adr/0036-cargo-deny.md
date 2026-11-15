# 0036. cargo-deny checks the dependencies

Date: 2026-11-15
Status: Accepted

## Context
ADR 0035 put every crate under AGPL-3.0-or-later. The licences of the dependencies change with every
`cargo update`, and nothing checked them. ADR 0004 limits the age of a dependency but says nothing about its
licence, known vulnerabilities or source. freemapper-print runs `cargo deny check` since its ADR 0068 and asked
for the same here.

## Decision
- `deny.toml` at the repository root configures `cargo deny check`. It allows the licences the dependencies use
  now (permissive ones, MPL-2.0 and AGPL-3.0-or-later), denies registries and git sources other than crates.io,
  denies yanked crates, and reports crates present in two versions as a warning only.
- Wildcard dependencies are allowed: the workspace crates are path dependencies without a version, and the check
  flags those.
- One advisory is ignored, with its reason in the file: RUSTSEC-2026-0192 says `ttf-parser` is unmaintained and
  offers no upgrade. freemapper-render and, through it, the apps read fonts with it. Replacing it, for example with
  `skrifa`, is a decision of its own and needs a new ADR.
- CI runs the check as its own job, with the action pinned to a commit.

## Consequences
- A new dependency with a licence not on the list fails CI until someone decides and adds it here. A new advisory
  fails CI as well, also on a day when nothing in the repository changed; the fix is an upgrade or a reasoned
  ignore.
- The `ttf-parser` ignore stays until the replacement is decided.

Source: freemapper-print ADR 0068.

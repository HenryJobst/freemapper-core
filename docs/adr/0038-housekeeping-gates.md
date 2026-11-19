# 0038. The repository checks its own housekeeping

Date: 2026-11-19
Status: Accepted

## Context

CI ran the compiler, the three system matrices and `cargo deny`. The documents, the ADR table, the
changelog pair and the dependency list were kept in order by hand, so a link to a renamed file, an
ADR without a row in `docs/adr/README.md`, a changelog that missed a version and a dependency that
no code used could only be caught in review. freemapper, freemapper-courseplan and freemapper-print
had the same gap, and the gates are cut the same way in all four.

Nothing measured rustdoc warnings or line coverage either, and the `msrv` claim was untested. The
first attempt to test it exposed something else: `freemapper-printrules` inherits
`freemapper-contract` from the workspace and overrides `default-features = false`, which cargo
rejects before 1.99. That made the whole core need 1.99 although its code does not.

## Decision

- `scripts/check.sh` is the single entry point for the gates and has no dependencies beyond git and
  the tools named below. CI calls the same commands as a person does.
- The gates are `spdx`, `typos`, `links`, `secrets`, `machete`, `actions`, `adr`, `changelog`,
  `doc`, `coverage` and `msrv`. The first eight need no build and run on every commit
  (`lefthook.yml`); `secrets` runs before a push because it walks the history; `doc`, `coverage`
  and `msrv` build the workspace and each have their own CI job.
- Every Rust file starts with `// SPDX-License-Identifier: AGPL-3.0-or-later`. Documents and data
  do not repeat the line: the licence is in `LICENSE` and the manifests.
- `typos` checks the English documents only — `*.en.md`, `AGENTS.md`, `CLAUDE.md`, `docs/adr/`,
  `docs/superpowers/` and anything named `en.md`. German documents and fixtures stay out; German
  words that stand as quotes in English text are listed in `_typos.toml` with their place.
- `lychee` checks links and anchors offline; `gitleaks` walks the whole history with redacted
  findings and allows a path only with a reason in `.gitleaks.toml`.
- `cargo doc` runs with `RUSTDOCFLAGS=-D warnings`; `cargo-machete` removed the one dependency that
  nothing used (`serde` in `freemapper-printrules`).
- Coverage has a floor of 90.0 % in `scripts/check.sh` and the last measurement in
  `docs/coverage.json`. A run below either fails; a run above the snapshot rewrites it, and the
  commit belongs with the change that raised it. The first measurement is 92.4 %.
- `default-features = false` for `freemapper-contract` moves from the member's dependency to the
  workspace entry. Only `freemapper-printrules` inherits the crate and it wants the shared types
  without the map reader, so nothing changes in the build; the manifest now reads with cargo before
  1.99 again. The MSRV stays 1.88 and a job checks it with 1.88.0.
- CI gives the gates their own jobs (`hygiene`, `docs`, `msrv`, `coverage`) beside `check` and
  `deny`, each guarded by the existing "at least one crate" condition. The tools are pinned by
  version and checksum in the workflow.

## Consequences

- A pull request now fails on a broken link or anchor, a missing ADR row, a changelog that misses a
  version, a declared but unused dependency, an action that hangs on a tag, a secret in the
  history, a rustdoc warning and a coverage drop.
- The core release after this one carries the workspace-dependency fix, so the apps that pin a core
  tag can drop their raised MSRV once they move to that tag.
- Raising coverage later means editing the floor in `scripts/check.sh` in the same commit as the
  better number.
- `typos` will meet a new German quote in English text and fail; the answer is an entry in
  `_typos.toml` with its reason, not switching the gate off.

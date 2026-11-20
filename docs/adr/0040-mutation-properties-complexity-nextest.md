# 0040. Mutation tests on the diff, property tests, nextest and a complexity limit

Date: 2026-11-20
Status: Accepted

## Context

ADR 0038 added the housekeeping gates. The next candidates measure what the tests prove and how
fast they run: mutation testing, property tests, a faster runner, a feature matrix, a complexity
limit and an SBOM. Each was measured here first (macOS, Apple silicon, warm `target`) and is
adopted or dropped by that number. The decisions follow freemapper-print ADR 0072, which found
the same tools worth different amounts: the numbers differ per repository, so the verdicts do too.

## Decision

- **Mutation tests on the diff: adopted.** `scripts/check.sh mutants` runs `cargo-mutants 27.1.0`
  with `--in-diff` over the Rust lines the branch changed against the base branch, in its own CI
  job on pull requests. Measured on `view.rs` (85 mutants, 2 min with two jobs, baseline 8 s):
  76 caught, 6 missed, 2 timeouts, 1 unviable. The misses were real gaps (`View::preset` and
  `View::is_empty` had no test) and are closed in the same change; the two timeouts delete the
  `Event::Eof` arm of an XML read loop, which then never ends, and are the tool's way to say
  "caught". A whole-workspace run is not a gate; the diff is. `cargo-mutants` copies
  the tree without `.git`: a test that reads the repository's history, or a path above the
  workspace, fails the baseline there (exit code 4) and is a bug in the test. A mutant that
  survives is fixed with a test, not with an exclusion; `mutants.out/missed.txt` names it.
- **Property tests: adopted at one place.** `proptest` checks `View::parse` and its canonical
  form (`Display`): every view built from presets and code lists reads back from its own text as
  the same view, `file_token` never holds `:` or `,`, and arbitrary text is read or refused,
  never a panic. One exception is pinned down on purpose: `only:` lists that share no code print
  as `only:`, which `parse` refuses. Two programs name files after this form (freemapper and
  freemapper-print), so the round trip is a contract. The `.omap` reader was not chosen: the
  fixtures of `freemapper-contract` already cover real maps, and a generator for valid OOM XML
  would be a second parser.
- **cargo-nextest: adopted.** 819 tests: `cargo test` 101 s, `cargo nextest run` 14 s, because
  nextest runs the test binaries of the eleven crates side by side. The `check` job runs
  `cargo nextest run` and `cargo test --doc` (nextest has no doc tests). Pinned to 0.9.152.
  freemapper-print measured the opposite (11:33 min against 8:42 min) and keeps `cargo test`.
- **cargo-hack: adopted with the powerset.** `check --feature-powerset --no-dev-deps` builds
  all 20 combinations of the features (`check`, `fixtures`, `fit`, `page-rules`,
  `test-support`) in its own job; 42 s for each feature alone, 2 s more for the rest (warm).
  The features gate optional dependencies, so a combination that does not build is a real
  defect for an app that picks it.
- **Complexity: adopted as a clippy threshold.** `clippy::cognitive_complexity` with the baseline
  `cognitive-complexity-threshold = 33` in `clippy.toml`; today's worst function scores 33 (eight
  are above clippy's default of 25). `scripts/check.sh complexity` fails on anything above.
  Lowering the number is welcome, raising it needs an ADR.
- **SBOM: rejected.** The core ships libraries, not a binary: nothing here is installed or
  delivered, and a list of eleven crates would repeat the workspace manifest. The apps pin
  core by tag and carry the SBOM of what they ship, core's dependencies included.

## Consequences

- Three new CI jobs (`complexity`, `hack`, `mutants`); the `check` job installs nextest. The
  badge for the number of checks counts the new gates.
- Tool versions are pinned in `ci.yml`: cargo-mutants 27.1.0, cargo-hack 0.6.46, cargo-nextest
  0.9.152, installed with `cargo install --locked --version`.

Source: measured in this repository; freemapper-print ADR 0072 for the same tools there.

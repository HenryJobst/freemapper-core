# 0044. Findings on running tests and mutation tests

Date: 2026-11-20
Status: Accepted

## Context

ADR 0040 adopted mutation tests on the diff, nextest and the other tools; ADR 0043 made the test count
and the test budget data. Since then the tools ran on real changes, among them the NaN fix in `water()`
(`crates/freemapper-render/src/overprint/symbols.rs`, core PR #22). This ADR keeps what that work taught
and settles two rules ADR 0040 left open. It adds to ADR 0040 and does not replace it.

## Decision

- **Read the exit code of `mutants`.** Exit 2 (a mutant was missed) and exit 4 (the baseline failed)
  fail the job. Exit 3 (timeout, for example a loop that no longer ends) counts as caught. The gate runs
  on pull requests only and only on the diff (`--in-diff`), as ADR 0040 says.
- **A changed line of arithmetic gets a test that pins the value.** In PR #22 the edge-case test of
  `water()` passed, and the gate still let three mutants of the stretch calculation (line 424) through.
  Only a second test closed them: it fixes the width of the cup exactly (size minus width between the
  line centres). A test that shows only that the fault is gone does not catch a changed operator; a test
  that fixes the number does. A survivor is closed with a test, never with an exclusion (ADR 0040).
- **Run `tests --fix` before `badges.sh`.** The badges read `docs/tests.json`, so the order is
  `scripts/check.sh tests --fix`, then `bash scripts/badges.sh`. In the first CI run of PR #22 the badges
  were stale because the order was reversed. The count is 911 at the time of writing.
- **Test servers end cleanly.** A process that ends through `kill()` writes no coverage data. A test
  server should close its stdin and exit on its own. Core has no such server today; the rule is a note
  for the first one.
- **No mutation run over the whole workspace as a gate.** ADR 0040 measured the diff gate on one file;
  a run over all crates would be far longer and say less about a change than the diff does. A whole-workspace
  run stays a manual probe.
- **Do not start mutation runs next to timing measurements.** Foreign `cargo-mutants` processes distort
  absolute times. Ratios (for example `cargo test` against nextest) stay reliable, absolute seconds do
  not. Measure first, then mutate.

## Consequences

- No new job and no new dependency; the decisions of ADR 0040 (nextest, hack powerset, complexity 33,
  SBOM rejected, proptest at `View::parse`, mutation gate on the diff) stand.
- A review of a change to a calculation asks for the test that fixes the value.
- Timings in later ADRs name whether other heavy processes ran at the same time.

Source: measured in this repository; core PR #22; ADR 0040 and ADR 0043.

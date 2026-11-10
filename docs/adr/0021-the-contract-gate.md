# 0021. The contract gate

Date: 2026-11-11
Status: Accepted

## Context
freemapper-print and freemapper-courseplan test themselves against the fixtures of `freemapper-contract`
(ADR 0018), but only against the core tag they pin. A change to a fixture or a rule here was checked by no
reader until that reader raised its pin, after the release.

## Decision
- A workflow `contract-gate.yml` runs when `crates/freemapper-contract/` or `crates/freemapper-omap/` changes
  (the contract crate reads maps with omap), on pushes to `main`, on pull requests and by hand.
- For each reader it checks out the reader's `main`, replaces the contract crate of the reader's pinned tag by
  the one in this checkout (`cargo --config 'patch.….freemapper-contract.path=…'`) and runs
  `cargo test -p <reader> --lib contract_`. Linux only.
- Before the tests it runs `cargo update -p freemapper-contract` with the same patch. The reader's lock file
  names the tagged crate, and Cargo silently ignores a patch of another version. One step then fails unless
  `cargo tree` shows the crate from this checkout, and the test step fails unless at least four tests ran.
- Only the contract crate is patched. The reader keeps its other core crates on its tag; that works because
  the contract crate's API is text and plain data.
- freemapper is not in the gate. Its producer tests name two fixtures and run in its own CI.
- Red means: the change breaks a reader as it is on `main`. The reader learns the new form first and merges;
  then the gate is green and core is released (readers first). A release never goes out over a red gate.

## Consequences
- Tried before the merge: with a wrong expected title in one fixture both reader jobs failed at
  `contract_title`; with the fixtures as released both passed.
- A reader whose `main` does not compile turns the gate red for a reason outside core. The run's log names the
  reader.
- The gate adds two Linux jobs of a few minutes to changes of two crates and nothing to other changes.
- A breaking change to `Expected` (a renamed field) cannot be green before the readers follow, and the readers
  cannot follow before the release. Such a change is made in two steps: add the new form, move the readers,
  remove the old form.
- Cost if wrong: the gate is one workflow file; removing it restores the state of v0.10.0.

Source: `docs/superpowers/specs/2026-11-10-contract-test-design.md` (stage 2), the user's approval of
2026-11-10, ADR 0018.

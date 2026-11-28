# 0047. More paper sizes

Date: 2026-11-28
Status: Accepted

## Context
`build::Paper` had two values, A4 and A3, the formats of an office printer. A map with a paper frame (ADR 0046) can
be A6, A5 or A2. freemapper-print printed those as free pages (`PageSpec::page_mm`) and named them again from the
size, which worked but left the project file with `page_mm = [148, 210]` where the person means A5.

## Decision
- `Paper` gains `A6`, `A5` and `A2`, in the order of size: A6, A5, A4, A3, A2. `size_mm` returns the DIN 476 sizes
  in portrait: 105 × 148, 148 × 210, 420 × 594 mm besides the two known ones.
- `PageSpec::page_mm` stays for every other size.
- The PDF of A4 and A3 does not change.

## Consequences
- An app that matches `Paper` without a wildcard must add three arms. freemapper and freemapper-courseplan only
  construct `Paper::A3` or `Paper::A4`.
- The variants are new public API of a 0.x crate: minor version step, changelog entry.

Source: agreed with the user 2026-11-28 (freemapper-print ADR 0079).

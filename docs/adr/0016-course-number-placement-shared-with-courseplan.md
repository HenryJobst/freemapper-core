# 0016. Course number placement shared with courseplan

Date: 2026-11-10
Status: Accepted

## Context
`placement::place_course_numbers` places the numbers of a whole course with the search that
freemapper-print already uses (`best`). freemapper-courseplan has had its own copy, `numbers::auto_place`
(courseplan ADR 0030 to 0032), and switches to the core function. Before it does, both implementations ran the
same cases: `crates/freemapper-course/tests/fixtures/placement/cases.toml` (33 cases, a byte copy in
courseplan). Reading the code had predicted six places where they could differ (D1 to D6). The first 24 cases
agreed everywhere, so nine probe cases were added, and about 10 000 generated cases (sweeps over leg angles,
bends, rule minimums, symbol ties, and 400 random courses with map clutter) were run on both sides and thrown
away afterwards.

## The six predicted differences

| | Where | courseplan (old) | core | Reachable? | Ruling |
|---|---|---|---|---|---|
| D1 | candidate rounding | rounds `preferred + 15·s`, s = -11..12, then normalises | rounds `preferred + 15·k`, k = 0..23 | Only when `preferred × 10` ends in exactly .5 (7.25 gives 352.2 in the old code, 352.3 in core). Preferred directions come from `atan2` and are never exactly x.x5, so no case can show it | core |
| D2 | leg trim at the own circle | trims at `r` | trims at `r + half_line` | Yes, when a bend lies within a line width of the circle and the gap is below `half_line` | core |
| D3 | point symbol at exactly 0.5 mm | hit at `<= 0.5` | `Circle` hits at `< radius` | No difference: the harness passes a point as a zero-length `Segment` with half width 0.5, which hits at `<=` | both agree, no code change |
| D4 | rule minimum with more than two decimals | tests "behind" at the unrounded minimum, stores the rounded one | tests at the rounded minimum | Yes, 0.331 and 0.333 | core |
| D5 | no legs | 45° | `current`, which `place_course_numbers` sets to `NO_LEGS_DIRECTION_DEG` (45°) | Same value; case `no_legs` agrees | core |
| D6 | tie order and last key | direction-major, plain `<` on direction | gap-major, an epsilon of 1e-9 on every key | Only for keys within 1e-9. Directions are rounded to 0.1°, so none | core |

D3 has one trap for later probes: courseplan's harness reads map symbols through the `.omap` model, which
stores coordinates in whole micrometres. A symbol placed a few ulps away from a tie lands on the tie in the old
code and not in core. That is a property of the harness, not of `auto_place`. The tie itself, on the
micrometre grid, is hit on both sides (`point_symbol_at_half_mm_beside_box`, plus the existing point and line
cases).

## Cases where the two implementations differ

| Case | core | courseplan (old) | Ruling | Reason |
|---|---|---|---|---|
| `bend_inside_circle_line_gap_0_01` | 0° at gap 0.01 | 90° | core | D2. The first bend of the outgoing leg lies 0.05 mm outside the circle, inside its line width. With a gap of 0.01 mm the box is closer to the circle line than `half_line`. The old trim leaves 0.05 mm of leg inside the circle line and calls that a conflict. That piece belongs to the circle, which the box cannot touch (courseplan ADR 0032: the leg starts outside the circle line). |
| `bend_inside_circle_line_gap_0_1` | 0° at gap 0.1 | 90° | core | D2, same reason, bend at 3.1 mm. |
| `rounded_min_behind_test_0_333` | 0° at gap 0.34 | 286.1° at gap 0.34 | core | D4. The rule minimum 0.333 mm is stored as 0.34 mm. Core asks "is behind blocked" for the gap it stores. The old code asked for a box at 0.333 mm, found a touch of the outgoing leg that does not exist at 0.34 mm, and moved the number away from behind. courseplan ADR 0031 and 0032: the value evaluated is the value stored. |
| `rounded_min_behind_test_0_331` | 0° at gap 0.34 | 286.1° at gap 0.34 | core | D4, same reason (0.331 rounds up to 0.34). |

All other 29 cases give the same directions and gaps within 1e-9 on both sides. That includes every case with
the default gap 0.5 mm and ordinary legs, so nothing in ordinary use moves. Both differences need a gap or a bend
in the range of a line width (0.175 mm at sprint scale) or a rule minimum with three decimals, which is rare.

## Decision
- Core's behaviour is the reference in all four differing cases. No change to `best`, `Target::directions`,
  `candidate` or `GapRange` was needed, so nothing needs freemapper-print's agreement beyond the agreement of
  2026-11-10 that created this module.
- `expected` is filled in for all 33 cases with these values. Core's `placement_cases_match_expected` checks them.
- courseplan's harness checks the same file against `auto_place` until the switch, with the four cases above in
  a documented allow-list (`KNOWN_DISAGREEMENTS`). The harness fails when a listed case agrees, so the list
  disappears with the old code.
- `large_map_places_a_course_quickly` (core, `tests/placement_cases.rs`) places 40 numbers on 1000 polylines of
  100 segments (100 000 map segments in 400 × 400 mm) with gaps 0.5 to 5.0 mm. The bound is the larger of 2 s
  and 30 times the old release time: three times as long as the old code in release mode would be the target,
  times ten for a debug build.

| Time | Value |
|---|---|
| courseplan `auto_place`, release | 1.1475 s |
| core `place_course_numbers`, release | 0.030 s |
| core `place_course_numbers`, debug | 0.077 s |
| bound in the test | 34.4 s |

## Consequences
- After the switch, courseplan positions move in exactly the two situations above: a bend within a line width of
  a circle (with a gap below that width), and a three-decimal rule minimum whose rounded gap clears a leg the
  unrounded one touched. Examples and projects with whole or half-millimetre gaps
  do not change (the `luebeck-sprint` fixture agrees).
- Probes for exact ties on map symbols must use the micrometre grid of the `.omap` model, or they measure the
  harness.
- D1 and D6 are arithmetic differences without a reachable case. If a later change makes an exact tie reachable
  (for example preferred directions taken from user input), core rounds the sum `preferred + 15·k`, as ADR 0031
  of courseplan describes it.

Source: agreement with freemapper-print and freemapper-courseplan of 2026-11-10; courseplan ADR 0030 to 0032;
freemapper-print ADR 0038; Task 4 of the plan `core-placement-and-symbols`.

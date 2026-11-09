# Course fixtures

Copied from freemapper-print, commit `918cec2` (`crates/freemapper-print/tests/fixtures/courses/`), together
with the reader tests that use them (`tests/read_courses.rs`, `tests/read_ppen.rs`). The fixtures that only
freemapper-print's own tests use (the Lübeck courses on its map fixture) stayed there.

The three IOF XML files were written by hand for freemapper-print on 2026-11-09 (its M3 Task 2), following the
public IOF data standard 3.0 `CourseData` structure. They contain made-up data and are released under the
same licence as this repository (GPL-3.0-or-later). The IOF datastandard-v3 examples were not copied: their
licence could not be confirmed.

The Purple Pen fixtures in `ppen/` come from Purple Pen's repository under its BSD licence; `ppen/README.md`
names source, commit and licence.

| File | Purpose |
|---|---|
| `y-up.xml` | Two courses; `MapPosition` values with y pointing up, as Purple Pen, Condes and OCAD write them. The reader keeps the values; detection happens later. Also `Position` (lat/lng), length and climb, a crossing point. |
| `text-position.xml` | A score course with a `MapTextPosition` (an offset from the circle centre) on a control. |
| `forked.xml` | Two courses of one `CourseFamily` (a fork), a plain course, and a course with alternative controls (a `CourseControl` with two `Control` children). |

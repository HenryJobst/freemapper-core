# 0024. The course overprint moves to the render crate

Date: 2026-11-11
Status: Accepted

## Context
The browser editor of freemapper-courseplan draws course symbols itself, with sizes of its own (circle,
start triangle, finish, gaps). The sheet that freemapper-print prints draws them by the IOF rules. A parity
test and a print preview in the editor need one drawing, not two. The overprint code of print takes a
`Course`, a `FittedEvent`, the number positions and the page transform, and returns shapes of the page model.
All of that is core types already, except one struct of sizes, `CourseSymbolSizes`.

## Decision
- The module `overprint` of `freemapper-render` holds the course overprint: `overprint(course, event,
  numbers, to_page, sizes, course_factor)` returns an `Overprint` with the lower and the upper purple.
  `on_map` places the lower layer among the map primitives. The helpers print's number placer and mask
  check use (`labels`, `leg_lines`, `stop_symbols`, `number_size`, `applies`, `control_count`,
  `full_strength`, `full_strength_priority`) are public.
- `CourseSymbolSizes` moves in with it. The rule set that fills it from a profile stays in print; print
  re-exports the struct from its `rules` module.
- The crate depends on `freemapper-course` for the course model and the fit. The drawing does not change: the
  example PDFs of print stay byte-identical.
- The scale policy (which size factor a print scale gets) stays in print. The caller passes `sizes` and
  `course_factor`, and a `to_page` whose scale is the geometry factor.
- Print keeps the tests that need its rule sets or its page builder.

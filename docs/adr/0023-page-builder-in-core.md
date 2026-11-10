# 0023. The page builder moves to core

Date: 2026-11-11
Status: Accepted

## Context
ADR 0019 left freemapper-print's page builder (`print.rs`) in print, and ADR 0022 built the SVG backend and the
hit test on the `RenderedPage` that builder makes. A browser editor that wants to show the page the PDF prints
then still needs print's code to get from a `MapDocument` to a `RenderedPage`. The builder itself knows nothing
about map kinds. Only its first lines do: they look up the print scale in print's scale tables (map kind,
allowed scales, IOF size factors) and turn it into a symbol factor and a geometry factor.

## Decision
- The module `build` of `freemapper-render` takes the builder: `build_page(map, spec)` returns the
  `RenderedPage` and a `BuildReport`. `build_page_within(map, spec, page_budget)` does the same with another
  page budget, for tests.
- The scale policy stays in freemapper-print. `PageSpec` takes `symbol_factor` and `geometry_factor` as plain
  numbers, with paper, orientation, centre, frame margin and whether to draw the information block. The
  builder does not check the factors. Print computes them as before and passes them in.
- `BuildReport` has the page fields of print's `PrintReport` under the same names. Print wraps it and adds
  `print_scale` and `non_standard_scale`. `BuildError::Symbols` wraps the `OmapError` from enlarging the
  symbols, with print's message.
- `Paper`, `Orientation`, `PAGE_MARGIN_MM`, `MAX_UNITS_PER_PAGE`, `frame_box`, the tag constants
  (`SHEET_TAG`, `SCALE_BAR_TAG`, `BASE_TAG`, `BASE_PART`) and the tag tests `is_sheet_object`,
  `is_base_object` and `is_north_line` move along and are public. `Paper` and `Orientation` keep their serde
  derives, so the crate now depends on `serde`.
- The code moves without a change in behaviour. Its unit tests move with it. A test that needed a print scale
  now passes the factors print computes for that scale. Tests of the scale tables, the footer and the PDF
  report stay in print.

## Consequences
- An editor gets the PDF's page from a map with core alone, and can draw it as SVG and hit-test it.
- Print's `render_page` shrinks to the scale policy plus a call to `build_page`, and a fix to the builder needs
  a core release (ADR 0006).
- `Primitive::object` counts the objects the builder draws, in the order of the map parts, without the sheet
  objects it leaves out and the objects it skips. An editor that maps a hit back to a map object has to repeat
  that count, or the builder reports the order in a later change.
- A caller can pass factors print would refuse (zero, negative, not finite). The builder does not catch them.
  What a page drawn with such factors looks like is undefined.

Source: print plan `2026-11-10-render-to-core.md`, freemapper-print `print.rs`, ADR 0019, ADR 0022.

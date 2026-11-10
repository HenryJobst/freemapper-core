# 0021. SVG backend and hit test

Date: 2026-11-11
Status: Accepted

## Context
The browser editors of freemapper-print and freemapper-courseplan show the page the PDF prints and have to
know which map object lies under the pointer. ADR 0019 kept backends apart from maps so that an SVG backend
can draw the same `RenderedPage` as the PDF one. The page builder still lives in freemapper-print, so
`RenderedPage` keeps its fields; everything below is derived from what the page already holds.

## Decision
- `RenderedPage::mapping` returns a `PageMapping` built from `centre_mm` and `page_mm`. It converts between
  the map in mm at the print scale and the page in mm, both with y down: page = map + page size / 2 − centre,
  the offset freemapper-print's page builder moves every primitive by. The geometry factor is not part of the
  page; a caller turns an OOM coordinate into map mm at the print scale by dividing by 1000 and multiplying by
  it.
- `RenderedPage::hits_at(p, tolerance_mm)` tests the map primitives from the last painted to the first and
  returns each object once (`Hit { object, symbol, priority }`). A fill counts by its fill rule, a stroke within
  half its width plus the tolerance (ends and corners count as round), a text inside the box of its glyph run
  (Arimo advance widths, ascent and descent, turned with the text), a pattern shape only inside its even-odd
  clip. The tolerance widens strokes only. A point outside `clip`, or inside the masked area of any
  `MapClip` (winding number 0 by the nonzero rule), has no hits. The overprint and the sheet layer are not
  tested. Base area and north lines are skipped: they run across the whole map and would lie under almost
  every point.
- `svg::write_svg(page, view)` writes one SVG document in the PDF's paint order: a white rectangle for the
  paper; inside a clip path of `clip` the map primitives, the lower overprint layer before the first primitive
  of `below_priority`, the rest of the map, the upper overprint layer; then the sheet items, unclipped.
- The viewBox is in page mm, so SVG coordinates, `hits_at` and the page mapping use the same numbers.
  `SvgView { rect, zoom }` chooses the part of the page (`None` for all of it) and the pixels per mm of the
  document's width and height; `SvgView::default()` shows the whole page at 4 px/mm.
- Colours: CMYK becomes sRGB by r = 255 · (1 − c) · (1 − k), g and b alike from m and y. The PDF keeps CMYK.
  The RGB that a map file may carry is not used, so screen and PDF come from the same numbers.
- Texts become one `<path>` of glyph outlines each, from the Arimo data the PDF embeds, advanced by the same
  widths and without kerning, as the PDF shows them. There are no `<text>` elements, so the browser needs no
  font.
- Each mask is one `<clipPath>` with `clip-rule="nonzero"` from `MapClip::rings`; a run of map primitives sits
  in one group per mask, and nested clips intersect, as successive clips do in the PDF. Pattern clips are
  even-odd clip paths, shared by consecutive shapes with the same rings.
- Every overprint element and every pattern group in the overprint gets `mix-blend-mode:multiply`. Overlapping
  overprint shapes then multiply with each other as well as with the map, as each object does under the PDF's
  Multiply graphics state.
- Images are PNG data URIs (the `image` crate), stretched to their rectangle.
- A primitive whose box (`bounds::shape_bounds`) misses the view, or for map and overprint the view inside
  `clip`, is left out. Each element costs one unit plus one per eight points; a document beyond `MAX_UNITS`
  (2 000 000, four times freemapper-print's page budget) fails with `SvgError::TooLarge`. A number that is not
  finite fails with `SvgError::NotFinite`, as in the PDF.
- Numbers have at most three decimals (1 µm) and no trailing zeros. Clip ids are `frame`, `m<i>` and `c<i>`,
  numbered in order of use. The same page and view always give the same text.

## Consequences
- An editor can draw a page and pick objects on it with the same `RenderedPage` the PDF uses, without a PDF
  viewer and without fonts.
- The golden SVGs in `crates/freemapper-render/tests/golden/` change with any change to drawing.
  `UPDATE_GOLDEN=1` writes them again; the diff is reviewed like code.
- The ids are fixed. Two SVGs inlined into one HTML document share one id space, so an editor shows each SVG
  as its own document (`<img>`, `<object>`) or rewrites the ids.
- Screen colours follow the simple formula, not a colour profile, and differ from what OOM shows on screen.

Source: print plan `2026-11-10-render-to-core.md` (Tasks 3 and 4), ADR 0019.

# 0033. The compact path writer is public

Date: 2026-11-13
Status: Accepted

## Context
ADR 0032 made `freemapper-render`'s PDF backend write paths compactly: coordinates rounded to 1/1000 point, and a
small fill or stroke written relative to its first point inside `q`, `1 0 0 1 x y cm` and `Q`. The code was private
to `pdf.rs`. freemapper writes PDFs of its own with `pdf_writer::Content` (pdf-writer 0.15.0, the version core pins),
from paths it has already turned into page points with y up, and wants the same savings without a copy of the code.
Its writer sets colour, line width, cap, join, miter limit and the clip itself and paints after the path. It must not
have to close the `q` of a small shape.

## Decision
- The new public module `freemapper_render::pdf_path` holds the path writer. `write_path(content, rings, closed,
  paint)` takes the rings of one path as `&[R]` with `R: AsRef<[(f32, f32)]>` (so `&[&[(f32, f32)]]` and
  `&[Vec<(f32, f32)>]` both work), in page points with y up. It writes `m`, `l` and, with `closed`, `h` for every
  ring, then the operator `paint` names, and returns `Result<(), NotFinite>`.
- `Paint` is an enum: `FillNonZero` (`f`), `FillEvenOdd` (`f*`), `Stroke` (`S`), `ClipNonZero` (`W n`) and
  `ClipEvenOdd` (`W* n`). An enum fits the core's writer better than a closure. Since the call writes the operator
  itself, it opens and closes the `q … Q` of a small shape around it, and the caller never sees an open graphics
  state. A clip is never written relative, because a clip set inside `q … Q` would end with it.
- The rules are those of ADR 0032 and use its constants, public now: `STEPS_PER_PT` (1000), `SMALL_MM` (7) and
  `MIN_RELATIVE_POINTS` (4). `write_path` compares the extent of the points in points, with `SMALL_MM · 72 / 25.4`.
  The translation `1 0 0 1 x y cm` does not scale, so a line width, dash pattern or cap the caller set before the call
  keeps its size inside the block.
- `write_path` checks every coordinate before it writes. One that is infinite or not a number returns `NotFinite` and
  writes nothing, since a `Content` cannot take bytes back. Empty rings are left out; a path without points writes
  nothing, not even the operator.
- `pdf.rs` writes its fills, strokes, pattern clips and mask clips with the same code, through a crate-private
  `write_in` and a `Frame`. The core's page is in mm with y down, and ADR 0032 computes a relative point as
  `(p − origin) · 72 / 25.4` in mm before scaling. Converting to points first and subtracting there would differ in
  the last bits and, rarely, in a rounded digit. A `Frame` keeps the core's order of operations, scale and y flip; the
  public function uses the identity frame. The core's own behaviour for numbers that are not finite stays: it writes 0,
  notes it and refuses the page with `PdfError::NotFinite`.

## Consequences
- freemapper calls `write_path` after setting colour and stroke state, with pdf-writer 0.15.0, and maps `NotFinite`
  to its own error. A shared fix of the path writer reaches both apps with the next core tag.
- Every PDF the core writes stays the same byte for byte. The golden file `tests/golden/compact-paths.txt` holds the
  content stream of a page with every kind of path, written both ways, from the code before the move. The test
  `write_path_writes_what_the_page_writer_writes` checks that the public function, given page points, writes what
  `write_pdf` writes for the same shape in mm. The 18 PDFs of the ignored pixel test (the forest sample, the
  Gleisdreieck, Rummelsburg, Lübeck and Viktoriapark maps, compact and as polylines) were the same before and after
  the move.
- `Paint`, `NotFinite` and the three constants become public API of a 0.x crate; changing them later needs a minor
  version step and a changelog entry for the apps.
- A caller that gives coordinates already rounded or in another unit than points gets the rounding of 1/1000 point
  anyway; it must pass points.

Source: user, 2026-11-13 (freemapper reuses the compact path writer instead of copying it; the paint operator as an
argument so that the caller never closes `Q`; points as `f32` with y up); ADR 0032.

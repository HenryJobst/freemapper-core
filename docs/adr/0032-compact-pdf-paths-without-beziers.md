# 0032. Compact PDF paths without Béziers

Date: 2026-11-13
Status: Accepted

## Context
Curves reach the backends flattened. `geometry.rs` cuts each Bézier into chords of at most 1 mm, `draw.rs` draws the
circle of a point symbol as a polygon of 48 sides, and the overprint draws a control circle with 72. Dashes, offsets,
patterns and culling (ADR 0031) all work on these polylines. `pdf.rs` wrote every point as an `l` operator, its
numbers with about five decimals. freemapper-print's training sheet A of the Gleisdreieck sprint map (A4, 1:4000) had
10.3 MB of page content with 452 000 `l` operators, 2.5 MB after deflate. 87 % of its points belong to rings less than
1 mm across, the dots of point symbols. Rounding every number of the page to two decimals would bring the file to
1.9 MB; one decimal moves points by up to 0.018 mm.

The first idea was to fit cubic Béziers to the polylines when the PDF is written. It was built and measured, with
the variants in the table below, before this decision was taken.

## Decision
- `pdf.rs` writes the paths of fills, strokes, clip rings and mask rings compactly, still point by point with `m`, `l`
  and `h`. Nothing upstream changes. `RenderedPage`, the shapes, culling, the clip blocks and the SVG backend stay as
  they were. Texts, images and the clip rectangle of the page are written as before.
- Path coordinates are rounded to 1/1000 point (0.00035 mm). pdf-writer prints the shortest decimal that gives the
  same `f32`, so a path number has at most three decimals instead of four or five. Line widths, colours, text matrices
  and image placements keep their precision.
- A small shape is written relative to its first point: `q`, a translation `1 0 0 1 x y cm` to that point, the path,
  the paint operator, `Q`. A fill or stroke is small when it has at least four points and all of them fit in a square
  of 7 mm (`pdf::SMALL_MM`). The dots and other parts of a point symbol, and the control circles, then give the same
  numbers at every place, and deflate stores them once. Clip and mask rings keep page coordinates, because a clip set
  inside `q … Q` would end with it. A shape with a coordinate that is not finite is never small, so `Transform::num`
  sees its numbers as before and refuses the page.
- The PDF holds no Béziers. Fitting them to the polylines saves 8 % on top of the small shapes, causes five times the
  pixel differences in poppler and needs a module of its own.

## Consequences
- freemapper-print's 13 example sheets of Remo Madella's Gleisdreieck and Rummelsburg sprint maps shrink from
  25.8 MB to 5.37 MB together, each by 76 to 82 %. Training sheet A goes from 2 564 KB to 533 KB.
- Writing is faster on large pages, because deflate has less to compress: in release builds `write_pdf` takes 0.21
  instead of 0.68 s for the Gleisdreieck map on a cropped A4 page and 0.23 instead of 0.81 s for Rummelsburg.
- The points stay where they were, moved by the rounding only. Rasterised directly at 150 dpi with poppler, 66 of the
  41 million pixels of the 13 sheets differ by more than 32 levels, by up to 191; on sheet A none, by up to 29. The
  largest are two hatch lines one pixel wide that poppler snaps to the next pixel column after their coordinates moved
  by less than 1/2000 point.
- The variants measured, over the 13 sheets (size, and pixels more than 32 levels off in poppler at 150 dpi):

  | Variant | Size | Pixels |
  |---|---|---|
  | Points as in 0.22.0 | 25.8 MB | 0 |
  | Béziers fitted to every path, numbers rounded | 6.94 MB | 132 495 |
  | Small shapes up to 3 mm relative, the rest fitted | 4.97 MB | 1 952 |
  | Small shapes up to 7 mm relative, the rest fitted | 4.92 MB | 393 |
  | The same, cubics only with a radius of 10 mm or more | 4.92 MB | 337 |
  | The same, straight runs merged into one line, no cubics | 4.99 MB | 422 |
  | Small shapes up to 3 mm relative, nothing fitted | 5.47 MB | 66 |
  | Small shapes up to 7 mm relative, nothing fitted (chosen) | 5.37 MB | 66 |

  The fits kept every path within 0.005 mm of its polyline. poppler, the renderer of most Linux viewers, draws a
  Bézier with chords of about 1 mm at 150 dpi, so it drew the fitted dots of point symbols as diamonds and patterns of
  tiny dots lighter. Cairo and Ghostscript at 150 dpi gave 34 045 and 27 037 such pixels for the first variant, Quartz
  239; averaged from 600 dpi, poppler gave 11. Not fitting small paths saved poppler but left the Gleisdreieck page at
  87 % of its old size until small shapes were written relative. A minimum length of 1 mm per Bézier changed
  nothing; a minimum radius did. Of the 8 % that fitting still saved, merging straight runs gave most and the cubics
  1.5 %. A shape needs four points to be written relative: with 16 the Gleisdreieck page grew by 3.5 %.
- PDFs change byte for byte. An app that compares committed PDFs writes them again; in freemapper-print these are the
  three tests in `tests/overprint_outputs.rs` and `export_project_writes_the_pdf_the_cli_writes` in the CLI's `mcp.rs`.
  Its other tests pass unchanged.
- If the 7 mm are wrong for a map, a large shape written relative costs a few bytes or a small one is not shared. The
  picture stays the same either way.
- The ignored test `compact_paths_keep_the_picture` (next to `culling_keeps_every_pixel`) writes each page both ways
  and rasterises them with `pdftoppm` at 150 dpi, and at 600 dpi averaged in 4 × 4 blocks. It fails if more than one
  pixel in 10 000 differs by more than 32 levels either way, or a pixel of the averaged images by more than 64.

The path writer is public since ADR 0033, as `freemapper_render::pdf_path`.

Source: user, 2026-11-13 (smaller PDFs without visible change; Béziers dropped after the measurements, path numbers in
three decimals); freemapper-print's example sheets; ADR 0031.

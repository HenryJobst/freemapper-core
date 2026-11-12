# 0031. The backends leave out what cannot show

Date: 2026-11-12
Status: Accepted

## Context
The page builder draws every object of the map, and the PDF backend wrote every primitive into the page inside
the clip of the frame. A sheet that shows part of a map carried the rest of it too, and the viewer clipped it
away. freemapper-print's example sheets of a 1:4000 sprint map of 1.4 MB are 3.5 to 3.6 MB each. The SVG backend
already left out shapes outside its view (ADR 0022), by `shape_bounds`, which grows a stroke by half its width
only and boxes a text by its advance width and the font's ascent.

## Decision
- `pdf::write_pdf` and `MultiPagePdf::add_page` leave out each map primitive and overprint shape whose ink box
  lies wholly outside the visible area. The visible area is the page clip, its corners in either order, cut to
  the page. Masks play no part, so a shape under a mask is written as before.
- The ink box (`bounds::ink_bounds`) holds every point a shape can colour:
  - a fill: the box of its points, because a polygon lies inside their hull;
  - a stroke: the box of its points grown by half its width, times √2 for a square cap and times the miter limit
    √5 for a miter join;
  - a text: the boxes of its glyph outlines at their pen positions, control points included, turned with the
    text; line framing adds half its width times √2, the framing miter limit;
  - a clipped shape (an area pattern): where the box of its clip meets the box of the inner shape. A pattern whose
    area lies outside the frame goes, even if its lines would run across the frame, because its own clip hides
    them there.
- Each ink box grows by `INK_SLACK_MM`, 5 mm, on every side. A box that touches the visible area counts as inside.
  A shape with a number that is not finite gets an unbounded box and stays.
- The slack is needed because an anti-aliased clip tints each pixel the frame edge runs through with every shape
  that covers part of that pixel, even a shape wholly outside the frame. poppler, Ghostscript and Cairo all do
  this. With a slack of 1 mm, sheets rasterised at 10 dpi (2.54 mm a pixel) differed in up to 26 pixels along the
  frame edge; at 18 dpi and above they were identical. Stroke adjustment, the thinnest line a device draws and
  glyph hinting can move an edge by one more pixel. Five millimetres are two pixels at 10 dpi, where an A4 page is
  a thumbnail 117 pixels tall.
- A shape that is left out is still painted, into a stream that is thrown away. A number on it that a PDF cannot
  hold still refuses the page (`PdfError::NotFinite`), and a character of its text without a glyph still shows up
  in `PdfInfo::missing_glyphs`. Its glyphs stay out of the embedded font subsets.
- The sheet layer and its images are not culled. The frame does not clip them, and they are small.
- `RenderedPage`, `BuildReport` and the hit test stay as they were. The builder still draws the whole map, so the
  object counts, `source_objects` and the positions number placement reads do not change.
- The SVG backend takes the same rule instead of `shape_bounds`. It now keeps a shape whose miter tip or glyph
  reaches into the view, and every shape within 5 mm of the view. In the golden `tiny-page-view.svg` this adds a
  line, a hatch line and a sheet glyph, all outside the viewBox. The browser shows the same pixels.

## Consequences
- freemapper-print's example sheets of the 1.4 MB sprint map (A4 at 1:4000, A3 at 1:3000, two training courses)
  shrink by 15 to 30 %, from 3.6 or 3.7 MB to between 2.6 and 3.2 MB. The forest sample on a page that shows one
  corner of it goes from 252 KB to 38 KB. A sheet whose map fits the frame stays the same byte for byte.
- The ignored test `culling_keeps_every_pixel` writes each page both ways and compares the images `pdftoppm`
  makes of them byte for byte. On the forest sample (cropped at 1:2000, 1:5000, 1:7500 and 1:10000, and once with a
  mask and an overprint across the frame) and on two 1:4000 sprint maps cropped to A4, no pixel differs at 6, 10,
  18, 36, 72, 150 and 300 dpi. `FREEMAPPER_CULLING_MAPS` adds maps that must stay out of the repository.
  freemapper-print's example sheets, written with and without culling, also give the same images at 10 and 150 dpi.
- PDFs of pages where the map reaches beyond the frame change byte for byte. An app that compares committed PDFs
  writes them again.
- Writing a page takes about as long as before. A shape that is left out is still formatted, only not
  compressed and stored.
- If the rule is wrong somewhere, a sliver of ink is missing at the frame edge: a miter tip, a glyph overhang, a
  tinted edge pixel. At 5 dpi and below the edge pixels of a thumbnail may differ.
- Culling removes only what lies outside. On the cropped A4 sheet of the 1.4 MB sprint map, 96 % of the points
  still written lie inside the frame, so the file stays at 3.1 MB. That is visible map: about 570 000 points,
  because curves reach the backends flattened, each written as two numbers of up to seven digits. Writing less of
  it needs a decision of its own.

Source: user, 2026-11-12 (the size fix for cropped sheets); freemapper-print's example sheets; ADR 0022.

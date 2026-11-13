# Golden files

Expected output of `tests/svg.rs`, and of one unit test in `src/pdf.rs`.

## tiny-page.svg, tiny-page-view.svg

- Source: written by `svg::write_svg` from the hand-built page `tiny_page()` in `tests/svg.rs`, on branch
  `render-svg` (2026-11-11). `tiny-page-view.svg` was written again on branch `crop-culling` (2026-11-12), when
  the SVG backend took the ink box of ADR 0031: it gained the blue line, one hatch line and the sheet glyph "T",
  which lie outside the view but within its 5 mm slack.
  `UPDATE_GOLDEN=1 cargo test -p freemapper-render --test svg` writes them again; review the diff before
  committing.
- Licence: GPL-3.0-or-later, like the crate. The text paths are outlines of the Arimo fonts in
  `assets/fonts/` (SIL OFL 1.1).

## compact-paths.txt

- Source: the content streams `pdf::write_pdf_with` wrote for the hand-built page `path_page()` in `src/pdf.rs`,
  with compact paths and with every point as it comes, before the path writer moved to `pdf_path` (v0.25.0, commit
  599add0, written on branch `pdf-path-api`, 2026-11-13). The test `the_path_writer_writes_the_bytes_it_wrote_before`
  compares with it. `UPDATE_GOLDEN=1 cargo test -p freemapper-render --lib the_path_writer` writes it again; review
  the diff before committing.
- Licence: GPL-3.0-or-later, like the crate.

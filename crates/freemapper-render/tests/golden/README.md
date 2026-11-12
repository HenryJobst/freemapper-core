# Golden SVGs

Expected output of `tests/svg.rs`.

## tiny-page.svg, tiny-page-view.svg

- Source: written by `svg::write_svg` from the hand-built page `tiny_page()` in `tests/svg.rs`, on branch
  `render-svg` (2026-11-11). `tiny-page-view.svg` was written again on branch `crop-culling` (2026-11-12), when
  the SVG backend took the ink box of ADR 0031: it gained the blue line, one hatch line and the sheet glyph "T",
  which lie outside the view but within its 5 mm slack.
  `UPDATE_GOLDEN=1 cargo test -p freemapper-render --test svg` writes them again; review the diff before
  committing.
- Licence: GPL-3.0-or-later, like the crate. The text paths are outlines of the Arimo fonts in
  `assets/fonts/` (SIL OFL 1.1).

# Golden SVGs

Expected output of `tests/svg.rs`.

## tiny-page.svg, tiny-page-view.svg

- Source: written by `svg::write_svg` from the hand-built page `tiny_page()` in `tests/svg.rs`, on branch
  `render-svg` (2026-11-11). `UPDATE_GOLDEN=1 cargo test -p freemapper-render --test svg` writes them again;
  review the diff before committing.
- Licence: GPL-3.0-or-later, like the crate. The text paths are outlines of the Arimo fonts in
  `assets/fonts/` (SIL OFL 1.1).

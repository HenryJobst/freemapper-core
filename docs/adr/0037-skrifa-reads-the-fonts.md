# 0037. skrifa reads the fonts

Date: 2026-11-15
Status: Accepted, completes ADR 0036

## Context
`ttf-parser` is unmaintained (RUSTSEC-2026-0192) and has no upgrade. ADR 0036 ignored the advisory until a
replacement was decided. freemapper-render read the four embedded Arimo faces with it: the glyph of a character,
advance widths, glyph outlines for the SVG and the overprint tests, glyph boxes, the cap height and the values of the
PDF font descriptor. Only `freemapper-render/src/fonts.rs` used it.

## Decision
- freemapper-render reads the fonts with `skrifa` 0.49.0 (MIT OR Apache-2.0, fontations of Google Fonts), pinned
  exactly with `default-features = false` and `std`. 0.50.0 is younger than two weeks (ADR 0004). The new
  dependencies (`read-fonts`, `font-types`, `yoke`, `bytemuck_derive` and others) are all permissive and older than
  two weeks.
- The public functions of `fonts.rs` do not change. Maps, SVGs and PDFs come out byte for byte as before; every
  golden stayed as it was. Two differences in skrifa are handled in `fonts.rs`: it closes a contour without the line
  back to its start, so the outline sink adds that line as `ttf-parser` did, and it gives an empty glyph (a space) the
  box 0, 0, 0, 0, which the glyph extent skips.
- The ignore for RUSTSEC-2026-0192 leaves `deny.toml`.

## Consequences
- `cargo deny check` passes without an ignore.
- freemapper-print has its own dependency on `ttf-parser`; it moves in its own step after this release.
- skrifa is a larger library than `ttf-parser`: seven more crates in `Cargo.lock`. It reads the font tables lazily and
  only for these faces.
- Cost if wrong: a skrifa release that changes an outline or a metric would change the goldens. The exact pin and the
  goldens catch that.

Source: freemapper-core ADR 0036; the advisory RUSTSEC-2026-0192.

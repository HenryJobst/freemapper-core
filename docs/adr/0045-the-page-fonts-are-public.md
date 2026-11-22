# 0045. The page fonts are public

Date: 2026-11-22
Status: Accepted

## Context
`freemapper-render` embeds Arimo into its PDFs as a subset of Type0 / CIDFontType2 fonts (freemapper-print ADR 0017).
The code sat in `fonts.rs` behind `pub(crate)`. freemapper writes the PDF of its map page itself with `pdf_writer`
(ADR 0033) and used Helvetica with WinAnsi encoding and no embedding. Letters outside WinAnsi (ł, ą, ę, ś, ź, ż, č, ř,
š, ž) fell back to a base letter or `?`, so "Słubice" printed as "Slubice" (freemapper issue 267). The same holds for
Czech, Baltic and Hungarian names.

## Decision
- `fonts::PageFonts` and `fonts::FontObjects` are public, with the surface the render crate already uses and nothing
  more: `PageFonts::new`, `resource_name(face)`, `encode(face, text) -> Vec<u8>` (two-byte CIDs for a show-text
  operator), `finish(self, &mut Ref) -> FontObjects`, `FontObjects::resources`, `FontObjects::write(&mut Pdf)` and
  the field `FontObjects::missing`, the characters no face had a glyph for, so that an app can warn.
- Three types stay three types. An app writes its content stream before it knows which glyphs it needs, so the
  collecting step (`PageFonts`) and the writing step (`FontObjects`) cannot be one object. This is the order the core's
  own page writer follows.
- `PageFonts::note_missing` stays crate-private. The core uses it to merge a probe pass; an app has no such pass.
- The PDF the core writes does not change.

## Consequences
- freemapper can drop its Adobe metrics tables and the WinAnsi code, take widths from `fonts::text_width` and show
  every Latin Extended-A letter. Line breaks and text widths move slightly, because Arimo has Arial's metrics.
- The two types and their methods become public API of a 0.x crate; changing them needs a minor version step and a
  changelog entry for the apps. The pdf-writer types in the signatures (`Name`, `Ref`, `Pdf`) follow the version the
  core pins, as with ADR 0033.
- The test `tests/page_fonts.rs` uses the writer the way an app does.

Source: freemapper-b8 (freemapper issue 267), agreed with the user 2026-11-22; ADR 0033.

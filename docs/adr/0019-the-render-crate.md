# 0019. The render crate

Date: 2026-11-10
Status: Accepted

## Context
freemapper-print draws OOM symbols itself (print ADR 0014: copied from freemapper `743ce4b`) and writes the
result as PDF. The training map of freemapper and a later SVG backend need the same drawing code. Copying it
again would repeat what ADR 0012 ended for the reader.

## Decision
- `freemapper-render` takes print's module `render/` with its behaviour unchanged: symbol drawing, the render
  page model (`page::RenderedPage`), the PDF backend (`pdf`), image decoding and the Arimo fonts with their
  metrics (`fonts`). Paths below the crate root are the old ones below `render`.
- freemapper-print owns the crate (ADR 0005), as it owned the code.
- The only suite dependency is `freemapper-omap`. The renderer reads no project files and no course data.
  Backends take a `RenderedPage` and know nothing about maps, so an SVG backend can sit beside the PDF one.
- The page builder (`print.rs`) stays in print for now. For its sake `bounds`, `draw`, `geometry` and `symbols`
  are public modules, and `bounds::{Bounds, shape_bounds, painted_bounds}`, `draw::{KEPT_POINTS_PER_UNIT,
  primitives_within, Drawn}` and `geometry::flatten_cubic` are public. The page builder moves into the crate
  later and these can shrink again.
- Test helpers print needs (`pdf::content_of`, `draw::work_units`, `draw::BUDGET_SLACK`) sit behind the feature
  `test-support`. An app enables it under `[dev-dependencies]` only.
- `PdfMeta::new` keeps naming freemapper-print and its public GitHub link as creator (`CREDIT`), so the PDFs do
  not change. An app that wants another creator sets the field.
- The Arimo fonts (SIL OFL 1.1) and `forest-sample.omap` (a test fixture) move along, each with a README that
  names source and licence.

## Consequences
- A change to drawing now needs a core release before print sees it; during development print uses a `[patch]`
  to the local checkout (ADR 0006).
- The crate adds about 1.9 MB of fonts to every build that depends on it, as print had before.
- The wider public surface is unstable (0.x) and meant for print only.

Source: print plan `2026-11-10-render-to-core.md` (Task 1), print ADR 0014.

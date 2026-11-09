# 0012. The omap crate

Date: 2026-11-09
Status: Accepted

## Context
freemapper-print copied freemapper's `.omap` model and reader at 743ce4b and developed them further, with the
limits for hostile files (print ADR 0014, 0019). freemapper and freemapper-courseplan need the same reader.

## Decision
- `freemapper-omap` takes print's module `omap/` from commit `918cec2` unchanged: model, reader, symbol sets,
  symbol enlargement, `MAX_XML_DEPTH` and `OmapError`. Its paths stay as they were, so
  `freemapper_omap::coord::MapCoord` replaces `freemapper_print::omap::coord::MapCoord`. The tests move with
  it; only their `crate::omap::` prefixes became `crate::`.
- freemapper-print owns the crate (ADR 0005). The renderer and everything that prints stay in the app.
- ISSprOM 2019 and the school and sheet additions live in `assets/symbol-sets/` inside the crate and are
  embedded with `include_str!`. `Cargo.toml` packages them, and a test fails if a source embeds a file from
  outside the crate.
- `too_deep`, `symbol_set::attribute` and `SymbolSet::without_codes` were crate-private; print's renderer and
  tests use them, so they are now public. `without_codes` is a test aid and hidden from the docs.
- quick-xml and thiserror are pinned to the versions in print's lock file (0.42.0, 2.0.21).

## Consequences
- An app reads maps without building the renderer or the PDF writer.
- `attribute` takes a quick-xml type, so a caller uses the same quick-xml version as this crate.
- Cost if wrong: freemapper writes `.omap` too; if its model differs, the crate needs a writer and fields
  that print does not use, or freemapper keeps its own model.

Source: freemapper-print ADR 0014 and 0019, plan "core extraction stage 2" Task 1.

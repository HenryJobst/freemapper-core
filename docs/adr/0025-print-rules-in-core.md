# 0025. The print rules move to core

Date: 2026-11-11
Status: Accepted

## Context
The sizes of the course symbols per map kind and the factor a print scale gives them lived in freemapper-print
(`rules.rs`, `scale.rs`, the profile `iof-print.toml`). freemapper-courseplan draws the same symbols in its
editor (ADR 0024) and had to copy the values. Two copies of a standard's numbers drift apart.

## Decision
- The crate `freemapper-printrules` holds `rules` (the embedded profile `iof-print`, its README, the schema,
  the `RuleSet` and its checks) and `scale` (`MapKind`, `north_line_spacing_m`, `standard_enlargement`,
  `is_standard`, `symbol_factor`, `geometry_factor`, `ScaleError`). The format and the generic checks stay in
  `freemapper-rules`.
- `scale::course_symbols(kind, print_scale)` returns the `CourseSymbolSizes` and the course factor, the two
  inputs of `freemapper_render::overprint::overprint`.
- `rules::load_for_tests` and `rules::embedded_text` are behind the feature `test-support`, for an app's tests.
  `MIN_CELL_MM` and `MAX_CELL_MM` (the range of a description cell) move into the crate, because the profile
  check needs them.
- Reading `freemapper.toml` and inferring the map kind from a map stay in freemapper-print.
- The golden snapshot `tests/fixtures/rules/resolved.txt` moves with the rules.

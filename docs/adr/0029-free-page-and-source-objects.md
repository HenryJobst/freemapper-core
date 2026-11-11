# 0029. A free page size and the source position of each object

Date: 2026-11-11
Status: Accepted

## Context
courseplan's editor prints on A4 or A3 only. A map larger than A3 at map scale is cut at the frame, and the
editor has no page size to offer for it. The editor also maps a hit (`Primitive::object`) back to an object of
the map. That number counts the objects the builder drew candidates for, so it differs from the position in the
file as soon as one object is skipped (hidden, unknown symbol, sheet object not shown).

## Decision
- `PageSpec::page_mm: Option<(f64, f64)>` replaces `paper` when set. The size is given in portrait; the
  orientation applies to it as to a paper. Each side must lie between `MIN_PAGE_MM` (10 mm) and `MAX_PAGE_MM`
  (5080 mm, the 14 400 pt a PDF viewer opens). Anything else, including NaN and infinity, is
  `BuildError::PageSize`; the builder does not clamp, because a silently smaller page would cut the map.
  `Paper` stays a closed enum with `Eq`, so no caller changes.
- `BuildReport::source_objects` lists, for each object number a primitive carries, the position of that
  object in the map: parts in file order, objects in each part in order. The two numbers are equal only when
  nothing was skipped.
- Which free sizes an app offers (print has A4 and A3 in its layouts) stays the app's decision.

## Consequences
Both fields are additive; a caller that builds `PageSpec` or `BuildReport` with a struct literal and no `..`
adds the field. Callers that match `BuildError` exhaustively add the variant.

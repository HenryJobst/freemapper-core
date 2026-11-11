# 0028. The print rules use the contract's map kind

Date: 2026-11-11
Status: Accepted, amends [0025](0025-print-rules-in-core.md)

## Context
ADR 0027 put `MapKind` (names, base scale, the scales freemapper draws) into `freemapper-contract`. The print
rules had an enum of their own with the same three variants.

## Decision
- `freemapper-printrules` depends on `freemapper-contract` without its default features and re-exports
  `MapKind` from `scale`. The paths `freemapper_printrules::scale::MapKind` and `freemapper_contract::MapKind`
  name one type.
- `allowed_scales` and `proposed_contour_interval_m` come from the rule profile and cannot be methods of a
  foreign type. They are the free functions `scale::allowed_scales(kind)` and
  `scale::proposed_contour_interval_m(kind)`. This is the one API break; callers of the methods change.
- A test checks that every scale freemapper draws for a kind is in the profile's scale table for it.

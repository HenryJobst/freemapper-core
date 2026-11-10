# 0015. The georef crate

Date: 2026-11-10
Status: Accepted

## Context
Two UTM implementations exist: freemapper's `crs.rs` (series to n^6, zones 32 and 33 on GRS80, convergence,
scale factor, Newton inverse) and freemapper-print's `tm` module in freemapper-course (series to n^4, forward
only, any ellipsoid, EPSG and PROJ text recognition, tested to 1 mm against PROJ). ADR 0013 kept `tm` private
because UTM belongs to a georef crate. The region adapters (freemapper ADR 0082) need `Crs`, `BoundingBox` and
`TileId` outside the app.

## Decision
- `freemapper-georef` holds `Crs`, `LatLon`, `ProjectedPoint`, `BoundingBox`, `TileId` and the tile functions
  of freemapper's `crs.rs` and `tiles.rs`, with names, behaviour and tests unchanged. freemapper owns it.
- `tm` is the single Transverse Mercator implementation, public: `Ellipsoid`, `GRS80`, `WGS84`,
  `TransverseMercator::{utm, forward, project}` and `utm_from_crs`. It takes the series to n^6 from `crs.rs`
  (a superset of print's n^4 terms) and the ellipsoid and text recognition from print. `Crs` projects through
  it (`Crs::transverse_mercator`, `Crs::utm_zone`); the inverse stays Newton iteration on the forward
  projection. The tests of both sources pass against this one code.
- The fit stays in `freemapper-course`: it needs omap types. Only `tm` moves. freemapper-course is unchanged
  in this release and keeps its private copy until freemapper-print switches `fit` to this crate.
- Magnetic declination stays in freemapper. It needs `chrono`, the World Magnetic Model crate and freemapper's
  error type, and no other app uses it.
- No dependencies, so no age rule applies (ADR 0004), and no network access (ADR 0007).

## Addendum 2026-11-10 (v0.5.2)

`TransverseMercator::inverse(easting, northing) -> (lat, lon)` joins `forward` for every UTM zone, hemisphere
and ellipsoid: Krüger series to n^6 as in `forward`, then Newton iteration from the conformal latitude. It
replaces the finite-difference Newton loop in `Crs::inverse`, which now calls it and rejects points that do
not project back to themselves within 1 mm. Results moved by less than 1e-9 degrees.

## Consequences
- `Crs` knows zones 32 and 33 only, as in freemapper. Other zones go through `tm`; a new `Crs` variant would
  break exhaustive matches in the apps and waits for a use.
- Until freemapper-print switches, the suite holds the print copy of `tm` once more. The switch deletes it.
- Cost if wrong: n^6 instead of n^4 moves print's fitted points by nanometres at most; the 1 mm PROJ tests
  guard that.

Source: freemapper ADR 0067 and 0082, the agreement with freemapper-print of 2026-11-10, core ADR 0013.

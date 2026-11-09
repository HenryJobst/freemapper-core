# Course fixtures

Copied from freemapper-print, commit `918cec2` (`crates/freemapper-print/tests/fixtures/courses/`), together
with the tests that use them (`tests/read_courses.rs`, `tests/read_ppen.rs`, `tests/fit_courses.rs`). The Lübeck
fixtures that only freemapper-print's own tests use stayed there.

The IOF XML files were written by hand for freemapper-print on 2026-11-09 (its M3 Tasks 2 and 3), following the
public IOF data standard 3.0 `CourseData` structure. They contain made-up data and are released under the
same licence as this repository (GPL-3.0-or-later). The IOF datastandard-v3 examples were not copied: their
licence could not be confirmed.

The Purple Pen fixtures in `ppen/` come from Purple Pen's repository under its BSD licence; `ppen/README.md`
names source, commit and licence.

| File | Purpose |
|---|---|
| `y-up.xml` | Two courses; `MapPosition` values with y pointing up, as Purple Pen, Condes and OCAD write them. The reader keeps the values; detection happens later. Also `Position` (lat/lng), length and climb, a crossing point. |
| `text-position.xml` | A score course with a `MapTextPosition` (an offset from the circle centre) on a control. |
| `forked.xml` | Two courses of one `CourseFamily` (a fork), a plain course, and a course with alternative controls (a `CourseControl` with two `Control` children). |
| `luebeck-georef.xml` | Course on `../maps/luebeck.omap` (EPSG:25832) with y-up `MapPosition` and `Position` (lat/lng) on every control; control 33 has only `Position`. See below for how the lat/lng were computed. |

## How the lat/lng of `luebeck-georef.xml` were computed

Written on 2026-11-09 (M3 Task 3). Each control got a map position in OOM map coordinates (µm, y down):
S1 (−50000, −60000), 31 (20000, 30000), 32 (100000, 120000), 33 (−80000, 90000), F1 (60000, −20000). The
`MapPosition` values are these in mm with y negated (y up, as Purple Pen writes).

The map's georeferencing (scale 4000, `grid_scale_factor` 0.999751, grivation 3.19°, reference point
E 610932.039697268, N 5969932.064166221 at map (0, 0)) turns a map position into EPSG:25832 the way OOM does.
OOM's `Georeferencing::updateTransformation` (`src/core/georeferencing.cpp`) builds `translate(ref) ·
rotate(−grivation) · scale(s, −s) · translate(−map_ref)`, and Qt's `QTransform::rotate(a)` maps (x, y) to
(x·cos a − y·sin a, x·sin a + y·cos a). Written out: `E = E0 + qx·cos g + qy·sin g`, `N = N0 − qx·sin g + qy·cos g`, with
`qx = s·x`, `qy = −s·y`, `s = 4000 · 0.999751 / 1000` m per map mm and `g` the grivation. PROJ 9.8.1 (pyproj 3.8.0,
installed in a throw-away virtual environment) turned E/N into ETRS89 lat/lng:

```python
from pyproj import Transformer
inv = Transformer.from_crs("EPSG:25832", "EPSG:4258", always_xy=True)
lng, lat = inv.transform(E, N)   # printed with 9 decimals
```

| Control | E (m) | N (m) | lat | lng |
|---|---|---|---|---|
| S1 | 610745.7513 | 5970182.7593 | 53.868292081 | 10.684258865 |
| 31 | 611005.2198 | 5969807.8293 | 53.864868203 | 10.688067408 |
| 32 | 611304.6164 | 5969430.6739 | 53.861415655 | 10.692481375 |
| 33 | 610592.5871 | 5969590.5142 | 53.863003863 | 10.681717122 |
| F1 | 611176.0588 | 5969998.5683 | 53.866545281 | 10.690733243 |

The test `lat_lng_near_the_x_axis_decides_nothing` (`tests/fit_courses.rs`) uses one more point computed the
same way: map (20000, −1000), E 611012.1184, N 5969931.6063, lat 53.865978767, lng 10.688217049.

This step uses the same formula as `fit`, so the fixture cannot catch an error in it; the unit test
`grivation_turns_map_up_east_of_grid_north` in `src/fit.rs` checks its direction on its own. The
grid-to-lat/lng step is independent (PROJ).

IOF XML gives WGS84; ETRS89 differs from it by under a metre in Germany, far below the 20 m tolerance.

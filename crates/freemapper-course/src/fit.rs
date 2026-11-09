//! Course fitting: puts the points of a course file into the map's coordinates (ADR 0032).
//!
//! A course file gives map positions in mm from the map's origin, but writers disagree on the y axis:
//! Purple Pen, Condes and OCAD write y up, the IOF standard and OOM y down. The project can set the axis. A
//! Purple Pen file is read y up otherwise, as Purple Pen always writes it (ruling R36); a project value against
//! that is a warning. For IOF XML both readings are tried. A reading fits when every control lies within the box
//! of the map's objects grown by [`MARGIN_MM`]; a Purple Pen file that does not fit y up is an error, never
//! read mirrored. When the map is georeferenced with a UTM CRS and the file gives
//! latitude and longitude, they decide between the readings and check the chosen one: a control more
//! than [`GEOREF_TOLERANCE_M`] from where its latitude and longitude put it is an error, never a silent
//! shift. Controls with only latitude and longitude are placed through the map's georeferencing. A map
//! scale stated in the file must equal the map's.

use freemapper_omap::coord::MapCoord;
use freemapper_omap::{MapDocument, MapGeoref};

use super::model::{CourseEvent, FittedEvent, Offset, RawEvent, RawPoint};
use super::tm::{TransverseMercator, utm_from_crs};

/// How far beyond the box of the map's objects a control may lie, in map mm.
pub const MARGIN_MM: f64 = 10.0;
/// The largest distance in metres between a control's map position and its latitude and longitude.
pub const GEOREF_TOLERANCE_M: f64 = 20.0;

/// The direction of y in a course file's map positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum YAxis {
    /// y grows north, as Purple Pen, Condes and OCAD write it.
    Up,
    /// y grows south, as the IOF standard and OOM count it.
    Down,
}

/// Settings from the project file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitOptions {
    /// The file's y axis when the user sets it; `None` detects it.
    pub y_axis: Option<YAxis>,
    /// Use latitude and longitude to check positions, choose the axis and place controls that have no
    /// map position. On by default.
    pub check_georef: bool,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self {
            y_axis: None,
            check_georef: true,
        }
    }
}

/// A fitted event and what the fit found out.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    pub event: FittedEvent,
    pub notes: Vec<FitNote>,
}

/// What the fit found out, for `check` to report.
#[derive(Debug, Clone, PartialEq)]
pub enum FitNote {
    /// The file writes y up, as only that reading fits or as latitude and longitude show; the fit
    /// flipped it.
    YUpDetected,
    /// Both readings fit and nothing decided between them, so y up was taken. A warning: set the axis in
    /// the project file to confirm it.
    OrientationAssumed,
    /// The project file sets the y axis `set`, but the file format always writes `format`. A warning: the
    /// sheet prints the course mirrored unless the file really is an exception.
    AxisAgainstFormat { set: YAxis, format: YAxis },
    /// The file states no map scale, so it was not compared with the map's.
    ScaleUnchecked,
    /// `controls` controls have both a map position and latitude and longitude; the largest distance
    /// between the two in the chosen reading is `metres`.
    GeorefChecked { controls: usize, metres: f64 },
    /// No check against latitude and longitude, and why.
    GeorefSkipped(SkipReason),
    /// The project file switched the check off; latitude and longitude were ignored.
    GeorefCheckOff,
    /// These controls have only latitude and longitude; the map's georeferencing placed them.
    PlacedByGeoref(Vec<String>),
}

impl FitNote {
    /// Whether `check` counts the note as a warning.
    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            Self::OrientationAssumed | Self::AxisAgainstFormat { .. }
        )
    }
}

/// Why latitude and longitude could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The map has no CRS, a blank one, or no projected reference point.
    NotGeoreferenced,
    /// The map's CRS is not a UTM projection; the CRS text.
    UnsupportedCrs(String),
    /// No control with a map position has latitude and longitude.
    NoLatLng,
    /// The project file switched the check off.
    CheckOff,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotGeoreferenced => f.write_str("the map is not georeferenced"),
            Self::UnsupportedCrs(spec) => write!(
                f,
                "the map's coordinate system \"{spec}\" is not UTM, the only one supported"
            ),
            Self::NoLatLng => f.write_str("no control has both a map position and lat/lng"),
            Self::CheckOff => f.write_str("the project file switches lat/lng off"),
        }
    }
}

/// Why a course file does not fit the map.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FitError {
    #[error(
        "the course file is drawn for 1:{file}, the map is 1:{map}; export the courses for the map's scale"
    )]
    ScaleMismatch { file: u32, map: u32 },
    #[error(
        "the course file is drawn for 1:{file}, and the map states no scale; set the map's scale in the map file"
    )]
    MapScaleUnknown { file: u32 },
    #[error(
        "only {inside} of {total} controls lie on the map; the course file belongs to another map, uses another origin or another y axis"
    )]
    OutsideMap { inside: usize, total: usize },
    #[error(
        "control {control} lies {metres:.0} m from where its lat/lng put it ({tolerance} m allowed); the course file does not match the map's georeferencing",
        tolerance = GEOREF_TOLERANCE_M
    )]
    GeorefMismatch { control: String, metres: f64 },
    #[error("controls {} have only lat/lng and cannot be placed: {reason}", controls.join(", "))]
    NoPositionNoGeoref {
        controls: Vec<String>,
        reason: SkipReason,
    },
}

/// A control with a map position: code, file point and lat/lng if any.
type Positioned<'a> = (&'a str, RawPoint, Option<(f64, f64)>);

impl YAxis {
    /// The map position (µm, y down) of a file point, unrounded.
    fn place(self, p: RawPoint) -> (f64, f64) {
        let y = match self {
            YAxis::Up => -p.y_mm,
            YAxis::Down => p.y_mm,
        };
        (p.x_mm * 1000.0, y * 1000.0)
    }

    /// The linear part only: offsets are not moved.
    fn turn(self, o: Offset) -> Offset {
        match self {
            YAxis::Up => Offset {
                dx_mm: o.dx_mm,
                dy_mm: -o.dy_mm,
            },
            YAxis::Down => o,
        }
    }
}

/// OOM's map-to-grid transform (`Georeferencing::updateTransformation`): map mm less the map reference
/// point, scaled to metres with y flipped, turned by the grivation, plus the projected reference point.
struct Georef {
    tm: TransverseMercator,
    ref_grid: (f64, f64),
    map_ref_um: (f64, f64),
    metres_per_um: f64,
    grivation_rad: f64,
}

impl Georef {
    fn of(g: &MapGeoref) -> Result<Self, SkipReason> {
        let (Some(spec), Some(ref_grid)) = (&g.crs_spec, g.ref_projected) else {
            return Err(SkipReason::NotGeoreferenced);
        };
        if spec.trim().is_empty() {
            return Err(SkipReason::NotGeoreferenced);
        }
        if g.scale_denominator == 0 {
            return Err(SkipReason::NotGeoreferenced);
        }
        let tm = utm_from_crs(spec).ok_or_else(|| SkipReason::UnsupportedCrs(spec.clone()))?;
        Ok(Self {
            tm,
            ref_grid,
            map_ref_um: (g.map_ref_mm.0 * 1000.0, g.map_ref_mm.1 * 1000.0),
            metres_per_um: f64::from(g.scale_denominator) * g.combined_scale_factor / 1e6,
            grivation_rad: g.grivation_deg.to_radians(),
        })
    }

    /// Grid easting and northing of a map position (µm, y down).
    fn to_grid(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let qx = (x - self.map_ref_um.0) * self.metres_per_um;
        let qy = -(y - self.map_ref_um.1) * self.metres_per_um;
        let (s, c) = self.grivation_rad.sin_cos();
        (
            self.ref_grid.0 + qx * c + qy * s,
            self.ref_grid.1 - qx * s + qy * c,
        )
    }

    /// The map position (µm, y down) of a grid point: the inverse of [`Self::to_grid`].
    fn to_map(&self, (e, n): (f64, f64)) -> (f64, f64) {
        let (de, dn) = (e - self.ref_grid.0, n - self.ref_grid.1);
        let (s, c) = self.grivation_rad.sin_cos();
        let qx = de * c - dn * s;
        let qy = de * s + dn * c;
        (
            qx / self.metres_per_um + self.map_ref_um.0,
            -qy / self.metres_per_um + self.map_ref_um.1,
        )
    }

    fn grid_of(&self, (lat, lng): (f64, f64)) -> (f64, f64) {
        self.tm.forward(lat, lng)
    }

    /// Metres between a map position and the grid point of `geo`; infinite when either is not finite.
    fn distance(&self, map_um: (f64, f64), geo: (f64, f64)) -> f64 {
        let a = self.to_grid(map_um);
        let b = self.grid_of(geo);
        let d = (a.0 - b.0).hypot(a.1 - b.1);
        if d.is_finite() { d } else { f64::INFINITY }
    }
}

/// The box around every object coordinate of the map (µm), grown by [`MARGIN_MM`].
fn map_box(map: &MapDocument) -> Option<((f64, f64), (f64, f64))> {
    let mut coords = map
        .parts
        .iter()
        .flat_map(|p| &p.objects)
        .flat_map(|o| &o.coords);
    let first = coords.next()?;
    let (min, max) = coords.fold(((first.x, first.y), (first.x, first.y)), |(lo, hi), c| {
        (
            (lo.0.min(c.x), lo.1.min(c.y)),
            (hi.0.max(c.x), hi.1.max(c.y)),
        )
    });
    let m = MARGIN_MM * 1000.0;
    Some((
        (f64::from(min.0) - m, f64::from(min.1) - m),
        (f64::from(max.0) + m, f64::from(max.1) + m),
    ))
}

/// Rounds a map position known to lie on the map; `as` saturates for anything else.
fn coord((x, y): (f64, f64)) -> MapCoord {
    MapCoord::new(x.round() as i32, y.round() as i32)
}

/// Puts the event's points into map coordinates (µm, y down). See the module documentation.
pub fn fit(event: RawEvent, map: &MapDocument, options: &FitOptions) -> Result<Fit, FitError> {
    let mut notes = Vec::new();
    let map_scale = map.georef.scale_denominator;
    match event.source.map_scale {
        Some(file) if map_scale == 0 => return Err(FitError::MapScaleUnknown { file }),
        Some(file) if file != map_scale => {
            return Err(FitError::ScaleMismatch {
                file,
                map: map_scale,
            });
        }
        Some(_) => {}
        None => notes.push(FitNote::ScaleUnchecked),
    }

    let georef = if options.check_georef {
        Georef::of(&map.georef)
    } else {
        Err(SkipReason::CheckOff)
    };
    let bounds = map_box(map);
    let inside = |(x, y): (f64, f64)| {
        bounds.is_some_and(|(lo, hi)| x >= lo.0 && x <= hi.0 && y >= lo.1 && y <= hi.1)
    };

    // Controls with only lat/lng, placed through the georeferencing.
    let geo_only: Vec<(usize, &str, (f64, f64))> = event
        .controls
        .iter()
        .enumerate()
        .filter(|(_, c)| c.position.is_none())
        .filter_map(|(i, c)| c.geo.map(|g| (i, c.code.as_str(), g)))
        .collect();
    let placed: Vec<(usize, (f64, f64))> = match &georef {
        Ok(g) => geo_only
            .iter()
            .map(|&(i, _, geo)| (i, g.to_map(g.grid_of(geo))))
            .collect(),
        Err(reason) if !geo_only.is_empty() => {
            return Err(FitError::NoPositionNoGeoref {
                controls: geo_only.iter().map(|&(_, c, _)| c.to_owned()).collect(),
                reason: reason.clone(),
            });
        }
        Err(_) => Vec::new(),
    };
    let placed_inside = placed.iter().filter(|(_, p)| inside(*p)).count();

    let positioned: Vec<Positioned> = event
        .controls
        .iter()
        .filter_map(|c| c.position.map(|p| (c.code.as_str(), p, c.geo)))
        .collect();
    let total = positioned.len() + placed.len();
    let count_inside = |axis: YAxis| {
        placed_inside
            + positioned
                .iter()
                .filter(|(_, p, _)| inside(axis.place(*p)))
                .count()
    };
    // The project's axis, else the format's, else both, y up first: it is taken when nothing decides.
    let format_axis = event.source.format.y_axis();
    if let (Some(set), Some(format)) = (options.y_axis, format_axis)
        && set != format
    {
        notes.push(FitNote::AxisAgainstFormat { set, format });
    }
    let decided = options.y_axis.or(format_axis);
    let candidates: &[YAxis] = match &decided {
        Some(axis) => std::slice::from_ref(axis),
        None => &[YAxis::Up, YAxis::Down],
    };
    // Whether the axis changes anything: a control with a map position, or any point, offset or circle gap in
    // the file. Controls with only lat/lng place themselves.
    let axis_matters = !positioned.is_empty()
        || !event.specials.is_empty()
        || event.courses.iter().any(|c| {
            c.legs.iter().any(|l| !l.bends.is_empty())
                || c.stops
                    .iter()
                    .any(|s| s.number_text_at.is_some() || !s.circle_gaps.is_empty())
        });
    let mut fitting: Vec<YAxis> = candidates
        .iter()
        .copied()
        .filter(|&a| count_inside(a) == total)
        .collect();
    if fitting.is_empty() {
        return Err(FitError::OutsideMap {
            inside: candidates
                .iter()
                .map(|&a| count_inside(a))
                .max()
                .unwrap_or(0),
            total,
        });
    }

    let checks: Vec<(&str, RawPoint, (f64, f64))> = positioned
        .iter()
        .filter_map(|&(code, p, geo)| geo.map(|g| (code, p, g)))
        .collect();
    match &georef {
        Ok(g) if !checks.is_empty() => {
            // The control farthest from its lat/lng, for each reading that fits the map.
            let worst = |axis: YAxis| {
                checks
                    .iter()
                    .map(|&(code, p, geo)| (code, g.distance(axis.place(p), geo)))
                    .fold(("", 0.0_f64), |w, c| if c.1 > w.1 { c } else { w })
            };
            let results: Vec<(YAxis, (&str, f64))> =
                fitting.iter().map(|&a| (a, worst(a))).collect();
            let within: Vec<(YAxis, f64)> = results
                .iter()
                .filter(|(_, (_, m))| *m <= GEOREF_TOLERANCE_M)
                .map(|&(a, (_, m))| (a, m))
                .collect();
            if within.is_empty() {
                let (_, (control, metres)) = results
                    .iter()
                    .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
                    .copied()
                    .unwrap_or((YAxis::Up, ("", f64::INFINITY)));
                return Err(FitError::GeorefMismatch {
                    control: control.to_owned(),
                    metres,
                });
            }
            // Only the readings that pass the check remain; more than one leaves the axis undecided.
            fitting = within.iter().map(|&(a, _)| a).collect();
            notes.push(FitNote::GeorefChecked {
                controls: checks.len(),
                metres: within[0].1,
            });
        }
        Ok(_) => notes.push(FitNote::GeorefSkipped(SkipReason::NoLatLng)),
        Err(SkipReason::CheckOff) => notes.push(FitNote::GeorefCheckOff),
        Err(reason) => notes.push(FitNote::GeorefSkipped(reason.clone())),
    }
    let axis = fitting[0];
    if fitting.len() > 1 && axis_matters {
        notes.insert(0, FitNote::OrientationAssumed);
    } else if axis == YAxis::Up && !positioned.is_empty() && decided.is_none() {
        notes.insert(0, FitNote::YUpDetected);
    }
    if !geo_only.is_empty() {
        notes.push(FitNote::PlacedByGeoref(
            geo_only.iter().map(|&(_, c, _)| c.to_owned()).collect(),
        ));
    }

    let mut fitted: CourseEvent<MapCoord> =
        event.map_points(|p| coord(axis.place(p)), |o| axis.turn(o));
    for (i, p) in placed {
        fitted.controls[i].position = Some(coord(p));
    }
    // The gap angles run from +x towards the file's +y; with y up, towards the map's -y after the flip.
    if axis == YAxis::Up {
        for stop in fitted.courses.iter_mut().flat_map(|c| &mut c.stops) {
            for gap in &mut stop.circle_gaps {
                *gap = (-gap.1, -gap.0);
            }
        }
    }
    Ok(Fit {
        event: fitted,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grivation_turns_map_up_east_of_grid_north() {
        // OOM: from_map = translate(ref) · rotate(−grivation) · scale(s, −s) · translate(−map_ref)
        // (georeferencing.cpp, Georeferencing::updateTransformation); QTransform::rotate(a) maps
        // (x, y) to (x·cos a − y·sin a, x·sin a + y·cos a). With grivation 90°, magnetic north (map up)
        // points grid east, so 1 mm up on the map at 1:1000 is 1 m east.
        let g = Georef {
            tm: TransverseMercator::utm(32, false, super::super::tm::GRS80),
            ref_grid: (500_000.0, 6_000_000.0),
            map_ref_um: (0.0, 0.0),
            metres_per_um: 1000.0 / 1e6,
            grivation_rad: 90f64.to_radians(),
        };
        let (e, n) = g.to_grid((0.0, -1000.0));
        assert!((e - 500_001.0).abs() < 1e-9 && (n - 6_000_000.0).abs() < 1e-9);
        // 1 mm right on the map (map east) points grid south.
        let (e, n) = g.to_grid((1000.0, 0.0));
        assert!((e - 500_000.0).abs() < 1e-9 && (n - 5_999_999.0).abs() < 1e-9);
        let back = g.to_map(g.to_grid((1234.0, -567.0)));
        assert!((back.0 - 1234.0).abs() < 1e-6 && (back.1 + 567.0).abs() < 1e-6);
    }
}

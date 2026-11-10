//! Coordinate reference systems: ETRS89 / UTM zone 32N and 33N (EPSG:25832, EPSG:25833).
//!
// Provenance: freemapper `crates/freemapper/src/crs.rs` (freemapper v0.9.0, commit 0f25841), public items
// and tests unchanged. The Krüger series moved to [`crate::tm`], the one Transverse Mercator implementation;
// `Crs` projects through it.

/// Geographic position in degrees (ETRS89, treated as identical to WGS84).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

/// Projected position (easting / northing) in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectedPoint {
    pub easting: f64,
    pub northing: f64,
}

/// Axis-parallel rectangle in projected coordinates (metres).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min_e: f64,
    pub min_n: f64,
    pub max_e: f64,
    pub max_n: f64,
}

impl BoundingBox {
    /// The box grown by `m` metres on every side.
    pub fn buffered(&self, m: f64) -> Self {
        Self {
            min_e: self.min_e - m,
            min_n: self.min_n - m,
            max_e: self.max_e + m,
            max_n: self.max_n + m,
        }
    }

    /// The smallest box with corners on multiples of `step` that contains this box.
    pub fn snapped_outward(&self, step: f64) -> Self {
        Self {
            min_e: (self.min_e / step).floor() * step,
            min_n: (self.min_n / step).floor() * step,
            max_e: (self.max_e / step).ceil() * step,
            max_n: (self.max_n / step).ceil() * step,
        }
    }

    /// The common part of both boxes, if it is not empty.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let b = Self {
            min_e: self.min_e.max(other.min_e),
            min_n: self.min_n.max(other.min_n),
            max_e: self.max_e.min(other.max_e),
            max_n: self.max_n.min(other.max_n),
        };
        (b.min_e < b.max_e && b.min_n < b.max_n).then_some(b)
    }

    pub fn width(&self) -> f64 {
        self.max_e - self.min_e
    }

    pub fn height(&self) -> f64 {
        self.max_n - self.min_n
    }
}

/// EPSG code of ETRS89 / UTM zone 33N.
pub const EPSG_UTM33: u32 = 25833;

/// Central meridian of UTM zone 33 in degrees (zone 32: 9°).
const UTM33_LON0: f64 = 15.0;
const UTM_FALSE_EASTING: f64 = 500_000.0;

use crate::tm::{GRS80, Projected, TransverseMercator};

/// A projected CRS freemapper works in: ETRS89 / UTM zone 32N or 33N.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Crs {
    /// EPSG:25832, Saxony-Anhalt, Hamburg, Thuringia, Hesse.
    Utm32,
    /// EPSG:25833, Brandenburg, Berlin, Saxony.
    Utm33,
}

impl Crs {
    pub fn epsg(self) -> u32 {
        match self {
            Self::Utm32 => 25832,
            Self::Utm33 => EPSG_UTM33,
        }
    }

    pub fn from_epsg(epsg: u32) -> Option<Self> {
        match epsg {
            25832 => Some(Self::Utm32),
            EPSG_UTM33 => Some(Self::Utm33),
            _ => None,
        }
    }

    /// Central meridian in degrees.
    fn lon0(self) -> f64 {
        match self {
            Self::Utm32 => 9.0,
            Self::Utm33 => UTM33_LON0,
        }
    }

    /// UTM zone number: 32 or 33.
    pub fn utm_zone(self) -> u8 {
        match self {
            Self::Utm32 => 32,
            Self::Utm33 => 33,
        }
    }

    /// The projection of this CRS, for callers that work with [`crate::tm`] directly.
    pub fn transverse_mercator(self) -> TransverseMercator {
        TransverseMercator::utm(self.utm_zone(), false, GRS80)
    }

    fn project(self, p: LatLon) -> Projected {
        self.transverse_mercator().project(p.lat, p.lon)
    }

    /// Projects a geographic position.
    pub fn forward(self, p: LatLon) -> ProjectedPoint {
        {
            let q = self.project(p);
            ProjectedPoint {
                easting: q.easting,
                northing: q.northing,
            }
        }
    }

    /// Geographic position of a projected point: Newton iteration on [`Crs::forward`], so both
    /// directions agree to well below a millimetre. `None` for points no position projects to
    /// (far outside the zone) or when the iteration does not converge.
    pub fn inverse(self, p: ProjectedPoint) -> Option<LatLon> {
        /// Finite-difference step in degrees (about 0.1 m).
        const STEP: f64 = 1e-6;
        if !p.easting.is_finite() || !p.northing.is_finite() {
            return None;
        }
        let lat = p.northing / 111_132.0;
        let mut q = LatLon {
            lat,
            lon: self.lon0()
                + (p.easting - UTM_FALSE_EASTING) / (111_320.0 * lat.to_radians().cos()),
        };
        for _ in 0..10 {
            let f = self.forward(q);
            let (de, dn) = (p.easting - f.easting, p.northing - f.northing);
            if de.abs() < 1e-6 && dn.abs() < 1e-6 {
                return (q.lat.abs() <= 90.0 && q.lon.is_finite()).then_some(q);
            }
            let at_lat = self.forward(LatLon {
                lat: q.lat + STEP,
                ..q
            });
            let at_lon = self.forward(LatLon {
                lon: q.lon + STEP,
                ..q
            });
            let (e_lat, e_lon) = (
                (at_lat.easting - f.easting) / STEP,
                (at_lon.easting - f.easting) / STEP,
            );
            let (n_lat, n_lon) = (
                (at_lat.northing - f.northing) / STEP,
                (at_lon.northing - f.northing) / STEP,
            );
            let det = e_lat * n_lon - e_lon * n_lat;
            q.lat += (n_lon * de - e_lon * dn) / det;
            q.lon += (e_lat * dn - n_lat * de) / det;
            if !q.lat.is_finite() || !q.lon.is_finite() {
                return None;
            }
        }
        None
    }

    /// Grid convergence in degrees: angle from true north to grid north, negative west of the
    /// central meridian (same sign convention as OOM).
    pub fn convergence_deg(self, p: LatLon) -> f64 {
        self.project(p).convergence_deg
    }

    /// Point scale factor of the projection at `p`.
    pub fn scale_factor(self, p: LatLon) -> f64 {
        self.project(p).scale_factor
    }

    /// `p`, given in `from`, in this CRS (through latitude/longitude).
    pub fn reproject(self, from: Crs, p: ProjectedPoint) -> Option<ProjectedPoint> {
        if from == self {
            return Some(p);
        }
        from.inverse(p).map(|q| self.forward(q))
    }
}

/// Projects a geographic position to EPSG:25833.
pub fn utm33_forward(p: LatLon) -> ProjectedPoint {
    Crs::Utm33.forward(p)
}

/// Geographic position of an EPSG:25833 point; see [`Crs::inverse`].
pub fn utm33_inverse(p: ProjectedPoint) -> Option<LatLon> {
    Crs::Utm33.inverse(p)
}

/// Grid convergence in EPSG:25833; see [`Crs::convergence_deg`].
pub fn grid_convergence_deg(p: LatLon) -> f64 {
    Crs::Utm33.convergence_deg(p)
}

/// Point scale factor in EPSG:25833.
pub fn point_scale_factor(p: LatLon) -> f64 {
    Crs::Utm33.scale_factor(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utm33_inverse_reports_impossible_points() {
        for (easting, northing) in [(500_000.0, 1e9), (-5e7, 5_800_000.0), (f64::NAN, 0.0)] {
            assert_eq!(utm33_inverse(ProjectedPoint { easting, northing }), None);
        }
    }

    #[test]
    fn utm33_roundtrip() {
        for (lat, lon) in [(52.5388, 13.8433), (51.4, 11.9), (53.5, 14.7), (52.0, 15.0)] {
            let p = LatLon { lat, lon };
            let back = utm33_inverse(utm33_forward(p)).unwrap();
            let again = utm33_forward(back);
            let there = utm33_forward(p);
            assert!(
                (again.easting - there.easting).abs() < 0.001
                    && (again.northing - there.northing).abs() < 0.001,
                "{p:?} -> {back:?}"
            );
            assert!((back.lat - lat).abs() < 1e-8 && (back.lon - lon).abs() < 1e-8);
        }
    }

    #[test]
    fn bbox_buffer_snap_intersection() {
        let b = BoundingBox {
            min_e: 0.3,
            min_n: 0.3,
            max_e: 9.7,
            max_n: 9.7,
        };
        let buffered = b.buffered(1.0);
        assert!((buffered.min_e + 0.7).abs() < 1e-9 && (buffered.max_n - 10.7).abs() < 1e-9);
        assert_eq!(
            buffered.snapped_outward(1.0),
            BoundingBox {
                min_e: -1.0,
                min_n: -1.0,
                max_e: 11.0,
                max_n: 11.0
            }
        );
        assert!((b.width() - 9.4).abs() < 1e-9 && (b.height() - 9.4).abs() < 1e-9);
        let other = BoundingBox {
            min_e: 20.0,
            min_n: 0.0,
            max_e: 30.0,
            max_n: 10.0,
        };
        assert_eq!(b.intersection(&other), None);
        let overlap = BoundingBox {
            min_e: 5.0,
            ..other
        };
        assert_eq!(
            b.intersection(&overlap),
            Some(BoundingBox {
                min_e: 5.0,
                min_n: 0.3,
                max_e: 9.7,
                max_n: 9.7
            })
        );
    }

    /// (lat, lon, easting, northing, convergence_deg, scale factor), reference values from the
    /// Krüger series of order n^6.
    const REFERENCE: [(f64, f64, f64, f64, f64, f64); 3] = [
        (
            52.54024494927058,
            13.844335699781933,
            421622.9917,
            5821756.0323,
            -0.917390,
            0.99967540,
        ),
        (52.0, 15.0, 500000.0, 5761038.2125, 0.0, 0.99960000),
        (53.0, 12.0, 298694.2000, 5876481.1399, -2.396705, 1.00009740),
    ];

    #[test]
    fn zone32_round_trip_and_reference_points() {
        // Latina August Hermann Francke, Halle: 25833 E 289 795 N 5 707 067 is 25832 E 706 451
        // N 5 706 913 (reference rounded to metres).
        let p = Crs::Utm33
            .inverse(ProjectedPoint {
                easting: 289_795.0,
                northing: 5_707_067.0,
            })
            .unwrap();
        let q = Crs::Utm32.forward(p);
        assert!((q.easting - 706_451.0).abs() < 1.0, "{q:?}");
        assert!((q.northing - 5_706_913.0).abs() < 1.0, "{q:?}");
        let back = Crs::Utm32.inverse(q).unwrap();
        assert!((back.lat - p.lat).abs() < 1e-9 && (back.lon - p.lon).abs() < 1e-9);
        assert_eq!(Crs::Utm32.epsg(), 25832);
        assert_eq!(Crs::Utm33.epsg(), 25833);
        assert_eq!(Crs::from_epsg(25832), Some(Crs::Utm32));
        assert_eq!(Crs::from_epsg(4326), None);
        // The zone-33 wrappers are unchanged.
        assert_eq!(utm33_forward(p), Crs::Utm33.forward(p));
    }

    #[test]
    fn zone32_convergence_is_positive_east_of_its_meridian() {
        let halle = LatLon {
            lat: 51.48,
            lon: 11.97,
        };
        assert!((Crs::Utm32.convergence_deg(halle) - 2.33).abs() < 0.02);
        assert!((Crs::Utm33.convergence_deg(halle) + 2.37).abs() < 0.02);
        let on_meridian = LatLon {
            lat: 51.0,
            lon: 9.0,
        };
        assert!((Crs::Utm32.scale_factor(on_meridian) - 0.9996).abs() < 1e-9);
    }

    #[test]
    fn forward_reference_points() {
        for (lat, lon, e, n, _, _) in REFERENCE {
            let p = utm33_forward(LatLon { lat, lon });
            assert!(
                (p.easting - e).abs() < 0.001,
                "easting {} vs {e}",
                p.easting
            );
            assert!(
                (p.northing - n).abs() < 0.001,
                "northing {} vs {n}",
                p.northing
            );
        }
    }

    #[test]
    fn convergence_reference_points() {
        for (lat, lon, _, _, gamma, _) in REFERENCE {
            let g = grid_convergence_deg(LatLon { lat, lon });
            assert!((g - gamma).abs() < 1e-6, "convergence {g} vs {gamma}");
        }
    }

    #[test]
    fn scale_factor_reference_points() {
        for (lat, lon, _, _, _, k) in REFERENCE {
            let s = point_scale_factor(LatLon { lat, lon });
            assert!((s - k).abs() < 1e-8, "scale factor {s} vs {k}");
        }
    }
}

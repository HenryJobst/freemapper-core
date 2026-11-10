//! Transverse Mercator on an ellipsoid: latitude and longitude to easting, northing, grid convergence and
//! point scale factor. Krüger's series to the sixth power of the third flattening n, as in EPSG Guidance
//! Note 7-2 §1.3.5.1 ("JHS formula") and Karney (2011), "Transverse Mercator with an accuracy of a few
//! nanometers", eq. (35). Within a UTM zone the series is accurate to nanometres, far beyond what a map or a
//! course needs. This is the one UTM implementation of the suite; [`crate::crs::Crs`] projects through it.
//!
//! Only the UTM projections are recognised from a map's CRS text ([`utm_from_crs`]): `EPSG:258NN` (ETRS89,
//! GRS80), `EPSG:326NN` and `EPSG:327NN` (WGS84 north and south), also as `+init=epsg:...`, and `+proj=utm
//! +zone=NN` on WGS84 or GRS80.

// Provenance: the projection, `Ellipsoid` and `utm_from_crs` with their tests come from freemapper-print's
// `course/tm.rs` (freemapper-course 0.4.2, core ADR 0013), which used the series to n^4 and was forward only.
// The series to n^6, the convergence and the scale factor come from freemapper's `crs.rs`
// (`transverse_mercator`). Both test sets run against this one implementation.

/// An ellipsoid by its semi-major axis (m) and flattening.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipsoid {
    pub a: f64,
    pub f: f64,
}

pub const GRS80: Ellipsoid = Ellipsoid {
    a: 6_378_137.0,
    f: 1.0 / 298.257_222_101,
};

pub const WGS84: Ellipsoid = Ellipsoid {
    a: 6_378_137.0,
    f: 1.0 / 298.257_223_563,
};

/// A transverse Mercator projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransverseMercator {
    pub ellipsoid: Ellipsoid,
    /// Latitude of natural origin, degrees.
    pub lat0: f64,
    /// Longitude of natural origin (central meridian), degrees.
    pub lon0: f64,
    pub k0: f64,
    pub false_easting: f64,
    pub false_northing: f64,
}

/// A projected point with the grid quantities at that point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projected {
    pub easting: f64,
    pub northing: f64,
    /// Grid convergence in degrees: angle from true north to grid north, negative west of the central
    /// meridian (same sign convention as OOM).
    pub convergence_deg: f64,
    /// Point scale factor.
    pub scale_factor: f64,
}

impl TransverseMercator {
    /// UTM zone `zone` (1 to 60) on `ellipsoid`, north or south.
    pub fn utm(zone: u8, south: bool, ellipsoid: Ellipsoid) -> Self {
        Self {
            ellipsoid,
            lat0: 0.0,
            lon0: f64::from(zone) * 6.0 - 183.0,
            k0: 0.9996,
            false_easting: 500_000.0,
            false_northing: if south { 10_000_000.0 } else { 0.0 },
        }
    }

    /// Easting and northing (m) of `lat`, `lon` (degrees). Not finite for points 90 degrees or more from
    /// the central meridian.
    pub fn forward(&self, lat: f64, lon: f64) -> (f64, f64) {
        let p = self.project(lat, lon);
        (p.easting, p.northing)
    }

    /// Easting, northing, grid convergence and point scale factor of `lat`, `lon` (degrees).
    pub fn project(&self, lat: f64, lon: f64) -> Projected {
        let Ellipsoid { a, f } = self.ellipsoid;
        let e = (f * (2.0 - f)).sqrt();
        let n = f / (2.0 - f);
        let (n2, n3, n4, n5, n6) = (n * n, n.powi(3), n.powi(4), n.powi(5), n.powi(6));
        // Rectifying radius: meridian arc length per radian.
        let big_a = a / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0 + n6 / 256.0);
        let alpha = [
            n / 2.0 - 2.0 * n2 / 3.0 + 5.0 * n3 / 16.0 + 41.0 * n4 / 180.0 - 127.0 * n5 / 288.0
                + 7891.0 * n6 / 37800.0,
            13.0 * n2 / 48.0 - 3.0 * n3 / 5.0 + 557.0 * n4 / 1440.0 + 281.0 * n5 / 630.0
                - 1_983_433.0 * n6 / 1_935_360.0,
            61.0 * n3 / 240.0 - 103.0 * n4 / 140.0
                + 15061.0 * n5 / 26880.0
                + 167_603.0 * n6 / 181_440.0,
            49561.0 * n4 / 161_280.0 - 179.0 * n5 / 168.0 + 6_601_661.0 * n6 / 7_257_600.0,
            34729.0 * n5 / 80640.0 - 3_418_889.0 * n6 / 1_995_840.0,
            212_378_941.0 * n6 / 319_334_400.0,
        ];
        // Gauss-Schreiber coordinates and the sums that give convergence and scale, for latitude `phi`
        // and longitude difference `lambda` (radians).
        let series = |phi: f64, lambda: f64| {
            let tau = phi.tan();
            let sigma = (e * (e * phi.sin()).atanh()).sinh();
            let tau_c = tau * (1.0 + sigma * sigma).sqrt() - sigma * (1.0 + tau * tau).sqrt();
            let xi1 = tau_c.atan2(lambda.cos());
            let eta1 = (lambda.sin() / (tau_c * tau_c + lambda.cos().powi(2)).sqrt()).asinh();
            let (mut xi, mut eta, mut p_sum, mut q_sum) = (xi1, eta1, 1.0, 0.0);
            for (j, al) in alpha.iter().enumerate() {
                let k = 2.0 * (j as f64 + 1.0);
                xi += al * (k * xi1).sin() * (k * eta1).cosh();
                eta += al * (k * xi1).cos() * (k * eta1).sinh();
                p_sum += k * al * (k * xi1).cos() * (k * eta1).cosh();
                q_sum += k * al * (k * xi1).sin() * (k * eta1).sinh();
            }
            (xi, eta, p_sum, q_sum, tau, tau_c)
        };
        let phi = lat.to_radians();
        let lambda = (lon - self.lon0).to_radians();
        let (xi, eta, p_sum, q_sum, tau, tau_c) = series(phi, lambda);
        let (xi0, ..) = series(self.lat0.to_radians(), 0.0);
        let gamma =
            (tau_c * lambda.tan() / (1.0 + tau_c * tau_c).sqrt()).atan() + q_sum.atan2(p_sum);
        let scale_factor = self.k0 * big_a / a
            * (1.0 + (1.0 - e * e) * tau * tau).sqrt()
            * (p_sum * p_sum + q_sum * q_sum).sqrt()
            / (tau_c * tau_c + lambda.cos().powi(2)).sqrt();
        Projected {
            easting: self.false_easting + self.k0 * big_a * eta,
            northing: self.false_northing + self.k0 * big_a * (xi - xi0),
            convergence_deg: gamma.to_degrees(),
            scale_factor,
        }
    }
}

/// The projection a map's CRS text names, when it is a UTM projection this module knows.
pub fn utm_from_crs(spec: &str) -> Option<TransverseMercator> {
    let s = spec.trim().to_ascii_lowercase();
    if let Some(i) = s.find("epsg:") {
        let code: String = s[i + 5..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let rest = &s[i + 5 + code.len()..];
        if !rest.trim().is_empty() {
            return None;
        }
        let code: u32 = code.parse().ok()?;
        let zone = u8::try_from(code % 100)
            .ok()
            .filter(|z| (1..=60).contains(z))?;
        return match code / 100 {
            258 => Some(TransverseMercator::utm(zone, false, GRS80)),
            326 => Some(TransverseMercator::utm(zone, false, WGS84)),
            327 => Some(TransverseMercator::utm(zone, true, WGS84)),
            _ => None,
        };
    }
    let mut proj = None;
    let mut zone = None;
    let mut south = false;
    let mut ellipsoid = WGS84;
    for token in s.split_whitespace() {
        let (key, value) = token.split_once('=').unwrap_or((token, ""));
        match key {
            "+proj" => proj = Some(value),
            "+zone" => zone = value.parse::<u8>().ok().filter(|z| (1..=60).contains(z)),
            "+south" => south = true,
            "+ellps" | "+datum" => match value {
                "wgs84" => ellipsoid = WGS84,
                "grs80" | "etrs89" => ellipsoid = GRS80,
                _ => return None,
            },
            "+towgs84" if value.split(',').all(|v| v.parse::<f64>() == Ok(0.0)) => {}
            "+units" if value == "m" => {}
            "+no_defs" | "+type" | "+wktext" => {}
            _ => return None,
        }
    }
    (proj == Some("utm")).then_some(())?;
    Some(TransverseMercator::utm(zone?, south, ellipsoid))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(got: (f64, f64), want: (f64, f64), tol: f64) {
        assert!(
            (got.0 - want.0).abs() <= tol && (got.1 - want.1).abs() <= tol,
            "got {got:?}, want {want:?} ±{tol}"
        );
    }

    #[test]
    fn utm_forward_matches_reference_points() {
        // EPSG Guidance Note 7-2 (IOGP Publication 373-7-2), §1.3.5.1 Transverse Mercator, worked
        // example "OSGB 1936 / British National Grid": Airy 1830, 50°30'N 0°30'E gives E 577274.99 m,
        // N 69740.50 m. Published to the centimetre; PROJ 9.8.1 gives 577274.9838 / 69740.4923.
        let bng = TransverseMercator {
            ellipsoid: Ellipsoid {
                a: 6_377_563.396,
                f: 1.0 / 299.324_964_6,
            },
            lat0: 49.0,
            lon0: -2.0,
            k0: 0.999_601_271_7,
            false_easting: 400_000.0,
            false_northing: -100_000.0,
        };
        close(bng.forward(50.5, 0.5), (577_274.99, 69_740.50), 0.01);
        // UTM points to the millimetre: PROJ 9.8.1 through pyproj 3.8.0, run on 2026-11-09
        // (`Transformer.from_crs("EPSG:4258", "EPSG:25832", always_xy=True)` and EPSG:4326 to
        // EPSG:32633). The first point is the reference point of tests/fixtures/maps/luebeck.omap,
        // which freemapper wrote as 610932.039697268 / 5969932.064166221.
        let z32 = utm_from_crs("EPSG:25832").unwrap();
        close(
            z32.forward(53.866, 10.687),
            (610932.0397, 5969932.0642),
            0.001,
        );
        close(z32.forward(47.5, 7.6), (394562.0988, 5261679.5601), 0.001);
        close(z32.forward(55.0, 12.0), (691875.6321, 6098907.8250), 0.001);
        let z33 = utm_from_crs("EPSG:32633").unwrap();
        close(z33.forward(52.0, 12.5), (328386.7762, 5763989.3667), 0.001);
        close(z33.forward(0.0, 15.0), (500_000.0, 0.0), 0.001);
    }

    #[test]
    fn crs_texts_recognised() {
        let z32 = TransverseMercator::utm(32, false, GRS80);
        for spec in [
            "EPSG:25832",
            "+init=epsg:25832",
            "+proj=utm +zone=32 +ellps=GRS80 +towgs84=0,0,0,0,0,0,0 +units=m +no_defs",
        ] {
            assert_eq!(utm_from_crs(spec), Some(z32), "{spec}");
        }
        assert_eq!(
            utm_from_crs("+proj=utm +zone=33 +datum=WGS84"),
            Some(TransverseMercator::utm(33, false, WGS84))
        );
        assert_eq!(
            utm_from_crs("EPSG:32733"),
            Some(TransverseMercator::utm(33, true, WGS84))
        );
        assert_eq!(
            utm_from_crs("+proj=utm +zone=33 +south"),
            Some(TransverseMercator::utm(33, true, WGS84))
        );
        for spec in [
            "EPSG:4326",
            "EPSG:25800",
            "EPSG:25861",
            "EPSG:31467",
            "+proj=lcc +lat_1=52",
            "+proj=utm +zone=32 +ellps=intl",
            "+proj=utm +zone=32 +towgs84=-87,-98,-121",
            "+proj=utm",
            "",
        ] {
            assert_eq!(utm_from_crs(spec), None, "{spec}");
        }
    }
}

//! Transverse Mercator, forward only: latitude and longitude to easting and northing. Krüger's series to
//! the fourth power of the third flattening n, as in EPSG Guidance Note 7-2 §1.3.5.1 ("JHS formula") and
//! Karney (2011), "Transverse Mercator with an accuracy of a few nanometers", eq. (35). Within a UTM zone
//! the series is accurate to well under a millimetre, far beyond what a course needs.
//!
//! Only the UTM projections are recognised from a map's CRS text (ADR 0032): `EPSG:258NN` (ETRS89, GRS80),
//! `EPSG:326NN` and `EPSG:327NN` (WGS84 north and south), also as `+init=epsg:…`, and `+proj=utm +zone=NN`
//! on WGS84 or GRS80.

/// An ellipsoid by its semi-major axis (m) and flattening.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ellipsoid {
    pub a: f64,
    pub f: f64,
}

pub(crate) const GRS80: Ellipsoid = Ellipsoid {
    a: 6_378_137.0,
    f: 1.0 / 298.257_222_101,
};

pub(crate) const WGS84: Ellipsoid = Ellipsoid {
    a: 6_378_137.0,
    f: 1.0 / 298.257_223_563,
};

/// A transverse Mercator projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TransverseMercator {
    pub ellipsoid: Ellipsoid,
    /// Latitude of natural origin, degrees.
    pub lat0: f64,
    /// Longitude of natural origin (central meridian), degrees.
    pub lon0: f64,
    pub k0: f64,
    pub false_easting: f64,
    pub false_northing: f64,
}

impl TransverseMercator {
    /// UTM zone `zone` (1 to 60) on `ellipsoid`, north or south.
    pub(crate) fn utm(zone: u8, south: bool, ellipsoid: Ellipsoid) -> Self {
        Self {
            ellipsoid,
            lat0: 0.0,
            lon0: f64::from(zone) * 6.0 - 183.0,
            k0: 0.9996,
            false_easting: 500_000.0,
            false_northing: if south { 10_000_000.0 } else { 0.0 },
        }
    }

    /// Easting and northing (m) of `lat`, `lon` (degrees). Not finite for points 90° or more from the
    /// central meridian.
    pub(crate) fn forward(&self, lat: f64, lon: f64) -> (f64, f64) {
        let Ellipsoid { a, f } = self.ellipsoid;
        let n = f / (2.0 - f);
        let (n2, n3, n4) = (n * n, n * n * n, n * n * n * n);
        // Rectifying radius: meridian arc length per radian.
        let big_a = a / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0);
        let alpha = [
            n / 2.0 - 2.0 * n2 / 3.0 + 5.0 * n3 / 16.0 + 41.0 * n4 / 180.0,
            13.0 * n2 / 48.0 - 3.0 * n3 / 5.0 + 557.0 * n4 / 1440.0,
            61.0 * n3 / 240.0 - 103.0 * n4 / 140.0,
            49561.0 * n4 / 161_280.0,
        ];
        let e = (f * (2.0 - f)).sqrt();
        let xi_eta = |phi: f64, lambda: f64| {
            // Conformal latitude via t = tan χ, then the Gauss-Schreiber coordinates.
            let s = phi.sin();
            let t = (s.atanh() - e * (e * s).atanh()).sinh();
            let xi1 = t.atan2(lambda.cos());
            let eta1 = (lambda.sin() / (1.0 + t * t).sqrt()).atanh();
            let mut xi = xi1;
            let mut eta = eta1;
            for (j, al) in alpha.iter().enumerate() {
                let k = 2.0 * (j as f64 + 1.0);
                xi += al * (k * xi1).sin() * (k * eta1).cosh();
                eta += al * (k * xi1).cos() * (k * eta1).sinh();
            }
            (xi, eta)
        };
        let (xi, eta) = xi_eta(lat.to_radians(), (lon - self.lon0).to_radians());
        let (xi0, _) = xi_eta(self.lat0.to_radians(), 0.0);
        (
            self.false_easting + self.k0 * big_a * eta,
            self.false_northing + self.k0 * big_a * (xi - xi0),
        )
    }
}

/// The projection a map's CRS text names, when it is a UTM projection this module knows.
pub(crate) fn utm_from_crs(spec: &str) -> Option<TransverseMercator> {
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

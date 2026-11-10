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

    /// Latitude and longitude (degrees) of `easting`, `northing` (m): the inverse of [`Self::forward`],
    /// by the Krüger series to n^6 (Karney 2011, eq. 36) and Newton iteration for the latitude from the
    /// conformal latitude. Valid for any zone and hemisphere and exact to nanometres inside the zone. Not finite for non-finite
    /// input. Points far outside the projection's range (about 90 degrees from the central meridian or
    /// more) are folded back onto the globe instead of rejected; check them with [`Self::forward`] if that
    /// matters.
    pub fn inverse(&self, easting: f64, northing: f64) -> (f64, f64) {
        let Ellipsoid { a, f } = self.ellipsoid;
        let e2 = f * (2.0 - f);
        let n = f / (2.0 - f);
        let (n2, n3, n4, n5, n6) = (n * n, n.powi(3), n.powi(4), n.powi(5), n.powi(6));
        let big_a = a / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0 + n6 / 256.0);
        let beta = [
            n / 2.0 - 2.0 * n2 / 3.0 + 37.0 * n3 / 96.0 - n4 / 360.0 - 81.0 * n5 / 512.0
                + 96199.0 * n6 / 604_800.0,
            n2 / 48.0 + n3 / 15.0 - 437.0 * n4 / 1440.0 + 46.0 * n5 / 105.0
                - 1_118_711.0 * n6 / 3_870_720.0,
            17.0 * n3 / 480.0 - 37.0 * n4 / 840.0 - 209.0 * n5 / 4480.0 + 5569.0 * n6 / 90720.0,
            4397.0 * n4 / 161_280.0 - 11.0 * n5 / 504.0 - 830_251.0 * n6 / 7_257_600.0,
            4583.0 * n5 / 161_280.0 - 108_847.0 * n6 / 3_991_680.0,
            20_648_693.0 * n6 / 638_668_800.0,
        ];
        // xi at the latitude of origin (forward series with lambda = 0), so that northing = k0 A (xi - xi0).
        let phi0 = self.lat0.to_radians();
        let sigma0 = (e2.sqrt() * (e2.sqrt() * phi0.sin()).atanh()).sinh();
        let tau0 = phi0.tan();
        let chi0 =
            (tau0 * (1.0 + sigma0 * sigma0).sqrt() - sigma0 * (1.0 + tau0 * tau0).sqrt()).atan();
        let xi0 = chi0
            + krueger_alpha(n)
                .iter()
                .enumerate()
                .map(|(j, al)| al * (2.0 * (j as f64 + 1.0) * chi0).sin())
                .sum::<f64>();
        let xi = (northing - self.false_northing) / (self.k0 * big_a) + xi0;
        let eta = (easting - self.false_easting) / (self.k0 * big_a);
        let (mut xi1, mut eta1) = (xi, eta);
        for (j, b) in beta.iter().enumerate() {
            let k = 2.0 * (j as f64 + 1.0);
            xi1 -= b * (k * xi).sin() * (k * eta).cosh();
            eta1 -= b * (k * xi).cos() * (k * eta).sinh();
        }
        // Conformal latitude as tan(chi), then the geodetic latitude by Newton iteration (Karney eq. 19).
        let tau_c = xi1.sin() / (eta1.sinh().powi(2) + xi1.cos().powi(2)).sqrt();
        let lambda = eta1.sinh().atan2(xi1.cos());
        let mut tau = tau_c;
        for _ in 0..6 {
            let sigma = (e2.sqrt() * (e2.sqrt() * tau / (1.0 + tau * tau).sqrt()).atanh()).sinh();
            let tau_c_i = tau * (1.0 + sigma * sigma).sqrt() - sigma * (1.0 + tau * tau).sqrt();
            let d_tau = (tau_c - tau_c_i) / (1.0 + tau_c_i * tau_c_i).sqrt()
                * (1.0 + (1.0 - e2) * tau * tau)
                / ((1.0 - e2) * (1.0 + tau * tau).sqrt());
            tau += d_tau;
            if d_tau.abs() < 1e-15 * (1.0 + tau.abs()) {
                break;
            }
        }
        (tau.atan().to_degrees(), self.lon0 + lambda.to_degrees())
    }

    /// Easting, northing, grid convergence and point scale factor of `lat`, `lon` (degrees).
    pub fn project(&self, lat: f64, lon: f64) -> Projected {
        let Ellipsoid { a, f } = self.ellipsoid;
        let e = (f * (2.0 - f)).sqrt();
        let n = f / (2.0 - f);
        let (n2, n4, n6) = (n * n, n.powi(4), n.powi(6));
        // Rectifying radius: meridian arc length per radian.
        let big_a = a / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0 + n6 / 256.0);
        let alpha = krueger_alpha(n);
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

/// Coefficients of the forward Krüger series in the third flattening `n`, to n^6.
fn krueger_alpha(n: f64) -> [f64; 6] {
    let (n2, n3, n4, n5, n6) = (n * n, n.powi(3), n.powi(4), n.powi(5), n.powi(6));
    [
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
    ]
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

    #[test]
    fn inverse_round_trips_in_every_zone_and_hemisphere() {
        // Round trip forward -> inverse: 1e-9 degrees is about 0.1 mm.
        let mut worst: f64 = 0.0;
        let projections = [
            TransverseMercator::utm(31, false, WGS84),
            TransverseMercator::utm(32, false, GRS80),
            TransverseMercator::utm(33, false, WGS84),
            TransverseMercator::utm(35, false, GRS80),
            TransverseMercator::utm(33, true, WGS84),
            TransverseMercator::utm(19, true, WGS84),
            TransverseMercator::utm(60, false, WGS84),
        ];
        for tm in projections {
            for dlat in [-80.0, -45.0, -30.5, -1.0, 0.0, 1e-3, 13.7, 47.5, 60.0, 84.0] {
                for dlon in [-3.0, -2.9, -1.0, 0.0, 0.3, 2.5, 3.0] {
                    let (lat, lon) = (dlat, tm.lon0 + dlon);
                    let (e, n) = tm.forward(lat, lon);
                    let (la, lo) = tm.inverse(e, n);
                    worst = worst.max((la - lat).abs()).max((lo - lon).abs());
                    assert!(
                        (la - lat).abs() < 1e-9 && (lo - lon).abs() < 1e-9,
                        "{tm:?} ({lat}, {lon}) -> ({la}, {lo})"
                    );
                }
            }
        }
        eprintln!("worst round-trip error: {worst:e} degrees");
    }

    #[test]
    fn inverse_matches_reference_points() {
        // The PROJ 9.8.1 values of utm_forward_matches_reference_points, backwards.
        let z32 = utm_from_crs("EPSG:25832").unwrap();
        close(
            z32.inverse(610932.0397, 5969932.0642),
            (53.866, 10.687),
            1e-8,
        );
        close(z32.inverse(394562.0988, 5261679.5601), (47.5, 7.6), 1e-8);
        close(z32.inverse(691875.6321, 6098907.8250), (55.0, 12.0), 1e-8);
        let z33 = utm_from_crs("EPSG:32633").unwrap();
        close(z33.inverse(328386.7762, 5763989.3667), (52.0, 12.5), 1e-8);
        close(z33.inverse(500_000.0, 0.0), (0.0, 15.0), 1e-12);
        // Southern hemisphere: the false northing of 10 000 000 m puts the equator at its top.
        let s33 = utm_from_crs("EPSG:32733").unwrap();
        close(s33.inverse(500_000.0, 10_000_000.0), (0.0, 15.0), 1e-12);
        let (e, n) = s33.forward(-33.9, 18.4);
        close(s33.inverse(e, n), (-33.9, 18.4), 1e-9);
    }

    #[test]
    fn inverse_of_unreachable_points_is_not_finite() {
        let z33 = TransverseMercator::utm(33, false, WGS84);
        let (lat, lon) = z33.inverse(f64::NAN, 0.0);
        assert!(lat.is_nan() || lon.is_nan());
    }
}

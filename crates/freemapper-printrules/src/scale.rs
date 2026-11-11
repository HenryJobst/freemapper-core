//! Map kinds and the symbol size table (ADR 0018).
//!
//! Printing at another scale than the map was drawn at scales geometry by the geometry factor and symbol
//! dimensions by the symbol factor. The IOF standards fix the symbol sizes per map kind and print scale:
//! an enlarged print enlarges every symbol in proportion, and a reduction below the base scale is not
//! foreseen (the symbols keep their base size, and the print is flagged as non-standard).
//!
//! The table, the north-line spacing and the proposed contour intervals come from the embedded base
//! rule profile `iof-print` ([`crate::rules::base`], ADR 0031).

use crate::rules;

/// Kartenart, the shared type of the contract crate (core ADR 0027).
pub use freemapper_contract::MapKind;

/// The contour interval (m) `init` proposes for a new project, for the user to confirm: 2 m for sprint maps
/// (ISSprOM 2019-2 §3.2), 5 m for forest maps (ISOM 2017-2 §2.10), none for schoolyard maps.
pub fn proposed_contour_interval_m(kind: MapKind) -> Option<f64> {
    rules::base().proposed_contour_interval_m(kind)
}

/// The scales (denominators) a map of `kind` may be printed at and drawn at, ascending.
pub fn allowed_scales(kind: MapKind) -> Vec<u32> {
    rules::base()
        .scale_entries(kind)
        .iter()
        .map(|e| e.scale)
        .collect()
}

fn join_scales(scales: &[u32]) -> String {
    scales
        .iter()
        .map(|s| format!("1:{s}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Ground distance (m) between north lines for a map of `kind` (ADR 0027): the standards set
/// 30 mm at the base scale of sprint maps (ISSprOM 2019-2, 120 m at 1:4000) and 20 mm at 1:15000 for
/// forest maps (ISOM 2017-2, 300 m).
///
/// UNVERIFIED: schoolyard 37.5 m (37.5 mm at 1:1000) is a working value the project owner gave; it
/// is not checked against the IOF school set.
pub fn north_line_spacing_m(kind: MapKind) -> f64 {
    rules::base().north_line_spacing_m(kind)
}

/// Errors for scales outside the table of a map kind.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScaleError {
    #[error(
        "a {} map cannot be printed at 1:{print_scale}; allowed print scales: {}",
        kind.name(),
        join_scales(allowed)
    )]
    PrintScaleNotAllowed {
        kind: MapKind,
        print_scale: u32,
        allowed: Vec<u32>,
    },
    #[error(
        "a {} map cannot be drawn at 1:{map_scale}; allowed map scales: {}",
        kind.name(),
        join_scales(allowed)
    )]
    MapScaleNotAllowed {
        kind: MapKind,
        map_scale: u32,
        allowed: Vec<u32>,
    },
}

/// Symbol size at `scale` relative to the base scale of the kind (sprint 1:4000, forest 1:15000,
/// schoolyard 1:1000), as the standard sets it. `None` outside the table.
///
/// The table (ADR 0018), from the rule profile:
/// - Sprint, ISSprOM 2019-2 §3.1 and §4.7: base 1:4000; 1:3000 enlarges every symbol to 133 %.
/// - Forest, ISOM 2017-2 §2.9.1 and §3.7: base 1:15000; 1:10000 is 150 %, 1:7500 200 %, 1:5000 300 %.
/// - Schoolyard: 1:1000 only (the IOF school set is drawn for that scale).
/// - Reductions (sprint 1:5000, forest 1:20000) are not in the standards; the symbols keep their base
///   size.
pub fn standard_enlargement(kind: MapKind, scale: u32) -> Option<f64> {
    rules::base()
        .scale_entry(kind, scale)
        .map(|e| e.size_factor)
}

/// Whether the standard foresees printing a map of `kind` at `scale`. Reductions below the base scale
/// and scales outside the table are not standard.
pub fn is_standard(kind: MapKind, scale: u32) -> bool {
    rules::base().is_standard_scale(kind, scale)
}

/// Factor for symbol dimensions when a map drawn at `map_scale` is printed at `print_scale`: the
/// standard size at the print scale divided by the standard size at the map scale.
pub fn symbol_factor(kind: MapKind, map_scale: u32, print_scale: u32) -> Result<f64, ScaleError> {
    let from =
        standard_enlargement(kind, map_scale).ok_or_else(|| ScaleError::MapScaleNotAllowed {
            kind,
            map_scale,
            allowed: allowed_scales(kind),
        })?;
    let to = standard_enlargement(kind, print_scale).ok_or_else(|| {
        ScaleError::PrintScaleNotAllowed {
            kind,
            print_scale,
            allowed: allowed_scales(kind),
        }
    })?;
    Ok(to / from)
}

/// Factor for object geometry: map scale divided by print scale.
pub fn geometry_factor(map_scale: u32, print_scale: u32) -> f64 {
    f64::from(map_scale) / f64::from(print_scale)
}

/// The course symbol sizes of the base profile for `kind` (mm at the base scale) and the factor that takes
/// them to `print_scale`: the two inputs of `freemapper_render::overprint::overprint`.
pub fn course_symbols(
    kind: MapKind,
    print_scale: u32,
) -> (freemapper_render::overprint::CourseSymbolSizes, f64) {
    let rules = rules::base();
    (
        rules.course_symbol(kind),
        rules.course_factor(kind, print_scale),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn course_symbols_scale_with_the_print_scale() {
        let (sizes, factor) = course_symbols(MapKind::Sprint, 4000);
        assert_eq!((sizes.circle_diameter_mm, sizes.line_width_mm), (6.0, 0.35));
        assert_eq!(factor, 1.0);
        let (_, factor) = course_symbols(MapKind::Sprint, 3000);
        assert!(close(factor, 4.0 / 3.0), "{factor}");
        let (sizes, _) = course_symbols(MapKind::Forest, 15000);
        assert_eq!(sizes.circle_diameter_mm, 5.0);
    }

    #[test]
    fn every_scale_freemapper_draws_is_in_the_table() {
        for kind in MapKind::ALL {
            for scale in kind.drawing_scales() {
                assert!(allowed_scales(kind).contains(scale), "{kind} {scale}");
            }
        }
    }

    #[test]
    fn proposed_contour_intervals() {
        // ISSprOM 2019 and freemapper both use 2 m for sprint maps.
        assert_eq!(proposed_contour_interval_m(MapKind::Sprint), Some(2.0));
        assert_eq!(proposed_contour_interval_m(MapKind::Forest), Some(5.0));
        assert_eq!(proposed_contour_interval_m(MapKind::Schoolyard), None);
    }

    #[test]
    fn north_line_spacing_per_kind() {
        assert_eq!(north_line_spacing_m(MapKind::Sprint), 120.0);
        assert_eq!(north_line_spacing_m(MapKind::Forest), 300.0);
        assert_eq!(north_line_spacing_m(MapKind::Schoolyard), 37.5);
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn sprint_enlarges_to_133_percent_at_3000() {
        let f = symbol_factor(MapKind::Sprint, 4000, 3000).unwrap();
        assert!(close(f, 4.0 / 3.0), "{f}");
        // The 0.27 mm small footpath (506 in ISSprOM 2019) prints 0.36 mm wide.
        assert!(close(270.0 * f, 360.0), "{}", 270.0 * f);
        assert_eq!(symbol_factor(MapKind::Sprint, 4000, 4000), Ok(1.0));
    }

    #[test]
    fn sprint_at_5000_is_a_non_standard_reduction() {
        assert_eq!(symbol_factor(MapKind::Sprint, 4000, 5000), Ok(1.0));
        assert!(!is_standard(MapKind::Sprint, 5000));
        assert!(is_standard(MapKind::Sprint, 4000) && is_standard(MapKind::Sprint, 3000));
    }

    #[test]
    fn forest_enlarges_as_isom_says() {
        assert_eq!(symbol_factor(MapKind::Forest, 15000, 10000), Ok(1.5));
        assert_eq!(symbol_factor(MapKind::Forest, 15000, 7500), Ok(2.0));
        assert_eq!(symbol_factor(MapKind::Forest, 15000, 5000), Ok(3.0));
        // A map drawn at 1:10000 (150 % symbols) printed at 1:15000 goes back to 100 %.
        assert!(close(
            symbol_factor(MapKind::Forest, 10000, 15000).unwrap(),
            1.0 / 1.5
        ));
        assert_eq!(symbol_factor(MapKind::Forest, 15000, 20000), Ok(1.0));
        assert!(!is_standard(MapKind::Forest, 20000));
        for scale in [5000, 7500, 10000, 15000] {
            assert!(is_standard(MapKind::Forest, scale), "{scale}");
        }
    }

    #[test]
    fn table_rows_are_unique_and_allowed_scales_follow_it() {
        for kind in [MapKind::Sprint, MapKind::Schoolyard, MapKind::Forest] {
            let scales = allowed_scales(kind);
            assert!(
                scales.windows(2).all(|w| w[0] < w[1]),
                "{kind:?}: {scales:?}"
            );
            for s in scales {
                assert!(standard_enlargement(kind, s).is_some());
            }
        }
        assert_eq!(allowed_scales(MapKind::Sprint), [3000, 4000, 5000]);
        assert_eq!(
            allowed_scales(MapKind::Forest),
            [5000, 7500, 10000, 15000, 20000]
        );
        assert_eq!(allowed_scales(MapKind::Schoolyard), [1000]);
        assert_eq!(standard_enlargement(MapKind::Schoolyard, 1000), Some(1.0));
        assert_eq!(standard_enlargement(MapKind::Forest, 4000), None);
    }

    #[test]
    fn schoolyard_only_1000() {
        assert_eq!(symbol_factor(MapKind::Schoolyard, 1000, 1000), Ok(1.0));
        let err = symbol_factor(MapKind::Schoolyard, 1000, 2000).unwrap_err();
        match &err {
            ScaleError::PrintScaleNotAllowed { allowed, .. } => assert_eq!(allowed, &[1000]),
            other => panic!("unexpected {other:?}"),
        }
        assert!(err.to_string().contains("1:1000"));
    }

    #[test]
    fn sprint_refuses_10000() {
        let err = symbol_factor(MapKind::Sprint, 4000, 10000).unwrap_err();
        match &err {
            ScaleError::PrintScaleNotAllowed { allowed, .. } => {
                assert_eq!(allowed, &[3000, 4000, 5000])
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            err.to_string(),
            "a sprint map cannot be printed at 1:10000; allowed print scales: 1:3000, 1:4000, 1:5000"
        );
        let err = symbol_factor(MapKind::Sprint, 10000, 4000).unwrap_err();
        assert!(matches!(err, ScaleError::MapScaleNotAllowed { .. }));
        assert_eq!(
            err.to_string(),
            "a sprint map cannot be drawn at 1:10000; allowed map scales: 1:3000, 1:4000, 1:5000"
        );
    }

    #[test]
    fn geometry_factor_4000_to_5000() {
        assert!(close(geometry_factor(4000, 5000), 0.8));
    }
}

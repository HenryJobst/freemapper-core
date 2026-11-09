//! The conversion between a course file's number offset and a number placement.

use freemapper_course::model::{NumberPlacement, Offset};
use freemapper_course::placement::{Placement, number_box, placement_of};
use freemapper_course::{offset_from_placement, placement_from_offset};

/// Offsets (map frame, y down), outer circle radii, box sizes and ratios. Every box lies outside its circle
/// centre, the case the conversion is exact for.
const OFFSETS: [(f64, f64); 8] = [
    (8.0, -6.0),
    (-8.0, -6.0),
    (0.0, 7.5),
    (6.0, 0.0),
    (-4.2, 3.9),
    (2.5, -9.0),
    (12.0, 12.0),
    (-0.4, -5.1),
];
const RADII: [f64; 3] = [2.5, 3.0 + 0.175, 4.0];
const SIZES: [(f64, f64); 3] = [(2.0, 2.0), (2.9, 3.5), (5.6, 3.5)];
const RATIOS: [f64; 4] = [1.0, 1.25, 0.8, 15000.0 / 4000.0];

#[test]
fn offset_placement_round_trip() {
    for (dx, dy) in OFFSETS {
        for r in RADII {
            for size in SIZES {
                for ratio in RATIOS {
                    let offset = Offset {
                        dx_mm: dx * ratio,
                        dy_mm: dy * ratio,
                    };
                    let p = placement_from_offset(offset, r, size, ratio);
                    let back = offset_from_placement(p, r, size, ratio);
                    let case =
                        format!("{offset:?} r {r} size {size:?} ratio {ratio}: {p:?} {back:?}");
                    assert!((back.dx_mm - offset.dx_mm).abs() < 1e-6, "{case}");
                    assert!((back.dy_mm - offset.dy_mm).abs() < 1e-6, "{case}");
                }
            }
        }
    }
}

#[test]
fn placement_offset_round_trip() {
    for direction in (0..24).map(|k| f64::from(k) * 15.0 + 7.5) {
        for gap_mm in [0.0, 0.5, 1.75, 5.0] {
            for ratio in RATIOS {
                let p = NumberPlacement { direction, gap_mm };
                let o = offset_from_placement(p, 3.175, (2.9, 3.5), ratio);
                let back = placement_from_offset(o, 3.175, (2.9, 3.5), ratio);
                assert!((back.direction - direction).abs() < 1e-6, "{p:?} {back:?}");
                assert!((back.gap_mm - gap_mm).abs() < 1e-6, "{p:?} {back:?}");
            }
        }
    }
}

#[test]
fn the_numbers_are_freemapper_prints() {
    // freemapper-print's `file_placement` (numbers.rs, commit 918cec2): the offset divided by the ratio is the
    // box centre at the nominal size, and the placement is `placement_of` around the origin.
    let (r, size) = (3.175, (2.9, 3.5));
    for (dx, dy) in OFFSETS {
        for ratio in RATIOS {
            let offset = Offset {
                dx_mm: dx,
                dy_mm: dy,
            };
            let want = placement_of((0.0, 0.0), r, (dx / ratio, dy / ratio), size);
            let got = placement_from_offset(offset, r, size, ratio);
            assert_eq!(Placement::from(got), want);
        }
    }
    // freemapper-print's test `file_offset_round_trips_at_the_print_scale`: north-east at (8, -6), and at
    // ratio 1.25 the box centre is at (6.4, -4.8) of the nominal size.
    let offset = Offset {
        dx_mm: 8.0,
        dy_mm: -6.0,
    };
    let p = placement_from_offset(offset, r, size, 1.0);
    assert!((p.direction - 6f64.atan2(8.0).to_degrees()).abs() < 1e-9);
    let p = placement_from_offset(offset, r, size, 1.25);
    let b = number_box((0.0, 0.0), r, size, p.into());
    assert!((b.centre().0 - 6.4).abs() < 1e-9 && (b.centre().1 + 4.8).abs() < 1e-9);
}

#[test]
fn a_ratio_that_is_not_positive_counts_as_one() {
    let offset = Offset {
        dx_mm: 5.0,
        dy_mm: 2.0,
    };
    let one = placement_from_offset(offset, 3.0, (2.0, 3.0), 1.0);
    for ratio in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        assert_eq!(placement_from_offset(offset, 3.0, (2.0, 3.0), ratio), one);
        let back = offset_from_placement(one, 3.0, (2.0, 3.0), ratio);
        assert!((back.dx_mm - 5.0).abs() < 1e-6 && (back.dy_mm - 2.0).abs() < 1e-6);
    }
}

#[test]
fn placement_and_number_placement_convert_both_ways() {
    let n = NumberPlacement {
        direction: 135.0,
        gap_mm: 0.75,
    };
    let p = Placement::from(n);
    assert_eq!((p.direction, p.gap_mm), (135.0, 0.75));
    assert_eq!(NumberPlacement::from(p), n);
}

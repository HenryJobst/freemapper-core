//! Scenarios ported from freemapper-courseplan's `numbers.rs` tests, same names, sprint sizes.

use super::super::{GAP_LIMITS_MM, Score, angle_diff, score};
use super::*;

/// Sprint circle: radius 3 mm, line 0.35 mm.
const CIRCLE_R: f64 = 3.0;
const HALF_LINE: f64 = 0.175;

/// A numbered stop with `digits` digits, 4 mm high, 0.6 × height per digit.
fn stop(x: f64, y: f64, digits: u32) -> NumberStop {
    NumberStop {
        centre: (x, y),
        size: Some((2.4 * f64::from(digits), 4.0)),
        kept: None,
    }
}

/// A stop without number (crossing point).
fn crossing(x: f64, y: f64) -> NumberStop {
    NumberStop {
        centre: (x, y),
        size: None,
        kept: None,
    }
}

/// Straight legs from `start` over the stops to `finish`.
fn straight_legs(start: Pt, stops: &[NumberStop], finish: Pt) -> Vec<Vec<Pt>> {
    let points: Vec<Pt> = std::iter::once(start)
        .chain(stops.iter().map(|s| s.centre))
        .chain(std::iter::once(finish))
        .collect();
    points.windows(2).map(|w| vec![w[0], w[1]]).collect()
}

fn course<'a>(stops: &'a [NumberStop], legs: &'a [Vec<Pt>], gaps: GapRange) -> CourseNumbers<'a> {
    CourseNumbers {
        stops,
        legs,
        circle_r: CIRCLE_R,
        half_line: HALF_LINE,
        unit: 1.0,
        gaps,
    }
}

fn place(course: &CourseNumbers<'_>) -> Vec<Option<Placement>> {
    place_course_numbers(course, &[], Rect::UNBOUNDED)
}

/// The box of stop `k` at its result.
fn box_of(course: &CourseNumbers<'_>, placed: &[Option<Placement>], k: usize) -> Rect {
    course.target(k).unwrap().box_at(placed[k].unwrap())
}

/// The score of stop `k`'s box in the finished course: the circles and legs it sees, and every other number at
/// its result.
fn final_score(course: &CourseNumbers<'_>, placed: &[Option<Placement>], k: usize) -> Score {
    let scene = Scene::new(course);
    let own = Scene::own_legs(course, k);
    let numbers: Vec<Obstacle> = (0..placed.len())
        .filter(|&j| j != k)
        .filter_map(|j| {
            let p = placed[j]?;
            Some(number_obstacle(j, course.target(j)?.box_at(p)))
        })
        .collect();
    score(
        box_of(course, placed, k),
        Rect::UNBOUNDED,
        scene.overprint(course, k, &own).chain(&numbers),
    )
}

fn has_label(score: &Score, prefix: char) -> bool {
    score.hard.iter().any(|l| l.starts_with(prefix))
}

#[test]
fn number_avoids_both_legs() {
    // y is down. The start lies north-east of the control (45 degrees), the finish south-east (315 degrees); the
    // widest gap is the west side. Behind the incoming leg is 225, and nothing is in the way there.
    let stops = [stop(100.0, 100.0, 1)];
    let legs = straight_legs((120.0, 80.0), &stops, (120.0, 120.0));
    let c = course(&stops, &legs, GapRange::DEFAULT);
    let placed = place(&c);
    assert_eq!(placed.len(), 1);
    let p = placed[0].unwrap();
    assert!(angle_diff(p.direction, 225.0) < 1e-9, "{p:?}");
    assert_eq!(p.gap_mm, 0.5);
    let s = final_score(&c, &placed, 0);
    assert!(!has_label(&s, 'l'), "{s:?}");
}

#[test]
fn number_avoids_a_neighbour_circle() {
    // Start west, second control south: behind the incoming leg is 0 (east). The third stop sits 8.5 mm away in
    // that very direction and is reached from the north, so only its circle can move the number off 0 degrees.
    let d = 8.5;
    let stops = [
        stop(100.0, 100.0, 1),
        stop(100.0, 130.0, 1),
        stop(100.0 + d, 100.0, 1),
    ];
    let legs = vec![
        vec![(70.0, 100.0), (100.0, 100.0)],
        vec![(100.0, 100.0), (100.0, 130.0)],
        vec![
            (100.0, 130.0),
            (140.0, 130.0),
            (140.0, 70.0),
            (100.0 + d, 70.0),
            (100.0 + d, 100.0),
        ],
        vec![(100.0 + d, 100.0), (130.0, 100.0)],
    ];
    let c = course(&stops, &legs, GapRange::DEFAULT);
    let placed = place(&c);
    let p = placed[0].unwrap();
    assert!(angle_diff(p.direction, 0.0) >= 15.0, "{p:?}");
    let other = stops[2].centre;
    assert!(box_of(&c, &placed, 0).distance_to_point(other) >= CIRCLE_R + HALF_LINE);
    for k in 0..stops.len() {
        let s = final_score(&c, &placed, k);
        assert!(!has_label(&s, 'c') && !has_label(&s, 'n'), "{k}: {s:?}");
    }
}

#[test]
fn numbers_do_not_overlap_each_other() {
    // Two controls 2 mm apart on a straight east-west course: the boxes must not overlap, whatever direction each
    // one takes.
    let stops = [stop(100.0, 100.0, 1), stop(102.0, 100.0, 1)];
    let legs = straight_legs((70.0, 100.0), &stops, (140.0, 100.0));
    let c = course(&stops, &legs, GapRange::DEFAULT);
    let placed = place(&c);
    let (a, b) = (box_of(&c, &placed, 0), box_of(&c, &placed, 1));
    assert!(!a.overlaps(&b), "{a:?} overlaps {b:?}");
    for k in 0..stops.len() {
        let s = final_score(&c, &placed, k);
        assert!(!has_label(&s, 'c') && !has_label(&s, 'n'), "{k}: {s:?}");
    }
}

#[test]
fn new_numbers_avoid_kept_numbers() {
    // The kept number of the first stop points north, exactly where the second would like to put its own.
    let kept = Placement {
        direction: 90.0,
        gap_mm: 0.5,
    };
    let mut stops = [stop(100.0, 100.0, 1), stop(102.0, 100.0, 1)];
    stops[0].kept = Some(kept);
    let legs = straight_legs((70.0, 100.0), &stops, (140.0, 100.0));
    let c = course(&stops, &legs, GapRange::DEFAULT);
    let placed = place(&c);
    assert_eq!(placed[0], Some(kept));
    let new = placed[1].unwrap();
    assert!(angle_diff(new.direction, 90.0) > 1.0, "{new:?}");
    let (a, b) = (box_of(&c, &placed, 0), box_of(&c, &placed, 1));
    assert!(!a.overlaps(&b), "{a:?} overlaps {b:?}");
    assert!(!has_label(&final_score(&c, &placed, 1), 'n'));
}

#[test]
fn kept_number_outside_range_is_not_moved() {
    let far = Placement {
        direction: 90.0,
        gap_mm: 7.0,
    };
    let mut stops = [stop(100.0, 100.0, 1)];
    stops[0].kept = Some(far);
    let legs = straight_legs((70.0, 100.0), &stops, (100.0, 140.0));
    let c = course(&stops, &legs, GapRange { min: 0.5, max: 0.5 });
    assert_eq!(place(&c), vec![Some(far)]);
}

#[test]
fn crossing_points_get_no_number() {
    let stops = [
        stop(100.0, 100.0, 1),
        crossing(130.0, 100.0),
        stop(160.0, 100.0, 1),
    ];
    let legs = straight_legs((70.0, 100.0), &stops, (190.0, 100.0));
    let placed = place(&course(&stops, &legs, GapRange::DEFAULT));
    assert_eq!(placed.len(), 3);
    assert!(placed[0].is_some() && placed[1].is_none() && placed[2].is_some());

    // The scene of `number_avoids_a_neighbour_circle` with the third stop a crossing point: its circle is no
    // obstacle, so the first number stays behind the control, at 0 degrees.
    let d = 8.5;
    let stops = [
        stop(100.0, 100.0, 1),
        stop(100.0, 130.0, 1),
        crossing(100.0 + d, 100.0),
    ];
    let legs = vec![
        vec![(70.0, 100.0), (100.0, 100.0)],
        vec![(100.0, 100.0), (100.0, 130.0)],
        vec![
            (100.0, 130.0),
            (140.0, 130.0),
            (140.0, 70.0),
            (100.0 + d, 70.0),
            (100.0 + d, 100.0),
        ],
        vec![(100.0 + d, 100.0), (130.0, 100.0)],
    ];
    let placed = place(&course(&stops, &legs, GapRange::DEFAULT));
    assert_eq!(placed[2], None);
    assert_eq!(placed[0].unwrap().direction, 0.0, "{placed:?}");
}

#[test]
fn a_control_without_legs_points_to_45_degrees() {
    let stops = [stop(100.0, 100.0, 1)];
    let legs = vec![Vec::new(), Vec::new()];
    let placed = place(&course(&stops, &legs, GapRange::DEFAULT));
    let p = placed[0].unwrap();
    assert_eq!(p.direction, NO_LEGS_DIRECTION_DEG);
    assert_eq!(p.direction, 45.0);
    assert_eq!(p.gap_mm, 0.5);
}

#[test]
fn the_same_control_twice_ignores_its_own_place() {
    // A control visited twice in a row: both stops lie on the same point. Neither sees the other's circle, and
    // the second number keeps clear of the first.
    let stops = [stop(100.0, 100.0, 1), stop(100.0, 100.0, 1)];
    let legs = straight_legs((70.0, 100.0), &stops, (100.0, 140.0));
    let c = course(&stops, &legs, GapRange::DEFAULT);
    let scene = Scene::new(&c);
    for k in 0..2 {
        let own = Scene::own_legs(&c, k);
        let labels: Vec<&str> = scene
            .overprint(&c, k, &own)
            .map(|o| o.label.as_str())
            .collect();
        assert!(
            !labels.iter().any(|l| l.starts_with('c')),
            "{k}: {labels:?}"
        );
    }
    let placed = place(&c);
    assert!(placed.iter().all(Option::is_some));
    let (a, b) = (box_of(&c, &placed, 0), box_of(&c, &placed, 1));
    assert!(!a.overlaps(&b), "{a:?} overlaps {b:?}");
}

#[test]
fn strictest_takes_largest_min_and_smallest_max() {
    assert_eq!(
        GapRange::strictest([(Some(0.5), Some(5.0)), (Some(1.0), None), (None, Some(3.0))]),
        Some((Some(1.0), Some(3.0)))
    );
    assert_eq!(GapRange::strictest([(None, None)]), None);
    assert_eq!(GapRange::strictest([(Some(f64::NAN), Some(-1.0))]), None);
    assert_eq!(GapRange::strictest([]), None);
    // An unusable value of one rule leaves the usable one of another alone.
    assert_eq!(
        GapRange::strictest([(Some(1.0), Some(3.0)), (None, Some(f64::NAN))]),
        Some((Some(1.0), Some(3.0)))
    );
}

/// courseplan's old reading of several `number_gap` rules: each rule on its own made usable (invalid values
/// dropped, the rest held to 0.01..10 mm, a maximum below the minimum raised to it), then the strictest sides,
/// then the range (its `normalise_sides` and `gap_range`).
fn courseplan_gap_range(rules: &[(Option<f64>, Option<f64>)]) -> GapRange {
    let clean = |v: Option<f64>| {
        v.filter(|v| v.is_finite() && *v > 0.0)
            .map(|v| v.clamp(GAP_LIMITS_MM.0, GAP_LIMITS_MM.1))
    };
    let sides: Vec<(Option<f64>, Option<f64>)> = rules
        .iter()
        .map(|&(lo, hi)| match (clean(lo), clean(hi)) {
            (Some(lo), Some(hi)) => (Some(lo), Some(hi.max(lo))),
            other => other,
        })
        .collect();
    let min = sides
        .iter()
        .filter_map(|s| s.0)
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        });
    let max = sides
        .iter()
        .filter_map(|s| s.1)
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.min(v)))
        });
    GapRange::from_limits(min, max)
}

#[test]
fn strictest_equals_per_rule_normalising() {
    let values = [None, Some(0.3), Some(1.0), Some(2.5)];
    let rules: Vec<(Option<f64>, Option<f64>)> = values
        .iter()
        .flat_map(|&lo| values.iter().map(move |&hi| (lo, hi)))
        .collect();
    assert_eq!(rules.len(), 16);
    for &a in &rules {
        for &b in &rules {
            let combined = GapRange::strictest([a, b])
                .map_or(GapRange::DEFAULT, |(lo, hi)| GapRange::from_limits(lo, hi));
            assert_eq!(combined, courseplan_gap_range(&[a, b]), "{a:?} {b:?}");
        }
    }
}

#[test]
fn course_module_has_no_crate_imports() {
    // Like the parent module, `course.rs` is shared with freemapper-courseplan and reaches only its parent.
    let source = include_str!("../course.rs");
    for forbidden in ["crate::", "freemapper_print"] {
        assert!(
            !source.contains(forbidden),
            "placement/course.rs uses {forbidden}"
        );
    }
}

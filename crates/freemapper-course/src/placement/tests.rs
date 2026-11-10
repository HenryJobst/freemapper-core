use super::*;

/// Sprint circle: radius 3 mm, line 0.35 mm.
const R_OUTER: f64 = 3.175;
const HALF_LINE: f64 = 0.175;
/// One digit and two digits, 4 mm high, as courseplan's stand-in (0.6 × height per digit).
const ONE: (f64, f64) = (2.4, 4.0);
const TWO: (f64, f64) = (4.8, 4.0);

fn big_bounds() -> Rect {
    Rect {
        min: (-1000.0, -1000.0),
        max: (1000.0, 1000.0),
    }
}

fn overprint(shape: ObstacleShape, label: &str) -> Obstacle {
    Obstacle {
        shape,
        weight: 0.0,
        tier: Tier::Overprint,
        label: label.to_string(),
    }
}

/// The legs from `from` (the previous point) through `at` to `to` as overprint obstacles, each line from the
/// circle edge outwards.
fn leg_obstacles(from: Pt, at: Pt, to: Pt) -> Vec<Obstacle> {
    [(from, "in"), (to, "out")]
        .into_iter()
        .map(|(p, label)| {
            let trimmed = trim_start(&[at, p], R_OUTER - HALF_LINE);
            overprint(
                ObstacleShape::Segment {
                    a: trimmed[0],
                    b: trimmed[1],
                    half_width: HALF_LINE,
                },
                label,
            )
        })
        .collect()
}

/// A control at `at` with one digit, reached from `from` and left towards `to`.
fn target(from: Option<Pt>, at: Pt, to: Option<Pt>) -> Target {
    Target {
        centre: at,
        circle_outer_r: R_OUTER,
        size: ONE,
        unit: 1.0,
        legs: Legs {
            incoming: from.and_then(|f| direction_to(at, f)),
            outgoing: to.map(|t| vec![at, t]).unwrap_or_default(),
            half_line: HALF_LINE,
        },
        gaps: PROFILE_GAPS,
    }
}

/// The range of `iof-print` (ruling R29).
const PROFILE_GAPS: GapRange = GapRange { min: 0.5, max: 5.0 };

const START: Placement = Placement {
    direction: 45.0,
    gap_mm: 0.5,
};

#[test]
fn gap_steps_run_from_min_to_exactly_max() {
    let steps = PROFILE_GAPS.steps();
    assert_eq!(steps.len(), 10);
    assert_eq!((steps[0], steps[9]), (0.5, 5.0));
    assert_eq!(GapRange { min: 0.5, max: 1.2 }.steps(), [0.5, 1.0, 1.2]);
    assert_eq!(GapRange::DEFAULT.steps(), [DEFAULT_GAP_MM]);
    // Every step comes out rounded to 0.01 mm (0.8 + 0.5 is 1.3, not 1.3000000000000003).
    let steps = GapRange { min: 0.8, max: 5.0 }.steps();
    assert_eq!(steps, [0.8, 1.3, 1.8, 2.3, 2.8, 3.3, 3.8, 4.3, 4.8, 5.0]);
    // Ruling R30, courseplan ADR 0032 and its `gap_range`: limits are rounded inwards, held to 0.01..10 mm, a
    // missing minimum is 0.5, a missing maximum equals the minimum (placement does not wander out to 10 mm), and a
    // maximum below the minimum is raised to it. Values that are not finite or not above 0 count as missing.
    let r = GapRange::from_limits(Some(0.123), Some(3.456));
    assert_eq!((r.min, r.max), (0.13, 3.45));
    let r = GapRange::from_limits(None, Some(12.0));
    assert_eq!((r.min, r.max), (0.5, 10.0));
    let r = GapRange::from_limits(Some(0.004), None);
    assert_eq!((r.min, r.max), (0.01, 0.01));
    let r = GapRange::from_limits(Some(0.8), None);
    assert_eq!((r.min, r.max), (0.8, 0.8));
    let r = GapRange::from_limits(Some(f64::NAN), Some(-1.0));
    assert_eq!((r.min, r.max), (0.5, 0.5));
    // The check keeps a missing maximum open, up to the largest gap a correction may have.
    let r = GapRange::allowed(Some(0.8), None);
    assert_eq!((r.min, r.max), (0.8, 10.0));
    let r = GapRange::allowed(None, Some(2.0));
    assert_eq!((r.min, r.max), (0.01, 2.0));
    let r = GapRange::from_limits(Some(3.0), Some(1.0));
    assert_eq!((r.min, r.max), (3.0, 3.0));
    assert_eq!(r.steps(), [3.0]);
    assert!(
        PROFILE_GAPS.contains(5.0) && !PROFILE_GAPS.contains(0.3) && !PROFILE_GAPS.contains(5.1)
    );
}

#[test]
fn number_box_gap_definition() {
    // Due east with gap 1: the box's left edge lies 1 mm beyond the circle line.
    for size in [ONE, TWO] {
        let b = number_box(
            (10.0, 20.0),
            R_OUTER,
            size,
            Placement {
                direction: 0.0,
                gap_mm: 1.0,
            },
        );
        assert!((b.min.0 - (10.0 + R_OUTER + 1.0)).abs() < 1e-9, "{b:?}");
        assert!((b.centre().1 - 20.0).abs() < 1e-9);
        // At 45 degrees with gap 0.3 the box is 0.3 mm from the circle line, at its nearest corner.
        let b = number_box(
            (10.0, 20.0),
            R_OUTER,
            size,
            Placement {
                direction: 45.0,
                gap_mm: 0.3,
            },
        );
        assert!(
            (b.distance_to_point((10.0, 20.0)) - (R_OUTER + 0.3)).abs() < 1e-9,
            "{b:?}"
        );
        // North-east is up and to the right: y is down.
        assert!(b.centre().0 > 10.0 && b.centre().1 < 20.0);
    }
}

#[test]
fn gap_is_the_clearance_between_circle_line_and_box() {
    // freemapper-courseplan's test of the same name (numbers.rs, their ADR 0030): every direction, both
    // number widths and two gaps give exactly the gap as clearance, with the box centred on the ray.
    for size in [ONE, TWO] {
        for gap in [0.3, 1.0] {
            for direction in [0.0, 30.0, 45.0, 60.0, 90.0, 135.0, 200.0, 315.0] {
                let p = Placement {
                    direction,
                    gap_mm: gap,
                };
                let rect = number_box((0.0, 0.0), R_OUTER, size, p);
                let clearance = rect.distance_to_point((0.0, 0.0)) - R_OUTER;
                assert!(
                    (clearance - gap).abs() < 1e-9,
                    "{size:?} at {direction} with gap {gap}: clearance {clearance}"
                );
                let (cx, cy) = rect.centre();
                let rad = direction.to_radians();
                assert!(
                    (cx * -rad.sin() - cy * rad.cos()).abs() < 1e-9,
                    "centre off the ray"
                );
                assert!(
                    cx * rad.cos() - cy * rad.sin() > 0.0,
                    "centre behind the circle"
                );
                // The inverse gives the placement back.
                let back = placement_of((0.0, 0.0), R_OUTER, rect.centre(), size);
                assert!(angle_diff(back.direction, direction) < 1e-9, "{back:?}");
                assert!((back.gap_mm - gap).abs() < 1e-9, "{back:?}");
            }
        }
    }
    // Courseplan's value straight above: the box's lower edge 3.175 + 0.5 above the centre.
    let up = number_box(
        (100.0, 100.0),
        R_OUTER,
        ONE,
        Placement {
            direction: 90.0,
            gap_mm: 0.5,
        },
    );
    assert!((up.max.1 - (100.0 - 3.675)).abs() < 1e-9);
}

#[test]
fn candidates_are_rounded_to_a_tenth_and_never_360() {
    for k in 0..CANDIDATES {
        let d = candidate(3.04, k);
        assert!((0.0..360.0).contains(&d), "{d}");
        assert!((d * 10.0 - (d * 10.0).round()).abs() < 1e-9, "{d}");
    }
    assert_eq!(candidate(359.97, 0), 0.0);
    assert_eq!(candidate(10.0, 23), 355.0);
    // Past 360 the value is wrapped before it is rounded, so it carries no float noise (462.4 - 360).
    assert_eq!(candidate(102.4, 0), 102.4);
    assert_eq!(candidate(342.4, 8), 102.4);
    for preferred in [102.4, 7.3, 251.7, 359.9] {
        for k in 0..CANDIDATES {
            let d = candidate(preferred, k);
            assert_eq!(d, (d * 10.0).round() / 10.0, "{preferred} {k}");
        }
    }
}

#[test]
fn preferred_is_behind_the_incoming_leg() {
    // In from the west, out to the south (y down: +y): behind is east, free of the outgoing leg.
    let (from, at, to) = ((70.0, 100.0), (100.0, 100.0), (100.0, 140.0));
    let t = target(Some(from), at, Some(to));
    let (p, s) = best(&t, big_bounds(), &leg_obstacles(from, at, to), START);
    assert!(angle_diff(p.direction, 0.0) < 1e-9, "{p:?}");
    assert!(s.is_free());
    // The smallest gap wins among free places.
    assert_eq!(p.gap_mm, 0.5);
}

#[test]
fn obtuse_angle_prefers_outer_bisector() {
    // In from the left, out to the right, 170 degrees, the tip slightly up (the control lies above the line
    // from start to finish; y is down). Behind is blocked by the outgoing leg, so the number goes straight
    // above: 90.
    let rise = 30.0 * 5f64.to_radians().tan();
    let (from, at, to) = ((70.0, 100.0 + rise), (100.0, 100.0), (130.0, 100.0 + rise));
    let t = target(Some(from), at, Some(to));
    let (preferred, outer) = t.directions(START);
    assert!(angle_diff(outer, 90.0) < 1e-9, "{outer}");
    assert!(angle_diff(preferred, 90.0) < 1e-9, "{preferred}");
    let (p, _) = best(&t, big_bounds(), &leg_obstacles(from, at, to), START);
    assert!(angle_diff(p.direction, 90.0) < 1e-9, "{p:?}");
}

#[test]
fn blocked_preferred_takes_nearest_free_direction() {
    // In from the west, out to the south: behind is east (0). A strip east of the circle (another number, say)
    // blocks straight behind at every gap. The result is free, deviates least from behind among all free
    // candidates, and among those has the smallest gap.
    let (from, at, to) = ((70.0, 100.0), (100.0, 100.0), (100.0, 140.0));
    let t = target(Some(from), at, Some(to));
    let mut obstacles = leg_obstacles(from, at, to);
    obstacles.push(overprint(
        ObstacleShape::Rect(Rect {
            min: (103.0, 99.0),
            max: (114.0, 101.0),
        }),
        "number 3",
    ));
    for gap in PROFILE_GAPS.steps() {
        let behind = t.box_at(Placement {
            direction: 0.0,
            gap_mm: gap,
        });
        assert!(!score(behind, big_bounds(), &obstacles).hard.is_empty());
    }
    let (p, s) = best(&t, big_bounds(), &obstacles, START);
    assert!(s.is_free(), "{s:?}");
    let free: Vec<(f64, f64)> = PROFILE_GAPS
        .steps()
        .into_iter()
        .flat_map(|gap| (0..CANDIDATES).map(move |k| (candidate(0.0, k), gap)))
        .filter(|&(direction, gap_mm)| {
            score(
                t.box_at(Placement { direction, gap_mm }),
                big_bounds(),
                &obstacles,
            )
            .is_free()
        })
        .map(|(d, g)| (angle_diff(d, 0.0), g))
        .collect();
    let least = free.iter().map(|f| f.0).fold(f64::INFINITY, f64::min);
    let gap = free
        .iter()
        .filter(|f| f.0 == least)
        .map(|f| f.1)
        .fold(f64::INFINITY, f64::min);
    assert!(least > 0.0);
    assert_eq!(
        (angle_diff(p.direction, 0.0), p.gap_mm),
        (least, gap),
        "{p:?}"
    );
    // On the outer side (the outer bisector is 45), not the mirrored one.
    assert!(p.direction < 90.0, "{p:?}");
}

#[test]
fn moves_out_before_turning() {
    // A boulder straight behind, small enough that a larger gap clears it: the number stays behind and moves
    // further out (ruling R29: the gap ranks before the direction).
    let (from, at, to) = ((70.0, 100.0), (100.0, 100.0), (100.0, 140.0));
    let t = target(Some(from), at, Some(to));
    let near = t.box_at(Placement {
        direction: 0.0,
        gap_mm: 0.5,
    });
    let boulder = Obstacle {
        shape: ObstacleShape::Circle {
            centre: near.centre(),
            radius: 0.5,
        },
        weight: 0.0,
        tier: Tier::MapSymbol,
        label: "boulder".into(),
    };
    let (p, s) = best(&t, big_bounds(), std::slice::from_ref(&boulder), START);
    assert!(s.is_free(), "{s:?}");
    assert_eq!(p.direction, 0.0);
    assert!(p.gap_mm > 0.5, "{p:?}");
    // The smallest gap that clears it.
    let cleared = |g: f64| {
        score(
            t.box_at(Placement {
                direction: 0.0,
                gap_mm: g,
            }),
            big_bounds(),
            std::slice::from_ref(&boulder),
        )
        .is_free()
    };
    assert!(cleared(p.gap_mm) && !cleared(p.gap_mm - GAP_STEP_MM));
}

#[test]
fn behind_blocked_is_tested_at_the_smallest_gap() {
    // In from the west, out at 340 degrees (20 below east). At 0.5 mm the box behind touches the outgoing leg,
    // at 5 mm it is clear of it: the test at the smallest gap decides.
    let at = (100.0, 100.0);
    let out = (
        100.0 + 30.0 * 20f64.to_radians().cos(),
        100.0 + 30.0 * 20f64.to_radians().sin(),
    );
    let mut t = target(Some((70.0, 100.0)), at, Some(out));
    let (preferred, outer) = t.directions(START);
    assert!(angle_diff(preferred, outer) < 1e-9, "{preferred} {outer}");
    t.gaps = GapRange { min: 5.0, max: 5.0 };
    let (preferred, _) = t.directions(START);
    assert!(angle_diff(preferred, 0.0) < 1e-9, "{preferred}");
}

#[test]
fn point_and_line_symbols_are_hard_area_symbols_soft() {
    let b = Rect {
        min: (0.0, 0.0),
        max: (2.0, 4.0),
    };
    let obstacles = vec![
        Obstacle {
            shape: ObstacleShape::Polygon(vec![(1.5, 1.5), (2.5, 1.5), (2.5, 2.5), (1.5, 2.5)]),
            weight: 0.0,
            tier: Tier::MapSymbol,
            label: "boulder".into(),
        },
        Obstacle {
            shape: ObstacleShape::Segment {
                a: (-5.0, 3.0),
                b: (5.0, 3.0),
                half_width: 0.1,
            },
            weight: 0.0,
            tier: Tier::MapSymbol,
            label: "wall".into(),
        },
        // The same wall drawn as a second primitive counts once.
        Obstacle {
            shape: ObstacleShape::Segment {
                a: (-5.0, 3.2),
                b: (5.0, 3.2),
                half_width: 0.1,
            },
            weight: 0.0,
            tier: Tier::MapSymbol,
            label: "wall".into(),
        },
        Obstacle {
            shape: ObstacleShape::Rect(Rect {
                min: (1.0, -10.0),
                max: (10.0, 10.0),
            }),
            weight: 4.0,
            tier: Tier::Soft,
            label: "building".into(),
        },
    ];
    let s = score(b, big_bounds(), &obstacles);
    assert!(s.hard.is_empty());
    assert_eq!(s.covered, ["boulder", "wall"]);
    // 1 × 4 mm² of the building at weight 4.
    assert!((s.cost - 16.0).abs() < 1e-9, "{}", s.cost);

    // A boulder behind the circle moves the number; a building there does not.
    let (from, at, to) = ((70.0, 100.0), (100.0, 100.0), (100.0, 140.0));
    let t = target(Some(from), at, Some(to));
    let behind = t.box_at(Placement {
        direction: 0.0,
        gap_mm: 0.5,
    });
    let area = Obstacle {
        shape: ObstacleShape::Rect(behind.grown(5.0)),
        weight: 10.0,
        tier: Tier::Soft,
        label: "building".into(),
    };
    let (p, s) = best(&t, big_bounds(), std::slice::from_ref(&area), START);
    assert_eq!((p.direction, p.gap_mm), (0.0, 0.5));
    assert!(s.cost > 0.0 && s.is_free());
    // A boulder that covers the place behind at every gap moves the number to another direction.
    let boulder = Obstacle {
        shape: ObstacleShape::Segment {
            a: behind.centre(),
            b: (behind.centre().0 + 20.0, behind.centre().1),
            half_width: 0.5,
        },
        weight: 0.0,
        tier: Tier::MapSymbol,
        label: "boulder".into(),
    };
    let (p, s) = best(&t, big_bounds(), &[area, boulder], START);
    assert!(s.is_free(), "{s:?}");
    assert!(angle_diff(p.direction, 0.0) > 1.0, "{p:?}");
}

#[test]
fn overprint_outranks_map_symbols() {
    // Every direction covers a map symbol (one big symbol around the circle), and the preferred box also hits
    // overprint: placement never fails and takes a candidate without overprint conflict.
    let t = target(None, (0.0, 0.0), None);
    let preferred = t.box_at(START);
    let obstacles = vec![
        Obstacle {
            shape: ObstacleShape::Circle {
                centre: (0.0, 0.0),
                radius: 30.0,
            },
            weight: 0.0,
            tier: Tier::MapSymbol,
            label: "everywhere".into(),
        },
        overprint(
            ObstacleShape::Circle {
                centre: preferred.centre(),
                radius: 0.5,
            },
            "circle 32",
        ),
    ];
    assert!(!score(preferred, big_bounds(), &obstacles).hard.is_empty());
    let (p, s) = best(&t, big_bounds(), &obstacles, START);
    assert!(s.hard.is_empty(), "{p:?} {s:?}");
    assert_eq!(s.covered, ["everywhere"]);
    assert_ne!(p, START);
}

#[test]
fn leaving_the_bounds_is_a_hard_conflict() {
    let bounds = Rect {
        min: (0.0, 0.0),
        max: (10.0, 10.0),
    };
    let inside = Rect {
        min: (1.0, 1.0),
        max: (10.0, 4.0),
    };
    assert!(score(inside, bounds, &[]).hard.is_empty());
    let out = Rect {
        min: (8.0, 1.0),
        max: (10.5, 4.0),
    };
    assert_eq!(score(out, bounds, &[]).hard, [OUTSIDE]);
    // A circle near the right edge: the number goes inward.
    let t = Target {
        centre: (6.0, 5.0),
        ..target(None, (6.0, 5.0), None)
    };
    let mut current = START;
    current.direction = 0.0;
    let (p, s) = best(
        &t,
        Rect {
            min: (0.0, -10.0),
            max: (10.0, 20.0),
        },
        &[],
        current,
    );
    assert!(s.hard.is_empty(), "{p:?} {s:?}");
}

#[test]
fn shapes_hit_and_overlap() {
    let r = Rect {
        min: (0.0, 0.0),
        max: (2.0, 2.0),
    };
    // A polygon around the rectangle hits it although no edge crosses.
    let around = ObstacleShape::Polygon(vec![(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)]);
    assert!(around.hits(&r));
    assert!((around.overlap_area(&r) - 4.0).abs() < 1e-9);
    // A triangle half over it.
    let half = ObstacleShape::Polygon(vec![(0.0, 0.0), (2.0, 0.0), (0.0, 2.0)]);
    assert!((half.overlap_area(&r) - 2.0).abs() < 1e-9);
    // Far away: no hit, no area.
    let far = ObstacleShape::Polygon(vec![(10.0, 10.0), (11.0, 10.0), (11.0, 11.0)]);
    assert!(!far.hits(&r));
    assert_eq!(far.overlap_area(&r), 0.0);
    // A segment counts within its half width, touching included.
    let line = ObstacleShape::Segment {
        a: (-1.0, 2.5),
        b: (3.0, 2.5),
        half_width: 0.5,
    };
    assert!(line.hits(&r));
    let line = ObstacleShape::Segment {
        a: (-1.0, 2.6),
        b: (3.0, 2.6),
        half_width: 0.5,
    };
    assert!(!line.hits(&r));
    // A circle counts when the rectangle reaches inside it.
    let c = ObstacleShape::Circle {
        centre: (4.0, 1.0),
        radius: 2.0,
    };
    assert!(!c.hits(&r));
    let c = ObstacleShape::Circle {
        centre: (3.9, 1.0),
        radius: 2.0,
    };
    assert!(c.hits(&r));
}

#[test]
fn placement_module_has_no_crate_imports() {
    // The module is shared with freemapper-courseplan (spec §5.7): it must not reach into the rest of the crate.
    let source = include_str!("mod.rs");
    for forbidden in ["crate::", "super::", "freemapper_print"] {
        assert!(
            !source.contains(forbidden),
            "placement/mod.rs uses {forbidden}"
        );
    }
}

#[test]
fn search_within_a_budget_is_all_or_nothing() {
    let (from, at, to) = ((70.0, 100.0), (100.0, 100.0), (100.0, 140.0));
    let t = target(Some(from), at, Some(to));
    let obstacles = leg_obstacles(from, at, to);
    let cost = evaluations(&t);
    assert_eq!(cost, PROFILE_GAPS.steps().len() * CANDIDATES);
    let mut budget = cost - 1;
    assert_eq!(
        best_within(&t, big_bounds(), &obstacles, START, &mut budget),
        None
    );
    assert_eq!(budget, cost - 1, "nothing spent");
    let mut budget = cost + 5;
    let found = best_within(&t, big_bounds(), &obstacles, START, &mut budget);
    assert_eq!(found, Some(best(&t, big_bounds(), &obstacles, START)));
    assert_eq!(budget, 5);
}

#[test]
fn giving_up_on_losing_candidates_keeps_the_ranking() {
    // Many overprint and map obstacles around a circle, so every candidate hits several: `best` stops scoring a
    // candidate once it is sure to lose, and still finds what scoring every candidate in full ranks first.
    let at = (100.0, 100.0);
    let t = target(Some((70.0, 100.0)), at, Some((100.0, 140.0)));
    let mut obstacles = leg_obstacles((70.0, 100.0), at, (100.0, 140.0));
    for i in 0..400 {
        let a = f64::from(i) * 0.37;
        let r = 3.5 + f64::from(i % 23) * 0.5;
        let c = (at.0 + r * a.cos(), at.1 + r * a.sin());
        let shape = ObstacleShape::Rect(Rect::centred(c, (2.0, 2.0)));
        obstacles.push(if i % 3 == 0 {
            Obstacle {
                shape,
                weight: 0.0,
                tier: Tier::MapSymbol,
                label: format!("m{i}"),
            }
        } else {
            overprint(shape, &format!("o{i}"))
        });
    }
    let (p, s) = best(&t, big_bounds(), &obstacles, START);
    let (preferred, outer) = t.directions(START);
    let key = |d: f64, g: f64, s: &Score| {
        (
            s.hard.len(),
            s.covered.len(),
            angle_diff(d, preferred),
            g,
            angle_diff(d, outer),
            s.cost,
            d,
        )
    };
    let mut all: Vec<_> = PROFILE_GAPS
        .steps()
        .into_iter()
        .flat_map(|g| (0..CANDIDATES).map(move |k| (candidate(preferred, k), g)))
        .map(|(d, g)| {
            let s = score(
                t.box_at(Placement {
                    direction: d,
                    gap_mm: g,
                }),
                big_bounds(),
                &obstacles,
            );
            (key(d, g, &s), d, g, s)
        })
        .collect();
    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let first = &all[0];
    assert!(
        !first.3.hard.is_empty(),
        "the test needs conflicts everywhere"
    );
    assert_eq!((p.direction, p.gap_mm), (first.1, first.2));
    assert_eq!(s, first.3);
}

#[test]
fn segments_that_pass_a_box_by_are_not_hits() {
    // The quick test before the distance: a segment that misses the box grown by its half width is no hit, one
    // that ends inside it is, and the distance decides the rest, as without the quick test.
    let r = Rect {
        min: (0.0, 0.0),
        max: (2.0, 2.0),
    };
    let seg = |a: Pt, b: Pt, half_width: f64| ObstacleShape::Segment { a, b, half_width };
    assert!(!seg((5.0, -10.0), (5.0, 10.0), 0.5).hits(&r));
    assert!(seg((2.4, -10.0), (2.4, 10.0), 0.5).hits(&r));
    assert!(seg((1.0, 1.0), (9.0, 9.0), 0.0).hits(&r));
    // Diagonal past the corner (2, 2): 0.707 mm from it at its closest.
    assert!(!seg((3.0, 2.0), (2.0, 3.0), 0.7).hits(&r));
    assert!(seg((3.0, 2.0), (2.0, 3.0), 0.71).hits(&r));
    for x in [-3.0, -0.5, 0.0, 1.0, 2.0, 2.3, 2.6, 4.0] {
        for hw in [0.0, 0.2, 0.5, 1.0] {
            let s = seg((x, -5.0), (x + 1.0, 7.0), hw);
            let ObstacleShape::Segment { a, b, .. } = s else {
                unreachable!()
            };
            assert_eq!(s.hits(&r), r.distance_to_segment(a, b) <= hw, "{x} {hw}");
        }
    }
}

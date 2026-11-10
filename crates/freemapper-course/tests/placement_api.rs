//! Guards the placement API that freemapper-print builds against (its `numbers.rs` and `rules.rs`). The test
//! builds every item in print's shape: struct literals with all fields and exhaustive matches. A field or
//! variant added to one of these types stops compiling here before it reaches print.

use freemapper_course::placement::{
    BoxGrid, CANDIDATE_STEP_DEG, CANDIDATES, DEFAULT_GAP_MM, GAP_LIMITS_MM, GAP_STEP_MM, GapRange,
    Legs, OUTSIDE, Obstacle, ObstacleShape, Placement, Rect, Target, Tier, angle_diff, best,
    best_within, candidate, direction_to, evaluations, normalize_direction, number_box,
    outer_bisector, placement_of, score,
};

fn tier_name(tier: Tier) -> &'static str {
    match tier {
        Tier::Overprint => "overprint",
        Tier::MapSymbol => "map symbol",
        Tier::Soft => "soft",
    }
}

fn shape_box(shape: &ObstacleShape) -> Rect {
    match shape {
        ObstacleShape::Rect(r) => *r,
        ObstacleShape::Circle { centre, radius } => {
            Rect::centred(*centre, (2.0 * radius, 2.0 * radius))
        }
        ObstacleShape::Segment { a, b, half_width } => Rect {
            min: (a.0.min(b.0), a.1.min(b.1)),
            max: (a.0.max(b.0), a.1.max(b.1)),
        }
        .grown(*half_width),
        ObstacleShape::Polygon(ring) => {
            let first = ring[0];
            Rect {
                min: first,
                max: first,
            }
            .grown(0.0)
        }
    }
}

#[test]
fn print_call_shapes_still_compile() {
    assert_eq!(GapRange::DEFAULT, GapRange { min: 0.5, max: 0.5 });
    assert_eq!(DEFAULT_GAP_MM, 0.5);
    assert_eq!(GAP_STEP_MM, 0.5);
    assert_eq!(GAP_LIMITS_MM, (0.01, 10.0));
    assert_eq!(CANDIDATE_STEP_DEG, 15.0);
    assert_eq!(CANDIDATES, 24);
    assert_eq!(OUTSIDE, "outside");

    // Gap ranges.
    let gaps = GapRange::from_limits(Some(0.5), Some(1.5));
    let allowed = GapRange::allowed(Some(0.5), None);
    assert_eq!(gaps.steps(), vec![0.5, 1.0, 1.5]);
    assert!(gaps.contains(1.0));
    assert!(!allowed.contains(0.1));
    let range = GapRange { min: 0.5, max: 2.0 };

    // Target, with Legs.
    let centre = (0.0, 0.0);
    let target = Target {
        centre,
        circle_outer_r: 3.0,
        size: (2.4, 4.0),
        unit: 1.0,
        legs: Legs {
            incoming: Some(180.0),
            outgoing: vec![centre, (30.0, 0.0)],
            half_line: 0.175,
        },
        gaps: range,
    };
    assert_eq!(evaluations(&target), 4 * CANDIDATES);

    // Obstacles of every tier and shape, plus a box grid over their boxes.
    let obstacle = |shape, tier, label: &str| Obstacle {
        shape,
        weight: 1.0,
        tier,
        label: label.to_string(),
    };
    let obstacles = vec![
        obstacle(
            ObstacleShape::Circle {
                centre: (10.0, 10.0),
                radius: 1.0,
            },
            Tier::Overprint,
            "circle",
        ),
        obstacle(
            ObstacleShape::Segment {
                a: (0.0, 20.0),
                b: (5.0, 20.0),
                half_width: 0.15,
            },
            Tier::MapSymbol,
            "line",
        ),
        obstacle(
            ObstacleShape::Rect(Rect {
                min: (-20.0, -20.0),
                max: (-15.0, -15.0),
            }),
            Tier::Soft,
            "area",
        ),
        obstacle(
            ObstacleShape::Polygon(vec![(20.0, 0.0), (25.0, 0.0), (25.0, 5.0)]),
            Tier::Soft,
            "ring",
        ),
    ];
    let names: Vec<&str> = obstacles.iter().map(|o| tier_name(o.tier)).collect();
    assert_eq!(names, ["overprint", "map symbol", "soft", "soft"]);
    let boxes: Vec<Rect> = obstacles.iter().map(|o| shape_box(&o.shape)).collect();
    let grid = BoxGrid::new(&boxes, 4.0, 512, 64);
    let region = Rect::centred(centre, (0.0, 0.0)).grown(target.reach());
    let near = grid.query(&region);
    assert!(near.iter().all(|&k| k < obstacles.len()));

    // Search and scoring.
    let bounds = Rect {
        min: (-100.0, -100.0),
        max: (100.0, 100.0),
    };
    let current = Placement {
        direction: 45.0,
        gap_mm: 0.5,
    };
    let (found, found_score) = best(&target, bounds, &obstacles, current);
    let mut budget = evaluations(&target);
    let (within, _) = best_within(&target, bounds, &obstacles, current, &mut budget)
        .expect("the budget holds one search");
    assert_eq!(budget, 0);
    assert_eq!(found, within);
    assert!(found_score.is_free());
    assert!(found_score.hard.is_empty() && found_score.covered.is_empty());
    assert_eq!(found_score.cost, 0.0);

    let placed = target.box_at(found);
    let rescored = score(placed, bounds, &obstacles);
    assert!(rescored.is_free());
    assert_eq!(
        score(placed, Rect::centred(centre, (1.0, 1.0)), &obstacles).hard,
        vec![OUTSIDE.to_string()]
    );

    // Geometry helpers.
    let b = number_box(centre, 3.0, (2.4, 4.0), found);
    let back = placement_of(centre, 3.0, b.centre(), (2.4, 4.0));
    assert!(angle_diff(back.direction, found.direction) < 1e-6);
    assert_eq!(normalize_direction(-90.0), 270.0);
    assert_eq!(direction_to(centre, (0.0, -1.0)), Some(90.0));
    assert_eq!(angle_diff(350.0, 10.0), 20.0);
    assert_eq!(outer_bisector(Some(0.0), Some(90.0)), Some(225.0));
    assert_eq!(candidate(10.0, 1), 25.0);
}

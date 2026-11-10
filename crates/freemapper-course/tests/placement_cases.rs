//! The shared number placement cases (`tests/fixtures/placement/cases.toml`, format described in its header)
//! run through `place_course_numbers`. freemapper-courseplan copies the file and runs it through its own
//! `auto_place`.

use freemapper_course::placement::{
    CourseNumbers, GapRange, NumberStop, Obstacle, ObstacleShape, Placement, Rect, Tier,
    place_course_numbers,
};
use serde::Deserialize;

const CASES: &str = include_str!("fixtures/placement/cases.toml");
const TOLERANCE: f64 = 1e-9;
/// Half width of a map point and of a map line, in coordinate mm (not scaled).
const POINT_HALF_WIDTH: f64 = 0.5;
const LINE_HALF_WIDTH: f64 = 0.15;

type Pt = (f64, f64);

#[derive(Deserialize)]
struct File {
    format: u32,
    case: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    map_kind: String,
    map_scale: u32,
    start: Pt,
    finish: Pt,
    #[serde(default)]
    start_bends: Vec<Pt>,
    gap_min: Option<f64>,
    gap_max: Option<f64>,
    #[serde(default)]
    stops: Vec<StopSpec>,
    #[serde(default)]
    map_points: Vec<Pt>,
    #[serde(default)]
    map_lines: Vec<Vec<Pt>>,
    expected: Option<Vec<Expected>>,
}

#[derive(Deserialize)]
struct StopSpec {
    at: Pt,
    #[serde(default)]
    crossing: bool,
    kept: Option<Spot>,
    #[serde(default)]
    bends: Vec<Pt>,
    repeat: Option<usize>,
    step: Option<Pt>,
}

#[derive(Deserialize, Clone, Copy)]
struct Spot {
    direction: f64,
    gap: f64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Expected {
    Word(String),
    At(Spot),
}

/// One stop after the expansion of `repeat`.
struct Stop {
    at: Pt,
    crossing: bool,
    kept: Option<Spot>,
    bends: Vec<Pt>,
}

fn expand(case: &Case) -> Vec<Stop> {
    let mut stops = Vec::new();
    for spec in &case.stops {
        let n = spec.repeat.unwrap_or(1);
        assert!(n >= 1, "{}: repeat must be at least 1", case.name);
        if n > 1 {
            assert!(
                spec.kept.is_none() && spec.bends.is_empty(),
                "{}: kept and bends do not combine with repeat",
                case.name
            );
        }
        let step = spec.step.unwrap_or((0.0, 0.0));
        for i in 0..n {
            let f = i as f64;
            stops.push(Stop {
                at: (spec.at.0 + f * step.0, spec.at.1 + f * step.1),
                crossing: spec.crossing,
                kept: spec.kept,
                bends: spec.bends.clone(),
            });
        }
    }
    stops
}

/// Print sizes of a map kind: circle diameter, line width, number height, base scale.
fn sizes(kind: &str) -> (f64, f64, f64, f64) {
    match kind {
        "sprint" => (6.0, 0.35, 4.0, 4000.0),
        "forest" => (5.0, 0.35, 4.0, 15000.0),
        other => panic!("unknown map_kind {other}"),
    }
}

fn digits(mut n: u32) -> u32 {
    let mut d = 1;
    while n >= 10 {
        n /= 10;
        d += 1;
    }
    d
}

fn map_obstacle(a: Pt, b: Pt, half_width: f64, label: String) -> Obstacle {
    Obstacle {
        shape: ObstacleShape::Segment { a, b, half_width },
        weight: 0.0,
        tier: Tier::MapSymbol,
        label,
    }
}

/// The placements of a case, one per stop (crossing points `None`).
fn run(case: &Case) -> Vec<Option<Placement>> {
    let (diameter, line, height, base) = sizes(&case.map_kind);
    let factor = base / f64::from(case.map_scale);
    let stops = expand(case);

    let mut number = 0;
    let number_stops: Vec<NumberStop> = stops
        .iter()
        .map(|s| {
            assert!(
                !(s.crossing && s.kept.is_some()),
                "{}: a crossing point cannot keep a number",
                case.name
            );
            let size = (!s.crossing).then(|| {
                number += 1;
                (
                    0.6 * height * f64::from(digits(number)) * factor,
                    height * factor,
                )
            });
            NumberStop {
                centre: s.at,
                size,
                kept: s.kept.map(|k| Placement {
                    direction: k.direction,
                    gap_mm: k.gap,
                }),
            }
        })
        .collect();

    // Leg k arrives at stop k; the bends of a leg sit on the stop it leaves.
    let mut legs: Vec<Vec<Pt>> = Vec::with_capacity(stops.len() + 1);
    let mut from = case.start;
    let mut bends: &[Pt] = &case.start_bends;
    for s in &stops {
        let mut leg = vec![from];
        leg.extend_from_slice(bends);
        leg.push(s.at);
        legs.push(leg);
        from = s.at;
        bends = &s.bends;
    }
    let mut last = vec![from];
    last.extend_from_slice(bends);
    last.push(case.finish);
    legs.push(last);

    let mut map = Vec::new();
    for (i, p) in case.map_points.iter().enumerate() {
        map.push(map_obstacle(
            *p,
            *p,
            POINT_HALF_WIDTH,
            format!("map-point-{i}"),
        ));
    }
    for (i, line) in case.map_lines.iter().enumerate() {
        for w in line.windows(2) {
            map.push(map_obstacle(
                w[0],
                w[1],
                LINE_HALF_WIDTH,
                format!("map-line-{i}"),
            ));
        }
    }

    let course = CourseNumbers {
        stops: &number_stops,
        legs: &legs,
        circle_r: diameter * factor / 2.0,
        half_line: line * factor / 2.0,
        unit: factor,
        gaps: GapRange::from_limits(case.gap_min, case.gap_max),
    };
    place_course_numbers(&course, &map, Rect::UNBOUNDED)
}

fn load() -> File {
    let file: File = toml::from_str(CASES).expect("cases.toml parses");
    assert_eq!(file.format, 1, "unknown case format");
    file
}

fn spot_text(p: &Option<Placement>) -> String {
    match p {
        Some(p) => format!("{{ direction = {:?}, gap = {:?} }}", p.direction, p.gap_mm),
        None => "\"none\"".to_string(),
    }
}

#[test]
fn placement_cases_match_expected() {
    let file = load();
    if let Ok(path) = std::env::var("PLACEMENT_CASES_DUMP") {
        let mut text = String::from(
            "# Results of place_course_numbers per case; one entry per stop.\nformat = 1\n",
        );
        for case in &file.case {
            let placements: Vec<String> = run(case).iter().map(spot_text).collect();
            text.push_str(&format!(
                "\n[[result]]\nname = {:?}\nplacements = [{}]\n",
                case.name,
                placements.join(", ")
            ));
        }
        std::fs::write(&path, text).expect("dump written");
        return;
    }
    for case in &file.case {
        let Some(expected) = &case.expected else {
            continue;
        };
        let got = run(case);
        assert_eq!(
            got.len(),
            expected.len(),
            "{}: one expected entry per stop",
            case.name
        );
        for (k, (g, e)) in got.iter().zip(expected).enumerate() {
            match (g, e) {
                (None, Expected::Word(w)) if w == "none" => {}
                (Some(g), Expected::At(e)) => {
                    assert!(
                        (g.direction - e.direction).abs() <= TOLERANCE
                            && (g.gap_mm - e.gap).abs() <= TOLERANCE,
                        "{} stop {k}: got direction {} gap {}, expected {} {}",
                        case.name,
                        g.direction,
                        g.gap_mm,
                        e.direction,
                        e.gap
                    );
                }
                _ => panic!(
                    "{} stop {k}: number presence differs from expected",
                    case.name
                ),
            }
        }
    }
}

#[test]
fn cases_have_unique_names() {
    let file = load();
    let mut names: Vec<&str> = file.case.iter().map(|c| c.name.as_str()).collect();
    assert!(names.len() >= 24, "all scenarios present");
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "case names are unique");
}

#[test]
fn crossing_points_have_no_placement_and_the_rest_do() {
    for case in &load().case {
        let stops = expand(case);
        let got = run(case);
        assert_eq!(got.len(), stops.len(), "{}", case.name);
        for (s, g) in stops.iter().zip(&got) {
            assert_eq!(s.crossing, g.is_none(), "{}", case.name);
        }
    }
}

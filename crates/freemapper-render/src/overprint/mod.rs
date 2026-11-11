//! The course overprint: one course as purple shapes in page millimetres, split into the lower and the
//! upper purple of IOF "Printing and Colour Definitions" Rev. 4 (spec §5.9, ADR 0034). The PDF writer paints
//! both layers in the course colour with the blend mode Multiply.

mod symbols;

use std::collections::{BTreeSet, HashMap};
use std::f64::consts::PI;

pub use crate::page::Overprint;
use crate::page::{DirectColour, Primitive, Pt, RenderColour};
use freemapper_course::{
    Control, ControlKind, Course, CourseFilter, CourseKind, FittedEvent, Special, Stop,
};
use freemapper_omap::MapCoord;

/// Sizes of the course planning symbols (7xx) in mm at the base scale of a map kind. Circle
/// diameters run from line centre to line centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CourseSymbolSizes {
    /// Control circle (703).
    pub circle_diameter_mm: f64,
    /// Lines of circles, legs, start and finish (705).
    pub line_width_mm: f64,
    /// Side of the start triangle (701).
    pub start_side_mm: f64,
    /// Inner finish circle (706).
    pub finish_inner_mm: f64,
    /// Outer finish circle (706).
    pub finish_outer_mm: f64,
    /// Height of a control number (704).
    pub number_height_mm: f64,
    /// Dash of a marked route (707).
    pub marked_route_dash_mm: f64,
    /// Gap of a marked route (707).
    pub marked_route_gap_mm: f64,
    /// Width of an out-of-bounds boundary (708).
    pub out_of_bounds_line_mm: f64,
    /// Length of a crossing point (710).
    pub crossing_point_mm: f64,
    /// Direction of the hatch lines of an out-of-bounds area (709), in degrees counter-clockwise from
    /// east on paper.
    pub out_of_bounds_hatch_angle_degrees: f64,
    /// Direction of a second set of hatch lines, for a cross hatch; `None` for one set.
    pub out_of_bounds_hatch_second_angle_degrees: Option<f64>,
    /// Distance between the centres of two hatch lines.
    pub out_of_bounds_hatch_spacing_mm: f64,
    /// Width of a hatch line.
    pub out_of_bounds_hatch_line_mm: f64,
    /// Length of each bar of a first aid post (712).
    pub first_aid_mm: f64,
    /// Width of a bar of a first aid post.
    pub first_aid_bar_mm: f64,
    /// Outer width and height of a refreshment point (713).
    pub refreshment_mm: f64,
    /// Line width of a refreshment point.
    pub refreshment_line_mm: f64,
}

/// The course colour of IOF Rev. 4, CMYK 35/85/0/0, until a rule set gives its own ([`on_map`]).
pub const IOF_PURPLE: [f64; 4] = [0.35, 0.85, 0.0, 0.0];

/// The overprint `o` in `purple` (the rule set's course colour), with its lower layer placed among the page's
/// map `primitives` by [`full_strength_priority`] of the `colours` they paint in. Colours no primitive uses do
/// not count: map symbol sets carry the course purple itself as their topmost colour, and it would put the
/// lower layer over everything.
pub fn on_map(
    o: Overprint,
    purple: [f64; 4],
    colours: &[RenderColour],
    primitives: &[Primitive],
) -> Overprint {
    let used: BTreeSet<i32> = primitives.iter().map(|p| p.priority).collect();
    let painted: Vec<RenderColour> = colours
        .iter()
        .filter(|c| used.contains(&c.priority))
        .cloned()
        .collect();
    Overprint {
        colour: DirectColour { cmyk: purple },
        below_priority: full_strength_priority(&painted),
        ..o
    }
}

/// Most hatch lines, dashes and crosses one overprint draws. An A3 page needs a few thousand; a special
/// object spanning kilometres of map would need millions. What lies beyond is left out and
/// [`Overprint::cut`] is set.
pub const MAX_REPEATS: usize = 200_000;

/// A full-strength map colour (IOF Rev. 4: lower purple lies below these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullStrength {
    Black,
    Brown,
    Blue,
}

/// How far each CMYK component may lie from a full-strength colour of [`FULL_STRENGTH_CMYK`].
const CMYK_TOLERANCE: f64 = 0.02;

/// The full-strength colours by CMYK: black, blue, and brown of IOF Rev. 4 and of Rev. 3.
const FULL_STRENGTH_CMYK: [(FullStrength, [f64; 4]); 4] = [
    (FullStrength::Black, [0.0, 0.0, 0.0, 1.0]),
    (FullStrength::Blue, [1.0, 0.0, 0.0, 0.0]),
    (FullStrength::Brown, [0.25, 0.75, 1.0, 0.0]),
    (FullStrength::Brown, [0.0, 0.56, 1.0, 0.18]),
];

/// The full-strength class a canonical colour name gives: "black", "brown" or "blue", alone or followed by
/// " 100%" or " 100 %", ignoring case and surrounding white space. Duplicates such as "Black for open land"
/// are not canonical.
pub fn canonical(name: &str) -> Option<FullStrength> {
    let name = name.trim().to_lowercase();
    [
        ("black", FullStrength::Black),
        ("brown", FullStrength::Brown),
        ("blue", FullStrength::Blue),
    ]
    .into_iter()
    .find_map(|(word, class)| {
        let rest = name.strip_prefix(word)?;
        matches!(rest, "" | " 100%" | " 100 %").then_some(class)
    })
}

/// The full-strength class of `cmyk`: within ±0.02 of black 0/0/0/100, blue 100/0/0/0, or brown 25/75/100/0
/// (Rev. 4) or 0/56/100/18 (Rev. 3).
pub fn by_cmyk(cmyk: [f64; 4]) -> Option<FullStrength> {
    FULL_STRENGTH_CMYK
        .iter()
        .find(|(_, want)| {
            want.iter()
                .zip(cmyk)
                .all(|(w, v)| (w - v).abs() <= CMYK_TOLERANCE)
        })
        .map(|(class, _)| *class)
}

/// The full-strength class of a map colour (ADR 0034): by its [`canonical`] name, else [`by_cmyk`].
pub fn full_strength(name: &str, cmyk: [f64; 4]) -> Option<FullStrength> {
    canonical(name).or_else(|| by_cmyk(cmyk))
}

/// The priority before whose primitives the lower purple goes (ADR 0034): the highest priority number,
/// painted first, among the colours with a [`canonical`] name. Only when no colour has one, the highest
/// among the colours [`by_cmyk`]. Pass the colours the page paints ([`on_map`]). Duplicates
/// deeper in the order ("Black for open land") then lie under the purple, and so do the fills painted
/// between them and the canonical colour; `None` when no colour is full strength.
pub fn full_strength_priority(colours: &[RenderColour]) -> Option<i32> {
    let deepest = |pick: &dyn Fn(&RenderColour) -> bool| {
        colours.iter().filter(|c| pick(c)).map(|c| c.priority).max()
    };
    deepest(&|c| canonical(&c.name).is_some()).or_else(|| deepest(&|c| by_cmyk(c.cmyk).is_some()))
}

/// Whether a stop of effective kind `kind` at `control` carries a control number.
fn numbered(kind: ControlKind, control: &Control<MapCoord>) -> bool {
    kind == ControlKind::Control
        || (kind == ControlKind::MapExchange && control.kind == ControlKind::Control)
}

/// The numbered stops of `course` with their control and text: a normal course counts its controls from 1,
/// a score course shows the control codes.
pub fn labels<'a>(
    course: &'a Course<MapCoord>,
    event: &'a FittedEvent,
) -> Vec<(usize, &'a Control<MapCoord>, String)> {
    let controls: HashMap<&str, &Control<MapCoord>> = event
        .controls
        .iter()
        .map(|c| (c.code.as_str(), c))
        .collect();
    let mut out = Vec::new();
    for (i, stop) in course.stops.iter().enumerate() {
        let Some(control) = controls.get(stop.control.as_str()) else {
            continue;
        };
        if !numbered(stop.kind.unwrap_or(control.kind), control) {
            continue;
        }
        let text = match course.kind {
            CourseKind::Normal => (out.len() + 1).to_string(),
            CourseKind::Score => control.code.clone(),
        };
        out.push((i, *control, text));
    }
    out
}

/// How many controls `course` has: its numbered stops, so start, finish, crossing points and ends of marked
/// routes do not count.
pub fn control_count(course: &Course<MapCoord>, event: &FittedEvent) -> usize {
    labels(course, event).len()
}

/// The width and height of the box of control number `text` with digits `height` tall (Arimo regular), as
/// [`overprint`] draws it.
pub fn number_size(text: &str, height: f64) -> (f64, f64) {
    let (_, w, h) = symbols::number_metrics(text, height);
    (w, h)
}

/// The course symbols of `course` on the page, as obstacles for control numbers: per stop on the map its index,
/// centre and the radius out to the outer edge of its line (control circle, outer finish circle, the circle
/// through the corners of a start or map exchange triangle, half a crossing point). Ends of marked routes draw
/// nothing and are left out.
pub fn stop_symbols(
    course: &Course<MapCoord>,
    event: &FittedEvent,
    to_page: impl Fn(MapCoord) -> Pt,
    sizes: &CourseSymbolSizes,
    course_factor: f64,
) -> Vec<(usize, Pt, f64)> {
    let s = Scaled::new(sizes, course_factor);
    placed_stops(course, event, &to_page)
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let p = p.as_ref()?;
            let r = match p.kind {
                ControlKind::EndOfMarkedRoute => return None,
                ControlKind::CrossingPoint => s.crossing / 2.0,
                _ => p.radius(&s),
            };
            Some((i, p.at, r + s.line / 2.0))
        })
        .collect()
}

/// The legs of a normal course as drawn, gaps left in: per leg its index and the line from where it leaves its
/// first stop's symbol to where it reaches the second's. Empty for a score course.
pub fn leg_lines(
    course: &Course<MapCoord>,
    event: &FittedEvent,
    to_page: impl Fn(MapCoord) -> Pt,
    sizes: &CourseSymbolSizes,
    course_factor: f64,
) -> Vec<(usize, Vec<Pt>)> {
    if course.kind != CourseKind::Normal {
        return Vec::new();
    }
    let s = Scaled::new(sizes, course_factor);
    let placed = placed_stops(course, event, &to_page);
    let at = |i: usize| placed.get(i).and_then(Option::as_ref);
    let mut out = Vec::new();
    for (i, leg) in course.legs.iter().enumerate() {
        let (Some(a), Some(b)) = (at(i), at(i + 1)) else {
            continue;
        };
        let mut points = vec![a.at];
        points.extend(leg.bends.iter().map(|c| to_page(*c)));
        points.push(b.at);
        let total = symbols::length(&points);
        let from = symbols::exit_length(&points, a.radius(&s));
        let reversed: Vec<Pt> = points.iter().rev().copied().collect();
        let to = total - symbols::exit_length(&reversed, b.radius(&s));
        if to > from {
            out.extend(
                symbols::pieces(&points, from, to, &[])
                    .into_iter()
                    .map(|piece| (i, piece)),
            );
        }
    }
    out
}

/// The stops of `course` on the page, `None` for a stop whose control is unknown or has no position.
fn placed_stops<'a>(
    course: &'a Course<MapCoord>,
    event: &'a FittedEvent,
    to_page: &impl Fn(MapCoord) -> Pt,
) -> Vec<Option<Placed<'a>>> {
    let controls: HashMap<&str, &Control<MapCoord>> = event
        .controls
        .iter()
        .map(|c| (c.code.as_str(), c))
        .collect();
    course
        .stops
        .iter()
        .map(|stop| {
            let control = *controls.get(stop.control.as_str())?;
            Some(Placed {
                at: to_page(control.position?),
                kind: stop.kind.unwrap_or(control.kind),
                control,
                stop,
            })
        })
        .collect()
}

/// The course symbol sizes on the page: the profile's sizes times the course factor, radii from
/// diameters.
struct Scaled {
    circle_r: f64,
    line: f64,
    start_side: f64,
    finish_inner_r: f64,
    finish_outer_r: f64,
    number_height: f64,
    dash: f64,
    gap: f64,
    boundary: f64,
    crossing: f64,
    /// Width, spacing and directions (radians, counter-clockwise on paper) of the out-of-bounds and
    /// dangerous-area hatches.
    hatch_line: f64,
    hatch_spacing: f64,
    hatch_angles: Vec<f64>,
    first_aid: f64,
    first_aid_bar: f64,
    refreshment: f64,
    refreshment_line: f64,
    /// Distance between the crosses of a forbidden route.
    forbidden_spacing: f64,
    /// Length of each arm of a forbidden-route cross.
    forbidden_cross: f64,
}

/// Distance (mm) between the centres of the crosses along a forbidden route: the middle of the 4.0 to 6.0 mm
/// that ISOM 2017-2 section 3.7, symbol 711, allows.
const FORBIDDEN_CROSS_SPACING_MM: f64 = 5.0;

/// Length (mm) of each of the two arms of a forbidden-route cross, ISOM 2017-2 section 3.7, symbol 711, as OOM's
/// ISOM 2017-2 set draws it (arms from (-1.061, 1.061) to (1.061, -1.061) mm): the cross spans 2.1 mm. ISSprOM
/// 2019-2 has no symbol 711; sprint maps take the same size.
const FORBIDDEN_CROSS_MM: f64 = 3.0;

impl Scaled {
    fn new(sizes: &CourseSymbolSizes, f: f64) -> Self {
        Scaled {
            circle_r: sizes.circle_diameter_mm / 2.0 * f,
            line: sizes.line_width_mm * f,
            start_side: sizes.start_side_mm * f,
            finish_inner_r: sizes.finish_inner_mm / 2.0 * f,
            finish_outer_r: sizes.finish_outer_mm / 2.0 * f,
            number_height: sizes.number_height_mm * f,
            dash: sizes.marked_route_dash_mm * f,
            gap: sizes.marked_route_gap_mm * f,
            boundary: sizes.out_of_bounds_line_mm * f,
            crossing: sizes.crossing_point_mm * f,
            hatch_line: sizes.out_of_bounds_hatch_line_mm * f,
            hatch_spacing: sizes.out_of_bounds_hatch_spacing_mm * f,
            hatch_angles: std::iter::once(sizes.out_of_bounds_hatch_angle_degrees)
                .chain(sizes.out_of_bounds_hatch_second_angle_degrees)
                .map(f64::to_radians)
                .collect(),
            first_aid: sizes.first_aid_mm * f,
            first_aid_bar: sizes.first_aid_bar_mm * f,
            refreshment: sizes.refreshment_mm * f,
            refreshment_line: sizes.refreshment_line_mm * f,
            forbidden_spacing: FORBIDDEN_CROSS_SPACING_MM * f,
            forbidden_cross: FORBIDDEN_CROSS_MM * f,
        }
    }
}

/// A stop drawn on the page.
struct Placed<'a> {
    at: Pt,
    kind: ControlKind,
    control: &'a Control<MapCoord>,
    stop: &'a Stop,
}

impl Placed<'_> {
    /// Where a leg starts or ends: the circle line, the start triangle's corner, the outer finish circle,
    /// or the centre of a crossing point or the end of a marked route.
    fn radius(&self, s: &Scaled) -> f64 {
        match self.kind {
            ControlKind::Start => symbols::triangle_radius(s.start_side),
            ControlKind::Control => s.circle_r,
            ControlKind::Finish => s.finish_outer_r,
            ControlKind::CrossingPoint | ControlKind::EndOfMarkedRoute => 0.0,
            ControlKind::MapExchange if self.control.kind == ControlKind::Control => s.circle_r,
            ControlKind::MapExchange => symbols::triangle_radius(s.start_side),
        }
    }
}

/// Whether a special object with `filter` belongs to the course named `course`.
pub fn applies(filter: &CourseFilter, course: &str) -> bool {
    match filter {
        CourseFilter::All => true,
        CourseFilter::Only(names) => names.iter().any(|n| n == course),
    }
}

/// The overprint of `course`: its stops, legs, the specials that apply to it and the control numbers at
/// `numbers` (control code and text centre in page mm, matched to the numbered stops in order; a stop
/// without an entry gets no number). the app's number placer gives the centres. `to_page` maps a map
/// coordinate to page mm (the map frame and the page share the y direction). Sizes are `sizes` × `course_factor`,
/// the course factor of the print scale (the app's rule set); leg gaps and offsets scale with the geometry
/// factor `to_page` applies. Returns `below_priority` unset and the IOF purple: [`on_map`] sets both for a page.
pub fn overprint(
    course: &Course<MapCoord>,
    event: &FittedEvent,
    numbers: &[(String, Pt)],
    to_page: impl Fn(MapCoord) -> Pt,
    sizes: &CourseSymbolSizes,
    course_factor: f64,
) -> Overprint {
    let s = Scaled::new(sizes, course_factor);
    // The geometry factor, from the transform itself: page mm per map mm.
    let geometry = symbols::dist(
        to_page(MapCoord::new(0, 0)),
        to_page(MapCoord::new(1000, 0)),
    );
    let mut budget = symbols::Budget::new(MAX_REPEATS);
    let mut o = Overprint {
        colour: DirectColour { cmyk: IOF_PURPLE },
        ..Overprint::default()
    };
    let placed = placed_stops(course, event, &to_page);
    let at = |i: usize| placed.get(i).and_then(Option::as_ref);
    // The page points of leg `i`: from its first stop through its bends to its second.
    let leg_points = |i: usize| -> Option<Vec<Pt>> {
        let leg = course.legs.get(i)?;
        let mut points = vec![at(i)?.at];
        points.extend(leg.bends.iter().map(|b| to_page(*b)));
        points.push(at(i + 1)?.at);
        Some(points)
    };

    for (i, p) in placed.iter().enumerate() {
        let Some(p) = p else { continue };
        // The direction a start or map exchange triangle points in: along the leg to the next stop (to its
        // first bend, if it has one), else north.
        let ahead = leg_points(i)
            .and_then(|l| symbols::direction(l[0], l[1]))
            .unwrap_or(-PI / 2.0);
        match p.kind {
            ControlKind::Start => {
                o.lower
                    .push(symbols::triangle(p.at, s.start_side, ahead, s.line))
            }
            ControlKind::Control => o.lower.extend(symbols::circle(
                p.at,
                s.circle_r,
                s.line,
                &p.stop.circle_gaps,
            )),
            ControlKind::Finish => {
                for r in [s.finish_inner_r, s.finish_outer_r] {
                    o.lower
                        .extend(symbols::circle(p.at, r, s.line, &p.stop.circle_gaps));
                }
            }
            ControlKind::CrossingPoint => {
                // The way through runs along the course: from the point before to the point after.
                let before = i
                    .checked_sub(1)
                    .and_then(leg_points)
                    .and_then(|l| l.get(l.len() - 2).copied());
                let after = leg_points(i).map(|l| l[1]);
                let angle = match (before, after) {
                    (Some(b), Some(a)) => symbols::direction(b, a),
                    (Some(b), None) => symbols::direction(b, p.at),
                    (None, Some(a)) => symbols::direction(p.at, a),
                    (None, None) => None,
                };
                o.lower.extend(symbols::crossing_point(
                    p.at,
                    s.crossing,
                    angle.unwrap_or(0.0),
                    s.line,
                ));
            }
            ControlKind::EndOfMarkedRoute => {}
            ControlKind::MapExchange => {
                if p.control.kind == ControlKind::Control {
                    o.lower.extend(symbols::circle(
                        p.at,
                        s.circle_r,
                        s.line,
                        &p.stop.circle_gaps,
                    ));
                }
                o.upper
                    .push(symbols::triangle(p.at, s.start_side, ahead, s.line));
            }
        }
    }

    if course.kind == CourseKind::Normal {
        for (i, leg) in course.legs.iter().enumerate() {
            let (Some(points), Some(a), Some(b)) = (leg_points(i), at(i), at(i + 1)) else {
                continue;
            };
            let total = symbols::length(&points);
            // Where the path leaves each circle: a bend inside a circle is not drawn.
            let from = symbols::exit_length(&points, a.radius(&s));
            let reversed: Vec<Pt> = points.iter().rev().copied().collect();
            let to = total - symbols::exit_length(&reversed, b.radius(&s));
            if to <= from {
                continue;
            }
            let gaps: Vec<(f64, f64)> = leg
                .gaps
                .iter()
                .map(|(x, y)| (x * geometry, y * geometry))
                .collect();
            for piece in symbols::pieces(&points, from, to, &gaps) {
                if leg.marked_route {
                    o.upper
                        .extend(symbols::dashes(&piece, s.dash, s.gap, s.line, &mut budget));
                } else {
                    o.lower.push(symbols::line(piece, s.line));
                }
            }
        }
    }

    for special in &event.specials {
        special_shapes(
            special,
            &course.name,
            &to_page,
            &s,
            geometry,
            &mut budget,
            &mut o,
        );
    }

    let mut used = vec![false; numbers.len()];
    for (_, control, text) in labels(course, event) {
        let found = numbers
            .iter()
            .enumerate()
            .position(|(j, (code, _))| !used[j] && *code == control.code);
        if let Some(j) = found {
            used[j] = true;
            o.upper
                .push(symbols::number(&text, numbers[j].1, s.number_height));
        }
    }
    o.cut = budget.cut;
    o
}

/// Adds the shapes of `special` to `o` when it applies to `course`.
fn special_shapes(
    special: &Special<MapCoord>,
    course: &str,
    to_page: &impl Fn(MapCoord) -> Pt,
    s: &Scaled,
    geometry: f64,
    budget: &mut symbols::Budget,
    o: &mut Overprint,
) {
    let path = |p: &[MapCoord]| -> Vec<Pt> { p.iter().map(|c| to_page(*c)).collect() };
    let rings = |r: &[Vec<MapCoord>]| -> Vec<Vec<Pt>> { r.iter().map(|v| path(v)).collect() };
    match special {
        Special::OutOfBounds {
            rings: r, courses, ..
        } if applies(courses, course) => {
            // The profile's angles run counter-clockwise on paper; the page's y points down.
            let angles: Vec<f64> = s.hatch_angles.iter().map(|a| -a).collect();
            o.upper.extend(symbols::hatch(
                rings(r),
                &angles,
                s.hatch_spacing,
                s.hatch_line,
                budget,
            ));
        }
        Special::Dangerous {
            rings: r, courses, ..
        } if applies(courses, course) => {
            // Always a cross hatch, with the line width and spacing of the out-of-bounds area, as Purple
            // Pen draws it.
            o.upper.extend(symbols::hatch(
                rings(r),
                &[PI / 4.0, -PI / 4.0],
                s.hatch_spacing,
                s.hatch_line,
                budget,
            ));
        }
        // Upper purple: ISOM 2017-2 section 3.7, 711 Out-of-bounds route (ISSprOM 2019-2 has no 711). The
        // "forbidden lines" of Rev. 4's lower list are the boundaries 707/708, `Special::Boundary` here.
        Special::ForbiddenRoute {
            path: p, courses, ..
        } if applies(courses, course) => {
            let points = path(p);
            match points[..] {
                [] => {}
                // Purple Pen's forbidden crossing: one cross.
                [one] => o
                    .upper
                    .extend(symbols::cross(one, s.forbidden_cross, 0.0, s.line)),
                // Crosses along the route, without a line between them.
                _ => o.upper.extend(symbols::crosses_along(
                    &points,
                    s.forbidden_cross,
                    s.forbidden_spacing,
                    s.line,
                    budget,
                )),
            }
        }
        Special::Boundary {
            path: p, courses, ..
        } if applies(courses, course) && p.len() > 1 => {
            o.lower.push(symbols::line(path(p), s.boundary));
        }
        Special::MarkedRoute {
            path: p, courses, ..
        } if applies(courses, course) && p.len() > 1 => {
            o.upper
                .extend(symbols::dashes(&path(p), s.dash, s.gap, s.line, budget));
        }
        // Lower purple: ISOM 2017-2 section 3.7, 712 First aid post and 713 Refreshment point (ISSprOM 2019-2
        // has neither).
        Special::FirstAid { pt, courses, .. } if applies(courses, course) => {
            o.lower.extend(symbols::first_aid(
                to_page(*pt),
                s.first_aid,
                s.first_aid_bar,
            ));
        }
        Special::Water { pt, courses, .. } if applies(courses, course) => {
            o.lower.extend(symbols::water(
                to_page(*pt),
                s.refreshment,
                s.refreshment_line,
            ));
        }
        Special::Text {
            pt,
            text,
            size_mm,
            courses,
            ..
        } if applies(courses, course) => {
            o.upper
                .extend(symbols::text_lines(text, to_page(*pt), size_mm * geometry));
        }
        Special::Line {
            path: p, courses, ..
        } if applies(courses, course) && p.len() > 1 => {
            o.lower.push(symbols::line(path(p), s.line));
        }
        // White-out becomes a mask in M4.
        _ => {}
    }
}

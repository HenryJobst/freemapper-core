// SPDX-License-Identifier: AGPL-3.0-or-later

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{FillRule, Shape};
    use freemapper_course::{CourseSource, Leg, SourceFormat};

    /// Page mm from map units (1/1000 mm each): the scale 1:1, geometry factor 1.
    fn page(c: MapCoord) -> Pt {
        (f64::from(c.x) / 1000.0, f64::from(c.y) / 1000.0)
    }

    /// The same map at half size on the page, geometry factor 0.5.
    fn half(c: MapCoord) -> Pt {
        (f64::from(c.x) / 2000.0, f64::from(c.y) / 2000.0)
    }

    fn sizes() -> CourseSymbolSizes {
        CourseSymbolSizes {
            circle_diameter_mm: 6.0,
            line_width_mm: 0.5,
            start_side_mm: 7.0,
            finish_inner_mm: 4.0,
            finish_outer_mm: 6.0,
            number_height_mm: 4.0,
            marked_route_dash_mm: 2.0,
            marked_route_gap_mm: 0.5,
            out_of_bounds_line_mm: 0.7,
            crossing_point_mm: 4.0,
            out_of_bounds_hatch_angle_degrees: 0.0,
            out_of_bounds_hatch_second_angle_degrees: None,
            out_of_bounds_hatch_spacing_mm: 2.0,
            out_of_bounds_hatch_line_mm: 0.3,
            first_aid_mm: 4.0,
            first_aid_bar_mm: 1.0,
            refreshment_mm: 4.0,
            refreshment_line_mm: 0.4,
        }
    }

    fn control(code: &str, kind: ControlKind, at_mm: Option<(i32, i32)>) -> Control<MapCoord> {
        Control {
            id: None,
            code: code.to_string(),
            kind,
            position: at_mm.map(|(x, y)| MapCoord::new(x * 1000, y * 1000)),
            geo: None,
            description: None,
        }
    }

    fn stop(code: &str) -> Stop {
        Stop {
            control: code.to_string(),
            kind: None,
            number: None,
            number_text_at: None,
            circle_gaps: Vec::new(),
            map_flip: false,
        }
    }

    fn leg() -> Leg<MapCoord> {
        Leg {
            bends: Vec::new(),
            gaps: Vec::new(),
            marked_route: false,
        }
    }

    fn course(kind: CourseKind, codes: &[&str]) -> Course<MapCoord> {
        Course {
            id: None,
            name: "Course A".to_string(),
            family: None,
            kind,
            legs: codes.iter().skip(1).map(|_| leg()).collect(),
            stops: codes.iter().map(|c| stop(c)).collect(),
            climb_m: None,
            length_m: None,
            print_scale_hint: None,
            course_obj_ratio: None,
            secondary_title: None,
        }
    }

    fn event(controls: Vec<Control<MapCoord>>, specials: Vec<Special<MapCoord>>) -> FittedEvent {
        FittedEvent {
            controls,
            courses: Vec::new(),
            specials,
            source: CourseSource {
                format: SourceFormat::PurplePen,
                event_name: None,
                event_date: None,
                organiser: None,
                map_scale: None,
                item_scaling: None,
            },
            notes: Vec::new(),
        }
    }

    /// Start at the origin, controls 31 and 32 east and south-east of it, finish below the start.
    fn square() -> FittedEvent {
        event(
            vec![
                control("S1", ControlKind::Start, Some((0, 0))),
                control("31", ControlKind::Control, Some((50, 0))),
                control("32", ControlKind::Control, Some((50, 50))),
                control("F1", ControlKind::Finish, Some((0, 50))),
            ],
            Vec::new(),
        )
    }

    fn colour(priority: i32, name: &str, cmyk: [f64; 4]) -> RenderColour {
        RenderColour {
            priority,
            name: name.to_string(),
            cmyk,
            rgb: [0.0; 3],
            opacity: 1.0,
        }
    }

    fn primitive(priority: i32) -> Primitive {
        Primitive::new(
            priority,
            Shape::Fill {
                rings: Vec::new(),
                rule: FillRule::EvenOdd,
            },
        )
    }

    fn path_of(shape: &Shape) -> &[Pt] {
        match shape {
            Shape::Stroke { path, .. } => path,
            other => panic!("not a stroke: {other:?}"),
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn texts(shapes: &[Shape]) -> Vec<(&str, Pt)> {
        shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text { text, origin, .. } => Some((text.as_str(), *origin)),
                _ => None,
            })
            .collect()
    }

    fn all() -> CourseFilter {
        CourseFilter::All
    }

    /// The shared placement cases of freemapper-course assume this width per digit and height.
    #[test]
    fn number_size_is_digits_times_the_shared_digit_width() {
        for (text, digits) in [("1", 1.0), ("10", 2.0), ("100", 3.0)] {
            let (w, h) = number_size(text, 4.0);
            assert!(
                (w - 0.808_374_733_853_797_1 * 4.0 * digits).abs() < 1e-9,
                "{text}: {w}"
            );
            assert_eq!(h, 4.0);
        }
    }

    #[test]
    fn canonical_names_ignore_case_and_space_and_take_an_optional_100_percent() {
        assert_eq!(canonical("Black"), Some(FullStrength::Black));
        assert_eq!(canonical("  BROWN 100% "), Some(FullStrength::Brown));
        assert_eq!(canonical("blue 100 %"), Some(FullStrength::Blue));
        // Duplicates and tints are not canonical.
        assert_eq!(canonical("Black for open land"), None);
        assert_eq!(canonical("Black 50%"), None);
        assert_eq!(canonical("Blue 100"), None);
        assert_eq!(canonical("Olive green"), None);
        assert_eq!(canonical(""), None);
    }

    #[test]
    fn cmyk_classes_hold_within_the_tolerance_and_not_beyond() {
        assert_eq!(by_cmyk([0.0, 0.0, 0.0, 1.0]), Some(FullStrength::Black));
        assert_eq!(by_cmyk([1.0, 0.0, 0.0, 0.0]), Some(FullStrength::Blue));
        // Brown of Rev. 4 and of Rev. 3.
        assert_eq!(by_cmyk([0.25, 0.75, 1.0, 0.0]), Some(FullStrength::Brown));
        assert_eq!(by_cmyk([0.0, 0.56, 1.0, 0.18]), Some(FullStrength::Brown));
        // 0.019 away in every component still counts, 0.03 in one does not.
        assert_eq!(
            by_cmyk([0.019, 0.019, 0.019, 0.981]),
            Some(FullStrength::Black)
        );
        assert_eq!(by_cmyk([0.03, 0.0, 0.0, 1.0]), None);
        assert_eq!(by_cmyk([0.0, 0.0, 0.0, 0.97]), None);
        // A 50 % tint of black and the course purple are not full strength.
        assert_eq!(by_cmyk([0.0, 0.0, 0.0, 0.5]), None);
        assert_eq!(by_cmyk(IOF_PURPLE), None);
    }

    #[test]
    fn the_name_decides_before_the_cmyk_values() {
        // A canonical name wins even when the values say another class.
        assert_eq!(
            full_strength("Blue", [0.0, 0.0, 0.0, 1.0]),
            Some(FullStrength::Blue)
        );
        // No canonical name: the values decide.
        assert_eq!(
            full_strength("Black for open land", [0.0, 0.0, 0.0, 1.0]),
            Some(FullStrength::Black)
        );
        assert_eq!(full_strength("Yellow", [0.0, 0.0, 1.0, 0.0]), None);
    }

    #[test]
    fn the_lower_purple_goes_before_the_deepest_canonical_colour() {
        let colours = [
            colour(2, "Black", [0.0, 0.0, 0.0, 1.0]),
            colour(5, "Black for open land", [0.0, 0.0, 0.0, 1.0]),
            colour(9, "Brown", [0.25, 0.75, 1.0, 0.0]),
            colour(12, "Yellow", [0.0, 0.0, 1.0, 0.0]),
        ];
        // The duplicate at 5 does not count while a canonical name exists; 9 is the highest canonical number.
        assert_eq!(full_strength_priority(&colours), Some(9));
    }

    #[test]
    fn the_cmyk_values_decide_only_when_no_colour_has_a_canonical_name() {
        let colours = [
            colour(3, "Dark", [0.0, 0.0, 0.0, 1.0]),
            colour(7, "Dark duplicate", [0.0, 0.0, 0.0, 1.0]),
            colour(9, "Yellow", [0.0, 0.0, 1.0, 0.0]),
        ];
        assert_eq!(full_strength_priority(&colours), Some(7));
        assert_eq!(full_strength_priority(&colours[2..]), None);
        assert_eq!(full_strength_priority(&[]), None);
    }

    #[test]
    fn on_map_counts_only_colours_the_page_paints() {
        let colours = [
            colour(2, "Black", [0.0, 0.0, 0.0, 1.0]),
            // The map set's own purple, the topmost colour, unused on this page.
            colour(20, "Blue", [1.0, 0.0, 0.0, 0.0]),
        ];
        let base = Overprint {
            cut: true,
            lower: vec![Shape::Fill {
                rings: Vec::new(),
                rule: FillRule::NonZero,
            }],
            ..Overprint::default()
        };
        let purple = [0.1, 0.9, 0.0, 0.0];
        let o = on_map(base, purple, &colours, &[primitive(2), primitive(2)]);
        assert_eq!(o.below_priority, Some(2));
        assert_eq!(o.colour.cmyk, purple);
        // The rest of the overprint is kept.
        assert!(o.cut);
        assert_eq!(o.lower.len(), 1);
        // With the blue painted, it is the deepest full-strength colour.
        let o = on_map(Overprint::default(), purple, &colours, &[primitive(20)]);
        assert_eq!(o.below_priority, Some(20));
        // Nothing painted in a full-strength colour: no priority.
        let o = on_map(Overprint::default(), purple, &colours, &[primitive(7)]);
        assert_eq!(o.below_priority, None);
    }

    #[test]
    fn a_special_applies_to_all_courses_or_to_the_named_ones() {
        assert!(applies(&CourseFilter::All, "Course A"));
        let only = CourseFilter::Only(vec!["Course B".into(), "Course A".into()]);
        assert!(applies(&only, "Course A"));
        assert!(!applies(&only, "Course C"));
        assert!(!applies(&CourseFilter::Only(Vec::new()), "Course A"));
    }

    #[test]
    fn a_normal_course_counts_its_controls_from_one_and_skips_start_and_finish() {
        let event = square();
        let course = course(CourseKind::Normal, &["S1", "31", "32", "F1"]);
        let labels = labels(&course, &event);
        let seen: Vec<(usize, &str, &str)> = labels
            .iter()
            .map(|(i, c, t)| (*i, c.code.as_str(), t.as_str()))
            .collect();
        assert_eq!(seen, [(1, "31", "1"), (2, "32", "2")]);
        assert_eq!(control_count(&course, &event), 2);
    }

    #[test]
    fn a_score_course_shows_the_codes_and_a_visited_control_is_numbered_each_time() {
        let event = square();
        let score = course(CourseKind::Score, &["S1", "32", "31", "F1"]);
        let texts: Vec<String> = labels(&score, &event).into_iter().map(|l| l.2).collect();
        assert_eq!(texts, ["32", "31"]);
        let again = course(CourseKind::Normal, &["31", "32", "31"]);
        let texts: Vec<String> = labels(&again, &event).into_iter().map(|l| l.2).collect();
        assert_eq!(texts, ["1", "2", "3"]);
    }

    #[test]
    fn unknown_controls_are_skipped_and_do_not_use_up_a_number() {
        let event = square();
        let course = course(CourseKind::Normal, &["31", "99", "32"]);
        let seen: Vec<(usize, String)> = labels(&course, &event)
            .into_iter()
            .map(|(i, _, t)| (i, t))
            .collect();
        assert_eq!(seen, [(0, "1".to_string()), (2, "2".to_string())]);
    }

    #[test]
    fn a_stop_kind_overrides_the_kind_of_its_control() {
        let event = square();
        let mut course = course(CourseKind::Normal, &["31", "32", "F1"]);
        // The control 32 is the finish of this course, the finish F1 a plain control.
        course.stops[1].kind = Some(ControlKind::Finish);
        course.stops[2].kind = Some(ControlKind::Control);
        let seen: Vec<(usize, String)> = labels(&course, &event)
            .into_iter()
            .map(|(i, _, t)| (i, t))
            .collect();
        assert_eq!(seen, [(0, "1".to_string()), (2, "2".to_string())]);
    }

    #[test]
    fn a_map_exchange_is_numbered_only_when_its_control_is_a_control() {
        let event = event(
            vec![
                control("31", ControlKind::Control, Some((10, 10))),
                control("S2", ControlKind::Start, Some((20, 20))),
            ],
            Vec::new(),
        );
        let mut course = course(CourseKind::Normal, &["31", "S2"]);
        course.stops[0].kind = Some(ControlKind::MapExchange);
        course.stops[1].kind = Some(ControlKind::MapExchange);
        let seen: Vec<usize> = labels(&course, &event).into_iter().map(|l| l.0).collect();
        assert_eq!(seen, [0]);
    }

    #[test]
    fn stop_symbols_reach_to_the_outer_edge_of_each_line() {
        let mut event = square();
        event
            .controls
            .push(control("X1", ControlKind::CrossingPoint, Some((25, 25))));
        event
            .controls
            .push(control("E1", ControlKind::EndOfMarkedRoute, Some((30, 30))));
        event
            .controls
            .push(control("N1", ControlKind::Control, None));
        let course = course(
            CourseKind::Normal,
            &["S1", "31", "32", "F1", "X1", "E1", "N1", "99"],
        );
        let s = stop_symbols(&course, &event, page, &sizes(), 1.0);
        let by_index = |i: usize| s.iter().find(|x| x.0 == i).copied();
        // Start: circumscribed circle of the triangle plus half the line.
        let (_, at, r) = by_index(0).unwrap();
        assert_eq!(at, (0.0, 0.0));
        assert!(near(r, 7.0 / 3f64.sqrt() + 0.25));
        // Control: circle radius plus half the line; finish: outer circle.
        assert!(near(by_index(1).unwrap().2, 3.25));
        assert_eq!(by_index(2).unwrap().1, (50.0, 50.0));
        assert!(near(by_index(3).unwrap().2, 3.25));
        // Crossing point: half its length.
        assert!(near(by_index(4).unwrap().2, 2.25));
        // The end of a marked route, a control without a position and an unknown control draw nothing.
        assert!(by_index(5).is_none());
        assert!(by_index(6).is_none());
        assert!(by_index(7).is_none());
        // The course factor scales every size.
        let big = stop_symbols(&course, &event, page, &sizes(), 2.0);
        assert!(near(big.iter().find(|x| x.0 == 1).unwrap().2, 6.5));
    }

    #[test]
    fn a_map_exchange_takes_the_radius_of_its_control() {
        let event = event(
            vec![
                control("31", ControlKind::Control, Some((10, 10))),
                control("S2", ControlKind::Start, Some((20, 20))),
            ],
            Vec::new(),
        );
        let mut course = course(CourseKind::Normal, &["31", "S2"]);
        course.stops[0].kind = Some(ControlKind::MapExchange);
        course.stops[1].kind = Some(ControlKind::MapExchange);
        let s = stop_symbols(&course, &event, page, &sizes(), 1.0);
        assert!(near(s[0].2, 3.25));
        assert!(near(s[1].2, 7.0 / 3f64.sqrt() + 0.25));
    }

    #[test]
    fn leg_lines_run_from_circle_to_circle() {
        let event = square();
        let course = course(CourseKind::Normal, &["S1", "31", "32", "F1"]);
        let legs = leg_lines(&course, &event, page, &sizes(), 1.0);
        assert_eq!(legs.len(), 3);
        // First leg: from the corner of the start triangle to the circle of control 31.
        let (i, line) = &legs[0];
        assert_eq!(*i, 0);
        assert!(near(line[0].0, 7.0 / 3f64.sqrt()));
        assert!(near(line.last().unwrap().0, 47.0));
        // Last leg ends on the outer finish circle, 3 mm from the finish.
        let (i, line) = &legs[2];
        assert_eq!(*i, 2);
        assert!(near(line[0].0, 47.0) && near(line[0].1, 50.0), "{line:?}");
        assert!(near(line.last().unwrap().0, 3.0));
    }

    #[test]
    fn a_score_course_has_no_leg_lines() {
        let event = square();
        let course = course(CourseKind::Score, &["S1", "31", "32"]);
        assert!(leg_lines(&course, &event, page, &sizes(), 1.0).is_empty());
    }

    #[test]
    fn touching_circles_leave_no_leg_and_a_bend_inside_a_circle_is_not_drawn() {
        let event = event(
            vec![
                control("31", ControlKind::Control, Some((0, 0))),
                control("32", ControlKind::Control, Some((5, 0))),
                control("33", ControlKind::Control, Some((60, 0))),
            ],
            Vec::new(),
        );
        // 5 mm apart with circles of radius 3 mm: they overlap.
        let close = course(CourseKind::Normal, &["31", "32"]);
        assert!(leg_lines(&close, &event, page, &sizes(), 1.0).is_empty());
        let o = overprint(&close, &event, &[], page, &sizes(), 1.0);
        assert_eq!(o.lower.len(), 2, "two circles and no leg");
        // A bend inside the first circle: the line starts where the path leaves the circle.
        let mut bent = course(CourseKind::Normal, &["31", "33"]);
        bent.legs[0].bends = vec![MapCoord::new(1000, 1000)];
        let legs = leg_lines(&bent, &event, page, &sizes(), 1.0);
        assert_eq!(legs.len(), 1);
        let line = &legs[0].1;
        assert!(line.iter().all(|p| *p != (1.0, 1.0)), "{line:?}");
        let first = line[0];
        assert!(near(first.0.hypot(first.1), 3.0), "{first:?}");
    }

    #[test]
    fn a_leg_gap_is_cut_out_in_page_distance() {
        let event = event(
            vec![
                control("31", ControlKind::Control, Some((0, 0))),
                control("32", ControlKind::Control, Some((100, 0))),
            ],
            Vec::new(),
        );
        let mut c = course(CourseKind::Normal, &["31", "32"]);
        // 20 to 30 mm along the leg in file mm; the page shows the map at half size.
        c.legs[0].gaps = vec![(20.0, 30.0)];
        let o = overprint(&c, &event, &[], half, &sizes(), 1.0);
        let lines: Vec<&Shape> = o.lower.iter().filter(|s| !is_closed(s)).collect();
        // Circles are closed strokes; the two leg pieces are the open ones.
        let ends: Vec<(f64, f64)> = lines
            .iter()
            .map(|s| (path_of(s)[0].0, path_of(s).last().unwrap().0))
            .collect();
        assert_eq!(ends.len(), 2, "{ends:?}");
        assert!(near(ends[0].0, 3.0) && near(ends[0].1, 10.0), "{ends:?}");
        assert!(near(ends[1].0, 15.0) && near(ends[1].1, 47.0), "{ends:?}");
    }

    fn is_closed(s: &Shape) -> bool {
        matches!(s, Shape::Stroke { closed: true, .. })
    }

    fn square_course() -> Course<MapCoord> {
        course(CourseKind::Normal, &["S1", "31", "32", "F1"])
    }

    #[test]
    fn the_overprint_of_a_course_draws_each_stop_and_leg_in_the_lower_purple() {
        let event = square();
        let o = overprint(&square_course(), &event, &[], page, &sizes(), 1.0);
        assert_eq!(o.colour.cmyk, IOF_PURPLE);
        assert_eq!(o.below_priority, None);
        assert!(!o.cut);
        // Start triangle, two control circles, two finish circles and three legs.
        assert_eq!(o.lower.len(), 1 + 2 + 2 + 3);
        assert_eq!(o.lower.iter().filter(|s| is_closed(s)).count(), 5);
        assert!(o.upper.is_empty(), "no numbers were placed");
        // The triangle points along the first leg, east.
        let triangle = path_of(&o.lower[0]);
        assert_eq!(triangle.len(), 3);
        assert!(near(triangle[0].0, 7.0 / 3f64.sqrt()) && near(triangle[0].1, 0.0));
    }

    #[test]
    fn a_lone_start_points_north() {
        let event = square();
        let c = course(CourseKind::Normal, &["S1"]);
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        let t = path_of(&o.lower[0]);
        assert!(
            near(t[0].0, 0.0) && near(t[0].1, -7.0 / 3f64.sqrt()),
            "{t:?}"
        );
    }

    #[test]
    fn the_course_factor_scales_every_symbol() {
        let event = square();
        let one = overprint(&square_course(), &event, &[], page, &sizes(), 1.0);
        let two = overprint(&square_course(), &event, &[], page, &sizes(), 2.0);
        let width = |s: &Shape| match s {
            Shape::Stroke { width, .. } => *width,
            _ => panic!("not a stroke"),
        };
        assert!(near(width(&one.lower[0]), 0.5));
        assert!(near(width(&two.lower[0]), 1.0));
        // The control circle's radius, from the first point of its closed path.
        let r = |o: &Overprint| path_of(&o.lower[1])[0].0 - 50.0;
        assert!(near(r(&one), 3.0) && near(r(&two), 6.0));
    }

    #[test]
    fn a_score_course_draws_no_legs() {
        let event = square();
        let c = course(CourseKind::Score, &["S1", "31", "32", "F1"]);
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        assert_eq!(o.lower.len(), 5, "only the symbols of the stops");
    }

    #[test]
    fn stops_without_a_known_placed_control_draw_nothing_and_cut_their_legs() {
        let mut event = square();
        event
            .controls
            .push(control("N1", ControlKind::Control, None));
        let c = course(CourseKind::Normal, &["31", "N1", "99", "32"]);
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        // Two circles only: every leg touches a stop that is not on the page.
        assert_eq!(o.lower.len(), 2);
    }

    #[test]
    fn a_marked_route_leg_is_dashed_in_the_upper_purple() {
        let event = square();
        let plain = overprint(&square_course(), &event, &[], page, &sizes(), 1.0);
        let mut c = square_course();
        c.legs[0].marked_route = true;
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        assert_eq!(o.lower.len(), plain.lower.len() - 1);
        // 4.04 .. 47 mm in dashes of 2 mm with 0.5 mm gaps.
        let from = 7.0 / 3f64.sqrt();
        let dashes = ((47.0 - from) / 2.5).ceil() as usize;
        assert_eq!(o.upper.len(), dashes);
        assert!(near(path_of(&o.upper[0])[0].0, from));
    }

    #[test]
    fn control_numbers_use_the_placed_centres_once_each() {
        let event = square();
        let c = course(CourseKind::Normal, &["S1", "31", "32", "31", "F1"]);
        let numbers = [
            ("31".to_string(), (10.0, 20.0)),
            ("31".to_string(), (30.0, 40.0)),
            ("99".to_string(), (1.0, 1.0)),
        ];
        let o = overprint(&c, &event, &numbers, page, &sizes(), 1.0);
        let placed = texts(&o.upper);
        // 32 has no entry, so it gets no number; the two visits of 31 take the two entries in order.
        assert_eq!(placed.len(), 2);
        let (w1, _) = number_size("1", 4.0);
        let (w3, _) = number_size("3", 4.0);
        assert_eq!(placed[0].0, "1");
        assert!(near(placed[0].1.0, 10.0 - w1 / 2.0) && near(placed[0].1.1, 22.0));
        assert_eq!(placed[1].0, "3");
        assert!(near(placed[1].1.0, 30.0 - w3 / 2.0) && near(placed[1].1.1, 42.0));
    }

    #[test]
    fn a_score_course_numbers_carry_the_control_codes() {
        let event = square();
        let c = course(CourseKind::Score, &["S1", "32", "F1"]);
        let numbers = [("32".to_string(), (5.0, 5.0))];
        let o = overprint(&c, &event, &numbers, page, &sizes(), 1.0);
        assert_eq!(texts(&o.upper)[0].0, "32");
    }

    fn event_vertical() -> FittedEvent {
        event(
            vec![
                control("31", ControlKind::Control, Some((50, -50))),
                control("X1", ControlKind::CrossingPoint, Some((50, 0))),
                control("32", ControlKind::Control, Some((50, 50))),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn a_crossing_point_lies_across_the_way_through() {
        let event = event(
            vec![
                control("31", ControlKind::Control, Some((0, 0))),
                control("X1", ControlKind::CrossingPoint, Some((50, 0))),
                control("32", ControlKind::Control, Some((100, 0))),
                control("33", ControlKind::Control, Some((100, 100))),
            ],
            Vec::new(),
        );
        let extent = |o: &Overprint, from: usize| -> (f64, f64) {
            let (mut dx, mut dy) = (0f64, 0f64);
            for shape in &o.lower[from..from + 2] {
                for p in path_of(shape) {
                    dx = dx.max((p.0 - 50.0).abs());
                    dy = dy.max(p.1.abs());
                }
            }
            (dx, dy)
        };
        // Along the way east, the two lines run 4 mm along x and bulge a little along y.
        let c = course(CourseKind::Normal, &["31", "X1", "32"]);
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        // Circle of 31, the crossing point (2 lines), circle of 32, then the legs.
        assert_eq!(o.lower.iter().filter(|s| is_closed(s)).count(), 2);
        let (dx, dy) = extent(&o, 1);
        assert!(near(dx, 2.0), "{dx}");
        assert!(dy > 0.5 && dy < 2.0, "{dy}");
        // Along a way south the extents swap.
        let event = event_vertical();
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        let (mut dx, mut dy) = (0f64, 0f64);
        for shape in &o.lower[1..3] {
            for p in path_of(shape) {
                dx = dx.max((p.0 - 50.0).abs());
                dy = dy.max(p.1.abs());
            }
        }
        assert!(dx > 0.5 && dx < 2.0, "{dx}");
        assert!(near(dy, 2.0), "{dy}");
    }

    #[test]
    fn a_map_exchange_is_a_triangle_in_the_upper_purple_and_a_circle_when_its_control_is_one() {
        let event = event(
            vec![
                control("31", ControlKind::Control, Some((0, 0))),
                control("S2", ControlKind::Start, Some((50, 0))),
            ],
            Vec::new(),
        );
        let mut c = course(CourseKind::Normal, &["31", "S2"]);
        c.stops[0].kind = Some(ControlKind::MapExchange);
        c.stops[1].kind = Some(ControlKind::MapExchange);
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        // The circle of the control (lower) and two triangles (upper); the leg joins the circle and the triangle.
        assert_eq!(o.upper.len(), 2);
        assert!(o.upper.iter().all(|s| path_of(s).len() == 3));
        assert_eq!(o.lower.iter().filter(|s| is_closed(s)).count(), 1);
    }

    #[test]
    fn circle_gaps_of_a_stop_open_its_circle() {
        let event = square();
        let mut c = square_course();
        c.stops[1].circle_gaps = vec![(-30.0, 30.0)];
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        // The circle of 31 is now one open arc.
        assert_eq!(o.lower.iter().filter(|s| is_closed(s)).count(), 4);
        // Finish: both circles take the gaps of their stop.
        c.stops[3].circle_gaps = vec![(90.0, 180.0)];
        let o = overprint(&c, &event, &[], page, &sizes(), 1.0);
        assert_eq!(o.lower.iter().filter(|s| is_closed(s)).count(), 2);
    }

    fn square_ring() -> Vec<Vec<MapCoord>> {
        vec![vec![
            MapCoord::new(0, 0),
            MapCoord::new(10_000, 0),
            MapCoord::new(10_000, 10_000),
            MapCoord::new(0, 10_000),
        ]]
    }

    fn with_specials(specials: Vec<Special<MapCoord>>) -> Overprint {
        let event = event(
            vec![control("31", ControlKind::Control, Some((50, 50)))],
            specials,
        );
        let c = course(CourseKind::Normal, &["31"]);
        overprint(&c, &event, &[], page, &sizes(), 1.0)
    }

    #[test]
    fn a_special_belongs_only_to_the_courses_it_names() {
        let oob = |courses| Special::OutOfBounds {
            id: None,
            rings: square_ring(),
            courses,
        };
        // The base has the control circle in the lower layer, nothing in the upper one.
        assert!(!with_specials(vec![oob(all())]).upper.is_empty());
        assert!(
            !with_specials(vec![oob(CourseFilter::Only(vec!["Course A".into()]))])
                .upper
                .is_empty()
        );
        assert!(
            with_specials(vec![oob(CourseFilter::Only(vec!["Course B".into()]))])
                .upper
                .is_empty()
        );
    }

    #[test]
    fn an_out_of_bounds_area_is_hatched_counter_clockwise_on_paper() {
        let hatched = |angle: f64, second: Option<f64>| {
            let mut sz = sizes();
            sz.out_of_bounds_hatch_angle_degrees = angle;
            sz.out_of_bounds_hatch_second_angle_degrees = second;
            let event = event(
                Vec::new(),
                vec![Special::OutOfBounds {
                    id: None,
                    rings: square_ring(),
                    courses: all(),
                }],
            );
            overprint(
                &course(CourseKind::Normal, &[]),
                &event,
                &[],
                page,
                &sz,
                1.0,
            )
        };
        // A 10 mm square, lines 2 mm apart: 0, 2, ..., 10 is six lines.
        assert_eq!(hatched(0.0, None).upper.len(), 6);
        assert_eq!(hatched(0.0, Some(90.0)).upper.len(), 12);
        // 45 degrees counter-clockwise on paper rises to the right: x grows while the page's y falls.
        let o = hatched(45.0, None);
        let Shape::Clipped { inner, .. } = &o.upper[0] else {
            panic!("a hatch line is clipped to the area");
        };
        let p = path_of(inner);
        let (dx, dy) = (p[1].0 - p[0].0, p[1].1 - p[0].1);
        assert!(dx > 0.0 && dy < 0.0, "{dx} {dy}");
        assert!(near(dx, -dy));
        // Every line of the area shares the clip.
        let Shape::Clipped { rings, .. } = &o.upper[1] else {
            panic!("clipped");
        };
        assert_eq!(rings.len(), 1);
    }

    #[test]
    fn a_dangerous_area_is_always_cross_hatched() {
        let o = with_specials(vec![Special::Dangerous {
            id: None,
            rings: square_ring(),
            courses: all(),
        }]);
        let mut slopes = (false, false);
        for s in &o.upper {
            let Shape::Clipped { inner, .. } = s else {
                panic!("clipped")
            };
            let p = path_of(inner);
            let (dx, dy) = (p[1].0 - p[0].0, p[1].1 - p[0].1);
            assert!(near(dx.abs(), dy.abs()), "{dx} {dy}");
            if dx * dy > 0.0 {
                slopes.0 = true;
            } else {
                slopes.1 = true;
            }
        }
        assert_eq!(slopes, (true, true));
    }

    #[test]
    fn a_forbidden_route_is_one_cross_or_crosses_along_the_path() {
        let route = |path: Vec<MapCoord>| {
            with_specials(vec![Special::ForbiddenRoute {
                id: None,
                path,
                courses: all(),
            }])
            .upper
        };
        assert!(route(Vec::new()).is_empty());
        // Purple Pen's forbidden crossing: a path of one point is one cross of two strokes.
        assert_eq!(route(vec![MapCoord::new(5000, 5000)]).len(), 2);
        // 30 mm with a cross every 5 mm: six crosses of two strokes.
        let line = vec![MapCoord::new(0, 0), MapCoord::new(30_000, 0)];
        assert_eq!(route(line).len(), 12);
    }

    #[test]
    fn boundaries_and_lines_need_two_points_and_marked_routes_are_dashed() {
        let path = |n: usize| -> Vec<MapCoord> {
            (0..n)
                .map(|i| MapCoord::new(i as i32 * 20_000, 0))
                .collect()
        };
        let base = with_specials(Vec::new()).lower.len();
        let lower = |sp: Special<MapCoord>| with_specials(vec![sp]).lower.len() - base;
        let boundary = |n| Special::Boundary {
            id: None,
            path: path(n),
            courses: all(),
        };
        let line = |n| Special::Line {
            id: None,
            path: path(n),
            courses: all(),
        };
        assert_eq!(lower(boundary(1)), 0);
        assert_eq!(lower(boundary(2)), 1);
        assert_eq!(lower(line(1)), 0);
        assert_eq!(lower(line(3)), 1);
        // The boundary takes its own width, the plain line the line width.
        let width = |sp: Special<MapCoord>| match with_specials(vec![sp]).lower.last().unwrap() {
            Shape::Stroke { width, .. } => *width,
            _ => panic!("stroke"),
        };
        assert!(near(width(boundary(2)), 0.7));
        assert!(near(width(line(2)), 0.5));
        let marked = |n| Special::MarkedRoute {
            id: None,
            path: path(n),
            courses: all(),
        };
        assert!(with_specials(vec![marked(1)]).upper.is_empty());
        // 20 mm in dashes of 2 mm every 2.5 mm: eight dashes.
        assert_eq!(with_specials(vec![marked(2)]).upper.len(), 8);
    }

    #[test]
    fn first_aid_water_and_text_go_to_their_layers() {
        let base = with_specials(Vec::new());
        let pt = MapCoord::new(10_000, 10_000);
        let aid = with_specials(vec![Special::FirstAid {
            id: None,
            pt,
            courses: all(),
        }]);
        assert_eq!(aid.lower.len() - base.lower.len(), 2, "two bars");
        let water = with_specials(vec![Special::Water {
            id: None,
            pt,
            courses: all(),
        }]);
        assert_eq!(
            water.lower.len() - base.lower.len(),
            4,
            "the cup's four strokes"
        );
        let text = |text: &str| {
            with_specials(vec![Special::Text {
                id: None,
                pt,
                text: text.to_string(),
                size_mm: 3.0,
                courses: all(),
            }])
        };
        let t = text("Water\n\nahead");
        assert_eq!(texts(&t.upper).len(), 2, "the empty line is left out");
        // The text size takes the geometry factor of the transform: here 1, so 3 mm.
        let Shape::Text { size, .. } = &t.upper[0] else {
            panic!("text")
        };
        assert!(near(*size, 3.0));
        let halved = {
            let event = event(
                Vec::new(),
                vec![Special::Text {
                    id: None,
                    pt,
                    text: "x".into(),
                    size_mm: 3.0,
                    courses: all(),
                }],
            );
            overprint(
                &course(CourseKind::Normal, &[]),
                &event,
                &[],
                half,
                &sizes(),
                1.0,
            )
        };
        let Shape::Text { size, .. } = &halved.upper[0] else {
            panic!("text")
        };
        assert!(near(*size, 1.5));
        assert!(text("Water").lower.len() == base.lower.len());
    }

    #[test]
    fn a_white_out_is_not_drawn_yet() {
        let o = with_specials(vec![Special::WhiteOut {
            id: None,
            rings: square_ring(),
            courses: all(),
        }]);
        assert_eq!(o, with_specials(Vec::new()));
    }

    #[test]
    fn a_hostile_hatch_stops_at_the_repeat_limit_and_says_so() {
        // 1000 mm square hatched every 0.002 mm would be 500 000 lines.
        let mut sz = sizes();
        sz.out_of_bounds_hatch_spacing_mm = 0.002;
        let ring = vec![vec![
            MapCoord::new(0, 0),
            MapCoord::new(1_000_000, 0),
            MapCoord::new(1_000_000, 1_000_000),
            MapCoord::new(0, 1_000_000),
        ]];
        let event = event(
            Vec::new(),
            vec![Special::OutOfBounds {
                id: None,
                rings: ring,
                courses: all(),
            }],
        );
        let o = overprint(
            &course(CourseKind::Normal, &[]),
            &event,
            &[],
            page,
            &sz,
            1.0,
        );
        assert!(o.cut);
        assert_eq!(o.upper.len(), MAX_REPEATS);
        assert!(!with_specials(Vec::new()).cut);
    }
}

//! The numbers of a whole course, placed in running order. A port of freemapper-courseplan's `auto_place`
//! (its ADR 0030 to 0032): kept numbers stay and block the others, each placed number blocks the ones after
//! it, and the circles and legs of the course are overprint obstacles. Uses only the parent module.

use super::{
    BoxGrid, EPS, GapRange, Legs, NO_LEGS_DIRECTION_DEG, Obstacle, ObstacleShape, Placement, Pt,
    Rect, Target, Tier, best, direction_to, trim_start,
};

/// The grid over the map obstacles has at most this many cells along a side.
const MAP_GRID_MAX_CELLS: usize = 512;
/// A map obstacle spanning more grid cells than this is offered to every control.
const MAP_GRID_WIDE_CELLS: usize = 64;

/// A stop of a course, for number placement.
#[derive(Debug, Clone, PartialEq)]
pub struct NumberStop {
    /// The circle centre (coordinate mm).
    pub centre: Pt,
    /// Width and height of the number box (coordinate mm). `None` for a stop without number (start, finish,
    /// crossing point): it gets neither a number nor a circle obstacle.
    pub size: Option<(f64, f64)>,
    /// A kept number: it comes back unchanged, and its box is an obstacle for the other numbers.
    pub kept: Option<Placement>,
}

/// A course whose numbers [`place_course_numbers`] places. All lengths are coordinate mm except `gaps`, which
/// holds gap mm (times `unit` on the map).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CourseNumbers<'a> {
    pub stops: &'a [NumberStop],
    /// `stops.len() + 1` polylines with their bends: `legs[k]` arrives at `stops[k]`, `legs[k + 1]` leaves it.
    pub legs: &'a [Vec<Pt>],
    /// The circle radius.
    pub circle_r: f64,
    /// Half the width of the circle and leg lines.
    pub half_line: f64,
    /// Coordinate mm per gap mm, as [`Target::unit`].
    pub unit: f64,
    /// The gaps the search tries.
    pub gaps: GapRange,
}

impl CourseNumbers<'_> {
    fn circle_outer_r(&self) -> f64 {
        self.circle_r + self.half_line
    }

    /// The number of stop `k` as a search target; `None` for a stop without number.
    fn target(&self, k: usize) -> Option<Target> {
        let stop = self.stops.get(k)?;
        let size = stop.size?;
        // The point before the stop on the arriving leg: its last bend, or the previous stop.
        let incoming = self.legs.get(k).and_then(|leg| {
            let i = leg.len().checked_sub(2)?;
            direction_to(stop.centre, leg[i])
        });
        Some(Target {
            centre: stop.centre,
            circle_outer_r: self.circle_outer_r(),
            size,
            unit: self.unit,
            legs: Legs {
                incoming,
                outgoing: self.legs.get(k + 1).cloned().unwrap_or_default(),
                half_line: self.half_line,
            },
            gaps: self.gaps,
        })
    }
}

/// The places of the numbers of `course`, one entry per stop: `None` for a stop without number, the kept
/// placement for a kept number, otherwise the [`best`] placement within `bounds`.
///
/// Kept boxes are obstacles from the start. The other numbers are placed in running order, and each placed
/// box is an obstacle for the ones after it (label `n{k}`). For stop `k` the course adds these overprint
/// obstacles: the circle of every other numbered stop not on the same point (`c{j}`, radius `circle_r +
/// half_line`), and every leg as segments `half_line` wide to each side (`l{i}`). Legs `k` and `k + 1` lose
/// the part within `circle_r + half_line` of the stop. `map` adds sheet elements and map detail; it is put
/// into one grid per call, and each stop sees only the map obstacles near it. Give map obstacles labels other
/// than `c…`, `l…` and `n…` followed by a number.
///
/// A control without legs starts from [`NO_LEGS_DIRECTION_DEG`].
pub fn place_course_numbers(
    course: &CourseNumbers<'_>,
    map: &[Obstacle],
    bounds: Rect,
) -> Vec<Option<Placement>> {
    let scene = Scene::new(course);
    let targets: Vec<Option<Target>> = (0..course.stops.len()).map(|k| course.target(k)).collect();
    let mut numbers: Vec<Obstacle> = targets
        .iter()
        .zip(course.stops)
        .enumerate()
        .filter_map(|(k, (target, stop))| {
            Some(number_obstacle(k, target.as_ref()?.box_at(stop.kept?)))
        })
        .collect();
    // Cells about the size of the largest number box.
    let cell = targets
        .iter()
        .flatten()
        .map(|t| t.size.0.max(t.size.1))
        .fold(0.0, f64::max);
    let boxes: Vec<Rect> = map.iter().map(|o| o.shape.bbox()).collect();
    let grid = BoxGrid::new(&boxes, cell, MAP_GRID_MAX_CELLS, MAP_GRID_WIDE_CELLS);
    let current = Placement {
        direction: NO_LEGS_DIRECTION_DEG,
        gap_mm: course.gaps.min,
    };
    let mut out = Vec::with_capacity(course.stops.len());
    for (k, (target, stop)) in targets.iter().zip(course.stops).enumerate() {
        let Some(target) = target else {
            out.push(None);
            continue;
        };
        if let Some(kept) = stop.kept {
            out.push(Some(kept));
            continue;
        }
        let own = Scene::own_legs(course, k);
        let region = Rect::centred(target.centre, (0.0, 0.0)).grown(target.reach());
        let near = grid.query(&region);
        let obstacles = scene
            .overprint(course, k, &own)
            .chain(&numbers)
            .chain(near.iter().map(|&i| &map[i]));
        let (p, _) = best(target, bounds, obstacles, current);
        numbers.push(number_obstacle(k, target.box_at(p)));
        out.push(Some(p));
    }
    out
}

/// The circles and legs of a course as obstacles, built once per course.
struct Scene {
    /// Per numbered stop: its index, centre and circle.
    circles: Vec<(usize, Pt, Obstacle)>,
    /// Per leg: its segments.
    legs: Vec<Vec<Obstacle>>,
}

impl Scene {
    fn new(course: &CourseNumbers<'_>) -> Scene {
        let radius = course.circle_outer_r();
        let circles = course
            .stops
            .iter()
            .enumerate()
            .filter(|(_, s)| s.size.is_some())
            .map(|(j, s)| {
                let circle = ObstacleShape::Circle {
                    centre: s.centre,
                    radius,
                };
                (j, s.centre, overprint(circle, format!("c{j}")))
            })
            .collect();
        let legs = course
            .legs
            .iter()
            .enumerate()
            .map(|(i, leg)| segments(i, leg, course.half_line))
            .collect();
        Scene { circles, legs }
    }

    /// Legs `k` and `k + 1` without their part within the circle line of stop `k`, as segments.
    fn own_legs(course: &CourseNumbers<'_>, k: usize) -> [Vec<Obstacle>; 2] {
        let trim = course.circle_outer_r();
        let arriving: Vec<Pt> = course
            .legs
            .get(k)
            .map(|leg| leg.iter().rev().copied().collect())
            .unwrap_or_default();
        let leaving = course.legs.get(k + 1).map_or(&[][..], Vec::as_slice);
        [
            segments(k, &trim_start(&arriving, trim), course.half_line),
            segments(k + 1, &trim_start(leaving, trim), course.half_line),
        ]
    }

    /// The circles and legs stop `k` must avoid: no circle of its own point, and its own legs from `own`.
    fn overprint<'o>(
        &'o self,
        course: &CourseNumbers<'_>,
        k: usize,
        own: &'o [Vec<Obstacle>; 2],
    ) -> impl Iterator<Item = &'o Obstacle> {
        let here = course.stops[k].centre;
        let circles = self
            .circles
            .iter()
            .filter(move |(j, at, _)| *j != k && (at.0 - here.0).hypot(at.1 - here.1) >= EPS)
            .map(|(_, _, o)| o);
        let legs = self.legs.iter().enumerate().flat_map(move |(i, segs)| {
            if i == k {
                &own[0]
            } else if i == k + 1 {
                &own[1]
            } else {
                segs
            }
        });
        circles.chain(legs)
    }
}

fn overprint(shape: ObstacleShape, label: String) -> Obstacle {
    Obstacle {
        shape,
        weight: 0.0,
        tier: Tier::Overprint,
        label,
    }
}

/// The segments of leg `i`, one label for the whole leg.
fn segments(i: usize, points: &[Pt], half_width: f64) -> Vec<Obstacle> {
    let label = format!("l{i}");
    points
        .windows(2)
        .map(|w| {
            let segment = ObstacleShape::Segment {
                a: w[0],
                b: w[1],
                half_width,
            };
            overprint(segment, label.clone())
        })
        .collect()
}

/// The box of the number of stop `k` as an obstacle.
fn number_obstacle(k: usize, box_: Rect) -> Obstacle {
    overprint(ObstacleShape::Rect(box_), format!("n{k}"))
}

#[cfg(test)]
mod tests;

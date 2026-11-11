//! The course symbols as shapes in page mm (y down). Sizes come in already multiplied by the symbol
//! factor. The point symbols (crossing point, first aid, water) and the area hatches follow the course
//! symbols of OOM's ISSprOM 2019 and ISOM 2017 sets (freemapper-omap's `assets/symbol-sets/`, the forest
//! test map).

use std::f64::consts::{FRAC_PI_4, PI};
use std::sync::Arc;

use crate::fonts::{self, FontFace};
use crate::geometry::flatten_cubic;
use crate::page::{Cap, Join, Pt, Shape};

/// Segments of a full circle; an arc gets one per this many degrees or part of it.
const CIRCLE_SEGMENTS: usize = 72;
const DEG_PER_SEGMENT: f64 = 360.0 / CIRCLE_SEGMENTS as f64;

/// Counts the repeated elements (hatch lines, dashes, crosses) an overprint draws, so a hostile special
/// object cannot fill memory ([`super::MAX_REPEATS`]).
pub(super) struct Budget {
    left: usize,
    pub(super) cut: bool,
}

impl Budget {
    pub(super) fn new(units: usize) -> Self {
        Budget {
            left: units,
            cut: false,
        }
    }

    /// Takes one unit; `false` (and `cut` set) when none is left.
    fn take(&mut self) -> bool {
        if self.left == 0 {
            self.cut = true;
            return false;
        }
        self.left -= 1;
        true
    }
}

pub(super) fn dist(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The direction from `a` to `b` in radians (page frame: from +x towards +y), `None` when they coincide.
pub(super) fn direction(a: Pt, b: Pt) -> Option<f64> {
    let d = (b.0 - a.0, b.1 - a.1);
    (d.0.hypot(d.1) > 1e-9).then(|| d.1.atan2(d.0))
}

fn polar(centre: Pt, r: f64, angle: f64) -> Pt {
    (centre.0 + r * angle.cos(), centre.1 + r * angle.sin())
}

fn stroke(path: Vec<Pt>, closed: bool, width: f64) -> Shape {
    Shape::Stroke {
        path,
        closed,
        width,
        cap: Cap::Flat,
        join: Join::Miter,
    }
}

/// An open line with flat ends and round bends (legs, routes, boundaries).
pub(super) fn line(path: Vec<Pt>, width: f64) -> Shape {
    Shape::Stroke {
        path,
        closed: false,
        width,
        cap: Cap::Flat,
        join: Join::Round,
    }
}

/// The angle ranges in `[0, 360)` that `gaps` (degrees from +x towards +y) leave of a circle, merged.
fn kept_arcs(gaps: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut cut: Vec<(f64, f64)> = Vec::new();
    for &(a, b) in gaps {
        if !(a.is_finite() && b.is_finite()) || b <= a {
            continue;
        }
        if b - a >= 360.0 {
            return Vec::new();
        }
        let start = a.rem_euclid(360.0);
        let end = start + (b - a);
        if end > 360.0 {
            cut.push((start, 360.0));
            cut.push((0.0, end - 360.0));
        } else {
            cut.push((start, end));
        }
    }
    cut.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut kept = Vec::new();
    let mut at = 0.0;
    for (a, b) in cut {
        if a > at {
            kept.push((at, a));
        }
        at = f64::max(at, b);
    }
    if at < 360.0 {
        kept.push((at, 360.0));
    }
    // An arc across +x: join its two halves.
    if kept.len() > 1 && kept[0].0 == 0.0 && kept[kept.len() - 1].1 == 360.0 {
        let last = kept.pop().unwrap();
        kept[0] = (last.0, kept[0].1 + 360.0);
    }
    kept
}

/// A circle of centre-line radius `r`, without the angle ranges `gaps`: one closed stroke when there are
/// none, else one open arc per piece left.
pub(super) fn circle(centre: Pt, r: f64, width: f64, gaps: &[(f64, f64)]) -> Vec<Shape> {
    let kept = kept_arcs(gaps);
    if kept == [(0.0, 360.0)] {
        let path = (0..CIRCLE_SEGMENTS)
            .map(|i| polar(centre, r, i as f64 * DEG_PER_SEGMENT.to_radians()))
            .collect();
        return vec![stroke(path, true, width)];
    }
    kept.into_iter()
        .map(|(a, b)| {
            let n = ((b - a) / DEG_PER_SEGMENT).ceil().max(1.0) as usize;
            let path = (0..=n)
                .map(|i| polar(centre, r, (a + (b - a) * i as f64 / n as f64).to_radians()))
                .collect();
            stroke(path, false, width)
        })
        .collect()
}

/// The circumradius of an equilateral triangle of side `side`: where a leg leaves the start.
pub(super) fn triangle_radius(side: f64) -> f64 {
    side / 3f64.sqrt()
}

/// An equilateral triangle of side `side` centred on `centre`, a corner pointing at `towards`.
pub(super) fn triangle(centre: Pt, side: f64, towards: f64, width: f64) -> Shape {
    let r = triangle_radius(side);
    let path = (0..3)
        .map(|i| polar(centre, r, towards + f64::from(i) * 2.0 * PI / 3.0))
        .collect();
    stroke(path, true, width)
}

/// Lengths along `points` from its start, one per point.
fn cumulative(points: &[Pt]) -> Vec<f64> {
    let mut cum = Vec::with_capacity(points.len());
    let mut s = 0.0;
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            s += dist(points[i - 1], *p);
        }
        cum.push(s);
    }
    cum
}

pub(super) fn length(points: &[Pt]) -> f64 {
    cumulative(points).last().copied().unwrap_or(0.0)
}

fn point_at(points: &[Pt], cum: &[f64], s: f64) -> Pt {
    let i = cum.partition_point(|&c| c <= s).clamp(1, points.len() - 1);
    let (a, b) = (points[i - 1], points[i]);
    let seg = cum[i] - cum[i - 1];
    let t = if seg > 0.0 {
        (s - cum[i - 1]) / seg
    } else {
        0.0
    };
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// The part of `points` between the lengths `a` and `b` along it.
fn between(points: &[Pt], cum: &[f64], a: f64, b: f64) -> Vec<Pt> {
    let mut out = vec![point_at(points, cum, a)];
    for (p, c) in points.iter().zip(cum) {
        if *c > a && *c < b {
            out.push(*p);
        }
    }
    out.push(point_at(points, cum, b));
    out
}

/// `[from, to]` without the ranges `gaps`.
fn without(from: f64, to: f64, gaps: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut gaps: Vec<(f64, f64)> = gaps
        .iter()
        .copied()
        .filter(|(a, b)| a.is_finite() && b.is_finite() && b > a)
        .collect();
    gaps.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out = Vec::new();
    let mut at = from;
    for (a, b) in gaps {
        if a > at {
            out.push((at, a.min(to)));
        }
        at = at.max(b);
        if at >= to {
            break;
        }
    }
    if at < to {
        out.push((at, to));
    }
    out.retain(|(a, b)| b - a > 1e-9);
    out
}

/// The pieces of `points` from length `from` to length `to`, minus the length ranges `gaps`.
pub(super) fn pieces(points: &[Pt], from: f64, to: f64, gaps: &[(f64, f64)]) -> Vec<Vec<Pt>> {
    let cum = cumulative(points);
    without(from, to, gaps)
        .into_iter()
        .map(|(a, b)| between(points, &cum, a, b))
        .collect()
}

/// `points` as dashes of `dash` with gaps of `gap`, starting with a dash.
pub(super) fn dashes(
    points: &[Pt],
    dash: f64,
    gap: f64,
    width: f64,
    budget: &mut Budget,
) -> Vec<Shape> {
    let cum = cumulative(points);
    let total = cum.last().copied().unwrap_or(0.0);
    let period = dash + gap;
    let mut out = Vec::new();
    if period.is_nan() || period <= 0.0 || dash.is_nan() || dash <= 0.0 {
        return out;
    }
    let mut at = 0.0;
    while at < total - 1e-9 {
        if !budget.take() {
            break;
        }
        out.push(line(
            between(points, &cum, at, (at + dash).min(total)),
            width,
        ));
        at += period;
    }
    out
}

/// An X of two strokes of length `size` across `centre`, at 45° to `angle`.
pub(super) fn cross(centre: Pt, size: f64, angle: f64, width: f64) -> [Shape; 2] {
    let arm = |a: f64| {
        stroke(
            vec![
                polar(centre, size / 2.0, a),
                polar(centre, size / 2.0, a + PI),
            ],
            false,
            width,
        )
    };
    [arm(angle + FRAC_PI_4), arm(angle - FRAC_PI_4)]
}

/// Crosses spread evenly along `points`, one in the middle of each of `n` equal parts, `n` the length
/// divided by `spacing`, rounded, at least one.
pub(super) fn crosses_along(
    points: &[Pt],
    size: f64,
    spacing: f64,
    width: f64,
    budget: &mut Budget,
) -> Vec<Shape> {
    let cum = cumulative(points);
    let total = cum.last().copied().unwrap_or(0.0);
    let n = (total / spacing).round().clamp(1.0, 1e9) as usize;
    let mut out = Vec::new();
    for i in 0..n {
        if !budget.take() {
            break;
        }
        let s = total * (i as f64 + 0.5) / n as f64;
        let at = point_at(points, &cum, s);
        let ahead = point_at(points, &cum, (s + 1e-3).min(total));
        let behind = point_at(points, &cum, (s - 1e-3).max(0.0));
        let angle = direction(behind, ahead).unwrap_or(0.0);
        out.extend(cross(at, size, angle, width));
    }
    out
}

/// The length along `points` at which the path first leaves the circle of radius `r` around its first point:
/// a bend inside the circle does not count. The whole length when the path never leaves it.
pub(super) fn exit_length(points: &[Pt], r: f64) -> f64 {
    let Some(&centre) = points.first() else {
        return 0.0;
    };
    if r <= 0.0 {
        return 0.0;
    }
    let mut along = 0.0;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = dist(a, b);
        if dist(b, centre) >= r {
            // `a` lies inside: the one root of |a + t (b - a) - centre| = r with t in [0, 1].
            let d = (b.0 - a.0, b.1 - a.1);
            let f = (a.0 - centre.0, a.1 - centre.1);
            let qa = d.0 * d.0 + d.1 * d.1;
            let qb = 2.0 * (f.0 * d.0 + f.1 * d.1);
            let qc = f.0 * f.0 + f.1 * f.1 - r * r;
            let t = (-qb + (qb * qb - 4.0 * qa * qc).max(0.0).sqrt()) / (2.0 * qa);
            return along + t.clamp(0.0, 1.0) * seg;
        }
        along += seg;
    }
    along
}

/// A symbol drawn in OOM's µm around the origin, as Bézier paths (`true`: the next three points are a
/// curve), placed at `centre`, turned by `angle` and scaled by `scale` mm per µm.
fn oom_path(path: &[(f64, f64, bool)], centre: Pt, angle: f64, scale: f64) -> Vec<Pt> {
    let (sin, cos) = angle.sin_cos();
    let place = |x: f64, y: f64| {
        let (x, y) = (x * scale, y * scale);
        (centre.0 + x * cos - y * sin, centre.1 + x * sin + y * cos)
    };
    let mut out = vec![place(path[0].0, path[0].1)];
    let mut i = 0;
    while i + 1 < path.len() {
        if path[i].2 && i + 3 < path.len() {
            let [p0, p1, p2, p3] = [0, 1, 2, 3].map(|k| place(path[i + k].0, path[i + k].1));
            flatten_cubic(p0, p1, p2, p3, &mut out, 0);
            i += 3;
        } else {
            out.push(place(path[i + 1].0, path[i + 1].1));
            i += 1;
        }
    }
    out
}

/// Crossing point (ISSprOM 710.1): two lines curving outwards on either side of the way through, which
/// runs along `angle`. `size` is the length of the lines.
pub(super) fn crossing_point(centre: Pt, size: f64, angle: f64, width: f64) -> Vec<Shape> {
    let scale = size / 3000.0;
    [1.0, -1.0]
        .map(|side| {
            let y = |v: f64| side * v;
            let path = [
                (-1500.0, y(950.0), true),
                (-500.0, y(675.0), false),
                (500.0, y(675.0), false),
                (1500.0, y(950.0), false),
            ];
            stroke(oom_path(&path, centre, angle, scale), false, width)
        })
        .into()
}

/// First aid (ISOM 2017-2 712): an upright cross of two bars `length` long and `bar` wide.
pub(super) fn first_aid(centre: Pt, length: f64, bar: f64) -> Vec<Shape> {
    let h = length / 2.0;
    vec![
        stroke(
            vec![(centre.0 - h, centre.1), (centre.0 + h, centre.1)],
            false,
            bar,
        ),
        stroke(
            vec![(centre.0, centre.1 - h), (centre.0, centre.1 + h)],
            false,
            bar,
        ),
    ]
}

/// Refreshment point (ISOM 2017-2 713): a cup `size` wide and high outside its line of `width`. The shape is
/// OOM's (forest set), stretched to that outside measure.
pub(super) fn water(centre: Pt, size: f64, width: f64) -> Vec<Shape> {
    // OOM's cup spans x ±1340 and y -1770..1605 µm at its line centres (the bottom curve peaks at 1605).
    let inner = (size - width).max(0.0);
    let (sx, sy) = (inner / 2680.0, inner / 3375.0);
    let centre = (centre.0, centre.1 - (1605.0 - 1770.0) / 2.0 * sy);
    let paths: [&[(f64, f64, bool)]; 4] = [
        &[(-1340.0, -1395.0, false), (-820.0, 1380.0, false)],
        &[(1340.0, -1395.0, false), (820.0, 1380.0, false)],
        &[
            (0.0, -1770.0, true),
            (600.0, -1770.0, false),
            (1200.0, -1620.0, false),
            (1340.0, -1395.0, true),
            (1200.0, -1170.0, false),
            (600.0, -1020.0, false),
            (0.0, -1020.0, true),
            (-600.0, -1020.0, false),
            (-1200.0, -1170.0, false),
            (-1340.0, -1395.0, true),
            (-1200.0, -1620.0, false),
            (-600.0, -1770.0, false),
            (0.0, -1770.0, false),
        ],
        &[
            (-820.0, 1380.0, true),
            (-680.0, 1680.0, false),
            (680.0, 1680.0, false),
            (820.0, 1380.0, false),
        ],
    ];
    paths
        .iter()
        .map(|p| {
            let stretched: Vec<(f64, f64, bool)> =
                p.iter().map(|&(x, y, c)| (x * sx / sy, y, c)).collect();
            line(oom_path(&stretched, centre, 0.0, sy), width)
        })
        .collect()
}

/// Parallel lines `width` wide, `spacing` apart, in each direction of `angles` (radians), clipped to
/// `rings` (even-odd). The lines of one area share their clip outline.
pub(super) fn hatch(
    rings: Vec<Vec<Pt>>,
    angles: &[f64],
    spacing: f64,
    width: f64,
    budget: &mut Budget,
) -> Vec<Shape> {
    let all: Vec<Pt> = rings.iter().flatten().copied().collect();
    if all.is_empty() || spacing.is_nan() || spacing <= 0.0 {
        return Vec::new();
    }
    let rings: Arc<[Vec<Pt>]> = rings.into();
    let mut out = Vec::new();
    for &angle in angles {
        let d = (angle.cos(), angle.sin());
        let n = (-d.1, d.0);
        let range = |v: Pt| {
            all.iter()
                .map(|p| p.0 * v.0 + p.1 * v.1)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                    (lo.min(x), hi.max(x))
                })
        };
        let (along_lo, along_hi) = range(d);
        let (across_lo, across_hi) = range(n);
        let mut k = (across_lo / spacing).ceil();
        while k * spacing <= across_hi {
            if !budget.take() {
                return out;
            }
            let o = k * spacing;
            let at = |s: f64| (n.0 * o + d.0 * s, n.1 * o + d.1 * s);
            out.push(Shape::Clipped {
                rings: rings.clone(),
                inner: Box::new(stroke(vec![at(along_lo), at(along_hi)], false, width)),
            });
            k += 1.0;
        }
    }
    out
}

/// Font size, width and digit height of `text` set so its digits are `height` tall (Arimo regular).
pub(super) fn number_metrics(text: &str, height: f64) -> (f64, f64, f64) {
    let face = FontFace::Regular;
    let size = height / fonts::cap_height(face);
    (size, fonts::text_width(text, face) * size, height)
}

/// A control number with its digits `height` tall, centred on `centre`.
pub(super) fn number(text: &str, centre: Pt, height: f64) -> Shape {
    let (size, w, h) = number_metrics(text, height);
    Shape::Text {
        origin: (centre.0 - w / 2.0, centre.1 + h / 2.0),
        size,
        bold: false,
        italic: false,
        text: text.to_string(),
        rotation: 0.0,
        stroke: None,
    }
}

/// `text` in Arimo of font size `size`, one shape per line, its first line's top at `top_left`.
pub(super) fn text_lines(text: &str, top_left: Pt, size: f64) -> Vec<Shape> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| Shape::Text {
            origin: (
                top_left.0,
                top_left.1 + (fonts::ASCENT + i as f64 * fonts::LINE_SPACING) * size,
            ),
            size,
            bold: false,
            italic: false,
            text: l.to_string(),
            rotation: 0.0,
            stroke: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_across_east_join_and_whole_gaps_drop_the_circle() {
        assert_eq!(kept_arcs(&[]), [(0.0, 360.0)]);
        assert_eq!(kept_arcs(&[(-30.0, 30.0)]), [(30.0, 330.0)]);
        assert_eq!(kept_arcs(&[(10.0, 20.0)]), [(20.0, 370.0)]);
        assert_eq!(kept_arcs(&[(0.0, 360.0)]), []);
        assert_eq!(
            kept_arcs(&[(100.0, 120.0), (110.0, 130.0), (200.0, 210.0)]),
            [(210.0, 460.0), (130.0, 200.0)]
        );
    }

    #[test]
    fn gaps_cut_a_line_and_dashes_follow_it() {
        assert_eq!(
            without(3.0, 97.0, &[(20.0, 30.0)]),
            [(3.0, 20.0), (30.0, 97.0)]
        );
        assert_eq!(
            without(3.0, 97.0, &[(0.0, 5.0), (90.0, 120.0)]),
            [(5.0, 90.0)]
        );
        assert!(without(3.0, 97.0, &[(0.0, 100.0)]).is_empty());
        let mut budget = Budget::new(5);
        let d = dashes(&[(0.0, 0.0), (30.0, 0.0)], 2.0, 0.5, 0.35, &mut budget);
        assert_eq!(d.len(), 5);
        assert!(budget.cut);
    }

    #[test]
    fn hatch_covers_the_box_and_respects_the_budget() {
        let square = vec![vec![(0.0, 0.0), (6.0, 0.0), (6.0, 6.0), (0.0, 6.0)]];
        let mut budget = Budget::new(1000);
        let vertical = hatch(square.clone(), &[PI / 2.0], 0.6, 0.25, &mut budget);
        assert_eq!(vertical.len(), 11);
        let mut budget = Budget::new(3);
        assert_eq!(hatch(square, &[PI / 2.0], 0.6, 0.25, &mut budget).len(), 3);
        assert!(budget.cut);
    }
}

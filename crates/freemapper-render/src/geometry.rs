//! Copied from freemapper 743ce4b (ADR 0014).
//! Path geometry for rendering (`docs/render/oom-render-reference.md` §2, §7): parts, Bézier
//! flattening, arc length, positions and directions along a part ([`Walker`]), and cutting a
//! part into pieces by arc-length intervals. Dash patterns live in `dash.rs`, mid symbols,
//! borders and caps in `line.rs`.
//!
//! Coordinates are paper millimetres with y down, as in OOM.

use freemapper_omap::MapCoord;
use freemapper_omap::coord::{FLAG_CLOSE, FLAG_CURVE_START, FLAG_DASH, FLAG_HOLE};

pub type Pt = (f64, f64);

/// One part of a path: a polyline (Béziers flattened) and whether it is closed.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub points: Vec<Pt>,
    pub closed: bool,
    /// Indices in `points` of the coordinates flagged as dash points, ascending.
    pub dash_points: Vec<usize>,
}

/// Bézier flattening: chords at most 1 mm, control polygon at most 0.005 mm longer than the chord.
const MAX_CHORD: f64 = 1.0;
const MAX_ERROR: f64 = 0.005;
/// Flattened points per path (all parts together). A curve needs up to 2^16 points at the depth
/// limit, so a hostile file could multiply its size by 65,536; once a path has this many points,
/// the remaining curves become single chords.
const MAX_FLATTENED_POINTS: usize = 1_000_000;
/// At most this many dashes or mid symbols per part; more means a broken symbol definition, and
/// the line is drawn solid (dashes) or without mid symbols.
pub(crate) const MAX_REPEATS: usize = 100_000;
/// Points closer than this are skipped when looking for a direction (0.025 mm).
const TANGENT_EPSILON: f64 = 0.025;

fn mm(c: MapCoord) -> Pt {
    (f64::from(c.x) / 1000.0, f64::from(c.y) / 1000.0)
}

fn dist(a: Pt, b: Pt) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}

fn normalize(v: Pt) -> Pt {
    let l = v.0.hypot(v.1);
    if l > 0.0 {
        (v.0 / l, v.1 / l)
    } else {
        (0.0, 0.0)
    }
}

/// Right of travel with y down: (x, y) → (−y, x).
fn perp_right(v: Pt) -> Pt {
    (-v.1, v.0)
}

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// Appends the cubic Bézier p0…p3 without p0, halving until flat enough.
pub fn flatten_cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, out: &mut Vec<Pt>, depth: u32) {
    let chord = dist(p0, p3);
    let polygon = dist(p0, p1) + dist(p1, p2) + dist(p2, p3);
    if depth >= 16 || (chord <= MAX_CHORD && polygon - chord <= MAX_ERROR) {
        out.push(p3);
        return;
    }
    let (a, b, c) = (lerp(p0, p1, 0.5), lerp(p1, p2, 0.5), lerp(p2, p3, 0.5));
    let (d, e) = (lerp(a, b, 0.5), lerp(b, c, 0.5));
    let m = lerp(d, e, 0.5);
    flatten_cubic(p0, a, d, m, out, depth + 1);
    flatten_cubic(m, e, c, p3, out, depth + 1);
}

/// The parts of an OOM path: they end at coordinates flagged `FLAG_HOLE`; a part is closed when
/// its last coordinate is flagged `FLAG_CLOSE`. Béziers are flattened.
pub fn parts(coords: &[MapCoord]) -> Vec<Part> {
    let n = coords.len();
    let mut parts = Vec::new();
    let mut points: Vec<Pt> = Vec::new();
    let mut dash_points = Vec::new();
    // The point just pushed for `c`: a dash point if `c` is flagged.
    let mark = |c: MapCoord, points: &Vec<Pt>, dash_points: &mut Vec<usize>| {
        if c.flags & FLAG_DASH != 0 {
            dash_points.push(points.len() - 1);
        }
    };
    let mut total = 0usize;
    let mut i = 0;
    // At each step `coords[i]` is the last point of the part so far.
    while i < n {
        if points.is_empty() {
            points.push(mm(coords[i]));
            mark(coords[i], &points, &mut dash_points);
        }
        let c = coords[i];
        if c.flags & FLAG_HOLE != 0 || i + 1 == n {
            total += points.len();
            parts.push(Part {
                points: std::mem::take(&mut points),
                closed: c.flags & FLAG_CLOSE != 0,
                dash_points: std::mem::take(&mut dash_points),
            });
            i += 1;
        } else if c.flags & FLAG_CURVE_START != 0 && i + 3 < n {
            let p0 = mm(c);
            let (p1, p2, p3) = (mm(coords[i + 1]), mm(coords[i + 2]), mm(coords[i + 3]));
            // Over the budget: start at the depth limit, which emits the chord p0..p3 only.
            let depth = if total + points.len() >= MAX_FLATTENED_POINTS {
                16
            } else {
                0
            };
            flatten_cubic(p0, p1, p2, p3, &mut points, depth);
            mark(coords[i + 3], &points, &mut dash_points);
            i += 3;
        } else {
            points.push(mm(coords[i + 1]));
            mark(coords[i + 1], &points, &mut dash_points);
            i += 1;
        }
    }
    parts
}

pub fn length(points: &[Pt]) -> f64 {
    points.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Cumulative arc length at every point.
pub(crate) fn cumulative(points: &[Pt]) -> Vec<f64> {
    let mut cum = Vec::with_capacity(points.len());
    let mut total = 0.0;
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            total += dist(points[i - 1], *p);
        }
        cum.push(total);
    }
    cum
}

/// The point at arc length `s` and the index of the segment it lies on.
fn locate(points: &[Pt], cum: &[f64], s: f64) -> (Pt, usize) {
    let s = s.clamp(0.0, *cum.last().unwrap_or(&0.0));
    let i = cum
        .partition_point(|&c| c < s)
        .saturating_sub(1)
        .min(points.len().saturating_sub(2));
    let seg = cum[i + 1] - cum[i];
    let t = if seg > 0.0 { (s - cum[i]) / seg } else { 0.0 };
    (lerp(points[i], points[i + 1], t), i)
}

/// Position and direction (radians, y down) at arc length `s`: the segment direction, the
/// bisector at a vertex, the single direction at an open end.
#[cfg(test)]
pub fn at(part: &Part, s: f64) -> (Pt, f64) {
    let mut walker = Walker::new(&part.points, part.closed);
    at_on(part, &cumulative(&part.points), &mut walker, s)
}

/// [`at`] with the cumulative lengths of the part computed once and the searches done by
/// `walker` (made for this part); a limited walker may leave the direction incomplete, which the
/// caller sees in [`Walker::exhausted`] [PC `SplitPathCoord::tangentVector`].
pub(crate) fn at_on(part: &Part, cum: &[f64], walker: &mut Walker, s: f64) -> (Pt, f64) {
    let points = &part.points;
    if points.len() < 2 {
        return (points.first().copied().unwrap_or((0.0, 0.0)), 0.0);
    }
    let (pos, _) = locate(points, cum, s);
    let n = points.len();
    // The nearest points before and after `pos` that are not within the epsilon.
    let i_next = cum.partition_point(|&c| c <= s).min(n);
    let i_prev = cum.partition_point(|&c| c < s).min(n);
    let mut next = walker.forward(pos, i_next, n);
    let mut prev = i_prev.checked_sub(1).and_then(|k| walker.back(pos, k, 0));
    if part.closed {
        if next.is_none() {
            next = walker.forward(pos, 1, n);
        }
        if prev.is_none() {
            prev = walker.back(pos, n - 2, 0);
        }
    }
    let out = next
        .map(|k| normalize(sub(points[k], pos)))
        .unwrap_or((0.0, 0.0));
    let inc = prev
        .map(|k| normalize(sub(pos, points[k])))
        .unwrap_or((0.0, 0.0));
    let t = (out.0 + inc.0, out.1 + inc.1);
    (pos, t.1.atan2(t.0))
}

/// Direction (radians) from the first point to the first point farther than 0.025 mm: the
/// orientation of a start symbol.
pub fn start_angle(part: &Part) -> f64 {
    let first = part.points[0];
    part.points
        .iter()
        .find(|&&p| dist(p, first) > TANGENT_EPSILON)
        .map_or(0.0, |&p| (p.1 - first.1).atan2(p.0 - first.0))
}

/// Direction (radians) from the last point farther than 0.025 mm to the last point: the
/// orientation of an end symbol.
pub fn end_angle(part: &Part) -> f64 {
    let last = *part.points.last().expect("parts are not empty");
    part.points
        .iter()
        .rev()
        .find(|&&p| dist(p, last) > TANGENT_EPSILON)
        .map_or(0.0, |&p| (last.1 - p.1).atan2(last.0 - p.0))
}

/// The polyline between arc lengths `a` and `b`, through the vertices in between.
fn between(points: &[Pt], cum: &[f64], a: f64, b: f64) -> Vec<Pt> {
    between_lengths(points, cum, a, b).0
}

/// [`between`] with the arc length of every point.
fn between_lengths(points: &[Pt], cum: &[f64], a: f64, b: f64) -> (Vec<Pt>, Vec<f64>) {
    let (start, i) = locate(points, cum, a);
    let (end, j) = locate(points, cum, b);
    let mut out = vec![start];
    let mut lengths = vec![a];
    for k in i + 1..=j {
        if cum[k] > a && cum[k] < b {
            out.push(points[k]);
            lengths.push(cum[k]);
        }
    }
    out.push(end);
    lengths.push(b);
    (out, lengths)
}

/// The polylines of arc-length `intervals` of `part` and whether each is closed: the whole part
/// is the part itself (closed if the part is); on a closed part the first piece is appended to
/// the last when they meet at the closing point [RI `LineRenderable`, `connectPath`].
pub fn pieces(part: &Part, intervals: &[(f64, f64)]) -> Vec<(Vec<Pt>, bool)> {
    let cum = cumulative(&part.points);
    let total = *cum.last().unwrap_or(&0.0);
    const EPS: f64 = 1e-9;
    if let [(a, b)] = intervals
        && *a <= EPS
        && *b >= total - EPS
    {
        return vec![(part.points.clone(), part.closed)];
    }
    let mut out: Vec<(Vec<Pt>, bool)> = intervals
        .iter()
        .map(|&(a, b)| (between(&part.points, &cum, a, b), false))
        .collect();
    let meet = intervals.first().is_some_and(|f| f.0 <= EPS)
        && intervals.last().is_some_and(|l| l.1 >= total - EPS);
    if part.closed && out.len() > 1 && meet {
        let (first, _) = out.remove(0);
        let last = &mut out.last_mut().expect("more than one").0;
        last.extend(first.into_iter().skip(1));
    }
    out
}

/// The parts of the arc-length intervals `a` that lie inside intervals of `b` (both ascending).
pub fn intersect(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let lo = a[i].0.max(b[j].0);
        let hi = a[i].1.min(b[j].1);
        if lo < hi {
            out.push((lo, hi));
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Searches a polyline for the nearest points farther than 0.025 mm, as OOM's tangent functions
/// do, without their quadratic cost on clustered points: runs of identical points are skipped in
/// one step (exact, since they all lie at the same distance), and every distance check is
/// counted against a limit so that the caller can charge it ([`Walker::checks`]).
pub struct Walker<'a> {
    points: &'a [Pt],
    closed: bool,
    /// First and last index of the run of identical points that holds each point.
    run_start: Vec<usize>,
    run_end: Vec<usize>,
    checks: usize,
    limit: usize,
}

impl<'a> Walker<'a> {
    /// Without a limit on checks; O(n).
    pub fn new(points: &'a [Pt], closed: bool) -> Self {
        let n = points.len();
        let mut run_start = vec![0; n];
        for i in 1..n {
            run_start[i] = if points[i] == points[i - 1] {
                run_start[i - 1]
            } else {
                i
            };
        }
        let mut run_end = vec![n.saturating_sub(1); n];
        for i in (0..n.saturating_sub(1)).rev() {
            run_end[i] = if points[i] == points[i + 1] {
                run_end[i + 1]
            } else {
                i
            };
        }
        Self {
            points,
            closed,
            run_start,
            run_end,
            checks: 0,
            limit: usize::MAX,
        }
    }

    /// Distance checks made so far.
    pub fn checks(&self) -> usize {
        self.checks
    }

    /// Allows `more` checks beyond those made so far.
    pub fn allow(&mut self, more: usize) {
        self.limit = self.checks.saturating_add(more);
    }

    /// The limit stopped a search, whose answer is then incomplete.
    pub fn exhausted(&self) -> bool {
        self.checks >= self.limit
    }

    /// Whether point `k` is farther than the epsilon from `q`; `None` at the limit.
    fn far(&mut self, q: Pt, k: usize) -> Option<bool> {
        if self.checks >= self.limit {
            return None;
        }
        self.checks += 1;
        Some(dist(self.points[k], q) > TANGENT_EPSILON)
    }

    /// The first index in `from..end` whose point is farther than the epsilon from `q`.
    fn forward(&mut self, q: Pt, from: usize, end: usize) -> Option<usize> {
        let mut k = from;
        while k < end {
            if self.far(q, k)? {
                return Some(k);
            }
            k = self.run_end[k] + 1;
        }
        None
    }

    /// The last index in `stop..=from` whose point is farther than the epsilon from `q`.
    fn back(&mut self, q: Pt, from: usize, stop: usize) -> Option<usize> {
        let mut k = from;
        loop {
            if k < stop {
                return None;
            }
            if self.far(q, k)? {
                return Some(k);
            }
            k = self.run_start[k].checked_sub(1)?;
        }
    }

    /// [VP `calculateIncomingTangent`]: from the nearest earlier point farther than the epsilon
    /// to point `i`; closed lines continue from the end.
    fn incoming(&mut self, i: usize) -> Option<Pt> {
        let n = self.points.len();
        let q = self.points[i];
        let mut k = i.checked_sub(1).and_then(|from| self.back(q, from, 0));
        if k.is_none() && self.closed && n > i + 2 {
            k = self.back(q, n - 1, i + 1);
        }
        k.map(|k| sub(q, self.points[k]))
    }

    /// [VP `calculateOutgoingTangent`]: from point `i` to the nearest later point farther than
    /// the epsilon; closed lines continue from the start.
    fn outgoing(&mut self, i: usize) -> Option<Pt> {
        let n = self.points.len();
        let q = self.points[i];
        let mut k = self.forward(q, i + 1, n);
        if k.is_none() && self.closed {
            k = self.forward(q, 0, i);
        }
        k.map(|k| sub(self.points[k], q))
    }

    /// Direction (radians, y down) and scale at point `i`: the bisector of the incoming and
    /// outgoing direction and 1 / cos(half the turn), the single direction and 1 at an end,
    /// perpendicular and infinite at a reversal [VP `calculateTangentScaling`].
    pub fn tangent_scaling(&mut self, i: usize) -> (f64, f64) {
        let (dir, scale) = self.tangent_scaling_vector(i);
        (dir.1.atan2(dir.0), scale)
    }

    fn tangent_scaling_vector(&mut self, i: usize) -> (Pt, f64) {
        let inc = self.incoming(i);
        let out = self.outgoing(i);
        match (inc, out) {
            (inc, None) => (inc.map_or((0.0, 0.0), normalize), 1.0),
            (None, Some(out)) => (normalize(out), 1.0),
            (Some(inc), Some(out)) => {
                let (inc, out) = (normalize(inc), normalize(out));
                let sum = (inc.0 + out.0, inc.1 + out.1);
                if sum.0.hypot(sum.1) < 1e-12 {
                    (perp_right(out), f64::INFINITY)
                } else {
                    let dir = normalize(sum);
                    (dir, 1.0 / (dir.0 * inc.0 + dir.1 * inc.1))
                }
            }
        }
    }
}

/// Direction and scale at point `i` of a polyline ([`Walker::tangent_scaling`], no limit).
#[cfg(test)]
pub fn tangent_scaling(points: &[Pt], closed: bool, i: usize) -> (f64, f64) {
    Walker::new(points, closed).tangent_scaling(i)
}

/// Overlap of a pointed cap into the line (mm) [LS `createPointedLineCap`].
const CAP_OVERLAP: f64 = 0.05;

/// The filled taper of a pointed cap on [a, b] of `part` (reference §2.7)
/// [LS `createPointedLineCap`]: the half width grows from 0 at the line's end to `half_width`
/// at `cap_length` from it, widened by 1 / cos(half the turn) at corners (at most twice the
/// half width); beyond 0.2 mm the cap reaches 0.05 mm into the line. The ring runs out along the
/// right side and back along the left side.
///
/// The tangent searches make at most `max_checks` distance checks; returns the ring and the
/// checks made, or `None` when the limit stopped them.
pub fn pointed_cap(
    part: &Part,
    (a, b): (f64, f64),
    is_end: bool,
    (half_width, cap_length): (f64, f64),
    max_checks: usize,
) -> Option<(Vec<Pt>, usize)> {
    let cum = cumulative(&part.points);
    let (middle, lengths) = between_lengths(&part.points, &cum, a, b);
    let mut walker = Walker::new(&middle, false);
    walker.allow(max_checks);
    let n = middle.len();
    let mut right = Vec::with_capacity(n + 1);
    let mut left = Vec::with_capacity(n + 1);
    for (i, (&c, &len)) in middle.iter().zip(&lengths).enumerate() {
        let from_tip = if is_end { b - len } else { len - a };
        let factor = (from_tip / cap_length).clamp(0.0, 1.0);
        let (dir, scale) = walker.tangent_scaling_vector(i);
        if walker.exhausted() {
            return None;
        }
        let side = normalize(perp_right(dir));
        let radius = (half_width * factor * scale).clamp(0.0, 2.0 * half_width);
        right.push((c.0 + radius * side.0, c.1 + radius * side.1));
        left.push((c.0 - radius * side.0, c.1 - radius * side.1));
    }
    if b - a > 4.0 * CAP_OVERLAP {
        // The tangent at the wide end, along the line towards its middle.
        let wide = if is_end { 0 } else { n - 1 };
        let tangent = if is_end {
            walker.outgoing(wide).map(|t| {
                let t = normalize(t);
                (-t.0 * CAP_OVERLAP, -t.1 * CAP_OVERLAP)
            })
        } else {
            walker.incoming(wide).map(|t| {
                let t = normalize(t);
                (t.0 * CAP_OVERLAP, t.1 * CAP_OVERLAP)
            })
        };
        if walker.exhausted() {
            return None;
        }
        if let Some(t) = tangent {
            let r = perp_right(if is_end { t } else { (-t.0, -t.1) });
            let r_point = (right[wide].0 + t.0 + r.0, right[wide].1 + t.1 + r.1);
            let l_point = (left[wide].0 + t.0 - r.0, left[wide].1 + t.1 - r.1);
            if is_end {
                right.insert(0, r_point);
                left.insert(0, l_point);
            } else {
                right.push(r_point);
                left.push(l_point);
            }
        }
    }
    // Right side, then the left side back (without its tip at an end cap).
    let skip = usize::from(is_end);
    let back = left.len() - skip;
    right.extend(left[..back].iter().rev());
    Some((right, walker.checks()))
}

/// The polyline offset by `shift` mm to the right of travel (negative: left), mitred at corners
/// (reference §2.3).
pub fn offset(points: &[Pt], closed: bool, shift: f64) -> Vec<Pt> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    // Without the duplicated closing point, so that tangents wrap around.
    let pts: Vec<Pt> = if closed && points[0] == points[n - 1] {
        points[..n - 1].to_vec()
    } else {
        points.to_vec()
    };
    let m = pts.len();
    let add = |a: Pt, v: Pt, f: f64| (a.0 + v.0 * f, a.1 + v.1 * f);
    let mut out = Vec::with_capacity(m + 1);
    for i in 0..m {
        let c = pts[i];
        let prev = if i > 0 {
            Some(pts[i - 1])
        } else if closed {
            Some(pts[m - 1])
        } else {
            None
        };
        let next = if i + 1 < m {
            Some(pts[i + 1])
        } else if closed {
            Some(pts[0])
        } else {
            None
        };
        let t_in = prev.map(|p| normalize(sub(c, p)));
        let t_out = next.map(|p| normalize(sub(p, c)));
        match (t_in, t_out) {
            (None, Some(t)) | (Some(t), None) => out.push(add(c, perp_right(t), shift)),
            (Some(t_in), Some(t_out)) => {
                let mid = normalize((t_in.0 + t_out.0, t_in.1 + t_out.1));
                let cross = t_out.0 * t_in.1 - t_in.0 * t_out.1;
                let base = add(c, perp_right(t_out), shift);
                if cross.abs() < 1e-12 && mid != (0.0, 0.0) {
                    out.push(base);
                } else if cross * shift > 0.0 {
                    // Outer side: the miter point, or a bevel for very sharp corners.
                    if mid.0 * t_in.0 + mid.1 * t_in.1 <= 0.4472 {
                        out.push(add(c, perp_right(t_in), shift));
                        out.push(base);
                    } else {
                        let pr = perp_right(mid);
                        let phi = (pr.0 * t_in.0 + pr.1 * t_in.1).clamp(-1.0, 1.0).acos();
                        let off = (shift / phi.tan()).abs();
                        out.push(add(base, t_out, -off));
                    }
                } else {
                    // Inner side: the intersection of the offset lines, bounded by the segments.
                    let pr = perp_right(mid);
                    let phi = (pr.0 * t_in.0 + pr.1 * t_in.1).clamp(-1.0, 1.0).acos();
                    let off = -(shift / phi.tan()).abs();
                    let len_in = prev.map_or(f64::INFINITY, |p| dist(p, c));
                    let len_out = next.map_or(f64::INFINITY, |p| dist(p, c));
                    if phi.tan() >= 1.0 || off.abs() < len_in.min(len_out) {
                        out.push(add(base, t_out, -off));
                    } else if len_in < len_out {
                        out.push(add(base, t_out, len_in));
                    } else {
                        out.push(add(add(c, perp_right(t_in), shift), t_in, -len_out));
                    }
                }
            }
            (None, None) => out.push(c),
        }
    }
    if closed {
        let first = out[0];
        out.push(first);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[Pt]) -> Part {
        Part {
            points: points.to_vec(),
            closed: false,
            dash_points: Vec::new(),
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn parts_split_at_holes_and_know_closed() {
        let mut coords = vec![
            MapCoord::new(0, 0),
            MapCoord::new(1000, 0),
            MapCoord::new(1000, 1000),
            MapCoord::new(0, 0),
        ];
        coords[3].flags = FLAG_HOLE | FLAG_CLOSE;
        coords.extend([MapCoord::new(5000, 0), MapCoord::new(6000, 0)]);
        let parts = parts(&coords);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].closed && !parts[1].closed);
        assert_eq!(parts[1].points, [(5.0, 0.0), (6.0, 0.0)]);
    }

    #[test]
    fn many_huge_curves_stay_within_the_point_budget() {
        // 200 curves of 2 km each would need 65,536 points apiece at the depth limit.
        let mut coords = Vec::new();
        for i in 0..200 {
            let x = i * 3_000_000;
            let mut c = [
                MapCoord::new(x, 0),
                MapCoord::new(x + 1_000_000, 2_000_000),
                MapCoord::new(x + 2_000_000, -2_000_000),
                MapCoord::new(x + 3_000_000, 0),
            ];
            c[0].flags = FLAG_CURVE_START;
            coords.extend(c);
        }
        let part = &parts(&coords)[0];
        assert!(
            part.points.len() <= MAX_FLATTENED_POINTS + 70_000,
            "{}",
            part.points.len()
        );
        assert!(
            part.points.len() > 100_000,
            "the first curves are still flattened"
        );
    }

    #[test]
    fn bezier_is_flattened_within_tolerance() {
        // A quarter circle of radius 10 mm as one cubic Bézier.
        let k = 0.552_284_75 * 10_000.0;
        let mut coords = vec![
            MapCoord::new(10_000, 0),
            MapCoord::new(10_000, k as i32),
            MapCoord::new(k as i32, 10_000),
            MapCoord::new(0, 10_000),
        ];
        coords[0].flags = FLAG_CURVE_START;
        let part = &parts(&coords)[0];
        assert!(part.points.len() > 10);
        for w in part.points.windows(2) {
            let chord = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            assert!(chord <= MAX_CHORD + 1e-9);
        }
        for p in &part.points {
            let r = (p.0 * p.0 + p.1 * p.1).sqrt();
            assert!((r - 10.0).abs() < 0.01, "{p:?} at r {r}");
        }
        let quarter = std::f64::consts::FRAC_PI_2 * 10.0;
        assert!((length(&part.points) - quarter).abs() < 0.01);
    }

    #[test]
    fn tangent_at_vertex_is_the_bisector() {
        let part = line(&[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)]);
        let (p, angle) = at(&part, 5.0);
        assert_eq!(p, (5.0, 0.0));
        assert!(near(angle, std::f64::consts::FRAC_PI_4));
        let (_, along) = at(&part, 2.0);
        assert!(near(along, 0.0));
        let (q, down) = at(&part, 7.5);
        assert!(near(q.1, 2.5) && near(down, std::f64::consts::FRAC_PI_2));
    }

    #[test]
    fn offset_of_a_right_angle_is_mitred() {
        // Going east then south (y down): the right side is the inside of the corner.
        let right = offset(&[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)], false, 1.0);
        assert_eq!(right.len(), 3);
        assert!(near(right[0].0, 0.0) && near(right[0].1, 1.0));
        assert!(near(right[1].0, 4.0) && near(right[1].1, 1.0), "{right:?}");
        let left = offset(&[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)], false, -1.0);
        assert!(near(left[1].0, 6.0) && near(left[1].1, -1.0), "{left:?}");
        assert!(near(left[2].0, 6.0) && near(left[2].1, 5.0));
    }

    #[test]
    fn pointed_cap_polygon() {
        // Start cap over 2 mm of a line 1 mm wide: from the tip at (0, 0) to the full half width
        // at 2 mm, plus the 0.05 mm overlap into the line [LS `createPointedLineCap`].
        let part = line(&[(0.0, 0.0), (10.0, 0.0)]);
        let cap = |part: &Part, range, is_end| {
            pointed_cap(part, range, is_end, (0.5, 2.0), usize::MAX)
                .unwrap()
                .0
        };
        let ring = cap(&part, (0.0, 2.0), false);
        assert_eq!(ring.first(), Some(&(0.0, 0.0)));
        assert_eq!(ring.last(), Some(&(0.0, 0.0)));
        let has = |p: Pt| ring.iter().any(|q| near(q.0, p.0) && near(q.1, p.1));
        assert!(has((2.0, 0.5)) && has((2.0, -0.5)), "{ring:?}");
        assert!(has((2.05, 0.45)) && has((2.05, -0.45)), "{ring:?}");
        // End cap: tip at (10, 0).
        let ring = cap(&part, (8.0, 10.0), true);
        assert!(ring.iter().any(|q| near(q.0, 10.0) && near(q.1, 0.0)));
        assert!(
            ring.iter()
                .all(|q| q.1.abs() <= 0.5 + 1e-9 && q.0 >= 7.95 - 1e-9)
        );
        // At a corner inside the cap the half width grows by 1 / cos(half the angle).
        let bent = line(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        let ring = cap(&bent, (0.0, 2.0), false);
        let r = 0.25 * std::f64::consts::SQRT_2;
        assert!(
            ring.iter()
                .any(|q| near(q.0, 1.0 - r * 0.5f64.sqrt()) && near(q.1, r * 0.5f64.sqrt())),
            "{ring:?}"
        );
    }

    #[test]
    fn tangent_scaling_at_a_corner() {
        let points = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)];
        let (angle, scale) = tangent_scaling(&points, false, 1);
        assert!(near(angle, std::f64::consts::FRAC_PI_4));
        assert!(near(scale, std::f64::consts::SQRT_2));
        assert_eq!(tangent_scaling(&points, false, 0), (0.0, 1.0));
        let (end, scale) = tangent_scaling(&points, false, 2);
        assert!(near(end, std::f64::consts::FRAC_PI_2) && scale == 1.0);
        // Reversal: perpendicular, unbounded scale.
        let back = [(0.0, 0.0), (1.0, 0.0), (0.0, 0.0)];
        assert_eq!(tangent_scaling(&back, false, 1).1, f64::INFINITY);
    }

    #[test]
    fn parts_record_dash_points() {
        let mut coords = vec![
            MapCoord::new(0, 0),
            MapCoord::new(1000, 0),
            MapCoord::new(2000, 0),
        ];
        coords[1].flags = FLAG_DASH;
        assert_eq!(parts(&coords)[0].dash_points, [1]);
        // A dash point that ends a Bézier: the last point of the flattened curve.
        let mut curved = vec![
            MapCoord::new(0, 0),
            MapCoord::new(1000, 1000),
            MapCoord::new(2000, 1000),
            MapCoord::new(3000, 0),
            MapCoord::new(4000, 0),
        ];
        curved[0].flags = FLAG_CURVE_START;
        curved[3].flags = FLAG_DASH;
        let part = &parts(&curved)[0];
        assert!(part.points.len() > 5, "flattened");
        assert_eq!(part.dash_points.len(), 1);
        assert_eq!(part.points[part.dash_points[0]], (3.0, 0.0));
        assert_eq!(part.dash_points[0], part.points.len() - 2);
    }
}

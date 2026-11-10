//! Dash patterns and mid-symbol positions of line symbols (`docs/render/oom-render-reference.md`
//! §2.2, §2.4, §2.5): OOM's `processDashedLine`, `createDashGroups` and
//! `createMidSymbolRenderables` on arc lengths (mm).

use crate::draw::POINTS_PER_UNIT;
use crate::geometry::{MAX_REPEATS, Part, Pt, Walker, at_on, cumulative};
use crate::symbols::{Dash, Mid, MidPlacement};

/// Distance checks that fit into `room` units when `checks` were made before (only whole
/// [`POINTS_PER_UNIT`] count).
pub(crate) fn check_room(room: usize, checks: usize) -> usize {
    room.saturating_mul(POINTS_PER_UNIT)
        .saturating_add(POINTS_PER_UNIT - 1 - checks % POINTS_PER_UNIT)
}

/// The arc lengths that end the sections of [start, end] (reference §2.2): every dash point
/// strictly inside, then `end` [VP `PathCoordVector::findNextDashPoint`].
fn section_ends(part: &Part, cum: &[f64], start: f64, end: f64) -> Vec<f64> {
    let mut ends: Vec<f64> = part
        .dash_points
        .iter()
        .filter_map(|&i| cum.get(i).copied())
        .filter(|&c| c > start && c < end)
        .collect();
    ends.dedup();
    ends.push(end);
    ends
}

/// Dashes and mid-symbol spots of a dashed line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DashPattern {
    /// Arc-length intervals [a, b] to stroke, in order.
    pub intervals: Vec<(f64, f64)>,
    /// Positions and directions of the mid symbols.
    pub spots: Vec<(Pt, f64)>,
    /// The output limit or `MAX_REPEATS` changed the result.
    pub limited: bool,
    /// Units of work: every section, interval and spot, every mid-symbol position tried and
    /// skipped, and one per [`POINTS_PER_UNIT`] distance checks of the tangent searches.
    pub steps: usize,
}

/// Why dashing stopped early.
enum Stop {
    /// The step limit: keep what there is.
    Full,
    /// More than `MAX_REPEATS` steps: a broken definition, drawn solid without mid symbols.
    Repeats,
}

/// The state of [`dash_pattern`]: OOM's `processDashedLine` and `createDashGroups` on arc
/// lengths (mm).
struct Dasher<'a> {
    part: &'a Part,
    cum: &'a [f64],
    walker: Walker<'a>,
    total: f64,
    /// Dash, break and in-group break length, dashes per group.
    d: f64,
    b: f64,
    g: f64,
    k: u32,
    half_outer: bool,
    placement: Option<MidPlacement>,
    /// Mid symbols per spot, their distance, `show_at_least_one_symbol`.
    m: u32,
    q: f64,
    at_least_one: bool,
    out: DashPattern,
    max: usize,
    steps: usize,
}

impl Dasher<'_> {
    /// Units used: the steps and the tangent searches' distance checks.
    fn used(&self) -> usize {
        self.steps + self.walker.checks() / POINTS_PER_UNIT
    }

    /// Takes one step: past `MAX_REPEATS` steps the definition is broken, past `max` units the
    /// caller's limit is reached.
    fn step(&mut self) -> Result<(), Stop> {
        if self.steps >= MAX_REPEATS {
            return Err(Stop::Repeats);
        }
        if self.used() >= self.max {
            return Err(Stop::Full);
        }
        self.steps += 1;
        Ok(())
    }

    fn interval(&mut self, a: f64, b: f64) -> Result<(), Stop> {
        self.step()?;
        self.out.intervals.push((a, b));
        Ok(())
    }

    fn place(&mut self, p: f64) -> Result<(), Stop> {
        self.step()?;
        let room = self.max.saturating_sub(self.used());
        self.walker.allow(check_room(room, self.walker.checks()));
        let spot = at_on(self.part, self.cum, &mut self.walker, p);
        if self.walker.exhausted() {
            return Err(Stop::Full);
        }
        self.out.spots.push(spot);
        Ok(())
    }

    /// [LS `processContinuousLine`]: the stretch [a, b], and if `set_mid` the spot's mid symbols
    /// centred on it.
    fn continuous(&mut self, a: f64, b: f64, set_mid: bool) -> Result<(), Stop> {
        self.interval(a, b)?;
        if set_mid {
            let msl = f64::from(self.m - 1) * self.q;
            if msl <= b - a {
                let mut pos = (a + b - msl) / 2.0;
                self.place(pos)?;
                for _ in 1..self.m {
                    pos += self.q;
                    self.place(pos)?;
                }
            }
        }
        Ok(())
    }

    /// [LS `createDashGroups`] for the section [start, end], the dash so far starting at
    /// `line_start`; returns where the next section's first dash starts.
    fn groups(
        &mut self,
        line_start: f64,
        start: f64,
        end: f64,
        is_part_start: bool,
        is_part_end: bool,
    ) -> Result<f64, Stop> {
        self.step()?;
        let (d, b, g, k, q, m) = (self.d, self.b, self.g, self.k, self.q, self.m);
        let (kf, mf) = (f64::from(k), f64::from(m));
        let closed = self.part.closed;
        // Inside a part the sections meet at dash points.
        let half_first = if is_part_start {
            self.half_outer || closed
        } else {
            k == 1
        };
        let half_last = if is_part_end {
            self.half_outer || closed
        } else {
            k == 1
        };
        let h = u32::from(half_first) + u32::from(half_last);
        let hf = f64::from(h);
        let period = kf * d + (kf - 1.0) * g + b;
        let length = end - start;
        let lpb = length + b;
        let nf = hf + (lpb - hf * d / 2.0) / period;
        let lo = nf.floor();
        let min_opt_dashes = 2.0 * kf - f64::from(h / 2);
        let sw = 0.2 * period / kf;
        let at_dash = self.placement == Some(MidPlacement::CentreOfDash);
        let mut set_mid = self.placement.is_some() && (length >= d - sw || self.at_least_one);
        if at_dash {
            if line_start < start || !is_part_start {
                // The dash through the dash point is drawn here: its mid symbols too.
                set_mid = false;
                let mut pos = start - (mf - 1.0) * q / 2.0;
                for _ in (0..m).step_by(2) {
                    if pos >= 0.0 {
                        self.place(pos)?;
                    } else {
                        self.step()?;
                    }
                    pos += q;
                }
            }
            if half_first {
                let mut pos = start + f64::from(m % 2 + 1) * q / 2.0;
                let mut s = 1;
                while s < m && pos <= self.total {
                    self.place(pos)?;
                    pos += q;
                    s += 2;
                }
            }
        }
        if length <= 0.0 || (lo <= 1.0 && length < 2.0 * period - min_opt_dashes * sw) {
            // Too short for dashes: solid if it ends the part, else handed to the next section.
            if !is_part_end {
                return Ok(line_start);
            }
            self.continuous(line_start, end, set_mid)?;
            return Ok(end);
        }
        let hi = nf.ceil();
        let n = if lo < 1.0 {
            hi
        } else {
            let dlo = (lpb - lo * period) / lo;
            let dhi = (hi * period - lpb) / hi;
            if dlo > dhi { hi } else { lo }
        };
        if n * kf > MAX_REPEATS as f64 {
            return Err(Stop::Repeats);
        }
        let dash_len =
            ((length - (n - 1.0) * b - n * (kf - 1.0) * g) * 2.0 / (2.0 * n * kf - hf)).max(0.0);
        let groups = n as u32;
        let mut cur = start;
        let mut dash_start = line_start;
        for grp in 1..=groups {
            let (first_grp, last_grp) = (grp == 1, grp == groups);
            if self.placement == Some(MidPlacement::CentreOfDashGroup) {
                let mut pos = cur;
                if last_grp && half_last {
                    pos = end;
                } else if !(first_grp && half_first) {
                    pos += (dash_len * kf + g * (kf - 1.0)) / 2.0;
                }
                pos -= q * (mf - 1.0) / 2.0;
                for _ in 0..m {
                    if pos > end {
                        break;
                    }
                    if pos > start {
                        self.place(pos)?;
                    } else {
                        self.step()?;
                    }
                    pos += q;
                }
            }
            for i in 1..=k {
                let first = first_grp && i == 1;
                let last = last_grp && i == k;
                let has_start = !(first && half_first);
                let has_end = !(last && half_last);
                let half = has_start != has_end;
                let len = if half { dash_len / 2.0 } else { dash_len };
                if !first {
                    dash_start = cur;
                }
                if last && !is_part_end {
                    // The last dash before a dash point continues in the next section.
                    return Ok(dash_start);
                }
                let dash_end = cur + len;
                self.continuous(dash_start, dash_end, at_dash && !half)?;
                cur += len;
                dash_start = dash_end;
                if i < k {
                    cur += g;
                }
            }
            if grp < groups {
                if self.placement == Some(MidPlacement::CentreOfGap) {
                    let mut pos = cur + (b - q * (mf - 1.0)) / 2.0;
                    for _ in 0..m {
                        if pos > end {
                            break;
                        }
                        if pos < start {
                            self.step()?;
                        } else {
                            self.place(pos)?;
                        }
                        pos += q;
                    }
                }
                cur += b;
            }
        }
        if half_last && at_dash {
            // The half dash at the closing point (or end) carries the joined dash's symbols.
            let mut pos = end - (mf - 1.0) * q / 2.0;
            for _ in (0..m).step_by(2) {
                self.place(pos)?;
                pos += q;
            }
        }
        Ok(end)
    }
}

/// The dashes (arc-length intervals) and mid-symbol spots of a dashed line on [start, end] of
/// `part` (reference §2.2, §2.5) [LS `processDashedLine`, `createDashGroups`]. The pattern
/// restarts at dash points. `mid`: the mid symbol of the dashed line, placed by its
/// [`MidPlacement`].
///
/// At most `max` steps (intervals, spots and skipped mid-symbol positions together); `limited`
/// is true when that limit, or `MAX_REPEATS` steps (the stretch is then one solid interval
/// without spots), changed the result.
pub fn dash_pattern(
    part: &Part,
    start: f64,
    end: f64,
    dash: &Dash,
    mid: Option<&Mid>,
    max: usize,
) -> DashPattern {
    let cum = cumulative(&part.points);
    let (d, b, g) = (
        dash.dash / 1000.0,
        dash.gap / 1000.0,
        dash.in_group_gap / 1000.0,
    );
    let k = dash.group.max(1);
    let q = mid.map_or(0.0, |m| m.distance / 1000.0);
    let period = f64::from(k) * d + f64::from(k - 1) * g + b;
    let solid = |limited, steps| DashPattern {
        intervals: vec![(start, end)],
        spots: Vec::new(),
        limited,
        steps,
    };
    let usable = d > 0.0
        && b >= 0.0
        && g >= 0.0
        && q >= 0.0
        && period.is_finite()
        && q.is_finite()
        && start.is_finite()
        && end.is_finite();
    if !usable || end <= start {
        return solid(false, 1);
    }
    let mut dasher = Dasher {
        part,
        total: *cum.last().unwrap_or(&0.0),
        cum: &cum,
        walker: Walker::new(&part.points, part.closed),
        d,
        b,
        g,
        k,
        half_outer: dash.half_outer,
        placement: mid.map(|m| m.placement),
        m: mid.map_or(1, |m| m.per_spot.max(1)),
        q,
        at_least_one: mid.is_some_and(|m| m.at_least_one),
        out: DashPattern::default(),
        max,
        steps: 0,
    };
    let ends = section_ends(part, &cum, start, end);
    let mut run = || -> Result<(), Stop> {
        let (mut line_start, mut groups_start) = (start, start);
        for (i, &groups_end) in ends.iter().enumerate() {
            let is_part_end = i + 1 == ends.len();
            line_start =
                dasher.groups(line_start, groups_start, groups_end, i == 0, is_part_end)?;
            groups_start = groups_end;
        }
        Ok(())
    };
    let result = run();
    let steps = dasher.used();
    match result {
        Ok(()) => DashPattern {
            steps,
            ..dasher.out
        },
        Err(Stop::Full) => DashPattern {
            limited: true,
            steps,
            ..dasher.out
        },
        Err(Stop::Repeats) => solid(true, steps),
    }
}

/// Collects mid-symbol spots of a solid line up to a limit of units.
struct Spots<'a> {
    part: &'a Part,
    cum: &'a [f64],
    walker: Walker<'a>,
    out: Vec<(Pt, f64)>,
    max: usize,
    steps: usize,
    limited: bool,
}

impl Spots<'_> {
    fn used(&self) -> usize {
        self.steps + self.walker.checks() / POINTS_PER_UNIT
    }

    /// One unit of work; false (and limited) once `max` units are used.
    fn step(&mut self) -> bool {
        if self.used() >= self.max {
            self.limited = true;
            return false;
        }
        self.steps += 1;
        true
    }

    /// False once the limit is reached.
    fn place(&mut self, p: f64) -> bool {
        if !self.step() {
            return false;
        }
        let room = self.max.saturating_sub(self.used());
        self.walker.allow(check_room(room, self.walker.checks()));
        let spot = at_on(self.part, self.cum, &mut self.walker, p);
        if self.walker.exhausted() {
            self.limited = true;
            return false;
        }
        self.out.push(spot);
        true
    }

    fn finish(self) -> DashPattern {
        let steps = self.used();
        DashPattern {
            intervals: Vec::new(),
            spots: self.out,
            limited: self.limited,
            steps,
        }
    }
}

/// Positions and directions of the mid symbols of a solid line on [start, end] of the part
/// (reference §2.4), the pattern restarting at every dash point [LS
/// `createMidSymbolRenderables`]. Returned as a [`DashPattern`] without intervals: at most `max`
/// units of work (one per section and spot, and the tangent searches); `limited` when that
/// limit, or `MAX_REPEATS` spots in a section (no mid symbols then), changed the result.
pub fn mid_positions_in(part: &Part, start: f64, end: f64, mid: &Mid, max: usize) -> DashPattern {
    let cum = cumulative(&part.points);
    let (s, e, q) = (
        mid.segment / 1000.0,
        mid.end / 1000.0,
        mid.distance / 1000.0,
    );
    let m = mid.per_spot.max(1);
    let msl = f64::from(m - 1) * q;
    let mut spots = Spots {
        part,
        cum: &cum,
        walker: Walker::new(&part.points, part.closed),
        out: Vec::new(),
        max,
        steps: 0,
        limited: false,
    };
    let usable = s > 0.0 && s.is_finite() && e >= 0.0 && q >= 0.0 && msl.is_finite();
    if !usable || !start.is_finite() || !end.is_finite() || end <= start {
        return spots.finish();
    }
    // Spots per section, bounded: a broken definition gets no mid symbols.
    let bound = |n: f64| (n + 1.0) * f64::from(m) <= MAX_REPEATS as f64;
    let broken = |steps| DashPattern {
        limited: true,
        steps,
        ..DashPattern::default()
    };
    if e == 0.0 && !part.closed && !spots.place(start) {
        return spots.finish();
    }
    let mut a = start;
    for b in section_ends(part, &cum, start, end) {
        if !spots.step() {
            return spots.finish();
        }
        let l = b - a;
        let seg = (l - 2.0 * e).max(0.0) - msl;
        let raw = (seg / (s + msl)).max(if e == 0.0 { 1.0 } else { 0.0 });
        let (lo, hi) = (raw.floor(), raw.ceil());
        if e > 0.0 {
            if l <= msl {
                if mid.at_least_one && !(spots.place(a) && spots.place(b)) {
                    return spots.finish();
                }
            } else {
                let dlo = (l - lo * s - (lo + 1.0) * msl - 2.0 * e).abs();
                let dhi = (l - hi * s - (hi + 1.0) * msl - 2.0 * e).abs();
                let (n, dev) = if dlo >= dhi { (hi, -dhi) } else { (lo, dlo) };
                if !bound(n) {
                    return broken(spots.used());
                }
                let ideal = n * s + 2.0 * e;
                let e2 = e + dev * e / ideal;
                let s2 = s + dev * s / ideal;
                if s2 >= 0.0 && (mid.at_least_one || hi > 0.0 || l > 2.0 * e - (s + msl) / 2.0) {
                    for i in 0..=n as u32 {
                        for j in 0..m {
                            if !spots.place(a + e2 + f64::from(i) * (s2 + msl) + f64::from(j) * q) {
                                return spots.finish();
                            }
                        }
                    }
                }
            }
        } else {
            if l > msl {
                let dlo = (l - lo * s - (lo + 1.0) * msl).abs() / lo;
                let dhi = (l - hi * s - (hi + 1.0) * msl).abs() / hi;
                let n = if dlo > dhi { hi } else { lo };
                if !bound(n) {
                    return broken(spots.used());
                }
                let step = (l - (n + 1.0) * msl) / n + msl;
                if step >= msl {
                    'spots: for i in 0..=n as u32 {
                        for j in 0..m {
                            if i == 0 && j == 0 {
                                continue;
                            }
                            if i == n as u32 && j == m - 1 {
                                break 'spots;
                            }
                            if !spots.place(a + f64::from(i) * step + f64::from(j) * q) {
                                return spots.finish();
                            }
                        }
                    }
                }
            }
            if !spots.place(b) {
                return spots.finish();
            }
        }
        a = b;
    }
    spots.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{length, pieces};
    use crate::symbols::PointDef;

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

    /// The dashes of the whole part as polylines, and whether a limit changed them.
    fn dashes(part: &Part, dash: &Dash, max: usize) -> (Vec<Vec<Pt>>, bool) {
        let p = dash_pattern(part, 0.0, length(&part.points), dash, None, max);
        let polylines = pieces(part, &p.intervals).into_iter().map(|(d, _)| d);
        (polylines.collect(), p.limited)
    }

    /// The mid-symbol spots of a solid line over the whole part.
    fn mid_positions(part: &Part, mid: &Mid, max: usize) -> (Vec<(Pt, f64)>, bool) {
        let p = mid_positions_in(part, 0.0, length(&part.points), mid, max);
        (p.spots, p.limited)
    }

    fn dash_506() -> Dash {
        Dash {
            dash: 1500.0,
            gap: 370.0,
            group: 1,
            in_group_gap: 0.0,
            half_outer: false,
        }
    }

    #[test]
    fn dashes_are_stretched_to_whole_dashes() {
        let dashes = dashes(&line(&[(0.0, 0.0), (10.0, 0.0)]), &dash_506(), usize::MAX).0;
        assert_eq!(dashes.len(), 6);
        let lengths: Vec<f64> = dashes.iter().map(|d| length(d)).collect();
        let d = (10.0 - 5.0 * 0.37) / 6.0;
        assert!(lengths.iter().all(|&l| near(l, d)), "{lengths:?}");
        assert!(near(dashes[0][0].0, 0.0) && near(dashes[5].last().unwrap().0, 10.0));
        assert!(near(dashes[1][0].0, d + 0.37));
    }

    #[test]
    fn dashes_run_through_corners() {
        // 6 mm: three dashes of 1.753 mm; the second spans the corner at 3 mm.
        let dashes = dashes(
            &line(&[(0.0, 0.0), (3.0, 0.0), (3.0, 3.0)]),
            &dash_506(),
            usize::MAX,
        )
        .0;
        assert_eq!(dashes.len(), 3);
        assert!(dashes[1].contains(&(3.0, 0.0)), "{dashes:?}");
        let total: f64 = dashes.iter().map(|d| length(d)).sum();
        assert!(near(total + (dashes.len() - 1) as f64 * 0.37, 6.0));
    }

    #[test]
    fn short_dashed_line_is_solid() {
        let dashes = dashes(&line(&[(0.0, 0.0), (2.5, 0.0)]), &dash_506(), usize::MAX).0;
        assert_eq!(dashes, [vec![(0.0, 0.0), (2.5, 0.0)]]);
    }

    #[test]
    fn closed_dash_halves_meet() {
        let square = Part {
            points: vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0), (0.0, 0.0)],
            closed: true,
            dash_points: Vec::new(),
        };
        let dashes = dashes(&square, &dash_506(), usize::MAX).0;
        // The first and last half dashes form one dash across the closing point.
        let at_origin = |p: &Pt| p.0.abs() < 1e-9 && p.1.abs() < 1e-9;
        let joined = dashes.iter().find(|d| d.iter().any(at_origin)).unwrap();
        let first = joined.first().unwrap();
        let last = joined.last().unwrap();
        assert!(
            first.1 > 0.0 && first.0 == 0.0,
            "starts on the left edge: {joined:?}"
        );
        assert!(
            last.0 > 0.0 && last.1 == 0.0,
            "ends on the top edge: {joined:?}"
        );
        let total: f64 = dashes.iter().map(|d| length(d)).sum();
        assert!(near(total + dashes.len() as f64 * 0.37, 20.0), "{total}");
    }

    fn fence_mid() -> Mid {
        Mid {
            symbol: Box::new(PointDef {
                rotatable: true,
                dot: None,
                ring: None,
                elements: Vec::new(),
            }),
            segment: 3750.0,
            end: 2325.0,
            per_spot: 1,
            distance: 0.0,
            at_least_one: false,
            placement: MidPlacement::CentreOfDash,
        }
    }

    #[test]
    fn mid_symbols_fill_the_line() {
        // 20 mm: n·3.75 + 4.65 closest to 20 for n = 4 (19.65); scaled by 20/19.65.
        let spots = mid_positions(&line(&[(0.0, 0.0), (20.0, 0.0)]), &fence_mid(), usize::MAX).0;
        assert_eq!(spots.len(), 5);
        let f = 20.0 / 19.65;
        assert!(near(spots[0].0.0, 2.325 * f));
        assert!(near(spots[1].0.0 - spots[0].0.0, 3.75 * f));
        assert!(near(spots[4].0.0, 20.0 - 2.325 * f));
        assert!(spots.iter().all(|s| near(s.1, 0.0)));
    }

    #[test]
    fn short_fence_has_no_tick() {
        assert!(
            mid_positions(&line(&[(0.0, 0.0), (2.5, 0.0)]), &fence_mid(), usize::MAX)
                .0
                .is_empty()
        );
        let one = mid_positions(&line(&[(0.0, 0.0), (3.0, 0.0)]), &fence_mid(), usize::MAX).0;
        assert_eq!(one.len(), 1);
        assert!(near(one[0].0.0, 1.5));
    }

    #[test]
    fn absurd_patterns_are_bounded() {
        let long = line(&[(0.0, 0.0), (10_000.0, 0.0)]);
        let tiny = Dash {
            dash: 0.001,
            gap: 0.0,
            group: 1_000_000,
            in_group_gap: 0.0,
            half_outer: false,
        };
        let (pieces, limited) = dashes(&long, &tiny, usize::MAX);
        assert!(limited);
        assert!(pieces.len() <= MAX_REPEATS, "{}", pieces.len());
        let mut mid = fence_mid();
        mid.segment = 0.001;
        mid.end = 0.001;
        mid.per_spot = 1_000_000;
        assert!(mid_positions(&long, &mid, usize::MAX).0.len() <= MAX_REPEATS);
        let nan = Dash {
            dash: f64::NAN,
            ..dash_506()
        };
        assert_eq!(
            dashes(&long, &nan, usize::MAX),
            (vec![long.points.clone()], false),
            "solid"
        );
    }

    #[test]
    fn dashes_and_mid_symbols_stop_at_the_limit() {
        let long = line(&[(0.0, 0.0), (1_000.0, 0.0)]);
        // 100 units: one for the section, 99 dashes.
        let (pieces, limited) = dashes(&long, &dash_506(), 100);
        assert_eq!((pieces.len(), limited), (99, true));
        let (all, limited) = dashes(&long, &dash_506(), usize::MAX);
        assert!(all.len() > 100 && !limited);
        let (spots, limited) = mid_positions(&long, &fence_mid(), 10);
        assert_eq!((spots.len(), limited), (9, true));
    }

    #[test]
    fn huge_dash_groups_do_not_overflow() {
        let huge = Dash {
            group: u32::MAX,
            ..dash_506()
        };
        assert_eq!(
            dashes(&line(&[(0.0, 0.0), (10.0, 0.0)]), &huge, usize::MAX)
                .0
                .len(),
            1
        );
    }

    fn dot_mid(placement: MidPlacement) -> Mid {
        Mid {
            placement,
            ..fence_mid()
        }
    }

    /// x of every spot, rounded to 1 µm.
    fn xs(spots: &[(Pt, f64)]) -> Vec<f64> {
        spots
            .iter()
            .map(|s| (s.0.0 * 1000.0).round() / 1000.0)
            .collect()
    }

    fn rounded(v: impl IntoIterator<Item = f64>) -> Vec<f64> {
        v.into_iter()
            .map(|x| (x * 1000.0).round() / 1000.0)
            .collect()
    }

    #[test]
    fn mid_symbols_on_dashed_line() {
        // 10 mm of 506: six dashes of d = (10 − 5 · 0.37) / 6 (reference §2.2).
        let part = line(&[(0.0, 0.0), (10.0, 0.0)]);
        let d = (10.0 - 5.0 * 0.37) / 6.0;
        let on_dash = dash_pattern(
            &part,
            0.0,
            10.0,
            &dash_506(),
            Some(&dot_mid(MidPlacement::CentreOfDash)),
            usize::MAX,
        );
        assert_eq!(on_dash.intervals.len(), 6);
        let centres = rounded((0..6).map(|i| f64::from(i) * (d + 0.37) + d / 2.0));
        assert_eq!(xs(&on_dash.spots), centres);
        assert!(on_dash.spots.iter().all(|s| near(s.1, 0.0)));
        // A group of one dash: the group centre is the dash centre.
        let on_group = dash_pattern(
            &part,
            0.0,
            10.0,
            &dash_506(),
            Some(&dot_mid(MidPlacement::CentreOfDashGroup)),
            usize::MAX,
        );
        assert_eq!(xs(&on_group.spots), centres);
        let on_gap = dash_pattern(
            &part,
            0.0,
            10.0,
            &dash_506(),
            Some(&dot_mid(MidPlacement::CentreOfGap)),
            usize::MAX,
        );
        let gaps = rounded((0..5).map(|i| f64::from(i) * (d + 0.37) + d + 0.185));
        assert_eq!(xs(&on_gap.spots), gaps);
    }

    #[test]
    fn mid_symbols_in_dash_groups_and_short_lines() {
        // Groups of two 1 mm dashes, 0.2 mm apart, 0.5 mm between groups, on 10 mm: n = 4
        // groups (deviation 0.1 < 0.45), dashes stretched to (10 − 3 · 0.5 − 4 · 0.2) / 8.
        let part = line(&[(0.0, 0.0), (10.0, 0.0)]);
        let pairs = Dash {
            dash: 1000.0,
            gap: 500.0,
            group: 2,
            in_group_gap: 200.0,
            half_outer: false,
        };
        let p = dash_pattern(
            &part,
            0.0,
            10.0,
            &pairs,
            Some(&dot_mid(MidPlacement::CentreOfDashGroup)),
            usize::MAX,
        );
        assert_eq!(p.intervals.len(), 8);
        let d = (10.0 - 3.0 * 0.5 - 4.0 * 0.2) / 8.0;
        let group = 2.0 * d + 0.2;
        let centres = rounded((0..4).map(|i| f64::from(i) * (group + 0.5) + group / 2.0));
        assert_eq!(xs(&p.spots), centres);
        // Too short for dashes: drawn solid, with one mid symbol in its middle.
        let short = line(&[(0.0, 0.0), (2.5, 0.0)]);
        let p = dash_pattern(
            &short,
            0.0,
            2.5,
            &dash_506(),
            Some(&dot_mid(MidPlacement::CentreOfDash)),
            usize::MAX,
        );
        assert_eq!(p.intervals, [(0.0, 2.5)]);
        assert_eq!(xs(&p.spots), [1.25]);
    }

    #[test]
    fn closed_dashed_line_has_its_mid_symbol_at_the_closing_point() {
        let square = Part {
            points: vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0), (0.0, 0.0)],
            closed: true,
            dash_points: Vec::new(),
        };
        let p = dash_pattern(
            &square,
            0.0,
            20.0,
            &dash_506(),
            Some(&dot_mid(MidPlacement::CentreOfDash)),
            usize::MAX,
        );
        // Half dashes at both ends form one dash: one spot per joined dash, the last at the
        // closing point, none at the start.
        assert_eq!(p.spots.len(), p.intervals.len() - 1);
        let last = p.spots.last().unwrap().0;
        assert!(near(last.0, 0.0) && near(last.1, 0.0), "{last:?}");
        let pieces = pieces(&square, &p.intervals);
        assert_eq!(pieces.len(), p.intervals.len() - 1, "halves joined");
    }

    #[test]
    fn dashes_restart_at_dash_points() {
        // A dash point at 4 mm splits the pattern: [0, 4] and [4, 10] are stretched on their own,
        // and the half dashes on either side of the dash point form one dash through it.
        let mut part = line(&[(0.0, 0.0), (4.0, 0.0), (10.0, 0.0)]);
        part.dash_points = vec![1];
        let p = dash_pattern(&part, 0.0, 10.0, &dash_506(), None, usize::MAX);
        assert!(near(p.intervals[0].0, 0.0));
        assert!(near(p.intervals.last().unwrap().1, 10.0));
        let through = p
            .intervals
            .iter()
            .filter(|(a, b)| *a < 4.0 && *b > 4.0)
            .count();
        assert_eq!(through, 1, "{:?}", p.intervals);
        // First section: H = 1, n = 2 (deviation 0.065 < 0.935), so D = (4 − 0.37) · 2 / 3.
        let d1 = (4.0 - 0.37) * 2.0 / 3.0;
        assert!(near(p.intervals[0].1, d1), "{:?}", p.intervals);
        // Without the dash point, the 10 mm line has six whole dashes.
        part.dash_points.clear();
        assert_eq!(
            dash_pattern(&part, 0.0, 10.0, &dash_506(), None, usize::MAX)
                .intervals
                .len(),
            6
        );
    }

    #[test]
    fn mid_symbols_restart_at_dash_points() {
        // End length 0: a symbol at every section end; 3 mm segments in [0, 4] and [4, 10].
        let mut mid = fence_mid();
        mid.segment = 3000.0;
        mid.end = 0.0;
        let mut part = line(&[(0.0, 0.0), (4.0, 0.0), (10.0, 0.0)]);
        part.dash_points = vec![1];
        let spots = mid_positions(&part, &mid, usize::MAX).0;
        assert_eq!(xs(&spots), [0.0, 4.0, 7.0, 10.0]);
    }
}

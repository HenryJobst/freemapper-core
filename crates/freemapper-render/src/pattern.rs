//! Area fill patterns: parallel lines and point grids clipped to the area, as OOM v0.9.6 draws
//! them (`docs/render/oom-render-reference.md` §4.2–4.5) [AS `FillPattern::createRenderables`,
//! `createPointPatternLine`; PS `createRenderablesIf*Inside`].
//!
//! Paper millimetres with y down.

use std::collections::HashSet;
use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use crate::bounds::{extent, painted_bounds};
use crate::draw::{self, FillRule, Primitive, Shape, Sink, mm};
use crate::geometry::Pt;
use crate::symbols::{Cap, Clipping, Join, Pattern, PointDef, RenderSymbols, Symbol};
use freemapper_omap::PatternOrigin;

/// The rectangle a pattern fills (mm): the area's extent grown by the extent of what is drawn
/// on each line.
#[derive(Debug, Clone, Copy)]
struct Canvas {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

/// Even-odd test of `p` against closed `rings` (OOM's `QPainterPath::contains`, OddEvenFill).
fn inside(rings: &[Vec<Pt>], p: Pt) -> bool {
    let mut inside = false;
    for ring in rings {
        for (i, &a) in ring.iter().enumerate() {
            let b = ring[(i + 1) % ring.len()];
            if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                inside = !inside;
            }
        }
    }
    inside
}

/// At most `max_lines` pattern lines through `canvas` as (first, second) points, and the
/// along-line shift that the pattern origin adds (reference §4.3) [AS
/// `FillPattern::createRenderables<T>`]. Callers ask for one line more than they may draw, to
/// see whether the cap cut the pattern ([`capped_lines`]).
///
/// Lines are `n · (p − origin) = offset + k · spacing` with the normal `n = (sin r, cos r)`
/// (paper, y down); the cases and the direction from `first` to `second` follow OOM. Unlike OOM,
/// each line is computed from its index k instead of by accumulation, and the diagonal cases
/// derive both ends from one k (reference §4.3).
fn pattern_lines(
    c: Canvas,
    rotation: f64,
    spacing: f64,
    line_offset: f64,
    origin: Pt,
    max_lines: usize,
) -> (Vec<(Pt, Pt)>, f64) {
    let (sin, cos) = rotation.sin_cos();
    let mut delta_along = cos * origin.0 - sin * origin.1;
    let offset = line_offset + sin * origin.0 + cos * origin.1;
    let mut lines = Vec::new();
    if (rotation - FRAC_PI_2).abs() < 0.0001 {
        // Vertical lines, drawn downwards.
        delta_along = -delta_along;
        let first = offset + ((c.left - offset) / spacing).ceil() * spacing;
        for k in 0..max_lines {
            let x = first + k as f64 * spacing;
            if x >= c.right {
                break;
            }
            lines.push(((x, c.top), (x, c.bottom)));
        }
    } else if rotation.abs() < 0.0001 {
        // Horizontal lines, drawn eastwards.
        let first = offset + ((c.top - offset) / spacing).ceil() * spacing;
        for k in 0..max_lines {
            let y = first + k as f64 * spacing;
            if y >= c.bottom {
                break;
            }
            lines.push(((c.left, y), (c.right, y)));
        }
    } else {
        let (dist_x, dist_y) = (spacing / sin, spacing / cos);
        let tan = rotation.tan();
        if rotation < FRAC_PI_2 {
            // From the top edge (start) to the left edge (end), first line at the upper left.
            delta_along = -delta_along;
            let offset_x = offset / sin - c.top / tan;
            let offset_y = offset / cos - c.left * tan;
            // One k for both ends (OOM rounds them separately and can mix k and k + 1).
            let k0 = ((c.left - offset_x) / dist_x).ceil();
            for k in 0..max_lines {
                let k = k as f64;
                let mut start = (offset_x + (k0 + k) * dist_x, c.top);
                let mut end = (c.left, offset_y + (k0 + k) * dist_y);
                if start.0 > c.right {
                    start.1 += (start.0 - c.right) / dist_x * dist_y;
                    start.0 = c.right;
                }
                if end.1 > c.bottom {
                    end.0 += (end.1 - c.bottom) / dist_y * dist_x;
                    end.1 = c.bottom;
                }
                if start.1 > c.bottom {
                    break;
                }
                lines.push((start, end));
            }
        } else {
            // From the bottom edge (start) to the left edge (end), first line at the lower left.
            let offset_x = offset / sin - c.bottom / tan;
            let offset_y = offset / cos - c.left * tan;
            let k0 = ((c.left - offset_x) / dist_x).ceil();
            for k in 0..max_lines {
                let k = k as f64;
                let mut start = (offset_x + (k0 + k) * dist_x, c.bottom);
                let mut end = (c.left, offset_y + (k0 + k) * dist_y);
                if start.0 > c.right {
                    start.1 += (start.0 - c.right) / dist_x * dist_y;
                    start.0 = c.right;
                }
                if end.1 < c.top {
                    end.0 += (end.1 - c.top) / dist_y * dist_x;
                    end.1 = c.top;
                }
                if start.1 < c.top {
                    break;
                }
                lines.push((start, end));
            }
        }
    }
    (lines, delta_along)
}

/// The pattern lines through `canvas` that the budget left in `sink` allows; when there would be
/// more, the sink is marked cut.
fn capped_lines(
    c: Canvas,
    rotation: f64,
    spacing: f64,
    line_offset: f64,
    origin: Pt,
    sink: &mut Sink,
) -> (Vec<(Pt, Pt)>, f64) {
    let max = sink.left();
    let (mut lines, delta_along) = pattern_lines(
        c,
        rotation,
        spacing,
        line_offset,
        origin,
        max.saturating_add(1),
    );
    if lines.len() > max {
        lines.truncate(max);
        sink.mark_cut();
    }
    (lines, delta_along)
}

/// `shape` clipped to `rings`, or as it is for the `no_clipping` modes.
fn clip(clipping: Clipping, rings: &Arc<[Vec<Pt>]>, shape: Shape) -> Shape {
    match clipping {
        Clipping::Clip => Shape::Clipped {
            rings: Arc::clone(rings),
            inner: Box::new(shape),
        },
        _ => shape,
    }
}

/// The lines or point placements of one area pattern (reference §4.2–4.5) [AS
/// `FillPattern::createRenderables`, `createPointPatternLine`]. Every line and placement costs
/// at least one unit of the sink's budget; drawing stops when it is spent.
pub(crate) fn area_pattern(
    pattern: &Pattern,
    rings: &Arc<[Vec<Pt>]>,
    object: Option<&PatternOrigin>,
    symbols: &RenderSymbols,
    sink: &mut Sink,
    depth: u32,
) {
    // A spent budget: the pattern would only ask for units it cannot get.
    if sink.stop() {
        return;
    }
    let (angle, rotatable, spacing, offset, clipping) = match pattern {
        Pattern::Lines {
            angle_rad,
            rotatable,
            spacing,
            offset,
            clipping,
            ..
        }
        | Pattern::Points {
            angle_rad,
            rotatable,
            spacing,
            offset,
            clipping,
            ..
        } => (*angle_rad, *rotatable, *spacing, *offset, *clipping),
    };
    let Some((min, max)) = extent(rings.iter().flatten(), 0.0) else {
        return;
    };
    if spacing <= 0.0 {
        return;
    }
    // The object's pattern rotation and origin count only for a rotatable pattern.
    let (delta, origin) = match object {
        Some(o) if rotatable => (
            o.rotation_rad,
            (mm(f64::from(o.origin.x)), mm(f64::from(o.origin.y))),
        ),
        _ => (0.0, (0.0, 0.0)),
    };
    let mut rotation = (angle + delta) % PI;
    if rotation < 0.0 {
        rotation += PI;
    }
    // A broken object rotation; angles are checked when the symbols are read.
    if !rotation.is_finite() {
        return;
    }
    // The canvas grows by the extent (lo, hi) of what one line or point paints around its
    // centre: left − hi.x, top − hi.y, right − lo.x, bottom − lo.y.
    let canvas = |(lo, hi): (Pt, Pt)| Canvas {
        left: min.0 - hi.0,
        top: min.1 - hi.1,
        right: max.0 - lo.0,
        bottom: max.1 - lo.1,
    };
    match pattern {
        Pattern::Lines { color, width, .. } => {
            let width = mm(*width);
            // Such a line draws nothing (OOM: no colour or zero width).
            if *color < 0 || width <= 0.0 {
                return;
            }
            let m = width / 2.0;
            let (lines, _) = capped_lines(
                canvas(((-m, -m), (m, m))),
                rotation,
                mm(spacing),
                mm(offset),
                origin,
                sink,
            );
            // A clipped pattern shares one outline among its lines: one clip block, paid just
            // before the first line is pushed (as point patterns pay per shape).
            let mut outline = Outline::new(rings);
            for (first, second) in lines {
                if sink.stop() {
                    return;
                }
                if clipping == Clipping::Clip && !outline.pay(*color, sink) {
                    return;
                }
                let stroke = Shape::Stroke {
                    path: vec![first, second],
                    closed: false,
                    width,
                    cap: Cap::Flat,
                    join: Join::Miter,
                };
                sink.push(Primitive::new(*color, clip(clipping, rings, stroke)));
            }
        }
        Pattern::Points {
            offset_along,
            distance,
            symbol,
            ..
        } => {
            if *distance <= 0.0 {
                return;
            }
            // Extent of the point object at (0, 0) with the object's pattern rotation, drawn into
            // the same sink (its work counts against the same budget) and then taken out again.
            // A point symbol that draws nothing there draws nothing anywhere: OOM still walks the
            // placements, without output (reference §4.4).
            let probe_rotation = if symbol.rotatable { -delta } else { 0.0 };
            let saved = std::mem::take(&mut sink.out);
            draw::point_symbol(symbol, (0.0, 0.0), probe_rotation, symbols, sink, depth + 1);
            let probe = std::mem::replace(&mut sink.out, saved);
            let Some(point_extent) = painted_bounds(&probe) else {
                return;
            };
            let (lines, delta_along) = capped_lines(
                canvas(point_extent),
                rotation,
                mm(spacing),
                mm(offset),
                origin,
                sink,
            );
            let step = mm(*distance);
            let mut outline = Outline::new(rings);
            for (first, second) in lines {
                let length = (second.0 - first.0).hypot(second.1 - first.1);
                let dir = ((second.0 - first.0) / length, (second.1 - first.1) / length);
                let along = dir.0 * first.0 + dir.1 * first.1 - mm(*offset_along) - delta_along;
                let start = (along / step).ceil() * step - along;
                // A line without points still costs one unit.
                let len = sink.out.len();
                for j in 0.. {
                    let cur = start + f64::from(j) * step;
                    if cur >= length || !cur.is_finite() || sink.stop() {
                        break;
                    }
                    let at = (first.0 + dir.0 * cur, first.1 + dir.1 * cur);
                    let placed = sink.out.len();
                    place_point(
                        symbol,
                        at,
                        -delta,
                        clipping,
                        &mut outline,
                        symbols,
                        sink,
                        depth,
                    );
                    sink.settle(placed);
                }
                sink.settle(len);
                if sink.stop() {
                    return;
                }
            }
        }
    }
}

/// The area outline of one pattern (one call of [`area_pattern`]): its rings, shared by all
/// clipped shapes, their point count, and the colours whose clip block is paid for.
struct Outline<'a> {
    rings: &'a Arc<[Vec<Pt>]>,
    points: usize,
    paid: HashSet<i32>,
}

impl<'a> Outline<'a> {
    fn new(rings: &'a Arc<[Vec<Pt>]>) -> Self {
        Self {
            rings,
            points: rings.iter().map(Vec::len).sum(),
            paid: HashSet::new(),
        }
    }

    /// Pays for the clip block that holds this pattern's shapes of colour `priority`
    /// ([`Sink::charge_outline`]). All shapes of one pattern are drawn one after the other, so
    /// after the page is sorted by colour they form one run per colour, and the PDF writes the
    /// outline once per run. Another pattern on the same outline is another call and pays again:
    /// the shapes of an unclipped pattern between them split the run. False when the block does
    /// not fit.
    fn pay(&mut self, priority: i32, sink: &mut Sink) -> bool {
        if self.paid.contains(&priority) {
            return true;
        }
        if !sink.charge_outline(self.points) {
            return false;
        }
        self.paid.insert(priority);
        true
    }

    /// [`inside`], charged to `sink` by the outline's size ([`draw::POINTS_PER_UNIT`]).
    fn contains(&self, p: Pt, sink: &mut Sink) -> bool {
        sink.charge_points(self.points);
        inside(self.rings, p)
    }
}

/// One point of a point pattern at `at`, rotated by `rotation` whether or not the point symbol
/// is rotatable, clipped or filtered by `clipping` (reference §4.4–4.5) [AS
/// `createPointPatternLine`; PS `createRenderablesScaled`, `createRenderablesIf*Inside`].
#[allow(clippy::too_many_arguments)]
fn place_point(
    def: &PointDef,
    at: Pt,
    rotation: f64,
    clipping: Clipping,
    outline: &mut Outline,
    symbols: &RenderSymbols,
    sink: &mut Sink,
    depth: u32,
) {
    let to_mm = |c: &freemapper_omap::MapCoord| (mm(f64::from(c.x)), mm(f64::from(c.y)));
    match clipping {
        Clipping::Clip => {
            let len = sink.out.len();
            draw::point_symbol(def, at, rotation, symbols, sink, depth + 1);
            for i in len..sink.out.len() {
                // The shared outline ([`Outline::pay`]); what does not fit is dropped.
                if !outline.pay(sink.out[i].priority, sink) {
                    sink.out.truncate(i);
                    break;
                }
                let p = &mut sink.out[i];
                let shape = std::mem::replace(
                    &mut p.shape,
                    Shape::Fill {
                        rings: Vec::new(),
                        rule: FillRule::EvenOdd,
                    },
                );
                p.shape = clip(clipping, outline.rings, shape);
            }
        }
        Clipping::NoClippingIfCentreInside => {
            if outline.contains(at, sink) {
                sink.extend(draw::dot(def, at));
                sink.extend(draw::ring(def, at));
            }
            for element in &def.elements {
                let coords = draw::element_coords(element, at, rotation, sink);
                if coords.is_empty() {
                    continue;
                }
                let n = coords.len() as f64;
                let sum = coords
                    .iter()
                    .map(to_mm)
                    .fold((0.0, 0.0), |s, q| (s.0 + q.0, s.1 + q.1));
                if outline.contains((sum.0 / n, sum.1 / n), sink) {
                    draw::path_symbol(&element.symbol, &coords, None, symbols, sink, depth + 2);
                }
            }
        }
        Clipping::NoClippingIfCompletelyInside | Clipping::NoClippingIfPartlyInside => {
            let all = clipping == Clipping::NoClippingIfCompletelyInside;
            dot_and_ring_if_inside(def, at, all, outline, sink);
            for element in &def.elements {
                let coords = draw::element_coords(element, at, rotation, sink);
                // A point element: only its dot and ring, tested the same way.
                if let Symbol::Point(point) = &element.symbol {
                    if let Some(c) = coords.first() {
                        dot_and_ring_if_inside(point, to_mm(c), all, outline, sink);
                    }
                    continue;
                }
                let mut tests = coords.iter().map(|c| outline.contains(to_mm(c), sink));
                let hit = if all {
                    tests.all(|t| t)
                } else {
                    tests.any(|t| t)
                };
                if hit && !coords.is_empty() {
                    draw::path_symbol(&element.symbol, &coords, None, symbols, sink, depth + 2);
                }
            }
        }
    }
}

/// The dot and ring of `def` at `at` if all (`all`) or any of the four points at their radius
/// (left, top, right, bottom) lie inside `rings` [PS `createPrimitivesIfCompletelyInside`,
/// `createPrimitivesIfPartiallyInside`].
fn dot_and_ring_if_inside(def: &PointDef, at: Pt, all: bool, outline: &Outline, sink: &mut Sink) {
    let test = |r: f64, sink: &mut Sink| {
        let mut tests = [
            (at.0 - r, at.1),
            (at.0, at.1 - r),
            (at.0 + r, at.1),
            (at.0, at.1 + r),
        ]
        .into_iter()
        .map(|p| outline.contains(p, sink));
        if all {
            tests.all(|t| t)
        } else {
            tests.any(|t| t)
        }
    };
    if def.dot.is_some_and(|(_, radius)| test(mm(radius), sink)) {
        sink.extend(draw::dot(def, at));
    }
    if def
        .ring
        .is_some_and(|(_, radius, _)| test(mm(radius), sink))
    {
        sink.extend(draw::ring(def, at));
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6};

    use super::*;
    use crate::draw::{MAX_UNITS_PER_OBJECT, primitives_counted};
    use crate::symbols::{Element, LineDef};
    use freemapper_omap::{MapCoord, MapObject, MapObjectKind, SymbolSet, closed_ring};

    const PATTERNED: u32 = 9_999;

    /// The colours of ISSprOM 2019, parsed once (parsing takes tens of milliseconds).
    fn render_symbols() -> RenderSymbols {
        static SYMBOLS: std::sync::OnceLock<RenderSymbols> = std::sync::OnceLock::new();
        SYMBOLS
            .get_or_init(|| {
                RenderSymbols::from_set(&SymbolSet::issprom_2019().with_sheet_symbols()).unwrap()
            })
            .clone()
    }

    /// The primitives of a `side` µm square area with `pattern`, and whether the budget cut them.
    fn patterned(
        side: i32,
        pattern: Pattern,
        origin: Option<PatternOrigin>,
    ) -> (Vec<Primitive>, bool) {
        let mut r = render_symbols();
        r.symbols.insert(
            PATTERNED,
            Symbol::Area {
                color: -1,
                patterns: vec![pattern],
            },
        );
        let object = MapObject {
            symbol_id: PATTERNED,
            kind: MapObjectKind::Path,
            coords: closed_ring(&[(0, 0), (side, 0), (side, side), (0, side)]),
            tags: Vec::new(),
            pattern: origin,
        };
        primitives_counted(&object, &r)
    }

    fn lines(angle_rad: f64, rotatable: bool, offset: f64, clipping: Clipping) -> Pattern {
        Pattern::Lines {
            angle_rad,
            rotatable,
            spacing: 1_000.0,
            offset,
            color: 3,
            width: 100.0,
            clipping,
        }
    }

    fn dot(radius: f64) -> PointDef {
        PointDef {
            rotatable: false,
            dot: Some((3, radius)),
            ring: None,
            elements: Vec::new(),
        }
    }

    /// A line element from local (x0, 0) to (x1, 0) µm.
    fn bar(x0: i32, x1: i32) -> Element {
        Element {
            symbol: Symbol::Line(LineDef {
                color: 3,
                width: 50.0,
                cap: Cap::Flat,
                join: Join::Miter,
                dash: None,
                border: None,
                mid: None,
                start: None,
                end: None,
                pointed_caps: false,
                start_offset: 0.0,
                end_offset: 0.0,
                dash_symbol: None,
            }),
            coords: vec![MapCoord::new(x0, 0), MapCoord::new(x1, 0)],
        }
    }

    fn inner(p: &Primitive) -> &Shape {
        match &p.shape {
            Shape::Clipped { inner, .. } => inner,
            other => other,
        }
    }

    /// Centre of a dot drawn by `draw::dot` (its first ring point lies `radius` east of it).
    fn dot_centre(shape: &Shape, radius: f64) -> Option<Pt> {
        match shape {
            Shape::Fill { rings, .. } => Some((rings[0][0].0 - radius, rings[0][0].1)),
            _ => None,
        }
    }

    /// `v` is a whole multiple of `step` (up to float noise).
    fn on_grid(v: f64, step: f64) -> bool {
        (v / step - (v / step).round()).abs() < 1e-6
    }

    #[test]
    fn diagonal_lines_keep_spacing_and_cover_the_square() {
        // Corner projections on n = (sin r, cos r) of the canvas [−0.05, 10.05]²: k = 0..=14
        // for π/4 and k = −7..=7 for 3π/4.
        for (angle, ks) in [(FRAC_PI_4, 0..=14), (3.0 * FRAC_PI_4, -7..=7)] {
            let (p, cut) = patterned(10_000, lines(angle, false, 0.0, Clipping::Clip), None);
            assert!(!cut);
            let (sin, cos) = angle.sin_cos();
            let mut seen = Vec::new();
            for prim in &p {
                let Shape::Stroke { path, .. } = inner(prim) else {
                    panic!()
                };
                let c: Vec<f64> = path.iter().map(|q| sin * q.0 + cos * q.1).collect();
                assert!((c[0] - c[1]).abs() < 1e-9, "{angle}: {path:?}");
                assert!(on_grid(c[0], 1.0), "spacing 1 mm: {c:?}");
                // Counter-clockwise on paper: direction ± (cos r, −sin r).
                let d = (path[1].0 - path[0].0, path[1].1 - path[0].1);
                assert!((d.0 * sin + d.1 * cos).abs() < 1e-9);
                seen.push(c[0].round() as i32);
            }
            seen.sort_unstable();
            assert_eq!(seen, ks.collect::<Vec<_>>(), "{angle}");
        }
    }

    #[test]
    fn diagonal_corner_on_line_no_tilt() {
        // The canvas corner with the smallest n · p lies exactly on a pattern line: start and end
        // of every line must come from the same k.
        let mut tested = 0;
        for i in 1..314 {
            let angle = f64::from(i) / 100.0;
            if (angle - FRAC_PI_2).abs() < 0.01 {
                continue;
            }
            let (sin, cos) = angle.sin_cos();
            let corner = if angle < FRAC_PI_2 {
                (-0.05, -0.05)
            } else {
                (-0.05, 10.05)
            };
            let offset = 1_000.0 * (sin * corner.0 + cos * corner.1);
            let (p, _) = patterned(10_000, lines(angle, false, offset, Clipping::Clip), None);
            assert!(!p.is_empty());
            for prim in &p {
                let Shape::Stroke { path, .. } = inner(prim) else {
                    panic!()
                };
                let c: Vec<f64> = path.iter().map(|q| sin * q.0 + cos * q.1).collect();
                assert!((c[0] - c[1]).abs() < 1e-9, "tilted at {angle}: {path:?}");
            }
            tested += 1;
        }
        assert!(tested > 300);
    }

    /// The primitives of `pattern` in a 10 mm square drawn into a sink of `budget` units, and
    /// whether the sink was cut.
    fn with_budget(pattern: &Pattern, budget: usize) -> (usize, bool) {
        let rings: Arc<[Vec<Pt>]> = vec![vec![
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (0.0, 0.0),
        ]]
        .into();
        let mut sink = Sink::new(budget);
        area_pattern(pattern, &rings, None, &render_symbols(), &mut sink, 0);
        (sink.out.len(), sink.cut)
    }

    #[test]
    fn a_clip_outline_is_charged_by_its_points() {
        // A 10 mm square traced with 64 000 points: clipping a pattern to it costs at least
        // 64 000 / 8 units (kept points), also when the pattern has a single line.
        let n = 16_000;
        let side: Vec<Pt> = (0..n).map(|i| (10.0 * i as f64 / n as f64, 0.0)).collect();
        let mut ring = side.clone();
        ring.extend(side.iter().map(|&(x, _)| (10.0, x)));
        ring.extend(side.iter().map(|&(x, _)| (10.0 - x, 10.0)));
        ring.extend(side.iter().map(|&(x, _)| (0.0, 10.0 - x)));
        let rings: Arc<[Vec<Pt>]> = vec![ring].into();
        let one_line = |clipping| Pattern::Lines {
            angle_rad: 0.0,
            rotatable: false,
            spacing: 100_000.0,
            offset: 5_000.0,
            color: 3,
            width: 100.0,
            clipping,
        };
        let mut sink = Sink::new(MAX_UNITS_PER_OBJECT);
        area_pattern(
            &one_line(Clipping::Clip),
            &rings,
            None,
            &render_symbols(),
            &mut sink,
            0,
        );
        assert_eq!(sink.out.len(), 1);
        assert!(
            sink.spent() >= 4 * n / draw::KEPT_POINTS_PER_UNIT,
            "{}",
            sink.spent()
        );
        // Unclipped modes do not pay for it.
        let unclipped = one_line(Clipping::NoClippingIfCentreInside);
        let mut sink = Sink::new(MAX_UNITS_PER_OBJECT);
        area_pattern(&unclipped, &rings, None, &render_symbols(), &mut sink, 0);
        assert!(sink.spent() < 10, "{}", sink.spent());
    }

    #[test]
    fn clipped_pattern_without_lines_pays_nothing() {
        // The only line falls 50 mm from the 10 mm square: nothing is drawn, so the clip outline
        // is not paid for and nothing is cut, not even with a budget of a single unit.
        let pattern = Pattern::Lines {
            angle_rad: 0.0,
            rotatable: false,
            spacing: 100_000.0,
            offset: 50_000.0,
            color: 3,
            width: 100.0,
            clipping: Clipping::Clip,
        };
        let rings: Arc<[Vec<Pt>]> = vec![vec![
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (0.0, 0.0),
        ]]
        .into();
        let mut sink = Sink::new(1);
        area_pattern(&pattern, &rings, None, &render_symbols(), &mut sink, 0);
        assert_eq!(sink.out.len(), 0);
        assert!(!sink.cut);
        assert_eq!(sink.spent(), 0);
    }

    #[test]
    fn line_cap_marks_the_cut() {
        // 11 lines (y = 0..=10 mm) fill the square, and its outline of 5 points costs a unit.
        let pattern = lines(0.0, false, 0.0, Clipping::Clip);
        assert_eq!(with_budget(&pattern, 12), (11, false));
        assert_eq!(
            with_budget(&pattern, 11),
            (10, true),
            "the cap dropped a line"
        );
        assert_eq!(with_budget(&pattern, 2), (1, true));
        assert_eq!(
            with_budget(&pattern, 1),
            (0, true),
            "not even the outline fits"
        );
    }

    #[test]
    fn unclipped_lines_span_canvas() {
        let pattern = lines(0.0, false, 0.0, Clipping::NoClippingIfCentreInside);
        let (p, _) = patterned(10_000, pattern, None);
        assert_eq!(p.len(), 11);
        for prim in &p {
            let Shape::Stroke { path, .. } = &prim.shape else {
                panic!("unclipped: {prim:?}")
            };
            // Canvas = square grown by half the line width.
            assert!(
                (path[0].0 + 0.05).abs() < 1e-9 && (path[1].0 - 10.05).abs() < 1e-9,
                "{path:?}"
            );
        }
    }

    #[test]
    fn clipping_modes() {
        let r = render_symbols();
        let square: Arc<[Vec<Pt>]> =
            vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]].into();
        let draw = |def: &PointDef, at: Pt, clipping: Clipping| {
            let mut sink = Sink::new(MAX_UNITS_PER_OBJECT);
            let mut outline = Outline::new(&square);
            place_point(def, at, 0.0, clipping, &mut outline, &r, &mut sink, 0);
            sink.out
        };
        let dot = dot(500.0);
        // Dot radius 0.5 mm: straddling with the centre outside, centre inside but crossing the
        // edge, completely inside.
        let straddling = (-0.3, 5.0);
        let centre_inside = (0.3, 5.0);
        let completely_inside = (5.0, 5.0);
        let count = |at, clipping| draw(&dot, at, clipping).len();
        use Clipping::*;
        assert_eq!(
            [straddling, centre_inside, completely_inside].map(|at| count(at, Clip)),
            [1, 1, 1]
        );
        assert_eq!(
            [straddling, centre_inside, completely_inside]
                .map(|at| count(at, NoClippingIfCompletelyInside)),
            [0, 0, 1]
        );
        assert_eq!(
            [straddling, centre_inside, completely_inside]
                .map(|at| count(at, NoClippingIfCentreInside)),
            [0, 1, 1]
        );
        assert_eq!(
            [straddling, centre_inside, completely_inside]
                .map(|at| count(at, NoClippingIfPartlyInside)),
            [1, 1, 1]
        );
        assert!(matches!(
            draw(&dot, straddling, Clip)[0].shape,
            Shape::Clipped { .. }
        ));
        for mode in [
            NoClippingIfCompletelyInside,
            NoClippingIfCentreInside,
            NoClippingIfPartlyInside,
        ] {
            let p = draw(&dot, completely_inside, mode);
            assert!(matches!(p[0].shape, Shape::Fill { .. }), "{mode:?}");
        }
        // A bar element from x − 1 to x + 1 mm around x = 0.5: one end outside, its centre inside.
        let bar_point = PointDef {
            rotatable: false,
            dot: None,
            ring: None,
            elements: vec![bar(-1_000, 1_000)],
        };
        let bars = |clipping| draw(&bar_point, (0.5, 5.0), clipping).len();
        assert_eq!(
            [
                bars(NoClippingIfCompletelyInside),
                bars(NoClippingIfCentreInside),
                bars(NoClippingIfPartlyInside)
            ],
            [0, 1, 1]
        );
    }

    #[test]
    fn point_pattern_starts_at_pattern_origin() {
        let pattern = Pattern::Points {
            angle_rad: 0.0,
            rotatable: true,
            spacing: 2_000.0,
            offset: 0.0,
            offset_along: 0.0,
            distance: 2_000.0,
            symbol: Box::new(dot(100.0)),
            clipping: Clipping::Clip,
        };
        let origin = PatternOrigin {
            rotation_rad: 0.0,
            origin: MapCoord::new(500, 500),
        };
        let (p, _) = patterned(10_000, pattern, Some(origin));
        // Lines y = 0.5, 2.5, …, 8.5 and points x = 0.5, 2.5, …, 8.5.
        assert_eq!(p.len(), 25);
        for prim in &p {
            let (cx, cy) = dot_centre(inner(prim), 0.1).unwrap();
            assert!(
                on_grid(cx - 0.5, 2.0) && on_grid(cy - 0.5, 2.0),
                "({cx}, {cy})"
            );
        }
    }

    #[test]
    fn rotated_pattern_with_origin() {
        // Pattern angle 0.25, object rotation δ, origin o = (1.3, 0.7) mm, line offset 0.2 mm,
        // offset along 0.3 mm, spacing 2 mm, distance 1.5 mm. Reference §4.2–4.4: r = 0.25 + δ;
        // lines n · (p − o) = 0.2 + 2k with n = (sin r, cos r); points dir · (p − o) ≡ 0.3
        // (mod 1.5) with dir = s · t, t = (cos r, −sin r), s = −1 in the vertical and r < π/2
        // cases; point symbols turned by −δ.
        let o = (1.3, 0.7);
        for (delta, s) in [
            (FRAC_PI_2 - 0.25, -1.0), // vertical
            (FRAC_PI_6, -1.0),        // 0 < r < π/2
            (2.0 * PI / 3.0, 1.0),    // π/2 < r < π
            (-0.25, 1.0),             // horizontal
        ] {
            let r = 0.25 + delta;
            let (sin, cos) = r.sin_cos();
            let n = |p: Pt| sin * (p.0 - o.0) + cos * (p.1 - o.1);
            let t = |p: Pt| cos * (p.0 - o.0) - sin * (p.1 - o.1);
            let origin = Some(PatternOrigin {
                rotation_rad: delta,
                origin: MapCoord::new(1_300, 700),
            });
            let line_pattern = Pattern::Lines {
                angle_rad: 0.25,
                rotatable: true,
                spacing: 2_000.0,
                offset: 200.0,
                color: 3,
                width: 100.0,
                clipping: Clipping::Clip,
            };
            let (p, _) = patterned(10_000, line_pattern, origin);
            assert!(p.len() >= 5, "{delta}");
            for prim in &p {
                let Shape::Stroke { path, .. } = inner(prim) else {
                    panic!()
                };
                for q in path {
                    assert!(on_grid(n(*q) - 0.2, 2.0), "{delta}: {path:?}");
                }
            }
            let mut point = dot(100.0);
            point.elements.push(bar(0, 1_000));
            let point_pattern = Pattern::Points {
                angle_rad: 0.25,
                rotatable: true,
                spacing: 2_000.0,
                offset: 200.0,
                offset_along: 300.0,
                distance: 1_500.0,
                symbol: Box::new(point),
                clipping: Clipping::Clip,
            };
            let (p, _) = patterned(10_000, point_pattern, origin);
            let mut dots = 0;
            for prim in &p {
                match inner(prim) {
                    Shape::Fill { .. } => {
                        let c = dot_centre(inner(prim), 0.1).unwrap();
                        assert!(on_grid(n(c) - 0.2, 2.0), "{delta}: line of {c:?}");
                        assert!(on_grid(s * t(c) - 0.3, 1.5), "{delta}: along {c:?}");
                        dots += 1;
                    }
                    Shape::Stroke { path, .. } => {
                        // Local +x turned by −δ (y down): (cos δ, −sin δ), not by r.
                        let d = (path[1].0 - path[0].0, path[1].1 - path[0].1);
                        let len = d.0.hypot(d.1);
                        assert!(
                            (d.0 / len - delta.cos()).abs() < 2e-3
                                && (d.1 / len + delta.sin()).abs() < 2e-3,
                            "{delta}: {d:?}"
                        );
                    }
                    other => panic!("{other:?}"),
                }
            }
            assert!(dots >= 20, "{delta}: {dots}");
        }
    }

    #[test]
    fn budget_cuts_dense_pattern() {
        // 1 m × 1 m with points every 0.05 mm: 4 · 10⁸ placements without a budget.
        let point = PointDef {
            rotatable: false,
            dot: None,
            ring: None,
            elements: vec![bar(0, 20)],
        };
        let pattern = Pattern::Points {
            angle_rad: 0.0,
            rotatable: false,
            spacing: 50.0,
            offset: 0.0,
            offset_along: 0.0,
            distance: 50.0,
            symbol: Box::new(point),
            clipping: Clipping::Clip,
        };
        let (p, cut) = patterned(1_000_000, pattern, None);
        assert!(cut);
        assert!(p.len() <= MAX_UNITS_PER_OBJECT);
        assert!(p.len() > MAX_UNITS_PER_OBJECT / 2, "{}", p.len());
    }

    #[test]
    fn nested_patterns_share_the_budget() {
        // Each point of the outer pattern is a small area with its own dense line pattern.
        let inner_area = Element {
            symbol: Symbol::Area {
                color: -1,
                patterns: vec![Pattern::Lines {
                    angle_rad: 0.0,
                    rotatable: false,
                    spacing: 50.0,
                    offset: 0.0,
                    color: 3,
                    width: 20.0,
                    clipping: Clipping::Clip,
                }],
            },
            coords: closed_ring(&[(0, 0), (400, 0), (400, 400), (0, 400)]),
        };
        let pattern = Pattern::Points {
            angle_rad: 0.0,
            rotatable: false,
            spacing: 500.0,
            offset: 0.0,
            offset_along: 0.0,
            distance: 500.0,
            symbol: Box::new(PointDef {
                rotatable: false,
                dot: None,
                ring: None,
                elements: vec![inner_area],
            }),
            clipping: Clipping::Clip,
        };
        let (p, cut) = patterned(1_000_000, pattern, None);
        assert!(cut);
        assert!(p.len() <= MAX_UNITS_PER_OBJECT, "{}", p.len());
    }

    /// A pattern whose point is an area with a pattern whose point is an area with a huge thin
    /// outline and a mode-1 point pattern that rejects every placement: much work, few
    /// primitives, most of it in the extent probes.
    fn hostile_nesting() -> Pattern {
        let thin: Vec<(i32, i32)> = (0..=2_000)
            .map(|i| (i, 0))
            .chain((0..=2_000).rev().map(|i| (i, 100)))
            .collect();
        let a2 = Element {
            symbol: Symbol::Area {
                color: -1,
                patterns: vec![Pattern::Points {
                    angle_rad: 0.0,
                    rotatable: false,
                    spacing: 50.0,
                    offset: 0.0,
                    offset_along: 0.0,
                    distance: 50.0,
                    symbol: Box::new(dot(400.0)),
                    clipping: Clipping::NoClippingIfCompletelyInside,
                }],
            },
            coords: closed_ring(&thin),
        };
        let p2 = PointDef {
            rotatable: false,
            dot: Some((3, 100.0)),
            ring: None,
            elements: vec![a2],
        };
        let a1 = Element {
            symbol: Symbol::Area {
                color: -1,
                patterns: vec![Pattern::Points {
                    angle_rad: 0.0,
                    rotatable: false,
                    spacing: 1_000.0,
                    offset: 0.0,
                    offset_along: 0.0,
                    distance: 1_000.0,
                    symbol: Box::new(p2),
                    clipping: Clipping::Clip,
                }],
            },
            coords: closed_ring(&[(0, 0), (2_000, 0), (2_000, 2_000), (0, 2_000)]),
        };
        Pattern::Points {
            angle_rad: 0.0,
            rotatable: false,
            spacing: 2_000.0,
            offset: 0.0,
            offset_along: 0.0,
            distance: 2_000.0,
            symbol: Box::new(PointDef {
                rotatable: false,
                dot: Some((3, 100.0)),
                ring: None,
                elements: vec![a1],
            }),
            clipping: Clipping::Clip,
        }
    }

    #[test]
    fn outline_counts_once_per_colour_it_clips() {
        // One placement of a point whose 200 elements fill in 200 colours, clipped to an
        // outline of 20 000 points: the PDF writes the outline once per colour (the shapes of a
        // colour are painted together), so each colour pays for it.
        let elements = (0..200)
            .map(|color| Element {
                symbol: Symbol::Area {
                    color,
                    patterns: Vec::new(),
                },
                coords: closed_ring(&[(0, 0), (100, 0), (100, 100)]),
            })
            .collect();
        let pattern = Pattern::Points {
            angle_rad: 0.0,
            rotatable: false,
            spacing: 1_000_000.0,
            offset: 0.0,
            offset_along: 0.0,
            distance: 1_000_000.0,
            symbol: Box::new(PointDef {
                rotatable: false,
                dot: None,
                ring: None,
                elements,
            }),
            clipping: Clipping::Clip,
        };
        let mut r = render_symbols();
        r.symbols.insert(
            PATTERNED,
            Symbol::Area {
                color: -1,
                patterns: vec![pattern],
            },
        );
        let zigzag: Vec<(i32, i32)> = (0..20_000)
            .map(|i| (i % 1000 * 10, i / 1000 * 10))
            .collect();
        let object = MapObject {
            symbol_id: PATTERNED,
            kind: MapObjectKind::Path,
            coords: closed_ring(&zigzag),
            tags: Vec::new(),
            pattern: None,
        };
        // Unfiltered: most of these colours are not in the colour table.
        let before = draw::work_units();
        let mut sink = Sink::new(MAX_UNITS_PER_OBJECT);
        draw::draw_object(&object, &r, &mut sink);
        let work = draw::work_units() - before;
        // 2500 units per colour: about a hundred colours fit.
        assert!(sink.cut, "{} primitives", sink.out.len());
        assert!((90..=100).contains(&sink.out.len()), "{}", sink.out.len());
        assert!(
            work <= MAX_UNITS_PER_OBJECT + 2_500 + draw::BUDGET_SLACK,
            "{work}"
        );
    }

    #[test]
    fn interleaved_clipped_patterns_each_pay_for_the_outline() {
        // 40 line patterns of one colour on one outline of 2000 points, clipped and unclipped in
        // turn. After sorting by colour the unclipped lines split the clipped ones into 20 runs,
        // and the PDF writes the outline once per run, so each clipped pattern pays for it.
        let patterns = (0..40)
            .map(|i| Pattern::Lines {
                angle_rad: 0.0,
                rotatable: false,
                spacing: 1_000_000.0,
                offset: 0.0,
                color: 3,
                width: 100.0,
                clipping: if i % 2 == 0 {
                    Clipping::Clip
                } else {
                    Clipping::NoClippingIfPartlyInside
                },
            })
            .collect();
        let mut r = render_symbols();
        r.symbols.insert(
            PATTERNED,
            Symbol::Area {
                color: -1,
                patterns,
            },
        );
        let circle: Vec<(i32, i32)> = (0..2000)
            .map(|i| {
                let a = f64::from(i) / 2000.0 * std::f64::consts::TAU;
                (
                    (50_000.0 * a.cos()).round() as i32,
                    (50_000.0 * a.sin()).round() as i32,
                )
            })
            .collect();
        let object = MapObject {
            symbol_id: PATTERNED,
            kind: MapObjectKind::Path,
            coords: closed_ring(&circle),
            tags: Vec::new(),
            pattern: None,
        };
        let before = draw::work_units();
        let mut sink = Sink::new(MAX_UNITS_PER_OBJECT);
        draw::draw_object(&object, &r, &mut sink);
        let work = draw::work_units() - before;
        assert!(!sink.cut);
        assert_eq!(sink.out.len(), 40);
        assert!(work >= 20 * 2000 / draw::KEPT_POINTS_PER_UNIT, "{work}");
    }

    #[test]
    fn probe_work_counts_against_the_budget() {
        // The work of the extent probes and of rejected placements (outline tests, element
        // copies) counts against the one budget: the object is cut, quickly.
        let mut r = render_symbols();
        r.symbols.insert(
            PATTERNED,
            Symbol::Area {
                color: -1,
                patterns: vec![hostile_nesting()],
            },
        );
        let object = MapObject {
            symbol_id: PATTERNED,
            kind: MapObjectKind::Path,
            coords: closed_ring(&[(0, 0), (100_000, 0), (100_000, 100_000), (0, 100_000)]),
            tags: Vec::new(),
            pattern: None,
        };
        // Units asked for by every sink of this thread, the object's and any other.
        let before = draw::work_units();
        let mut sink = Sink::new(MAX_UNITS_PER_OBJECT);
        draw::draw_object(&object, &r, &mut sink);
        let work = draw::work_units() - before;
        assert!(sink.cut);
        assert_eq!(sink.left(), 0);
        assert!(sink.out.len() <= MAX_UNITS_PER_OBJECT);
        let bound = MAX_UNITS_PER_OBJECT + draw::BUDGET_SLACK;
        assert!(sink.spent() <= bound, "object's sink: {}", sink.spent());
        assert!(work <= bound, "all sinks: {work}");
        let (_, cut) = primitives_counted(&object, &r);
        assert!(cut);
    }
}

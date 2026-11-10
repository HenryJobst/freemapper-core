//! Which map objects a point on the page touches: the hit test of the browser editors (ADR 0021).
//!
//! It answers from the drawn page alone, in page mm (y down), with the same rules the backends paint by: the
//! clip rectangle, the masks and each shape's fill rule.

use crate::bounds::extent;
use crate::draw::{FillRule, Shape};
use crate::fonts::{ASCENT, DESCENT, face, text_width};
use crate::geometry::Pt;
use crate::page::{PaperBox, RenderedPage};

/// One map object under a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// The object's index among the objects of its page ([`crate::draw::Primitive::object`]).
    pub object: u32,
    /// The id of the object's symbol ([`crate::draw::Primitive::symbol`]).
    pub symbol: u32,
    /// The colour priority of the topmost primitive of the object under the point.
    pub priority: i32,
}

impl RenderedPage {
    /// The map objects under `p` (page mm), topmost first: the object whose primitive is painted last comes
    /// first, and each object comes once. A stroke counts within `tolerance_mm` of its painted edge; a fill, a
    /// text and a pattern clip count only where they paint. A negative or non-finite tolerance counts as 0.
    ///
    /// Only map primitives are tested, never the course overprint or the sheet layer. A point outside
    /// [`RenderedPage::clip`] or hidden by a mask ([`RenderedPage::map_clips`]) has no hits. Primitives of the base
    /// area ([`crate::draw::Primitive::base`]) and of north lines are left out: both cover the whole map, so
    /// they would lie under every point, and neither is detail a user picks on the map.
    pub fn hits_at(&self, p: Pt, tolerance_mm: f64) -> Vec<Hit> {
        let tolerance = if tolerance_mm.is_finite() {
            tolerance_mm.max(0.0)
        } else {
            0.0
        };
        if !p.0.is_finite() || !p.1.is_finite() || !inside_box(p, self.clip) {
            return Vec::new();
        }
        if self
            .map_clips
            .iter()
            .any(|mask| winding(mask.rings(), p) == 0)
        {
            return Vec::new();
        }
        let mut hits: Vec<Hit> = Vec::new();
        for primitive in self.primitives.iter().rev() {
            if primitive.base
                || primitive.north_line
                || hits.iter().any(|h| h.object == primitive.object)
            {
                continue;
            }
            if touches(&primitive.shape, p, tolerance) {
                hits.push(Hit {
                    object: primitive.object,
                    symbol: primitive.symbol,
                    priority: primitive.priority,
                });
            }
        }
        hits
    }
}

fn inside_box(p: Pt, b: PaperBox) -> bool {
    b.min.0 <= p.0 && p.0 <= b.max.0 && b.min.1 <= p.1 && p.1 <= b.max.1
}

/// True when `shape` paints at `p`, a stroke also within `tolerance` of its edge.
fn touches(shape: &Shape, p: Pt, tolerance: f64) -> bool {
    match shape {
        Shape::Fill { rings, rule } => inside(rings, *rule, p),
        Shape::Stroke {
            path,
            closed,
            width,
            ..
        } => near_polyline(path, *closed, p, width / 2.0 + tolerance),
        Shape::Text {
            origin,
            size,
            bold,
            italic,
            text,
            rotation,
            stroke,
        } => {
            // The point in text coordinates (x along the baseline, y down), the inverse of the turn in
            // `bounds::shape_bounds`.
            let (sin, cos) = rotation.sin_cos();
            let d = (p.0 - origin.0, p.1 - origin.1);
            let (x, y) = (d.0 * cos - d.1 * sin, d.0 * sin + d.1 * cos);
            let grow = stroke.unwrap_or(0.0) / 2.0;
            let width = text_width(text, face(*bold, *italic)) * size;
            -grow <= x
                && x <= width + grow
                && -ASCENT * size - grow <= y
                && y <= DESCENT * size + grow
        }
        Shape::Clipped { rings, inner } => {
            inside(rings, FillRule::EvenOdd, p) && touches(inner, p, tolerance)
        }
    }
}

/// True when `p` lies inside `rings` by `rule`. Each ring is closed from its last point to its first.
fn inside(rings: &[Vec<Pt>], rule: FillRule, p: Pt) -> bool {
    // Rings whose box misses the point cross no ray from it.
    if !extent(rings.iter().flatten(), 0.0)
        .is_some_and(|(min, max)| min.0 <= p.0 && p.0 <= max.0 && min.1 <= p.1 && p.1 <= max.1)
    {
        return false;
    }
    match rule {
        FillRule::NonZero => winding(rings, p) != 0,
        FillRule::EvenOdd => crossings(rings, p) % 2 == 1,
    }
}

/// The edges of `rings`, each ring closed.
fn edges(rings: &[Vec<Pt>]) -> impl Iterator<Item = (Pt, Pt)> + '_ {
    rings
        .iter()
        .filter(|r| !r.is_empty())
        .flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
}

/// How often the edges cross the ray from `p` to the right.
fn crossings(rings: &[Vec<Pt>], p: Pt) -> usize {
    edges(rings)
        .filter(|&(a, b)| {
            (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0)
        })
        .count()
}

/// The winding number of `rings` around `p`: upward crossings of the ray to the right minus downward ones.
fn winding(rings: &[Vec<Pt>], p: Pt) -> i64 {
    // Which side of the line a→b the point lies on: positive to the left (y down: as seen on paper,
    // to the right).
    let side = |a: Pt, b: Pt| (b.0 - a.0) * (p.1 - a.1) - (p.0 - a.0) * (b.1 - a.1);
    edges(rings)
        .map(|(a, b)| {
            if a.1 <= p.1 {
                if b.1 > p.1 && side(a, b) > 0.0 {
                    return 1;
                }
            } else if b.1 <= p.1 && side(a, b) < 0.0 {
                return -1;
            }
            0
        })
        .sum()
}

/// True when `p` lies within `reach` of the polyline `path` (closed back to its first point when `closed`).
fn near_polyline(path: &[Pt], closed: bool, p: Pt, reach: f64) -> bool {
    let reach2 = reach * reach;
    match path {
        [] => false,
        [only] => dist2(*only, p) <= reach2,
        _ => {
            let closing = closed.then(|| (path[path.len() - 1], path[0]));
            path.windows(2)
                .map(|w| (w[0], w[1]))
                .chain(closing)
                .any(|(a, b)| segment_dist2(a, b, p) <= reach2)
        }
    }
}

fn dist2(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

/// The squared distance from `p` to the segment a–b.
fn segment_dist2(a: Pt, b: Pt, p: Pt) -> f64 {
    let d = (b.0 - a.0, b.1 - a.1);
    let len2 = d.0 * d.0 + d.1 * d.1;
    if len2 == 0.0 {
        return dist2(a, p);
    }
    let t = (((p.0 - a.0) * d.0 + (p.1 - a.1) * d.1) / len2).clamp(0.0, 1.0);
    dist2((a.0 + t * d.0, a.1 + t * d.1), p)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::draw::Primitive;
    use crate::page::{MapClip, Overprint};
    use crate::symbols::{Cap, Join};

    fn square(min: Pt, max: Pt) -> Vec<Pt> {
        vec![min, (max.0, min.1), max, (min.0, max.1)]
    }

    fn primitive(object: u32, priority: i32, shape: Shape) -> Primitive {
        Primitive {
            object,
            symbol: 100 + object,
            ..Primitive::new(priority, shape)
        }
    }

    fn page(primitives: Vec<Primitive>) -> RenderedPage {
        RenderedPage {
            primitives,
            clip: PaperBox {
                min: (10.0, 10.0),
                max: (90.0, 90.0),
            },
            page_mm: (100.0, 100.0),
            centre_mm: (0.0, 0.0),
            colours: Vec::new(),
            sheet: Vec::new(),
            overprint: Overprint::default(),
            map_clips: Vec::new(),
        }
    }

    /// An area of 20..60 with a hole of 30..40, by `rule`.
    fn area_with_hole(object: u32, rule: FillRule) -> Primitive {
        primitive(
            object,
            5,
            Shape::Fill {
                rings: vec![
                    square((20.0, 20.0), (60.0, 60.0)),
                    square((30.0, 30.0), (40.0, 40.0)),
                ],
                rule,
            },
        )
    }

    fn line(object: u32, priority: i32, width: f64) -> Primitive {
        primitive(
            object,
            priority,
            Shape::Stroke {
                path: vec![(20.0, 70.0), (60.0, 70.0)],
                closed: false,
                width,
                cap: Cap::Flat,
                join: Join::Miter,
            },
        )
    }

    fn objects(hits: &[Hit]) -> Vec<u32> {
        hits.iter().map(|h| h.object).collect()
    }

    #[test]
    fn a_point_in_an_area_hits_it() {
        let page = page(vec![area_with_hole(3, FillRule::EvenOdd)]);
        assert_eq!(
            page.hits_at((25.0, 25.0), 0.0),
            vec![Hit {
                object: 3,
                symbol: 103,
                priority: 5
            }]
        );
        assert!(page.hits_at((70.0, 25.0), 0.0).is_empty());
    }

    #[test]
    fn a_point_in_a_hole_misses_the_area() {
        let page = page(vec![area_with_hole(3, FillRule::EvenOdd)]);
        assert!(page.hits_at((35.0, 35.0), 1.0).is_empty());
    }

    #[test]
    fn the_fill_rule_decides_about_a_hole() {
        // Both rings run the same way, so the nonzero rule fills the hole.
        let page = page(vec![area_with_hole(3, FillRule::NonZero)]);
        assert_eq!(objects(&page.hits_at((35.0, 35.0), 0.0)), [3]);
    }

    #[test]
    fn a_line_counts_within_half_its_width_and_the_tolerance() {
        let page = page(vec![line(1, 2, 1.0)]);
        assert_eq!(objects(&page.hits_at((40.0, 70.4), 0.0)), [1]);
        assert!(page.hits_at((40.0, 71.0), 0.0).is_empty());
        assert_eq!(objects(&page.hits_at((40.0, 71.0), 0.6)), [1]);
        assert!(page.hits_at((40.0, 71.2), 0.6).is_empty());
        // Beyond the end of the line.
        assert_eq!(objects(&page.hits_at((60.5, 70.0), 0.0)), [1]);
        assert!(page.hits_at((61.5, 70.0), 0.6).is_empty());
    }

    #[test]
    fn tolerance_zero_needs_the_painted_shape() {
        let page = page(vec![line(1, 2, 1.0), area_with_hole(2, FillRule::EvenOdd)]);
        assert_eq!(objects(&page.hits_at((40.0, 70.5), 0.0)), [1]);
        assert!(page.hits_at((40.0, 70.6), 0.0).is_empty());
        // A negative or NaN tolerance counts as zero.
        assert!(page.hits_at((40.0, 70.6), -5.0).is_empty());
        assert!(page.hits_at((40.0, 70.6), f64::NAN).is_empty());
        // Tolerance widens strokes only.
        assert!(page.hits_at((61.0, 40.0), 0.0).is_empty());
        assert!(page.hits_at((61.0, 40.0), 2.0).is_empty());
    }

    #[test]
    fn a_point_outside_the_clip_has_no_hits() {
        let mut page = page(vec![primitive(
            1,
            5,
            Shape::Fill {
                rings: vec![square((0.0, 0.0), (100.0, 100.0))],
                rule: FillRule::EvenOdd,
            },
        )]);
        assert_eq!(objects(&page.hits_at((50.0, 50.0), 0.0)), [1]);
        assert!(page.hits_at((5.0, 50.0), 0.0).is_empty());
        assert!(page.hits_at((50.0, 95.0), 10.0).is_empty());
        assert!(page.hits_at((f64::NAN, 50.0), 0.0).is_empty());
        page.primitives.clear();
        assert!(page.hits_at((50.0, 50.0), 0.0).is_empty());
    }

    #[test]
    fn a_point_under_a_mask_has_no_hits() {
        let mut page = page(vec![area_with_hole(1, FillRule::EvenOdd)]);
        page.map_clips = vec![MapClip::new(
            page.clip,
            [square((20.0, 20.0), (30.0, 30.0))],
            [square((22.0, 22.0), (24.0, 24.0))],
        )];
        assert!(page.hits_at((26.0, 26.0), 0.0).is_empty());
        // A window in the mask shows the map again; outside the mask the map is visible.
        assert_eq!(objects(&page.hits_at((23.0, 23.0), 0.0)), [1]);
        assert_eq!(objects(&page.hits_at((50.0, 50.0), 0.0)), [1]);
        // A second mask hides its own area too.
        page.map_clips.push(MapClip::new(
            page.clip,
            [square((45.0, 45.0), (55.0, 55.0))],
            [],
        ));
        assert!(page.hits_at((50.0, 50.0), 0.0).is_empty());
        assert_eq!(objects(&page.hits_at((23.0, 23.0), 0.0)), [1]);
    }

    #[test]
    fn the_topmost_object_comes_first_and_each_object_once() {
        // Paint order: object 1's area, object 2's line, object 1's border on top.
        let page = page(vec![
            area_with_hole(1, FillRule::EvenOdd),
            line(2, 3, 50.0),
            primitive(
                1,
                0,
                Shape::Stroke {
                    path: square((20.0, 20.0), (60.0, 60.0)),
                    closed: true,
                    width: 0.5,
                    cap: Cap::Flat,
                    join: Join::Miter,
                },
            ),
        ]);
        let hits = page.hits_at((20.0, 50.0), 0.0);
        assert_eq!(
            hits,
            vec![
                Hit {
                    object: 1,
                    symbol: 101,
                    priority: 0
                },
                Hit {
                    object: 2,
                    symbol: 102,
                    priority: 3
                },
            ]
        );
        // Away from the border the area lies under the line.
        assert_eq!(objects(&page.hits_at((50.0, 50.0), 0.0)), [2, 1]);
    }

    #[test]
    fn base_area_and_north_lines_are_left_out() {
        let mut base = primitive(
            1,
            9,
            Shape::Fill {
                rings: vec![square((0.0, 0.0), (100.0, 100.0))],
                rule: FillRule::EvenOdd,
            },
        );
        base.base = true;
        let mut north = line(2, 0, 50.0);
        north.north_line = true;
        let page = page(vec![base, north]);
        assert!(page.hits_at((40.0, 70.0), 0.0).is_empty());
    }

    #[test]
    fn a_text_counts_within_its_box() {
        let text = |rotation: f64| {
            page(vec![primitive(
                4,
                1,
                Shape::Text {
                    origin: (30.0, 50.0),
                    size: 4.0,
                    bold: false,
                    italic: false,
                    text: "Wald".into(),
                    rotation,
                    stroke: None,
                },
            )])
        };
        let width = text_width("Wald", face(false, false)) * 4.0;
        let flat = text(0.0);
        assert_eq!(objects(&flat.hits_at((31.0, 49.0), 0.0)), [4]);
        assert!(flat.hits_at((31.0, 45.0), 0.0).is_empty());
        assert!(flat.hits_at((30.0 + width + 0.1, 49.0), 0.0).is_empty());
        // A quarter turn counter-clockwise: the text runs up the page.
        let turned = text(std::f64::consts::FRAC_PI_2);
        assert_eq!(objects(&turned.hits_at((29.0, 49.0), 0.0)), [4]);
        assert!(turned.hits_at((31.0, 49.0), 0.0).is_empty());
    }

    #[test]
    fn a_clipped_shape_counts_inside_its_clip_only() {
        let rings: Arc<[Vec<Pt>]> = vec![square((20.0, 20.0), (40.0, 40.0))].into();
        let page = page(vec![primitive(
            1,
            1,
            Shape::Clipped {
                rings,
                inner: Box::new(Shape::Stroke {
                    path: vec![(10.0, 30.0), (80.0, 30.0)],
                    closed: false,
                    width: 1.0,
                    cap: Cap::Flat,
                    join: Join::Miter,
                }),
            },
        )]);
        assert_eq!(objects(&page.hits_at((30.0, 30.0), 0.0)), [1]);
        assert!(page.hits_at((50.0, 30.0), 0.0).is_empty());
        assert!(page.hits_at((30.0, 35.0), 0.0).is_empty());
    }
}

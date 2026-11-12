//! Where primitives paint: axis-parallel bounding boxes on paper (mm, y down). Used for the extent
//! of a point symbol in a pattern and for the part of a page the map covers.

use std::f64::consts::SQRT_2;

use crate::draw::{Primitive, Shape};
use crate::fonts::{ASCENT, DESCENT, face, glyph_extent, text_width};
use crate::geometry::Pt;
use crate::page::PaperBox;
use crate::pdf::{MITER_LIMIT, TEXT_FRAMING_MITER_LIMIT};
use crate::symbols::{Cap, Join};

/// A bounding box: lower-left and upper-right corner (mm).
pub type Bounds = (Pt, Pt);

/// The box around `points` (mm), grown by `extra` on every side.
pub(crate) fn extent<'a>(points: impl IntoIterator<Item = &'a Pt>, extra: f64) -> Option<Bounds> {
    let mut points = points.into_iter();
    let first = *points.next()?;
    let (min, max) = points.fold((first, first), |(min, max), q| {
        (
            (min.0.min(q.0), min.1.min(q.1)),
            (max.0.max(q.0), max.1.max(q.1)),
        )
    });
    Some((
        (min.0 - extra, min.1 - extra),
        (max.0 + extra, max.1 + extra),
    ))
}

fn union(a: Bounds, b: Bounds) -> Bounds {
    (
        (a.0.0.min(b.0.0), a.0.1.min(b.0.1)),
        (a.1.0.max(b.1.0), a.1.1.max(b.1.1)),
    )
}

fn intersection(a: Bounds, b: Bounds) -> Option<Bounds> {
    let min = (a.0.0.max(b.0.0), a.0.1.max(b.0.1));
    let max = (a.1.0.min(b.1.0), a.1.1.min(b.1.1));
    (min.0 <= max.0 && min.1 <= max.1).then_some((min, max))
}

/// The box a shape paints in. A stroke grows by half its width on every side (more than a flat
/// cap paints, never less); a text spans its advance width and the font's ascent and descent,
/// turned with it, plus half a framing stroke; a clipped shape paints only where its clip and its
/// inner shape overlap.
pub fn shape_bounds(shape: &Shape) -> Option<Bounds> {
    match shape {
        Shape::Fill { rings, .. } => extent(rings.iter().flatten(), 0.0),
        Shape::Stroke { path, width, .. } => extent(path, width / 2.0),
        Shape::Text {
            origin,
            size,
            bold,
            italic,
            text,
            rotation,
            stroke,
        } => {
            let width = text_width(text, face(*bold, *italic)) * size;
            let (sin, cos) = rotation.sin_cos();
            // Text coordinates (x, y down) turned counter-clockwise on paper (reference §6).
            let corners = [
                (0.0, -ASCENT * size),
                (width, -ASCENT * size),
                (width, DESCENT * size),
                (0.0, DESCENT * size),
            ]
            .map(|(x, y)| (origin.0 + x * cos + y * sin, origin.1 - x * sin + y * cos));
            extent(&corners, stroke.unwrap_or(0.0) / 2.0)
        }
        Shape::Clipped { rings, inner } => {
            intersection(extent(rings.iter().flatten(), 0.0)?, shape_bounds(inner)?)
        }
    }
}

/// How far (mm) past the exact outline of a shape a rasteriser may still colour a pixel. An anti-aliased clip
/// tints each pixel the frame edge runs through with every shape that covers part of that pixel, also with a shape
/// that lies wholly outside the frame: poppler, Ghostscript and Cairo all do so. Stroke adjustment, the thinnest
/// line a device draws and glyph hinting can move an edge by one more pixel. Five millimetres are two pixels at
/// 10 dpi, where an A4 page is a thumbnail 117 pixels tall (ADR 0031).
pub(crate) const INK_SLACK_MM: f64 = 5.0;

/// The box `shape` can put ink in on paper (mm, y down), grown by [`INK_SLACK_MM`] on every side; `None` when it
/// paints nothing anywhere. Unlike [`shape_bounds`], a stroke reaches as far as a miter tip or the corner of a
/// square cap, and a text spans the outlines of its glyphs, not its advance width and line height. A shape with a
/// number that is not finite gets an unbounded box (ADR 0031).
pub(crate) fn ink_bounds(shape: &Shape) -> Option<Bounds> {
    match shape {
        Shape::Fill { rings, .. } => reach(rings.iter().flatten(), 0.0),
        Shape::Stroke {
            path,
            width,
            cap,
            join,
            ..
        } => {
            // The corner of a square cap lies √2 half widths from the end of the line, a miter tip at most the
            // miter limit (both backends write MITER_LIMIT) half widths from its corner; flat and round caps,
            // round and bevel joins stay within one half width.
            let cap = if *cap == Cap::Square { SQRT_2 } else { 1.0 };
            let join = if *join == Join::Miter {
                f64::from(MITER_LIMIT)
            } else {
                1.0
            };
            reach(path, width.abs() / 2.0 * cap.max(join))
        }
        Shape::Text {
            origin,
            size,
            bold,
            italic,
            text,
            rotation,
            stroke,
        } => {
            let ((x0, y0), (x1, y1)) = glyph_extent(text, face(*bold, *italic))?;
            let (sin, cos) = rotation.sin_cos();
            // Text coordinates (em, x along the baseline, y down) turned counter-clockwise on paper, as in
            // `shape_bounds`.
            let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| {
                let (x, y) = (x * size, y * size);
                (origin.0 + x * cos + y * sin, origin.1 - x * sin + y * cos)
            });
            // Line framing strokes the glyph outlines with miter joins up to the framing limit.
            let framing =
                stroke.map_or(0.0, |w| w.abs() / 2.0 * f64::from(TEXT_FRAMING_MITER_LIMIT));
            reach(&corners, framing)
        }
        // The clip and the inner shape both carry the slack, so a shape that touches its clip only within the
        // slack still counts.
        Shape::Clipped { rings, inner } => {
            intersection(reach(rings.iter().flatten(), 0.0)?, ink_bounds(inner)?)
        }
    }
}

/// A box that holds everything: the ink box of a shape with a number that is not finite.
const UNBOUNDED: Bounds = (
    (f64::NEG_INFINITY, f64::NEG_INFINITY),
    (f64::INFINITY, f64::INFINITY),
);

/// The box around `points` grown by `extra` and [`INK_SLACK_MM`]; [`UNBOUNDED`] when a number is not finite,
/// `None` without points.
fn reach<'a>(points: impl IntoIterator<Item = &'a Pt>, extra: f64) -> Option<Bounds> {
    let mut finite = extra.is_finite();
    let points = points
        .into_iter()
        .inspect(|p| finite &= p.0.is_finite() && p.1.is_finite());
    // `extent` folds over every point, so `finite` has seen them all when it returns.
    let bounds = extent(points, extra + INK_SLACK_MM)?;
    Some(if finite { bounds } else { UNBOUNDED })
}

/// Whether `shape` may put ink in `area` ([`ink_bounds`]). A box that touches `area` counts.
pub(crate) fn may_show(shape: &Shape, area: PaperBox) -> bool {
    ink_bounds(shape).is_some_and(|(min, max)| {
        min.0 <= area.max.0 && area.min.0 <= max.0 && min.1 <= area.max.1 && area.min.1 <= max.1
    })
}

/// The box all of `primitives` paint in, `None` when they paint nothing. The base area of a sprint map
/// ([`Primitive::base`]) does not count: it is the map plus a margin, not the map.
pub fn painted_bounds<'a>(primitives: impl IntoIterator<Item = &'a Primitive>) -> Option<Bounds> {
    primitives
        .into_iter()
        .filter(|p| !p.base)
        .filter_map(|p| shape_bounds(&p.shape))
        .reduce(union)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::draw::FillRule;
    use crate::symbols::{Cap, Join};

    fn close(a: Bounds, b: Bounds) -> bool {
        [a.0.0 - b.0.0, a.0.1 - b.0.1, a.1.0 - b.1.0, a.1.1 - b.1.1]
            .iter()
            .all(|d| d.abs() < 1e-9)
    }

    #[test]
    fn stroke_grows_by_half_its_width() {
        let s = Shape::Stroke {
            path: vec![(0.0, 0.0), (10.0, 0.0)],
            closed: false,
            width: 1.0,
            cap: Cap::Flat,
            join: Join::Miter,
        };
        assert!(close(
            shape_bounds(&s).unwrap(),
            ((-0.5, -0.5), (10.5, 0.5))
        ));
    }

    #[test]
    fn text_spans_ascent_descent_and_width() {
        let text = |rotation: f64| Shape::Text {
            origin: (10.0, 20.0),
            size: 4.0,
            bold: false,
            italic: false,
            text: "Wald".into(),
            rotation,
            stroke: None,
        };
        let w = text_width("Wald", face(false, false)) * 4.0;
        let b = shape_bounds(&text(0.0)).unwrap();
        assert!(
            close(
                b,
                (
                    (10.0, 20.0 - ASCENT * 4.0),
                    (10.0 + w, 20.0 + DESCENT * 4.0)
                )
            ),
            "{b:?}"
        );
        // A quarter turn counter-clockwise: the text runs up the page from its origin.
        let b = shape_bounds(&text(std::f64::consts::FRAC_PI_2)).unwrap();
        assert!(
            close(
                b,
                (
                    (10.0 - ASCENT * 4.0, 20.0 - w),
                    (10.0 + DESCENT * 4.0, 20.0)
                )
            ),
            "{b:?}"
        );
    }

    #[test]
    fn clipped_shape_paints_inside_its_clip_only() {
        let rings: Arc<[Vec<Pt>]> =
            vec![vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]].into();
        let clipped = Shape::Clipped {
            rings: Arc::clone(&rings),
            inner: Box::new(Shape::Fill {
                rings: vec![vec![(2.0, 2.0), (9.0, 2.0), (9.0, 9.0)]],
                rule: FillRule::EvenOdd,
            }),
        };
        assert!(close(
            shape_bounds(&clipped).unwrap(),
            ((2.0, 2.0), (4.0, 4.0))
        ));
        let outside = Shape::Clipped {
            rings,
            inner: Box::new(Shape::Fill {
                rings: vec![vec![(7.0, 7.0), (9.0, 7.0), (9.0, 9.0)]],
                rule: FillRule::EvenOdd,
            }),
        };
        assert_eq!(shape_bounds(&outside), None);
    }

    // --- What may show in a frame (ADR 0031) ---

    /// The frame of the tests: 100 × 100 mm from the origin.
    fn frame() -> PaperBox {
        PaperBox {
            min: (0.0, 0.0),
            max: (100.0, 100.0),
        }
    }

    fn fill(ring: Vec<Pt>) -> Shape {
        Shape::Fill {
            rings: vec![ring],
            rule: FillRule::NonZero,
        }
    }

    /// A triangle whose right-most point lies at `x`, beside the frame's left edge.
    fn triangle_ending_at(x: f64) -> Shape {
        fill(vec![(x - 20.0, 40.0), (x, 50.0), (x - 20.0, 60.0)])
    }

    fn stroke(path: Vec<Pt>, width: f64, cap: Cap, join: Join) -> Shape {
        Shape::Stroke {
            path,
            closed: false,
            width,
            cap,
            join,
        }
    }

    #[test]
    fn a_fill_wholly_outside_the_frame_is_left_out_on_every_side() {
        let gap = INK_SLACK_MM + 0.01;
        let outside = [
            fill(vec![(-30.0, 10.0), (-gap, 10.0), (-gap, 20.0)]),
            fill(vec![(100.0 + gap, 10.0), (130.0, 10.0), (130.0, 20.0)]),
            fill(vec![(10.0, -30.0), (20.0, -gap), (30.0, -30.0)]),
            fill(vec![(10.0, 100.0 + gap), (20.0, 130.0), (30.0, 130.0)]),
            // Far away in a corner.
            fill(vec![(-500.0, -500.0), (-400.0, -500.0), (-400.0, -400.0)]),
        ];
        for shape in &outside {
            assert!(!may_show(shape, frame()), "{shape:?}");
        }
    }

    #[test]
    fn a_fill_that_touches_the_frame_or_ends_within_the_slack_is_kept() {
        assert!(may_show(&triangle_ending_at(0.0), frame()), "touching");
        assert!(may_show(&triangle_ending_at(50.0), frame()), "inside");
        assert!(
            may_show(&triangle_ending_at(-INK_SLACK_MM + 0.01), frame()),
            "within the slack"
        );
        assert!(!may_show(
            &triangle_ending_at(-INK_SLACK_MM - 0.01),
            frame()
        ));
        // A shape larger than the frame on every side.
        let cover = fill(vec![
            (-50.0, -50.0),
            (150.0, -50.0),
            (150.0, 150.0),
            (-50.0, 150.0),
        ]);
        assert!(may_show(&cover, frame()));
    }

    // The shapes below lie one slack further out than the ink they describe, so what decides is whether that ink
    // comes within `INK_SLACK_MM` of the frame.

    #[test]
    fn a_thick_line_outside_whose_half_width_reaches_in_is_kept() {
        let x = -INK_SLACK_MM - 3.0;
        // 8 mm wide: the ink reaches 4 mm towards the frame, to 1 mm inside the slack.
        let thick = stroke(vec![(x, 10.0), (x, 90.0)], 8.0, Cap::Flat, Join::Round);
        assert!(may_show(&thick, frame()));
        // 2 mm wide, the same line ends 2 mm beyond the slack.
        let thin = stroke(vec![(x, 10.0), (x, 90.0)], 2.0, Cap::Flat, Join::Round);
        assert!(!may_show(&thin, frame()));
        // A round cap reaches half the width beyond the end of the line.
        let cap = stroke(
            vec![(x - 30.0, 50.0), (x - 0.5, 50.0)],
            8.0,
            Cap::Round,
            Join::Round,
        );
        assert!(may_show(&cap, frame()));
    }

    #[test]
    fn a_miter_tip_from_outside_is_kept() {
        // A 60° spike pointing at the frame, 4 mm wide. The miter is 1 / sin 30° = 2 times the half width long
        // (within OOM's limit √5), so its tip lies 4 mm beyond the apex, 0.5 mm inside the slack. Half the width
        // ends 1.5 mm short of the slack.
        let (sin, cos) = 30f64.to_radians().sin_cos();
        let apex = (-INK_SLACK_MM - 3.5, 50.0);
        let leg = (apex.0 - 20.0 * cos, 20.0 * sin);
        let spike = |join| {
            stroke(
                vec![(leg.0, apex.1 - leg.1), apex, (leg.0, apex.1 + leg.1)],
                4.0,
                Cap::Flat,
                join,
            )
        };
        assert!(shape_bounds(&spike(Join::Miter)).unwrap().1.0 + INK_SLACK_MM < 0.0);
        assert!(may_show(&spike(Join::Miter), frame()));
        // Round and bevel joins stay within half the width.
        assert!(!may_show(&spike(Join::Round), frame()));
        assert!(!may_show(&spike(Join::Bevel), frame()));
    }

    #[test]
    fn a_square_cap_from_outside_is_kept() {
        // A line at 45°, 8 mm wide: one corner of its square cap lies √2 · 4 mm = 5.66 mm right of the end, 1.36 mm
        // inside the slack. Half the width ends 0.3 mm short of it.
        let end = (-INK_SLACK_MM - 4.3, 50.0);
        let diagonal = |cap| stroke(vec![(end.0 - 20.0, 30.0), end], 8.0, cap, Join::Round);
        assert!(shape_bounds(&diagonal(Cap::Square)).unwrap().1.0 + INK_SLACK_MM < 0.0);
        assert!(may_show(&diagonal(Cap::Square), frame()));
        assert!(!may_show(&diagonal(Cap::Flat), frame()));
    }

    fn italic_f(origin: Pt, size: f64, rotation: f64, stroke: Option<f64>) -> Shape {
        Shape::Text {
            origin,
            size,
            bold: false,
            italic: true,
            text: "f".into(),
            rotation,
            stroke,
        }
    }

    #[test]
    fn a_text_wholly_outside_is_left_out_and_one_whose_glyphs_reach_in_is_kept() {
        // Arimo Italic's `f` leans 0.086 em past its advance. At 20 mm the glyph reaches 1.7 mm beyond the box of
        // its advance width; with that box ending 0.5 mm short of the slack, the glyph comes within it.
        let size = 20.0;
        let advance = text_width("f", face(false, true)) * size;
        let origin = (-INK_SLACK_MM - 0.5 - advance, 50.0);
        let f = italic_f(origin, size, 0.0, None);
        assert!(shape_bounds(&f).unwrap().1.0 + INK_SLACK_MM < 0.0);
        assert!(may_show(&f, frame()));
        // Turned a half turn, the same text runs away from the frame.
        assert!(!may_show(
            &italic_f(origin, size, std::f64::consts::PI, None),
            frame()
        ));
        // 3 mm further out it is left out; a framing stroke reaches √2 times its half width further.
        let far = (origin.0 - 3.0, 50.0);
        assert!(!may_show(&italic_f(far, size, 0.0, None), frame()));
        assert!(may_show(&italic_f(far, size, 0.0, Some(3.0)), frame()));
        // A text without glyph outlines paints nothing.
        let spaces = Shape::Text {
            origin: (50.0, 50.0),
            size: 4.0,
            bold: false,
            italic: false,
            text: "  ".into(),
            rotation: 0.0,
            stroke: None,
        };
        assert_eq!(ink_bounds(&spaces), None);
    }

    #[test]
    fn a_text_box_holds_its_turned_glyph_outlines() {
        use crate::fonts::{OutlineSegment, text_outline};
        for rotation in [0.0, 0.4, 1.9, -2.6] {
            let (origin, size) = ((30.0, 40.0), 6.0);
            let text = "Ąfjg Å";
            let shape = Shape::Text {
                origin,
                size,
                bold: true,
                italic: true,
                text: text.into(),
                rotation,
                stroke: None,
            };
            let ((x0, y0), (x1, y1)) = ink_bounds(&shape).unwrap();
            let (sin, cos) = f64::sin_cos(rotation);
            text_outline(text, face(true, true), |s| {
                let points = match s {
                    OutlineSegment::Move(p) | OutlineSegment::Line(p) => vec![p],
                    OutlineSegment::Quad(c, p) => vec![c, p],
                    OutlineSegment::Cubic(c1, c2, p) => vec![c1, c2, p],
                    OutlineSegment::Close => Vec::new(),
                };
                for (x, y) in points {
                    let (x, y) = (x * size, y * size);
                    let (px, py) = (origin.0 + x * cos + y * sin, origin.1 - x * sin + y * cos);
                    // The slack is extra: the outline lies inside the box shrunk by it.
                    assert!(
                        x0 + INK_SLACK_MM <= px + 1e-9
                            && px <= x1 - INK_SLACK_MM + 1e-9
                            && y0 + INK_SLACK_MM <= py + 1e-9
                            && py <= y1 - INK_SLACK_MM + 1e-9,
                        "{rotation}: ({px}, {py})"
                    );
                }
            });
        }
    }

    /// Lines of a hatch clipped to `ring`, running across the whole frame at `y`.
    fn hatch_line(ring: Vec<Pt>, y: f64) -> Shape {
        Shape::Clipped {
            rings: vec![ring].into(),
            inner: Box::new(stroke(
                vec![(-50.0, y), (150.0, y)],
                0.2,
                Cap::Flat,
                Join::Miter,
            )),
        }
    }

    #[test]
    fn a_pattern_paints_inside_its_outline_only() {
        // An area outside the frame, beyond the slack, whose hatch lines cross the frame: its clip hides them
        // there.
        let edge = -INK_SLACK_MM - 5.0;
        let outside = vec![(-40.0, 20.0), (edge, 20.0), (edge, 60.0), (-40.0, 60.0)];
        assert!(!may_show(&hatch_line(outside.clone(), 30.0), frame()));
        // The same area reaching 5 mm into the frame keeps its lines.
        let across = vec![(-40.0, 20.0), (5.0, 20.0), (5.0, 60.0), (-40.0, 60.0)];
        assert!(may_show(&hatch_line(across.clone(), 30.0), frame()));
        // A line that runs beside its own outline, both slacks apart, paints nothing, wherever the outline lies.
        let beside = 60.0 + 2.0 * INK_SLACK_MM + 1.0;
        assert_eq!(ink_bounds(&hatch_line(across, beside)), None);
        // A pattern inside a pattern: the outer outline outside the frame hides everything.
        let nested = Shape::Clipped {
            rings: vec![outside].into(),
            inner: Box::new(hatch_line(
                vec![(-20.0, 0.0), (50.0, 0.0), (50.0, 100.0), (-20.0, 100.0)],
                30.0,
            )),
        };
        assert!(!may_show(&nested, frame()));
    }

    #[test]
    fn a_number_that_is_not_finite_keeps_the_shape() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let far = fill(vec![(-500.0, 10.0), (bad, 10.0), (-400.0, 20.0)]);
            assert!(may_show(&far, frame()), "{bad}");
            let width = stroke(
                vec![(-500.0, 10.0), (-400.0, 10.0)],
                bad,
                Cap::Flat,
                Join::Round,
            );
            assert!(may_show(&width, frame()), "{bad}");
            let size = Shape::Text {
                origin: (-500.0, 10.0),
                size: bad,
                bold: false,
                italic: false,
                text: "Wald".into(),
                rotation: 0.0,
                stroke: None,
            };
            assert!(may_show(&size, frame()), "{bad}");
            assert!(
                may_show(&italic_f((-500.0, 10.0), 4.0, bad, None), frame()),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_ink_box_of_fills_strokes_and_patterns_holds_the_painted_box() {
        // Fills, strokes and clipped shapes: the ink box holds `shape_bounds` grown by the slack. A text is the
        // exception: its ink box follows the glyphs, which are lower than the font's ascent (test
        // `a_text_box_holds_its_turned_glyph_outlines`).
        let shapes = [
            triangle_ending_at(30.0),
            stroke(vec![(1.0, 2.0), (30.0, 9.0)], 1.5, Cap::Flat, Join::Bevel),
            hatch_line(vec![(0.0, 20.0), (50.0, 20.0), (50.0, 60.0)], 40.0),
        ];
        for shape in &shapes {
            let ((a0, a1), (b0, b1)) = shape_bounds(shape).unwrap();
            let ((c0, c1), (d0, d1)) = ink_bounds(shape).unwrap();
            let s = INK_SLACK_MM - 1e-9;
            assert!(
                c0 <= a0 - s && c1 <= a1 - s && d0 >= b0 + s && d1 >= b1 + s,
                "{shape:?}"
            );
        }
    }
}

//! Where primitives paint: axis-parallel bounding boxes on paper (mm, y down). Used for the extent
//! of a point symbol in a pattern and for the part of a page the map covers.

use crate::draw::{Primitive, Shape};
use crate::fonts::{ASCENT, DESCENT, face, text_width};
use crate::geometry::Pt;

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
}

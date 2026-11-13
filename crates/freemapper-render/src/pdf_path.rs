//! Compact PDF paths (ADR 0032), public for other PDF writers of the suite (ADR 0033).
//!
//! [`write_path`] writes the path of one fill, stroke or clip into a [`pdf_writer::Content`] with `m`, `l` and `h`,
//! in page points with y up, and then the operator that uses it. Coordinates are rounded to 1/[`STEPS_PER_PT`]
//! point. A small fill or stroke, at least [`MIN_RELATIVE_POINTS`] points within a square of [`SMALL_MM`], is
//! written relative to its first point inside `q`, `1 0 0 1 x y cm` and `Q`, so repeated symbols give the same
//! numbers and deflate stores them once.
//!
//! [`crate::pdf`] writes its paths with the same code. It works in page millimetres with y down and gives the code
//! its own scale and origin, so its PDFs keep every byte they had before this module was public.
//!
//! ```
//! use freemapper_render::pdf_path::{Paint, write_path};
//! use pdf_writer::Content;
//!
//! let mut content = Content::new();
//! // A square of 100 pt with a square hole, filled even-odd.
//! let outer = [(100.0, 100.0), (200.0, 100.0), (200.0, 200.0), (100.0, 200.0)];
//! let hole = [(120.0, 120.0), (180.0, 120.0), (180.0, 180.0), (120.0, 180.0)];
//! content.set_fill_cmyk(0.0, 0.0, 0.0, 1.0);
//! write_path(&mut content, &[&outer[..], &hole[..]], true, Paint::FillEvenOdd)?;
//! // A dot of 1 pt, stroked: small, so it is written relative to its first point.
//! let dot = [(300.25, 400.5), (301.25, 400.5), (301.25, 401.5), (300.25, 401.5)];
//! content.set_line_width(0.3);
//! write_path(&mut content, &[&dot[..]], true, Paint::Stroke)?;
//! let text = String::from_utf8(content.finish().into_vec()).unwrap();
//! assert!(text.contains("100 100 m\n200 100 l\n"));
//! assert!(text.ends_with("q\n1 0 0 1 300.25 400.5 cm\n0 0 m\n1 0 l\n1 1 l\n0 1 l\nh\nS\nQ"));
//! # Ok::<(), freemapper_render::pdf_path::NotFinite>(())
//! ```

use pdf_writer::Content;

use crate::pdf::PT_PER_MM;

/// Path coordinates are written in steps of 1/1000 point, 0.00035 mm (ADR 0032).
pub const STEPS_PER_PT: f64 = 1000.0;
/// A fill or stroke whose points all fit in a square of this side, in mm on paper, is small: a dot or another part of
/// a point symbol, a control circle (ADR 0032). [`write_path`] compares in points, with this side times 72 / 25.4.
pub const SMALL_MM: f64 = 7.0;
/// A small fill or stroke of at least this many points, over all its rings, is written relative to its first point.
/// With fewer, the translation would take more room than it saves.
pub const MIN_RELATIVE_POINTS: usize = 4;

/// What follows the path: the operator that paints it, or the one that makes it the clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// `f`: fill by the nonzero winding rule.
    FillNonZero,
    /// `f*`: fill by the even-odd rule, for an area with holes.
    FillEvenOdd,
    /// `S`: stroke with the line width, cap, join, miter limit and dash pattern the caller set before.
    Stroke,
    /// `W n`: intersect the clip with the path by the nonzero rule. Never relative, because a clip set inside
    /// `q … Q` would end with it; the caller opens and closes the clip's own graphics state.
    ClipNonZero,
    /// `W* n`: intersect the clip with the path by the even-odd rule. Never relative, as [`Paint::ClipNonZero`].
    ClipEvenOdd,
}

impl Paint {
    fn is_clip(self) -> bool {
        matches!(self, Paint::ClipNonZero | Paint::ClipEvenOdd)
    }

    fn write(self, content: &mut Content) {
        match self {
            Paint::FillNonZero => content.fill_nonzero(),
            Paint::FillEvenOdd => content.fill_even_odd(),
            Paint::Stroke => content.stroke(),
            Paint::ClipNonZero => content.clip_nonzero().end_path(),
            Paint::ClipEvenOdd => content.clip_even_odd().end_path(),
        };
    }
}

/// A coordinate of a path was infinite or not a number. [`write_path`] then writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a path coordinate is not a finite number")]
pub struct NotFinite;

/// Writes the rings of one path in page points (y up, as written to the PDF), then `paint`.
///
/// Each ring starts with `m` and goes on with `l`; with `closed` every ring ends with `h`. Several rings make one
/// path, an area with holes for [`Paint::FillEvenOdd`]. A stroke is one ring, open or closed. Coordinates are rounded
/// to 1/[`STEPS_PER_PT`] point.
///
/// A fill or stroke with at least [`MIN_RELATIVE_POINTS`] points that all fit in a square of [`SMALL_MM`] is written
/// inside `q` and `Q`: a translation `1 0 0 1 x y cm` to its first point, the rings relative to that point, `paint`.
/// The translation does not scale, so the line width, dash pattern, caps and joins the caller set before keep their
/// size, and `Q` gives the caller back the graphics state it had. Clips are never written that way.
///
/// The caller sets colour, line width, cap, join, miter limit and dashes before the call, and the clip and the page
/// transformation around it. Empty rings are left out; a path without points writes nothing, not even `paint`.
///
/// # Errors
///
/// [`NotFinite`] when a coordinate is infinite or not a number. Nothing is written then.
pub fn write_path<R: AsRef<[(f32, f32)]>>(
    content: &mut Content,
    rings: &[R],
    closed: bool,
    paint: Paint,
) -> Result<(), NotFinite> {
    let finite = |&(x, y): &(f32, f32)| x.is_finite() && y.is_finite();
    if !rings.iter().all(|r| r.as_ref().iter().all(finite)) {
        return Err(NotFinite);
    }
    let rings = rings.iter().map(AsRef::as_ref).filter(|r| !r.is_empty());
    if rings.clone().next().is_none() {
        return Ok(());
    }
    if write_in(content, &Frame::PT, rings, closed, paint) {
        Ok(())
    } else {
        Err(NotFinite)
    }
}

/// A point of a path in the units of a [`Frame`].
pub(crate) trait Coord: Copy {
    fn xy(self) -> (f64, f64);
}

impl Coord for (f64, f64) {
    fn xy(self) -> (f64, f64) {
        self
    }
}

impl Coord for (f32, f32) {
    fn xy(self) -> (f64, f64) {
        (f64::from(self.0), f64::from(self.1))
    }
}

/// How the points given to [`write_in`] become page points: `(p - page_origin) · pt_per_unit`, with y negated when
/// `flip_y`. [`Frame::PT`] is the identity of [`write_path`]; [`Frame::page_mm`] is the page of [`crate::pdf`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pt_per_unit: f64,
    /// The input point at the page's origin, bottom left.
    page_origin: (f64, f64),
    /// The input's y axis points down.
    flip_y: bool,
    /// [`SMALL_MM`] in input units.
    small_side: f64,
    /// Rounded coordinates and small shapes relative to their first point; without it every number is written as it
    /// comes (the comparison of ADR 0032).
    compact: bool,
}

impl Frame {
    /// Page points, y up.
    const PT: Frame = Frame {
        pt_per_unit: 1.0,
        page_origin: (0.0, 0.0),
        flip_y: false,
        small_side: SMALL_MM * PT_PER_MM,
        compact: true,
    };

    /// Page mm, y down, on a page `page_height` mm high.
    pub(crate) fn page_mm(page_height: f64, compact: bool) -> Frame {
        Frame {
            pt_per_unit: PT_PER_MM,
            page_origin: (0.0, page_height),
            flip_y: true,
            small_side: SMALL_MM,
            compact,
        }
    }

    /// `p` in page points, or relative to `origin` (x right, y up) when there is one; rounded when compact. A number
    /// that is not finite as an `f32` clears `finite` and is written as 0.
    fn point(&self, p: (f64, f64), origin: Option<(f64, f64)>, finite: &mut bool) -> (f32, f32) {
        let (ox, oy) = origin.unwrap_or(self.page_origin);
        let dx = (p.0 - ox) * self.pt_per_unit;
        let dy = (if self.flip_y { oy - p.1 } else { p.1 - oy }) * self.pt_per_unit;
        let mut num = |v: f64| {
            let n = v as f32;
            if n.is_finite() {
                n
            } else {
                *finite = false;
                0.0
            }
        };
        if !self.compact {
            return (num(dx), num(dy));
        }
        let mut round = |v: f64| num((v * STEPS_PER_PT).round() / STEPS_PER_PT);
        (round(dx), round(dy))
    }

    /// Whether `points` are finite and fit in a square of side [`SMALL_MM`].
    pub(crate) fn is_small(&self, points: impl IntoIterator<Item = (f64, f64)>) -> bool {
        let mut points = points.into_iter();
        let finite = |p: (f64, f64)| p.0.is_finite() && p.1.is_finite();
        let Some(first) = points.next() else {
            return false;
        };
        if !finite(first) {
            return false;
        }
        let (mut low, mut high) = (first, first);
        for p in points {
            if !finite(p) {
                return false;
            }
            low = (low.0.min(p.0), low.1.min(p.1));
            high = (high.0.max(p.0), high.1.max(p.1));
        }
        high.0 - low.0 < self.small_side && high.1 - low.1 < self.small_side
    }

    /// The first point of a small shape with `points` when it is written relative to that point: compact, at least
    /// [`MIN_RELATIVE_POINTS`] points, all finite and within [`SMALL_MM`].
    fn small_origin(&self, points: impl Iterator<Item = (f64, f64)> + Clone) -> Option<(f64, f64)> {
        let first = points.clone().next()?;
        (self.compact
            && points.clone().nth(MIN_RELATIVE_POINTS - 1).is_some()
            && self.is_small(points))
        .then_some(first)
    }
}

/// Writes `rings` in `frame`, each closed with `h` when `closed`, then `paint`; a small fill or stroke relative to its
/// first point inside `q … Q`. Returns whether every number was finite; one that was not is written as 0.
pub(crate) fn write_in<'a, P: Coord + 'a>(
    content: &mut Content,
    frame: &Frame,
    rings: impl Iterator<Item = &'a [P]> + Clone,
    closed: bool,
    paint: Paint,
) -> bool {
    let mut finite = true;
    let origin = if paint.is_clip() {
        None
    } else {
        frame.small_origin(rings.clone().flatten().map(|p| p.xy()))
    };
    if let Some(o) = origin {
        let (x, y) = frame.point(o, None, &mut finite);
        content.save_state();
        content.transform([1.0, 0.0, 0.0, 1.0, x, y]);
    }
    for ring in rings {
        for (i, p) in ring.iter().enumerate() {
            let (x, y) = frame.point(p.xy(), origin, &mut finite);
            if i == 0 {
                content.move_to(x, y);
            } else {
                content.line_to(x, y);
            }
        }
        if closed {
            content.close_path();
        }
    }
    paint.write(content);
    if origin.is_some() {
        content.restore_state();
    }
    finite
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The content `f` writes, as text.
    fn written(f: impl FnOnce(&mut Content)) -> String {
        let mut content = Content::new();
        f(&mut content);
        String::from_utf8(content.finish().into_vec()).unwrap()
    }

    fn path(rings: &[&[(f32, f32)]], closed: bool, paint: Paint) -> String {
        written(|c| write_path(c, rings, closed, paint).unwrap())
    }

    /// A square of `side` pt from `(x, y)`.
    fn square(x: f32, y: f32, side: f32) -> [(f32, f32); 4] {
        [(x, y), (x + side, y), (x + side, y + side), (x, y + side)]
    }

    #[test]
    fn coordinates_are_rounded_to_a_thousandth_point() {
        let line = [(10.123_45, 20.000_4), (500.987_6, 700.000_6)];
        assert_eq!(
            path(&[&line[..]], false, Paint::Stroke),
            "10.123 20 m\n500.988 700.001 l\nS"
        );
    }

    #[test]
    fn a_big_shape_keeps_page_coordinates() {
        // 30 pt is more than 7 mm (19.84 pt).
        let s = square(100.0, 200.0, 30.0);
        assert_eq!(
            path(&[&s[..]], true, Paint::FillNonZero),
            "100 200 m\n130 200 l\n130 230 l\n100 230 l\nh\nf"
        );
    }

    #[test]
    fn a_small_shape_is_written_relative_to_its_first_point() {
        let s = square(100.5, 200.25, 2.0);
        assert_eq!(
            path(&[&s[..]], true, Paint::FillNonZero),
            "q\n1 0 0 1 100.5 200.25 cm\n0 0 m\n2 0 l\n2 2 l\n0 2 l\nh\nf\nQ"
        );
        // The same dot elsewhere gives the same path.
        let t = square(400.125, 10.0, 2.0);
        let (a, b) = (
            path(&[&s[..]], true, Paint::FillNonZero),
            path(&[&t[..]], true, Paint::FillNonZero),
        );
        assert_eq!(
            a.split_once(" cm").unwrap().1,
            b.split_once(" cm").unwrap().1
        );
        // Just below 7 mm is small, 7 mm is not.
        let side = (SMALL_MM * PT_PER_MM) as f32;
        let below = square(0.0, 0.0, side - 0.01);
        assert!(path(&[&below[..]], true, Paint::FillNonZero).starts_with("q\n"));
        let at = square(0.0, 0.0, side + 0.001);
        assert!(!path(&[&at[..]], true, Paint::FillNonZero).contains("cm"));
    }

    #[test]
    fn fewer_than_four_points_keep_page_coordinates() {
        let three = [(10.0, 10.0), (11.0, 10.0), (11.0, 11.0)];
        assert_eq!(
            path(&[&three[..]], false, Paint::Stroke),
            "10 10 m\n11 10 l\n11 11 l\nS"
        );
        // Four points over two rings count.
        let two = [(10.0, 10.0), (11.0, 10.0)];
        assert!(path(&[&two[..], &two[..]], false, Paint::Stroke).starts_with("q\n"));
    }

    #[test]
    fn closed_and_open_paths() {
        let s = square(0.0, 0.0, 50.0);
        let open = path(&[&s[..]], false, Paint::Stroke);
        let closed = path(&[&s[..]], true, Paint::Stroke);
        assert_eq!(open, "0 0 m\n50 0 l\n50 50 l\n0 50 l\nS");
        assert_eq!(closed, "0 0 m\n50 0 l\n50 50 l\n0 50 l\nh\nS");
    }

    #[test]
    fn rings_with_holes_share_one_path_and_one_fill() {
        let outer = square(0.0, 0.0, 100.0);
        let hole = square(25.0, 25.0, 50.0);
        assert_eq!(
            path(&[&outer[..], &hole[..]], true, Paint::FillEvenOdd),
            "0 0 m\n100 0 l\n100 100 l\n0 100 l\nh\n\
             25 25 m\n75 25 l\n75 75 l\n25 75 l\nh\nf*"
        );
        // A small ring with a hole: both relative to the first point of the first ring.
        let outer = square(10.0, 10.0, 4.0);
        let hole = square(11.0, 11.0, 2.0);
        assert_eq!(
            path(&[&outer[..], &hole[..]], true, Paint::FillEvenOdd),
            "q\n1 0 0 1 10 10 cm\n0 0 m\n4 0 l\n4 4 l\n0 4 l\nh\n1 1 m\n3 1 l\n3 3 l\n1 3 l\nh\nf*\nQ"
        );
    }

    #[test]
    fn a_translation_leaves_the_stroke_settings_alone() {
        // The line width and dashes come before `q` and stay in force inside it: `q` copies the graphics state, and
        // `1 0 0 1 x y cm` moves without scaling, so 0.5 pt stays 0.5 pt and the dashes keep their length.
        let dot = square(300.0, 300.0, 1.0);
        let text = written(|c| {
            c.set_line_width(0.5);
            c.set_dash_pattern([2.0, 1.0], 0.0);
            write_path(c, &[&dot[..]], true, Paint::Stroke).unwrap();
        });
        assert_eq!(
            text,
            "0.5 w\n[2 1] 0 d\nq\n1 0 0 1 300 300 cm\n0 0 m\n1 0 l\n1 1 l\n0 1 l\nh\nS\nQ"
        );
        let cm = text.lines().find(|l| l.ends_with(" cm")).unwrap();
        assert!(cm.starts_with("1 0 0 1 "), "{cm}");
    }

    #[test]
    fn clips_are_never_relative() {
        let dot = square(300.0, 300.0, 1.0);
        assert_eq!(
            path(&[&dot[..]], true, Paint::ClipNonZero),
            "300 300 m\n301 300 l\n301 301 l\n300 301 l\nh\nW\nn"
        );
        assert!(path(&[&dot[..]], true, Paint::ClipEvenOdd).ends_with("h\nW*\nn"));
    }

    #[test]
    fn a_coordinate_that_is_not_finite_writes_nothing() {
        let s = square(0.0, 0.0, 1.0);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let ring = [(0.0, 0.0), (bad, 1.0), (1.0, 1.0), (1.0, 0.0)];
            let mut content = Content::new();
            content.set_line_width(1.0);
            let before = content.finish().into_vec();
            let mut content = Content::new();
            content.set_line_width(1.0);
            let err = write_path(&mut content, &[&s[..], &ring[..]], true, Paint::Stroke);
            assert_eq!(err, Err(NotFinite));
            assert_eq!(content.finish().into_vec(), before);
        }
        assert_eq!(
            NotFinite.to_string(),
            "a path coordinate is not a finite number"
        );
    }

    #[test]
    fn empty_rings_are_left_out() {
        let s = square(0.0, 0.0, 50.0);
        let empty: &[(f32, f32)] = &[];
        assert_eq!(
            path(&[empty, &s[..], empty], true, Paint::FillNonZero),
            path(&[&s[..]], true, Paint::FillNonZero)
        );
        assert_eq!(path(&[empty], true, Paint::FillNonZero), "");
        assert_eq!(path(&[], true, Paint::Stroke), "");
    }

    #[test]
    fn rings_may_be_vectors() {
        let rings = vec![square(0.0, 0.0, 50.0).to_vec()];
        let text = written(|c| write_path(c, &rings, true, Paint::FillNonZero).unwrap());
        assert_eq!(text, path(&[&rings[0][..]], true, Paint::FillNonZero));
    }

    #[test]
    fn small_means_within_a_square_of_7_mm() {
        let mm = Frame::page_mm(297.0, true);
        let dot: Vec<(f64, f64)> = (0..48)
            .map(|i| {
                let a = f64::from(i) / 48.0 * std::f64::consts::TAU;
                (10.0 + 0.25 * a.cos(), 20.0 + 0.25 * a.sin())
            })
            .collect();
        assert!(mm.is_small(dot.iter().copied()));
        // Dots and circles of point symbols up to a control circle of 6 mm.
        for r in [0.2, 0.5, 1.0, 3.0] {
            let circle = (0..48).map(|i| {
                let a = f64::from(i) / 48.0 * std::f64::consts::TAU;
                (r * a.cos(), r * a.sin())
            });
            assert!(mm.is_small(circle), "r {r}");
        }
        assert!(!mm.is_small([(0.0, 0.0), (7.0, 1.0)]));
        assert!(!mm.is_small([(0.0, 0.0), (1.0, 7.0)]));
        assert!(mm.is_small([(0.0, 0.0), (6.99, 6.99)]));
        // A point that is not a number makes nothing small, wherever it is.
        assert!(!mm.is_small([(0.0, 0.0), (f64::NAN, 0.0), (1.0, 1.0)]));
        assert!(!mm.is_small([(f64::INFINITY, 0.0), (1.0, 1.0)]));
        assert!(!mm.is_small([]));
    }
}

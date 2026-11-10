//! One rendered page as every backend needs it: drawing primitives in page millimetres (origin top
//! left, y down), the clip rectangle, the page size and the colours.

use std::sync::Arc;

pub use crate::draw::{FillRule, Primitive, Shape, SymbolClass};
pub use crate::geometry::Pt;
pub use crate::image::{ImageError, RasterImage};
pub use crate::symbols::{Cap, Join, RenderColour};

/// An axis-parallel rectangle on paper (mm, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaperBox {
    pub min: Pt,
    pub max: Pt,
}

/// One page as every backend needs it, in page millimetres (origin top left, y down).
///
/// The PDF writer takes nothing else, so a later SVG or tile backend for the browser editors
/// draws the same value.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPage {
    /// The primitives in paint order: each one is painted over the ones before it. They are
    /// sorted by colour priority (highest number first, priority 0 on top) and keep the order of
    /// the map objects within one priority. A backend paints them as they come and clips them to
    /// `clip`.
    pub primitives: Vec<Primitive>,
    pub clip: PaperBox,
    /// Width and height of the page: the paper size.
    pub page_mm: (f64, f64),
    /// The point of the map that lies at the centre of the page, in mm at the print scale (the
    /// map's own coordinates times the geometry factor; y down as the page).
    pub centre_mm: Pt,
    pub colours: Vec<RenderColour>,
    /// The sheet elements (title, texts, logos, scale bar, footer), painted after all map
    /// primitives in this order, over the whole page: they are not clipped to `clip`.
    pub sheet: Vec<SheetItem>,
    /// The course overprint, clipped to `clip` like the map: its lower layer among the map primitives
    /// ([`Overprint::below_priority`]), its upper layer after them, both before the sheet layer. Empty
    /// on a page without a course.
    pub overprint: Overprint,
    /// The masks of the page (ADR 0045): the map primitives are painted only where every one of them leaves the
    /// map visible, so the map is hidden in the union of their masked areas. The overprint, the sheet layer and
    /// everything outside `clip` are not affected. Empty on a page without a mask.
    pub map_clips: Vec<MapClip>,
}

/// Where the map stays visible under one mask: rings in page mm (y down) for the nonzero rule. The frame and the
/// windows (holes) run counter-clockwise on paper, the outlines of the masked areas clockwise. A point in the frame
/// outside every outline winds once and shows the map; inside an outline the frame and the outline cancel; in a
/// window, or where windows overlap, the count is one or more again, so overlapping windows unite.
#[derive(Debug, Clone, PartialEq)]
pub struct MapClip {
    rings: Vec<Vec<Pt>>,
}

impl MapClip {
    /// The clip of the `outlines` and `holes` (page mm, either orientation) in `frame`. The orientation of each
    /// ring is set here by its signed area on paper, so a mask written clockwise or counter-clockwise clips the
    /// same. A ring of fewer than 3 points has no area and is left out.
    pub fn new(
        frame: PaperBox,
        outlines: impl IntoIterator<Item = Vec<Pt>>,
        holes: impl IntoIterator<Item = Vec<Pt>>,
    ) -> Self {
        let (a, b) = (frame.min, frame.max);
        let mut rings = vec![turned(vec![a, (b.0, a.1), b, (a.0, b.1)], true)];
        let usable = |r: &Vec<Pt>| r.len() >= 3;
        rings.extend(
            outlines
                .into_iter()
                .filter(usable)
                .map(|r| turned(r, false)),
        );
        rings.extend(holes.into_iter().filter(usable).map(|r| turned(r, true)));
        MapClip { rings }
    }

    /// The frame ring first, then the outlines, then the holes.
    pub fn rings(&self) -> &[Vec<Pt>] {
        &self.rings
    }
}

/// `ring` running counter-clockwise on paper (`ccw`) or clockwise. On paper y points down, so a ring that runs
/// counter-clockwise as seen has a negative shoelace sum in page mm.
fn turned(mut ring: Vec<Pt>, ccw: bool) -> Vec<Pt> {
    let n = ring.len();
    let twice_area: f64 = (0..n)
        .map(|i| {
            let (p, q) = (ring[i], ring[(i + 1) % n]);
            p.0 * q.1 - q.0 * p.1
        })
        .sum();
    if (twice_area < 0.0) != ccw {
        ring.reverse();
    }
    ring
}

/// Shapes painted over the map in one colour with the blend mode Multiply, in two layers: the course
/// overprint (ADR 0034). The renderer knows only the shapes; what they show is decided elsewhere.
#[derive(Debug, Clone, PartialEq)]
pub struct Overprint {
    /// Painted among the map primitives, before the first one of [`Overprint::below_priority`].
    pub lower: Vec<Shape>,
    /// Painted over all map primitives.
    pub upper: Vec<Shape>,
    /// The priority before whose first primitive the lower layer is painted. `None` paints it after all map
    /// primitives, just before the upper layer.
    pub below_priority: Option<i32>,
    pub colour: DirectColour,
    /// A limit left out some of the shapes (hatch lines, dashes or crosses).
    pub cut: bool,
}

impl Default for Overprint {
    /// Nothing to paint, in black.
    fn default() -> Self {
        Overprint {
            lower: Vec::new(),
            upper: Vec::new(),
            below_priority: None,
            colour: DirectColour::BLACK,
            cut: false,
        }
    }
}

impl Overprint {
    /// Nothing to paint.
    pub fn is_empty(&self) -> bool {
        self.lower.is_empty() && self.upper.is_empty()
    }
}

/// A colour of a sheet element, given directly instead of by map colour priority. CMYK, so that
/// print colours match the map's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectColour {
    /// Cyan, magenta, yellow and black, each 0 to 1 (as [`RenderColour::cmyk`]).
    pub cmyk: [f64; 4],
}

impl DirectColour {
    pub const BLACK: DirectColour = DirectColour {
        cmyk: [0.0, 0.0, 0.0, 1.0],
    };
    pub const WHITE: DirectColour = DirectColour {
        cmyk: [0.0, 0.0, 0.0, 0.0],
    };
}

/// One element of the sheet layer.
#[derive(Debug, Clone, PartialEq)]
pub enum SheetItem {
    /// `shape` painted in `colour`. Text uses the same embedded fonts as map text.
    Shape { colour: DirectColour, shape: Shape },
    /// `image` stretched to fill `rect` exactly.
    Image {
        rect: PaperBox,
        image: Arc<RasterImage>,
    },
}

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

impl RenderedPage {
    /// The page mapping of this page, from [`RenderedPage::centre_mm`] and [`RenderedPage::page_mm`].
    pub fn mapping(&self) -> PageMapping {
        PageMapping {
            centre_mm: self.centre_mm,
            page_mm: self.page_mm,
        }
    }
}

/// Converts between the map in mm at the print scale and the page in mm, both with y down. A map position in mm
/// at the print scale is the map's own coordinate (OOM 1/1000 mm) divided by 1000 and multiplied by the geometry
/// factor. The page builder moves every primitive by the same offset, so that `centre_mm` lands in the middle
/// of the page; this value applies that offset and takes it back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageMapping {
    /// The point of the map at the centre of the page, in mm at the print scale.
    pub centre_mm: Pt,
    /// Width and height of the page.
    pub page_mm: (f64, f64),
}

impl PageMapping {
    /// What the page builder adds to every map position: from the centre to the middle of the page.
    fn offset(&self) -> Pt {
        (
            self.page_mm.0 / 2.0 - self.centre_mm.0,
            self.page_mm.1 / 2.0 - self.centre_mm.1,
        )
    }

    /// The page position of `map_mm`, a map position in mm at the print scale.
    pub fn to_page(&self, map_mm: Pt) -> Pt {
        let d = self.offset();
        (map_mm.0 + d.0, map_mm.1 + d.1)
    }

    /// The map position, in mm at the print scale, that lies at `page_mm` on the page.
    pub fn to_map(&self, page_mm: Pt) -> Pt {
        let d = self.offset();
        (page_mm.0 - d.0, page_mm.1 - d.1)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A page as freemapper-print's page builder (`print::render_page_within`) places it: the map drawn in mm at
    /// the print scale (OOM coordinate / 1000 × geometry factor), the centre in the middle of the painted box, and
    /// every primitive moved by `page / 2 − centre` (`print::translate`).
    fn placed(
        points_um: &[(i32, i32)],
        geometry_factor: f64,
        page_mm: (f64, f64),
    ) -> (Vec<Pt>, Vec<Pt>, Pt) {
        let map: Vec<Pt> = points_um
            .iter()
            .map(|&(x, y)| {
                (
                    f64::from(x) * geometry_factor / 1000.0,
                    f64::from(y) * geometry_factor / 1000.0,
                )
            })
            .collect();
        let (min, max) = map.iter().fold(
            ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN)),
            |(min, max), p| {
                (
                    (min.0.min(p.0), min.1.min(p.1)),
                    (max.0.max(p.0), max.1.max(p.1)),
                )
            },
        );
        let centre = ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
        let offset = (page_mm.0 / 2.0 - centre.0, page_mm.1 / 2.0 - centre.1);
        let page = map
            .iter()
            .map(|p| (p.0 + offset.0, p.1 + offset.1))
            .collect();
        (map, page, centre)
    }

    #[test]
    fn mapping_follows_the_page_builder_and_round_trips() {
        let points = [
            (-120_000, 35_500),
            (64_250, -80_000),
            (3_000, 3_000),
            (250_000, 190_125),
        ];
        // 1:15000 printed at 1:15000, 1:10000 and 1:7500; 1:4000 printed at 1:5000.
        for geometry_factor in [1.0, 1.5, 2.0, 0.8] {
            let page_mm = (210.0, 297.0);
            let (map, page, centre) = placed(&points, geometry_factor, page_mm);
            let mapping = RenderedPage {
                primitives: Vec::new(),
                clip: PaperBox {
                    min: (0.0, 0.0),
                    max: page_mm,
                },
                page_mm,
                centre_mm: centre,
                colours: Vec::new(),
                sheet: Vec::new(),
                overprint: Overprint::default(),
                map_clips: Vec::new(),
            }
            .mapping();
            for (m, p) in map.iter().zip(&page) {
                let to_page = mapping.to_page(*m);
                assert!(
                    (to_page.0 - p.0).abs() < 1e-9 && (to_page.1 - p.1).abs() < 1e-9,
                    "{to_page:?} vs {p:?}"
                );
                let back = mapping.to_map(*p);
                assert!(
                    (back.0 - m.0).abs() < 1e-9 && (back.1 - m.1).abs() < 1e-9,
                    "{back:?} vs {m:?}"
                );
            }
            // The centre of the map is the middle of the page.
            let middle = mapping.to_page(centre);
            assert!((middle.0 - 105.0).abs() < 1e-9 && (middle.1 - 148.5).abs() < 1e-9);
        }
    }
}

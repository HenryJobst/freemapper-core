//! The SVG backend for the browser editors (ADR 0022): a [`RenderedPage`] as one SVG document, painted in the
//! order of the PDF backend.
//!
//! The viewBox is in page mm (y down), so every coordinate in the document is a page millimetre, the same
//! numbers [`RenderedPage::hits_at`] and the page mapping use. Colours are CMYK converted to sRGB by one simple
//! formula ([`srgb`]); the PDF keeps CMYK. Texts are glyph outlines from the same Arimo data as the PDF, so the
//! browser needs no font. Images are PNG data URIs.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt::Write as _;
use std::sync::Arc;

use image::ImageEncoder as _;
use image::codecs::png::PngEncoder;

use crate::bounds::{Bounds, may_show};
use crate::draw::{FillRule, KEPT_POINTS_PER_UNIT, Primitive, Shape};
use crate::fonts::{OutlineSegment, face, text_outline};
use crate::geometry::Pt;
use crate::image::RasterImage;
use crate::page::{MapClip, PaperBox, RenderedPage, SheetItem};
use crate::pdf::{MITER_LIMIT, TEXT_FRAMING_MITER_LIMIT, map_cmyk};
use crate::symbols::{Cap, Join, RenderColour};

/// Pixels per page millimetre of [`SvgView::default`]: about 100 dots per inch, a page of A4 at 840 × 1188 px.
pub const DEFAULT_ZOOM: f64 = 4.0;

/// The most units one SVG may use. Each element (path, group, clip path, rectangle, image) costs one unit plus
/// one per [`KEPT_POINTS_PER_UNIT`] points it writes, as primitives cost in the page budget. Four times the page
/// budget of freemapper-print (500 000), so a full page with clips and texts fits, and a document stays below
/// a few hundred MB.
pub const MAX_UNITS: usize = 2_000_000;

/// The part of the page an SVG shows, and at what size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgView {
    /// The part of the page in page mm; `None` is the whole page. Primitives that paint only outside it are
    /// left out.
    pub rect: Option<PaperBox>,
    /// Pixels per page millimetre: sets the `width` and `height` of the document, not its coordinates.
    pub zoom: f64,
}

impl Default for SvgView {
    /// The whole page at [`DEFAULT_ZOOM`].
    fn default() -> Self {
        SvgView {
            rect: None,
            zoom: DEFAULT_ZOOM,
        }
    }
}

/// A page that could not be written as SVG.
#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    #[error(
        "the view needs a zoom above 0 and a rectangle whose minimum is not larger than its maximum, all finite"
    )]
    InvalidView,
    #[error("the SVG would need more than {MAX_UNITS} units; show a smaller part of the page")]
    TooLarge,
    /// A number on the page is infinite or not a number. That is a bug upstream, as for the PDF.
    #[error("a number on the page is not finite; the page is not written")]
    NotFinite,
    #[error("could not encode an image as PNG: {0}")]
    Image(String),
}

/// The blend attribute of the course overprint.
const MULTIPLY: &str = " style=\"mix-blend-mode:multiply\"";

/// `cmyk` (each 0 to 1) as sRGB: r = 255 · (1 − c) · (1 − k), g and b alike from m and y, rounded. Values outside
/// 0 to 1 are clamped. The same formula gives a map colour its RGB when the map file has none.
pub fn srgb(cmyk: [f64; 4]) -> [u8; 3] {
    let [c, m, y, k] = cmyk.map(|v| v.clamp(0.0, 1.0));
    // The product lies in 0..=255, so the cast is exact after rounding.
    let channel = |v: f64| (255.0 * (1.0 - v) * (1.0 - k)).round() as u8;
    [channel(c), channel(m), channel(y)]
}

/// The SVG document being written, with its budget and the clip paths defined so far.
struct Svg {
    out: String,
    units: usize,
    max_units: usize,
    not_finite: bool,
    /// The id number of each pattern clip already defined, by the address of its shared rings. Only looked
    /// up, never iterated, so the output does not depend on addresses.
    clips: BTreeMap<usize, usize>,
    /// Each image's data URI, by the address of its `Arc`.
    images: BTreeMap<usize, String>,
}

impl Svg {
    fn new(max_units: usize) -> Self {
        Svg {
            out: String::new(),
            units: 0,
            max_units,
            not_finite: false,
            clips: BTreeMap::new(),
            images: BTreeMap::new(),
        }
    }

    /// Charges one element with `points` points.
    fn charge(&mut self, points: usize) -> Result<(), SvgError> {
        self.units = self.units.saturating_add(1 + points / KEPT_POINTS_PER_UNIT);
        if self.units > self.max_units {
            return Err(SvgError::TooLarge);
        }
        Ok(())
    }

    /// Writes `v` with at most three decimals (1 µm), without trailing zeros. A number that is not finite is
    /// noted and written as 0; [`write_svg`] refuses the page then.
    fn num(&mut self, v: f64) {
        if !v.is_finite() {
            self.not_finite = true;
            self.out.push('0');
            return;
        }
        let start = self.out.len();
        let _ = write!(self.out, "{v:.3}");
        let trimmed = self.out[start..]
            .trim_end_matches('0')
            .trim_end_matches('.')
            .len();
        self.out.truncate(start + trimmed);
        if &self.out[start..] == "-0" {
            self.out.truncate(start);
            self.out.push('0');
        }
    }

    fn pt(&mut self, p: Pt) {
        self.num(p.0);
        self.out.push(' ');
        self.num(p.1);
    }

    /// ` name="v"`.
    fn attr_num(&mut self, name: &str, v: f64) {
        let _ = write!(self.out, " {name}=\"");
        self.num(v);
        self.out.push('"');
    }

    /// `#rrggbb` of `cmyk` ([`srgb`]).
    fn colour(&mut self, cmyk: [f64; 4]) {
        if cmyk.iter().any(|v| !v.is_finite()) {
            self.not_finite = true;
        }
        let [r, g, b] = srgb(cmyk);
        let _ = write!(self.out, "#{r:02x}{g:02x}{b:02x}");
    }

    /// Path data of the polylines `rings`, each closed when `close`.
    fn polylines<'a>(&mut self, rings: impl IntoIterator<Item = &'a Vec<Pt>>, close: bool) {
        for ring in rings {
            for (i, &p) in ring.iter().enumerate() {
                self.out.push(if i == 0 { 'M' } else { 'L' });
                self.pt(p);
            }
            if close && !ring.is_empty() {
                self.out.push('Z');
            }
        }
    }

    /// `<path d="…"` for closed `rings`, charged.
    fn ring_path(&mut self, rings: &[Vec<Pt>]) -> Result<(), SvgError> {
        self.charge(rings.iter().map(Vec::len).sum())?;
        self.out.push_str("<path d=\"");
        self.polylines(rings, true);
        self.out.push('"');
        Ok(())
    }

    /// A clip path `id` of the closed `rings` by `rule`.
    fn clip_path(&mut self, id: &str, rings: &[Vec<Pt>], rule: FillRule) -> Result<(), SvgError> {
        self.charge(0)?;
        let _ = write!(self.out, "<clipPath id=\"{id}\">");
        self.ring_path(rings)?;
        self.out.push_str(match rule {
            FillRule::EvenOdd => " clip-rule=\"evenodd\"/>",
            FillRule::NonZero => " clip-rule=\"nonzero\"/>",
        });
        self.out.push_str("</clipPath>\n");
        Ok(())
    }

    /// Opens a group clipped to the pattern rings `rings` (even-odd, as the PDF clips them), defining the clip
    /// path on its first use.
    fn open_clip(&mut self, rings: &Arc<[Vec<Pt>]>, blend: bool) -> Result<(), SvgError> {
        let key = Arc::as_ptr(rings).cast::<()>().addr();
        let id = match self.clips.get(&key) {
            Some(&id) => id,
            None => {
                let id = self.clips.len();
                self.clips.insert(key, id);
                self.clip_path(&format!("c{id}"), rings, FillRule::EvenOdd)?;
                id
            }
        };
        self.charge(0)?;
        let _ = write!(self.out, "<g clip-path=\"url(#c{id})\"");
        if blend {
            self.out.push_str(MULTIPLY);
        }
        self.out.push_str(">\n");
        Ok(())
    }

    /// Paints `shape` in `cmyk`; with `blend` every element multiplies with what lies under it, as the PDF's
    /// overprint graphics state does.
    fn paint(&mut self, cmyk: [f64; 4], shape: &Shape, blend: bool) -> Result<(), SvgError> {
        match shape {
            Shape::Fill { rings, rule } => {
                self.ring_path(rings)?;
                self.out.push_str(" fill=\"");
                self.colour(cmyk);
                self.out.push('"');
                if *rule == FillRule::EvenOdd {
                    self.out.push_str(" fill-rule=\"evenodd\"");
                }
            }
            Shape::Stroke {
                path,
                closed,
                width,
                cap,
                join,
            } => {
                self.charge(path.len())?;
                self.out.push_str("<path d=\"");
                self.polylines([path], *closed);
                self.out.push_str("\" fill=\"none\" stroke=\"");
                self.colour(cmyk);
                self.out.push('"');
                self.attr_num("stroke-width", *width);
                self.out.push_str(match cap {
                    Cap::Flat => " stroke-linecap=\"butt\"",
                    Cap::Round => " stroke-linecap=\"round\"",
                    Cap::Square => " stroke-linecap=\"square\"",
                });
                self.out.push_str(match join {
                    Join::Miter => " stroke-linejoin=\"miter\"",
                    Join::Round => " stroke-linejoin=\"round\"",
                    Join::Bevel => " stroke-linejoin=\"bevel\"",
                });
                if *join == Join::Miter {
                    self.attr_num("stroke-miterlimit", f64::from(MITER_LIMIT));
                }
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
                self.text(*origin, *size, face(*bold, *italic), text, *rotation)?;
                match stroke {
                    // Text line framing: the outlines stroked, miter-joined with OOM's limit for framing.
                    Some(width) => {
                        self.out.push_str(" fill=\"none\" stroke=\"");
                        self.colour(cmyk);
                        self.out.push('"');
                        self.attr_num("stroke-width", *width);
                        self.out.push_str(" stroke-linejoin=\"miter\"");
                        self.attr_num("stroke-miterlimit", f64::from(TEXT_FRAMING_MITER_LIMIT));
                    }
                    None => {
                        self.out.push_str(" fill=\"");
                        self.colour(cmyk);
                        self.out.push('"');
                    }
                }
            }
            Shape::Clipped { rings, inner } => {
                self.open_clip(rings, blend)?;
                self.paint(cmyk, inner, blend)?;
                self.out.push_str("</g>\n");
                return Ok(());
            }
        }
        if blend {
            self.out.push_str(MULTIPLY);
        }
        self.out.push_str("/>\n");
        Ok(())
    }

    /// `<path d="…"` with the glyph outlines of `text`, placed and turned as the PDF's text matrix places them.
    fn text(
        &mut self,
        origin: Pt,
        size: f64,
        font: crate::fonts::FontFace,
        text: &str,
        rotation: f64,
    ) -> Result<(), SvgError> {
        let mut segments = Vec::new();
        text_outline(text, font, |s| segments.push(s));
        let points = segments
            .iter()
            .map(|s| match s {
                OutlineSegment::Move(_) | OutlineSegment::Line(_) => 1,
                OutlineSegment::Quad(..) => 2,
                OutlineSegment::Cubic(..) => 3,
                OutlineSegment::Close => 0,
            })
            .sum();
        self.charge(points)?;
        // Text coordinates (em, x along the baseline, y down) turned counter-clockwise on paper, as in
        // `bounds::shape_bounds`.
        let (sin, cos) = rotation.sin_cos();
        let place = |(x, y): Pt| {
            let (x, y) = (x * size, y * size);
            (origin.0 + x * cos + y * sin, origin.1 - x * sin + y * cos)
        };
        self.out.push_str("<path d=\"");
        for s in segments {
            match s {
                OutlineSegment::Move(p) => {
                    self.out.push('M');
                    self.pt(place(p));
                }
                OutlineSegment::Line(p) => {
                    self.out.push('L');
                    self.pt(place(p));
                }
                OutlineSegment::Quad(c, p) => {
                    self.out.push('Q');
                    self.pt(place(c));
                    self.out.push(' ');
                    self.pt(place(p));
                }
                OutlineSegment::Cubic(c1, c2, p) => {
                    self.out.push('C');
                    self.pt(place(c1));
                    self.out.push(' ');
                    self.pt(place(c2));
                    self.out.push(' ');
                    self.pt(place(p));
                }
                OutlineSegment::Close => self.out.push('Z'),
            }
        }
        self.out.push('"');
        Ok(())
    }

    /// Paints `shapes` in order, as the PDF's paint run does: consecutive shapes clipped to the same rings share
    /// one clip group, nested one level for each clip of a pattern inside a pattern.
    fn run<'a>(
        &mut self,
        shapes: impl IntoIterator<Item = ([f64; 4], &'a Shape)>,
        blend: bool,
    ) -> Result<(), SvgError> {
        let mut open: Vec<&'a Arc<[Vec<Pt>]>> = Vec::new();
        for (cmyk, shape) in shapes {
            let mut chain = Vec::new();
            let mut shape = shape;
            while let Shape::Clipped { rings, inner } = shape {
                chain.push(rings);
                shape = inner;
            }
            let kept = open
                .iter()
                .zip(&chain)
                .take_while(|(a, b)| Arc::ptr_eq(a, b))
                .count();
            for _ in kept..open.len() {
                self.out.push_str("</g>\n");
            }
            open.truncate(kept);
            for rings in &chain[kept..] {
                self.open_clip(rings, blend)?;
                open.push(rings);
            }
            self.paint(cmyk, shape, blend)?;
        }
        for _ in &open {
            self.out.push_str("</g>\n");
        }
        Ok(())
    }

    /// The map primitives `items` inside one group per mask (`masks` defined as `m0`, `m1`, …): nested clips
    /// intersect, so the map is hidden in the union of the masked areas, as in the PDF. Nothing is written for
    /// no items.
    fn masked_run(
        &mut self,
        masks: &[MapClip],
        colours: &[RenderColour],
        items: &[&Primitive],
    ) -> Result<(), SvgError> {
        if items.is_empty() {
            return Ok(());
        }
        for i in 0..masks.len() {
            self.charge(0)?;
            let _ = writeln!(self.out, "<g clip-path=\"url(#m{i})\">");
        }
        self.run(
            items
                .iter()
                .map(|p| (map_cmyk(colours, p.priority), &p.shape)),
            false,
        )?;
        for _ in masks {
            self.out.push_str("</g>\n");
        }
        Ok(())
    }

    /// `image` stretched over `rect`, as a PNG data URI.
    fn image(&mut self, rect: PaperBox, image: &Arc<RasterImage>) -> Result<(), SvgError> {
        self.charge(0)?;
        let key = Arc::as_ptr(image).cast::<()>().addr();
        if let Entry::Vacant(slot) = self.images.entry(key) {
            slot.insert(data_uri(image)?);
        }
        let (x0, x1) = (rect.min.0.min(rect.max.0), rect.min.0.max(rect.max.0));
        let (y0, y1) = (rect.min.1.min(rect.max.1), rect.min.1.max(rect.max.1));
        self.out.push_str("<image");
        self.attr_num("x", x0);
        self.attr_num("y", y0);
        self.attr_num("width", x1 - x0);
        self.attr_num("height", y1 - y0);
        self.out.push_str(" preserveAspectRatio=\"none\" href=\"");
        let uri = &self.images[&key];
        self.out.push_str(uri);
        self.out.push_str("\"/>\n");
        Ok(())
    }
}

/// `image` as `data:image/png;base64,…`, with its alpha channel when it has one.
fn data_uri(image: &RasterImage) -> Result<String, SvgError> {
    let mut png = Vec::new();
    let encoder = PngEncoder::new(&mut png);
    let (w, h) = (image.width(), image.height());
    let written = match image.alpha() {
        None => encoder.write_image(image.rgb(), w, h, image::ExtendedColorType::Rgb8),
        Some(alpha) => {
            let rgba: Vec<u8> = image
                .rgb()
                .as_chunks::<3>()
                .0
                .iter()
                .zip(alpha)
                .flat_map(|(&[r, g, b], &a)| [r, g, b, a])
                .collect();
            encoder.write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
        }
    };
    written.map_err(|e| SvgError::Image(e.to_string()))?;
    let mut uri = String::from("data:image/png;base64,");
    base64(&png, &mut uri);
    Ok(uri)
}

/// Standard base64 with padding (RFC 4648).
fn base64(bytes: &[u8], out: &mut String) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
}

/// True when the boxes share a point.
fn overlaps(b: Bounds, r: PaperBox) -> bool {
    b.0.0 <= r.max.0 && r.min.0 <= b.1.0 && b.0.1 <= r.max.1 && r.min.1 <= b.1.1
}

/// The common part of two boxes, `None` when they do not meet.
fn common(a: PaperBox, b: PaperBox) -> Option<PaperBox> {
    let min = (a.min.0.max(b.min.0), a.min.1.max(b.min.1));
    let max = (a.max.0.min(b.max.0), a.max.1.min(b.max.1));
    (min.0 <= max.0 && min.1 <= max.1).then_some(PaperBox { min, max })
}

/// The primitives of `part` that paint somewhere in `r`.
fn shown(part: &[Primitive], r: PaperBox) -> Vec<&Primitive> {
    part.iter().filter(|p| visible(&p.shape, r)).collect()
}

/// True when `shape` may paint somewhere in `r`, by the rule the PDF backend leaves shapes out with
/// ([`may_show`], ADR 0031).
fn visible(shape: &Shape, r: PaperBox) -> bool {
    may_show(shape, r)
}

/// `page` as an SVG document showing `view`.
///
/// The layers come in the order of the PDF backend: a white rectangle for the paper; then, clipped to
/// [`RenderedPage::clip`], the map primitives in paint order with the lower overprint layer before the first
/// primitive of [`crate::page::Overprint::below_priority`], then the upper overprint layer; then the sheet items,
/// unclipped. The masks clip the map only, through `clip-path` with `clip-rule="nonzero"` from
/// [`MapClip::rings`]. Every overprint element multiplies with what lies under it (`mix-blend-mode:multiply`).
///
/// Each map primitive, overprint shape and sheet shape that may paint inside the view (and, for the map and the
/// overprint, inside the clip) becomes one `<path>` element; clip paths hold further `<path>`s. A shape whose ink
/// box ([`crate::bounds`], ADR 0031) lies wholly outside is left out, by the rule the PDF backend uses. The same
/// page and view always give the same text.
///
/// Fails for a zoom that is not above 0, a rectangle that is not finite or whose minimum lies beyond its
/// maximum, a number on the page that is not finite, an image that cannot be encoded, and a document that would
/// exceed [`MAX_UNITS`].
pub fn write_svg(page: &RenderedPage, view: &SvgView) -> Result<String, SvgError> {
    write_within(page, view, MAX_UNITS)
}

/// [`write_svg`] within `max_units` units.
fn write_within(page: &RenderedPage, view: &SvgView, max_units: usize) -> Result<String, SvgError> {
    if !page.page_mm.0.is_finite() || !page.page_mm.1.is_finite() {
        return Err(SvgError::NotFinite);
    }
    let whole = PaperBox {
        min: (0.0, 0.0),
        max: page.page_mm,
    };
    let rect = view.rect.unwrap_or(whole);
    let finite = [rect.min.0, rect.min.1, rect.max.0, rect.max.1, view.zoom]
        .iter()
        .all(|v| v.is_finite());
    if !finite || view.zoom <= 0.0 || rect.min.0 > rect.max.0 || rect.min.1 > rect.max.1 {
        return Err(SvgError::InvalidView);
    }
    let (w, h) = (rect.max.0 - rect.min.0, rect.max.1 - rect.min.1);
    let mut svg = Svg::new(max_units);
    svg.charge(0)?;
    svg.out
        .push_str("<svg xmlns=\"http://www.w3.org/2000/svg\"");
    svg.attr_num("width", w * view.zoom);
    svg.attr_num("height", h * view.zoom);
    svg.out.push_str(" viewBox=\"");
    svg.pt(rect.min);
    svg.out.push(' ');
    svg.pt((w, h));
    svg.out.push_str("\">\n");
    // The paper.
    svg.charge(0)?;
    svg.out.push_str("<rect");
    svg.attr_num("x", 0.0);
    svg.attr_num("y", 0.0);
    svg.attr_num("width", page.page_mm.0);
    svg.attr_num("height", page.page_mm.1);
    svg.out.push_str(" fill=\"#ffffff\"/>\n");

    let items = &page.primitives;
    let overprint = &page.overprint;
    // The map and the overprint, inside the frame and the view.
    let map_box = (!items.is_empty() || !overprint.is_empty())
        .then(|| common(page.clip, rect))
        .flatten();
    if let Some(map_box) = map_box {
        let clip = page.clip;
        svg.charge(0)?;
        svg.out.push_str("<clipPath id=\"frame\"><rect");
        svg.attr_num("x", clip.min.0);
        svg.attr_num("y", clip.min.1);
        svg.attr_num("width", clip.max.0 - clip.min.0);
        svg.attr_num("height", clip.max.1 - clip.min.1);
        svg.out.push_str("/></clipPath>\n");
        for (i, mask) in page.map_clips.iter().enumerate() {
            svg.clip_path(&format!("m{i}"), mask.rings(), FillRule::NonZero)?;
        }
        svg.charge(0)?;
        svg.out.push_str("<g clip-path=\"url(#frame)\">\n");
        let split = match overprint.below_priority {
            Some(below) if !overprint.lower.is_empty() => items
                .iter()
                .position(|p| p.priority <= below)
                .unwrap_or(items.len()),
            _ => items.len(),
        };
        let purple = overprint.colour.cmyk;
        svg.masked_run(
            &page.map_clips,
            &page.colours,
            &shown(&items[..split], map_box),
        )?;
        svg.run(
            overprint
                .lower
                .iter()
                .filter(|s| visible(s, map_box))
                .map(|s| (purple, s)),
            true,
        )?;
        svg.masked_run(
            &page.map_clips,
            &page.colours,
            &shown(&items[split..], map_box),
        )?;
        svg.run(
            overprint
                .upper
                .iter()
                .filter(|s| visible(s, map_box))
                .map(|s| (purple, s)),
            true,
        )?;
        svg.out.push_str("</g>\n");
    }

    // The sheet layer: over the whole page, in its own colours.
    for item in &page.sheet {
        match item {
            SheetItem::Shape { colour, shape } => {
                if visible(shape, rect) {
                    svg.paint(colour.cmyk, shape, false)?;
                }
            }
            SheetItem::Image { rect: at, image } => {
                let b = (
                    (at.min.0.min(at.max.0), at.min.1.min(at.max.1)),
                    (at.min.0.max(at.max.0), at.min.1.max(at.max.1)),
                );
                if overlaps(b, rect) {
                    svg.image(*at, image)?;
                }
            }
        }
    }
    svg.out.push_str("</svg>\n");
    if svg.not_finite {
        return Err(SvgError::NotFinite);
    }
    Ok(svg.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_have_at_most_three_decimals() {
        let mut svg = Svg::new(MAX_UNITS);
        for v in [0.0, -0.0, 1.0, 1.5, 0.1234, -0.0004, 12.3456, 1e6, -2.25] {
            svg.num(v);
            svg.out.push(',');
        }
        assert_eq!(svg.out, "0,0,1,1.5,0.123,0,12.346,1000000,-2.25,");
        svg.num(f64::NAN);
        assert!(svg.not_finite);
    }

    #[test]
    fn a_page_beyond_the_budget_is_refused() {
        let triangle = Shape::Fill {
            rings: vec![vec![(1.0, 1.0), (2.0, 1.0), (2.0, 2.0)]],
            rule: FillRule::EvenOdd,
        };
        let page = RenderedPage {
            primitives: vec![Primitive::new(0, triangle); 100],
            clip: PaperBox {
                min: (0.0, 0.0),
                max: (10.0, 10.0),
            },
            page_mm: (10.0, 10.0),
            centre_mm: (0.0, 0.0),
            colours: Vec::new(),
            sheet: Vec::new(),
            overprint: crate::page::Overprint::default(),
            map_clips: Vec::new(),
        };
        // The document, the paper, the frame and its group take four units, each triangle one.
        assert!(write_within(&page, &SvgView::default(), 104).is_ok());
        assert!(matches!(
            write_within(&page, &SvgView::default(), 103),
            Err(SvgError::TooLarge)
        ));
    }

    #[test]
    fn base64_follows_rfc_4648() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            let mut out = String::new();
            base64(input.as_bytes(), &mut out);
            assert_eq!(out, expected);
        }
    }

    #[test]
    fn cmyk_converts_by_the_simple_formula() {
        assert_eq!(srgb([0.0, 0.0, 0.0, 0.0]), [255, 255, 255]);
        assert_eq!(srgb([0.0, 0.0, 0.0, 1.0]), [0, 0, 0]);
        assert_eq!(srgb([1.0, 0.0, 1.0, 0.0]), [0, 255, 0]);
        assert_eq!(srgb([0.0, 0.0, 0.0, 0.6]), [102, 102, 102]);
        assert_eq!(srgb([0.0, 1.0, 0.0, 0.0]), [255, 0, 255]);
        assert_eq!(srgb([2.0, -1.0, 0.5, 0.0]), [0, 255, 128]);
    }
}

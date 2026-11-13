//! Copied from freemapper 743ce4b (ADR 0014).
//! Drawing primitives as a one-page PDF: DeviceCMYK colours, painted in the page's order, texts in
//! embedded subsets of Arimo (freemapper-print ADR 0017), paths with rounded coordinates and small shapes relative to
//! their first point (ADR 0032).

use std::cell::Cell;
use std::collections::BTreeSet;
use std::sync::Arc;

use pdf_writer::types::{BlendMode, LineCapStyle, LineJoinStyle, TextRenderingMode};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::bounds::may_show;
use crate::draw::{FillRule, Primitive, Shape};
use crate::fonts::{PageFonts, face};
use crate::geometry::Pt;
use crate::image::RasterImage;
use crate::page::{MapClip, PaperBox, RenderedPage, SheetItem};
use crate::symbols::{Cap, Join, RenderColour};

/// Points per millimetre.
const PT_PER_MM: f64 = 72.0 / 25.4;
/// OOM's miter limit for PDF (reference §2.1).
pub(crate) const MITER_LIMIT: f32 = 2.236_068;
/// OOM's miter limit for text framing in PDF: Qt 0.5 → √(1 + 4 · 0.5²) = √2 (reference §2.1).
pub(crate) const TEXT_FRAMING_MITER_LIMIT: f32 = std::f32::consts::SQRT_2;
/// Path coordinates are written in steps of 1/1000 point, 0.00035 mm (ADR 0032).
const PATH_STEPS_PER_PT: f64 = 1000.0;
/// A fill or stroke whose points all fit in a square of this side, in page mm, is small ([`is_small`]): a dot or another
/// part of a point symbol, a control circle. It is written relative to its first point ([`small_origin`]).
pub(crate) const SMALL_MM: f64 = 7.0;
/// A small fill or stroke of at least this many points is written relative to its first point; with fewer, the
/// translation would take more room than it saves.
const MIN_RELATIVE_POINTS: usize = 4;

/// Page mm (y down) → page points (y up). Every number written to the page goes through
/// [`Transform::num`], which notes one that is not finite as an `f32`.
struct Transform {
    page_height: f64,
    /// Paths are written compactly (ADR 0032): coordinates rounded to [`PATH_STEPS_PER_PT`], small shapes relative to
    /// their first point ([`small_origin`]).
    compact: bool,
    /// A number was not finite: the page is refused ([`PdfError::NotFinite`]).
    not_finite: Cell<bool>,
}

impl Transform {
    fn new(page_height: f64, compact: bool) -> Self {
        Transform {
            page_height,
            compact,
            not_finite: Cell::new(false),
        }
    }

    /// `v` as written to the PDF. `inf` and `NaN` are no PDF numbers; one that is (or overflows
    /// `f32` into) one is a bug upstream, noted here and refused by [`write_pdf`].
    fn num(&self, v: f64) -> f32 {
        let n = v as f32;
        if !n.is_finite() {
            self.not_finite.set(true);
            return 0.0;
        }
        n
    }

    /// A length in mm as points.
    fn len(&self, mm: f64) -> f32 {
        self.num(mm * PT_PER_MM)
    }

    fn pt(&self, p: Pt) -> (f32, f32) {
        (
            self.num(p.0 * PT_PER_MM),
            self.num((self.page_height - p.1) * PT_PER_MM),
        )
    }

    /// A point of a path: [`Transform::pt`], or with [`Transform::compact`] rounded to [`PATH_STEPS_PER_PT`] and, with
    /// an `origin`, relative to it (x right, y up). A number too large for that is still noted.
    fn path_pt(&self, p: Pt, origin: Option<Pt>) -> (f32, f32) {
        if !self.compact {
            return self.pt(p);
        }
        let round = |v: f64| self.num((v * PATH_STEPS_PER_PT).round() / PATH_STEPS_PER_PT);
        let (x0, y0) = origin.unwrap_or((0.0, self.page_height));
        (round((p.0 - x0) * PT_PER_MM), round((y0 - p.1) * PT_PER_MM))
    }
}

/// Writes the polyline `points`, closed with `h` when `close`, relative to `origin` if there is one
/// ([`small_origin`]).
fn path(content: &mut Content, t: &Transform, points: &[Pt], close: bool, origin: Option<Pt>) {
    for (i, &p) in points.iter().enumerate() {
        let (x, y) = t.path_pt(p, origin);
        if i == 0 {
            content.move_to(x, y);
        } else {
            content.line_to(x, y);
        }
    }
    if close {
        content.close_path();
    }
}

/// Opens a clip block: `q`, the rings, `W* n` (even-odd, as the area fill).
fn begin_clip(content: &mut Content, t: &Transform, rings: &[Vec<Pt>]) {
    content.save_state();
    for ring in rings {
        path(content, t, ring, true, None);
    }
    content.clip_even_odd();
    content.end_path();
}

/// The CMYK of the map colour with `priority` (black if the page has none).
pub(crate) fn map_cmyk(colours: &[RenderColour], priority: i32) -> [f64; 4] {
    colours
        .iter()
        .find(|c| c.priority == priority)
        .map_or([0.0, 0.0, 0.0, 1.0], |c| c.cmyk)
}

/// Whether `points` are finite and fit in a square of side [`SMALL_MM`].
fn is_small<'a>(points: impl IntoIterator<Item = &'a Pt>) -> bool {
    let mut points = points.into_iter();
    let finite = |p: Pt| p.0.is_finite() && p.1.is_finite();
    let Some(&first) = points.next() else {
        return false;
    };
    if !finite(first) {
        return false;
    }
    let (mut low, mut high) = (first, first);
    for &p in points {
        if !finite(p) {
            return false;
        }
        low = (low.0.min(p.0), low.1.min(p.1));
        high = (high.0.max(p.0), high.1.max(p.1));
    }
    high.0 - low.0 < SMALL_MM && high.1 - low.1 < SMALL_MM
}

/// The first point of a small fill or stroke with `points`, when [`Transform::compact`] writes it relative to that point
/// (ADR 0032): finite points that fit in a square of [`SMALL_MM`], at least [`MIN_RELATIVE_POINTS`] of them. The
/// repeated parts of point symbols then give the same numbers each time, which deflate stores once.
fn small_origin<'a>(t: &Transform, points: impl Iterator<Item = &'a Pt> + Clone) -> Option<Pt> {
    let first = *points.clone().next()?;
    (t.compact && points.clone().nth(MIN_RELATIVE_POINTS - 1).is_some() && is_small(points))
        .then_some(first)
}

/// Opens the graphics state of a shape written relative to `origin`: `q` and a translation to it. [`end_relative`]
/// closes it.
fn begin_relative(content: &mut Content, t: &Transform, origin: Option<Pt>) {
    if let Some(o) = origin {
        let (x, y) = t.path_pt(o, None);
        content.save_state();
        content.transform([1.0, 0.0, 0.0, 1.0, x, y]);
    }
}

fn end_relative(content: &mut Content, origin: Option<Pt>) {
    if origin.is_some() {
        content.restore_state();
    }
}

/// Paints `shape` in `cmyk`. A clipped shape gets its own clip block here;
/// [`write_pdf`] shares one block among consecutive shapes with the same clip instead.
fn paint(
    content: &mut Content,
    t: &Transform,
    cmyk: [f64; 4],
    shape: &Shape,
    fonts: &mut PageFonts,
) {
    let [c, m, y, k] = cmyk.map(|v| t.num(v));
    match shape {
        Shape::Fill { rings, rule } => {
            content.set_fill_cmyk(c, m, y, k);
            let origin = small_origin(t, rings.iter().flatten());
            begin_relative(content, t, origin);
            for ring in rings {
                path(content, t, ring, true, origin);
            }
            match rule {
                FillRule::EvenOdd => content.fill_even_odd(),
                FillRule::NonZero => content.fill_nonzero(),
            };
            end_relative(content, origin);
        }
        Shape::Stroke {
            path: points,
            closed,
            width,
            cap,
            join,
        } => {
            content.set_stroke_cmyk(c, m, y, k);
            content.set_line_width(t.len(*width));
            content.set_line_cap(match cap {
                Cap::Flat => LineCapStyle::ButtCap,
                Cap::Round => LineCapStyle::RoundCap,
                Cap::Square => LineCapStyle::ProjectingSquareCap,
            });
            content.set_line_join(match join {
                Join::Miter => LineJoinStyle::MiterJoin,
                Join::Round => LineJoinStyle::RoundJoin,
                Join::Bevel => LineJoinStyle::BevelJoin,
            });
            content.set_miter_limit(MITER_LIMIT);
            let origin = small_origin(t, points.iter());
            begin_relative(content, t, origin);
            path(content, t, points, *closed, origin);
            content.stroke();
            end_relative(content, origin);
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
            let (x, y0) = t.pt(*origin);
            let font_face = face(*bold, *italic);
            let cids = fonts.encode(font_face, text);
            match stroke {
                // Text line framing: the glyph outlines stroked, miter-joined with OOM's limit
                // for framing; in its own graphics state, which holds the render mode.
                Some(width) => {
                    content.save_state();
                    content.set_stroke_cmyk(c, m, y, k);
                    content.set_line_width(t.len(*width));
                    content.set_line_join(LineJoinStyle::MiterJoin);
                    content.set_miter_limit(TEXT_FRAMING_MITER_LIMIT);
                }
                None => {
                    content.set_fill_cmyk(c, m, y, k);
                }
            }
            content.begin_text();
            content.set_font(PageFonts::resource_name(font_face), t.len(*size));
            if stroke.is_some() {
                content.set_text_rendering_mode(TextRenderingMode::Stroke);
            }
            // Counter-clockwise on paper is counter-clockwise in PDF too.
            let (sin, cos) = rotation.sin_cos();
            let (cos, sin) = (t.num(cos), t.num(sin));
            content.set_text_matrix([cos, sin, -sin, cos, x, y0]);
            content.show(Str(&cids));
            content.end_text();
            if stroke.is_some() {
                content.restore_state();
            }
        }
        Shape::Clipped { rings, inner } => {
            begin_clip(content, t, rings);
            paint(content, t, cmyk, inner, fonts);
            content.restore_state();
        }
    }
}

/// The resource name of the `index`-th image of a page.
fn image_name(index: usize) -> String {
    format!("Im{index}")
}

/// `image` as a Flate-compressed DeviceRGB XObject, with a DeviceGray soft mask for its alpha.
fn write_image(pdf: &mut Pdf, image: &RasterImage, id: Ref, mask: Option<Ref>, level: u8) {
    // The whole image is held in memory; pixel limits are enforced where images are decoded (Task 8).
    let data = miniz_oxide::deflate::compress_to_vec_zlib(image.rgb(), level);
    let mut x = pdf.image_xobject(id, &data);
    x.filter(Filter::FlateDecode);
    x.width(image.width() as i32);
    x.height(image.height() as i32);
    x.color_space().device_rgb();
    x.bits_per_component(8);
    if let Some(mask) = mask {
        x.s_mask(mask);
    }
    x.finish();
    if let (Some(mask), Some(alpha)) = (mask, image.alpha()) {
        let data = miniz_oxide::deflate::compress_to_vec_zlib(alpha, level);
        let mut x = pdf.image_xobject(mask, &data);
        x.filter(Filter::FlateDecode);
        x.width(image.width() as i32);
        x.height(image.height() as i32);
        x.color_space().device_gray();
        x.bits_per_component(8);
        x.finish();
    }
}

/// A page that could not be written.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("could not embed the font {face}: {message}")]
    Font { face: &'static str, message: String },
    /// A number on the page is infinite or not a number, or too large for a PDF number. That is
    /// a bug: the project limits keep real input far below it.
    #[error("a number on the page is not finite; the page is not written")]
    NotFinite,
}

/// What a written page could not show as asked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PdfInfo {
    /// Characters of the page's texts that have no glyph in the embedded font (drawn as `.notdef`).
    pub missing_glyphs: BTreeSet<char>,
}

/// The document information a PDF carries about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfMeta {
    pub title: String,
    pub creator: String,
    /// Deflate level (0 to 10) of the page content and image streams: 6 for files; `check` builds the PDF only
    /// to learn what the page cannot show and uses [`PdfMeta::FAST`].
    pub compression: u8,
}

impl PdfMeta {
    /// `title` with freemapper-print and its link as the creator ([`crate::CREDIT`]).
    pub fn new(title: &str) -> Self {
        PdfMeta {
            title: title.to_string(),
            creator: crate::CREDIT.to_string(),
            compression: 6,
        }
    }

    /// The fastest compression, for a PDF that is thrown away.
    pub const FAST: u8 = 0;

    /// `self` with the page streams compressed at `level`.
    pub fn compressed(self, level: u8) -> Self {
        PdfMeta {
            compression: level,
            ..self
        }
    }
}

/// `primitives` with the CMYK of their map colours.
fn map_run<'a>(
    colours: &'a [RenderColour],
    primitives: &'a [Primitive],
) -> impl Iterator<Item = ([f64; 4], &'a Shape)> + 'a {
    primitives
        .iter()
        .map(|p| (map_cmyk(colours, p.priority), &p.shape))
}

/// The resource name of the graphics state that paints the course overprint with the blend mode
/// Multiply.
const MULTIPLY: &[u8] = b"Mul";

/// Paints `shapes` in order, each in its CMYK. Shapes clipped to the same rings share one clip
/// block, nested in it one for each run of shapes clipped to the same rings (the lines of one area
/// pattern); every block is closed at the end.
fn paint_run<'a>(
    content: &mut Content,
    t: &Transform,
    shapes: impl IntoIterator<Item = ([f64; 4], &'a Shape)>,
    fonts: &mut PageFonts,
) {
    // The clip blocks open, outermost first. A pattern inside a pattern clips its shapes twice;
    // each level stays open while the next shapes share it.
    let mut open: Vec<&'a Arc<[Vec<Pt>]>> = Vec::new();
    for (cmyk, shape) in shapes {
        let mut chain = Vec::new();
        let mut shape = shape;
        while let Shape::Clipped { rings, inner } = shape {
            chain.push(rings);
            shape = inner;
        }
        // The shapes of one pattern share their rings, also after the page was moved into place:
        // compare pointers only. Equal rings in separate `Arc`s get a block each; each is paid
        // for when drawn.
        let kept = open
            .iter()
            .zip(&chain)
            .take_while(|(a, b)| Arc::ptr_eq(a, b))
            .count();
        for _ in kept..open.len() {
            content.restore_state();
        }
        open.truncate(kept);
        for rings in &chain[kept..] {
            begin_clip(content, t, rings);
            open.push(rings);
        }
        paint(content, t, cmyk, shape, fonts);
    }
    for _ in &open {
        content.restore_state();
    }
}

/// Paints the map shapes `items` where every mask of `masks` leaves the map visible. Without masks this is
/// [`paint_run`] alone. With masks the run gets one clip block: `q`, then each mask's rings followed by `W n`
/// (nonzero), then the run, then `Q`. Successive clips in one graphics state intersect, so the map is hidden in the
/// union of the masked areas, and the nesting depth stays the same however many masks there are (ruling R10). The
/// map before and after the lower purple each get such a block, so the paint order is the same as without a mask
/// (ruling R11); an empty run writes nothing.
fn masked_run(
    content: &mut Content,
    t: &Transform,
    masks: &[MapClip],
    items: Vec<([f64; 4], &Shape)>,
    fonts: &mut PageFonts,
) {
    if masks.is_empty() {
        paint_run(content, t, items, fonts);
        return;
    }
    if items.is_empty() {
        return;
    }
    content.save_state();
    for mask in masks {
        for ring in mask.rings() {
            path(content, t, ring, true, None);
        }
        content.clip_nonzero();
        content.end_path();
    }
    paint_run(content, t, items, fonts);
    content.restore_state();
}

/// The part of the page that shows the map and the overprint: the page clip, its corners in either order, as far
/// as it lies on the page. `None` when that is not a finite rectangle; then nothing is left out.
fn visible_area(page: &RenderedPage) -> Option<PaperBox> {
    let (a, b) = (page.clip.min, page.clip.max);
    let min = (a.0.min(b.0).max(0.0), a.1.min(b.1).max(0.0));
    let max = (
        a.0.max(b.0).min(page.page_mm.0),
        a.1.max(b.1).min(page.page_mm.1),
    );
    let finite = [min.0, min.1, max.0, max.1].iter().all(|v| v.is_finite());
    (finite && min.0 <= max.0 && min.1 <= max.1).then_some(PaperBox { min, max })
}

/// The shapes of `shapes` that may put ink in `area` ([`may_show`]), in their order; all of them without an area.
///
/// A shape left out is still painted, into a stream that is thrown away. So a number on it that the PDF cannot
/// hold still refuses the page, and a character of its text that has no glyph is still reported, as when the
/// shape was written (ADR 0031). Its glyphs do not join the embedded subsets.
fn kept<'a>(
    t: &Transform,
    area: Option<PaperBox>,
    shapes: impl IntoIterator<Item = ([f64; 4], &'a Shape)>,
    fonts: &mut PageFonts,
) -> Vec<([f64; 4], &'a Shape)> {
    let Some(area) = area else {
        return shapes.into_iter().collect();
    };
    let mut kept = Vec::new();
    for (cmyk, shape) in shapes {
        if may_show(shape, area) {
            kept.push((cmyk, shape));
        } else {
            let mut unused = PageFonts::new();
            paint(&mut Content::new(), t, cmyk, shape, &mut unused);
            fonts.note_missing(unused);
        }
    }
    kept
}

/// One page drawn into a content stream, before any PDF object is written: the content, the images it uses, and
/// its MediaBox in points. The fonts it uses come beside it ([`page_body`]).
struct PageBody<'a> {
    content: pdf_writer::Buf,
    /// Each image once, in the order the content names them ([`image_name`]).
    images: Vec<&'a Arc<RasterImage>>,
    media_box: Rect,
    /// The page has a course overprint and needs the Multiply graphics state.
    overprint: bool,
}

/// The objects of one page: the references [`write_page`] writes them under.
struct PageRefs {
    page: Ref,
    contents: Ref,
    fonts: crate::fonts::FontObjects,
    images: Vec<(Ref, Option<Ref>)>,
    multiply: Option<Ref>,
}

/// How [`page_body`] writes a page. Files get [`Writing::FILE`]; tests compare it with the other ways.
#[derive(Debug, Clone, Copy)]
struct Writing {
    /// Map primitives and overprint shapes that cannot show inside the clip are left out ([`kept`], ADR 0031).
    cull: bool,
    /// Path coordinates are rounded and small shapes written relative to their first point ([`small_origin`],
    /// ADR 0032).
    compact: bool,
}

impl Writing {
    const FILE: Writing = Writing {
        cull: true,
        compact: true,
    };
}

/// Draws `page` into a content stream: every primitive clipped to `page.clip`, the course overprint among them,
/// the sheet layer on top, as `writing` says. Refuses a page with a number that is not finite before anything is
/// written.
fn page_body(page: &RenderedPage, writing: Writing) -> Result<(PageBody<'_>, PageFonts), PdfError> {
    let (page_w, page_h) = page.page_mm;
    let t = Transform::new(page_h, writing.compact);
    let colours = &page.colours;
    let area = if writing.cull {
        visible_area(page)
    } else {
        None
    };

    // The page's primitives are in paint order already ([`RenderedPage::primitives`]).
    let items = &page.primitives;

    let mut content = Content::new();
    let mut fonts = PageFonts::new();
    let overprint = &page.overprint;
    // One clip block for the page: the map and the course overprint lie inside the frame.
    let clipped = !items.is_empty() || !overprint.is_empty();
    if clipped {
        let (cx0, cy0) = t.pt((page.clip.min.0, page.clip.max.1));
        let (cx1, cy1) = t.pt((page.clip.max.0, page.clip.min.1));
        content.save_state();
        content.rect(cx0, cy0, cx1 - cx0, cy1 - cy0);
        content.clip_nonzero();
        content.end_path();
    }
    // The masks clip the map, never the overprint (ADR 0045): see [`masked_run`].
    let masks: &[MapClip] = if clipped { &page.map_clips } else { &[] };
    // The lower purple goes before the first primitive of the full-strength colour painted first
    // (ADR 0034), or after all of them. The place is found among all primitives, before any is left out.
    let split = match overprint.below_priority {
        Some(below) if !overprint.lower.is_empty() => items
            .iter()
            .position(|p| p.priority <= below)
            .unwrap_or(items.len()),
        _ => items.len(),
    };
    let below = kept(&t, area, map_run(colours, &items[..split]), &mut fonts);
    masked_run(&mut content, &t, masks, below, &mut fonts);
    let purple = overprint.colour.cmyk;
    let layer = |content: &mut Content, fonts: &mut PageFonts, shapes: &[Shape]| {
        let shapes = kept(&t, area, shapes.iter().map(|s| (purple, s)), fonts);
        if shapes.is_empty() {
            return;
        }
        content.save_state();
        content.set_parameters(Name(MULTIPLY));
        paint_run(content, &t, shapes, fonts);
        content.restore_state();
    };
    layer(&mut content, &mut fonts, &overprint.lower);
    let above = kept(&t, area, map_run(colours, &items[split..]), &mut fonts);
    masked_run(&mut content, &t, masks, above, &mut fonts);
    layer(&mut content, &mut fonts, &overprint.upper);
    if clipped {
        content.restore_state();
    }

    // The sheet layer: over the whole page, in its own colours, one graphics state per item.
    // Images are embedded once per `Arc`; the content names them by position.
    let mut images: Vec<&Arc<RasterImage>> = Vec::new();
    for item in &page.sheet {
        content.save_state();
        match item {
            SheetItem::Shape { colour, shape } => {
                paint(&mut content, &t, colour.cmyk, shape, &mut fonts);
            }
            SheetItem::Image { rect, image } => {
                let index = images
                    .iter()
                    .position(|i| Arc::ptr_eq(i, image))
                    .unwrap_or_else(|| {
                        images.push(image);
                        images.len() - 1
                    });
                let (x, y) = t.pt((rect.min.0, rect.max.1));
                let w = t.len(rect.max.0 - rect.min.0);
                let h = t.len(rect.max.1 - rect.min.1);
                content.transform([w, 0.0, 0.0, h, x, y]);
                content.x_object(Name(image_name(index).as_bytes()));
            }
        }
        content.restore_state();
    }
    let media_box = Rect::new(0.0, 0.0, t.len(page_w), t.len(page_h));
    if t.not_finite.get() {
        return Err(PdfError::NotFinite);
    }
    let body = PageBody {
        content: content.finish(),
        images,
        media_box,
        overprint: !overprint.is_empty(),
    };
    Ok((body, fonts))
}

/// The references of `body`'s images: an object each, and a second for a soft mask.
fn image_refs(body: &PageBody<'_>, next: &mut Ref) -> Vec<(Ref, Option<Ref>)> {
    body.images
        .iter()
        .map(|i| (next.bump(), i.alpha().is_some().then(|| next.bump())))
        .collect()
}

/// Writes the page object of `body` under `parent`, then its content stream, fonts, images and graphics state.
fn write_page(
    pdf: &mut Pdf,
    parent: Ref,
    body: &PageBody<'_>,
    refs: &PageRefs,
    level: u8,
) -> Result<(), PdfError> {
    let mut p = pdf.page(refs.page);
    p.media_box(body.media_box);
    p.parent(parent);
    p.contents(refs.contents);
    let mut resources = p.resources();
    resources.fonts().pairs(refs.fonts.resources());
    // Only a page with a course overprint has the graphics state, so other pages stay as they were.
    if let Some(id) = refs.multiply {
        resources.ext_g_states().pair(Name(MULTIPLY), id);
    }
    if !refs.images.is_empty() {
        let names: Vec<String> = (0..refs.images.len()).map(image_name).collect();
        resources.x_objects().pairs(
            names
                .iter()
                .zip(&refs.images)
                .map(|(name, (id, _))| (Name(name.as_bytes()), *id)),
        );
    }
    resources.finish();
    p.finish();
    // The content stream comes first, the font programs after it.
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&body.content, level);
    pdf.stream(refs.contents, &compressed)
        .filter(Filter::FlateDecode);
    refs.fonts.write(pdf)?;
    for (image, (id, mask)) in body.images.iter().zip(&refs.images) {
        write_image(pdf, image, *id, *mask, level);
    }
    if let Some(id) = refs.multiply {
        pdf.ext_graphics(id).blend_mode(BlendMode::Multiply);
    }
    Ok(())
}

fn write_info(pdf: &mut Pdf, id: Ref, meta: &PdfMeta) {
    pdf.document_info(id)
        .title(TextStr(&meta.title))
        .creator(TextStr(&meta.creator));
}

/// The one-page PDF of `page`: the MediaBox is the page size, one page millimetre is one PDF
/// millimetre, and every primitive is clipped to `page.clip`. Also returns what the page could
/// not show.
pub fn write_pdf(page: &RenderedPage, meta: &PdfMeta) -> Result<(Vec<u8>, PdfInfo), PdfError> {
    write_pdf_with(page, meta, Writing::FILE)
}

/// [`write_pdf`] the way `writing` says. Tests compare the ways.
fn write_pdf_with(
    page: &RenderedPage,
    meta: &PdfMeta,
    writing: Writing,
) -> Result<(Vec<u8>, PdfInfo), PdfError> {
    let (body, fonts) = page_body(page, writing)?;
    // References come from a counter, so a page holds only the fonts it uses.
    let mut next = Ref::new(1);
    let (catalog, pages, page, contents) = (next.bump(), next.bump(), next.bump(), next.bump());
    let fonts = fonts.finish(&mut next);
    let info = next.bump();
    let images = image_refs(&body, &mut next);
    let multiply = body.overprint.then(|| next.bump());
    let refs = PageRefs {
        page,
        contents,
        fonts,
        images,
        multiply,
    };
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(pages);
    pdf.pages(pages).kids([page]).count(1);
    write_page(&mut pdf, pages, &body, &refs, meta.compression)?;
    write_info(&mut pdf, info, meta);
    let info = PdfInfo {
        missing_glyphs: refs.fonts.missing,
    };
    Ok((pdf.finish(), info))
}

/// A PDF of several pages, written one page at a time: [`MultiPagePdf::add_page`] turns a page into PDF objects
/// at once, so the caller can drop it before drawing the next. Only the PDF bytes grow. Each page embeds its own
/// font subsets and images, as a one-page PDF of it would.
pub struct MultiPagePdf {
    pdf: Pdf,
    next: Ref,
    catalog: Ref,
    pages: Ref,
    kids: Vec<Ref>,
}

impl Default for MultiPagePdf {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiPagePdf {
    pub fn new() -> Self {
        let mut next = Ref::new(1);
        let (catalog, pages) = (next.bump(), next.bump());
        MultiPagePdf {
            pdf: Pdf::new(),
            next,
            catalog,
            pages,
            kids: Vec::new(),
        }
    }

    /// Appends `page` with its own MediaBox. A page refused for a number that is not finite leaves the document as
    /// it was; after a font that cannot be embedded the document is incomplete and must be dropped.
    pub fn add_page(&mut self, page: &RenderedPage) -> Result<PdfInfo, PdfError> {
        let (body, fonts) = page_body(page, Writing::FILE)?;
        let mut next = self.next;
        let (page_ref, contents) = (next.bump(), next.bump());
        let fonts = fonts.finish(&mut next);
        let images = image_refs(&body, &mut next);
        let multiply = body.overprint.then(|| next.bump());
        let refs = PageRefs {
            page: page_ref,
            contents,
            fonts,
            images,
            multiply,
        };
        write_page(&mut self.pdf, self.pages, &body, &refs, 6)?;
        self.next = next;
        self.kids.push(page_ref);
        Ok(PdfInfo {
            missing_glyphs: refs.fonts.missing,
        })
    }

    /// The number of pages added.
    pub fn len(&self) -> usize {
        self.kids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kids.is_empty()
    }

    /// The finished PDF, its pages in the order they were added.
    pub fn finish(mut self, meta: &PdfMeta) -> Vec<u8> {
        self.pdf.catalog(self.catalog).pages(self.pages);
        let count = i32::try_from(self.kids.len()).unwrap_or(i32::MAX);
        self.pdf
            .pages(self.pages)
            .kids(self.kids.iter().copied())
            .count(count);
        let info = self.next.bump();
        write_info(&mut self.pdf, info, meta);
        self.pdf.finish()
    }
}

/// The decompressed page content of a PDF written by [`write_pdf`] (for tests; feature `test-support`).
#[cfg(any(test, feature = "test-support"))]
pub fn content_of(pdf: &[u8]) -> Vec<u8> {
    let find = |needle: &[u8]| pdf.windows(needle.len()).position(|w| w == needle);
    let start = find(b"stream\n").expect("content stream") + 7;
    let end = find(b"\nendstream").expect("end of stream");
    miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]).expect("FlateDecode")
}

#[cfg(test)]
mod culling_pixels;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::Primitive;
    use crate::page::{DirectColour, PaperBox, RasterImage, SheetItem};

    fn colours() -> Vec<RenderColour> {
        vec![
            RenderColour {
                priority: 2,
                name: "Black".into(),
                cmyk: [0.0, 0.0, 0.0, 1.0],
                rgb: [0.0, 0.0, 0.0],
                opacity: 1.0,
            },
            RenderColour {
                priority: 9,
                name: "Black 60%".into(),
                cmyk: [0.0, 0.0, 0.0, 0.6],
                rgb: [0.4, 0.4, 0.4],
                opacity: 1.0,
            },
        ]
    }

    fn square(priority: i32) -> Primitive {
        Primitive::new(
            priority,
            Shape::Fill {
                rings: vec![vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0), (10.0, 20.0)]],
                rule: FillRule::EvenOdd,
            },
        )
    }

    fn clip() -> PaperBox {
        PaperBox {
            min: (0.0, 0.0),
            max: (100.0, 100.0),
        }
    }

    /// An A4 portrait page of `primitives`, clipped to [`clip`].
    fn page(primitives: &[Primitive]) -> RenderedPage {
        RenderedPage {
            primitives: primitives.to_vec(),
            clip: clip(),
            page_mm: (210.0, 297.0),
            centre_mm: (0.0, 0.0),
            colours: colours(),
            sheet: Vec::new(),
            overprint: Default::default(),
            map_clips: Vec::new(),
        }
    }

    fn write_info(primitives: &[Primitive], title: &str) -> (Vec<u8>, PdfInfo) {
        write_pdf(&page(primitives), &PdfMeta::new(title)).unwrap()
    }

    fn write(primitives: &[Primitive]) -> Vec<u8> {
        write_info(primitives, "Test").0
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    #[test]
    fn fast_compression_changes_only_the_bytes() {
        let square = Primitive::new(
            2,
            Shape::Stroke {
                path: vec![(10.0, 10.0), (50.0, 10.0), (50.0, 50.0)],
                closed: true,
                width: 0.3,
                cap: Cap::Flat,
                join: Join::Miter,
            },
        );
        let p = page(&[square]);
        let (normal, a) = write_pdf(&p, &PdfMeta::new("t")).unwrap();
        let (fast, b) = write_pdf(&p, &PdfMeta::new("t").compressed(PdfMeta::FAST)).unwrap();
        assert_eq!(content_of(&normal), content_of(&fast));
        assert_eq!(a, b);
        assert_eq!(PdfMeta::new("t").compression, 6);
    }

    #[test]
    fn numbers_beyond_f32_refuse_the_page() {
        let far = Primitive::new(
            2,
            Shape::Stroke {
                path: vec![(1e300, 0.0), (0.0, 0.0)],
                closed: false,
                width: 0.1,
                cap: Cap::Flat,
                join: Join::Miter,
            },
        );
        let err = write_pdf(&page(&[far]), &PdfMeta::new("t")).unwrap_err();
        assert!(matches!(err, PdfError::NotFinite), "{err}");
        // A sheet item too: a text of infinite size.
        let mut p = page(&[]);
        p.sheet.push(SheetItem::Shape {
            colour: DirectColour::BLACK,
            shape: Shape::Text {
                origin: (10.0, 10.0),
                size: f64::INFINITY,
                bold: false,
                italic: false,
                text: "x".into(),
                rotation: 0.0,
                stroke: None,
            },
        });
        let err = write_pdf(&p, &PdfMeta::new("t")).unwrap_err();
        assert!(matches!(err, PdfError::NotFinite), "{err}");
        // A logo whose box is NaN wide.
        let mut p = page(&[]);
        p.sheet.push(SheetItem::Image {
            rect: PaperBox {
                min: (0.0, 0.0),
                max: (f64::NAN, 1.0),
            },
            image: Arc::new(RasterImage::new(1, 1, vec![0, 0, 0], None).unwrap()),
        });
        assert!(matches!(
            write_pdf(&p, &PdfMeta::new("t")),
            Err(PdfError::NotFinite)
        ));
    }

    #[test]
    fn primitives_are_painted_in_the_given_order() {
        // `render_page` sorts by priority; the writer keeps the order it gets.
        let pdf = content_of(&write(&[square(2), square(9)]));
        let black = find(&pdf, b"0 0 0 1 k").expect("black fill");
        let grey = find(&pdf, b"0 0 0 0.6 k").expect("grey fill");
        assert!(black < grey, "the first primitive is painted first");
        assert!(find(&pdf, b"f*").is_some(), "even-odd fill");
        assert!(
            find(&pdf, b"re\nW\nn").is_some(),
            "map primitives are clipped"
        );
    }

    #[test]
    fn strokes_carry_width_caps_and_miter_limit() {
        let line = Primitive::new(
            2,
            Shape::Stroke {
                path: vec![(0.0, 0.0), (10.0, 0.0)],
                closed: false,
                width: 0.35,
                cap: Cap::Round,
                join: Join::Miter,
            },
        );
        let pdf = content_of(&write(&[line]));
        assert!(find(&pdf, b"0 0 0 1 K").is_some(), "stroke colour");
        assert!(find(&pdf, b"1 J").is_some(), "round cap");
        assert!(find(&pdf, b"2.236068 M").is_some(), "miter limit");
        assert!(find(&pdf, b"0.992126 w").is_some(), "0.35 mm in points");
    }

    fn clipped_line(y: f64) -> Primitive {
        Primitive::new(
            2,
            Shape::Clipped {
                rings: vec![vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0), (10.0, 20.0)]].into(),
                inner: Box::new(Shape::Stroke {
                    path: vec![(5.0, y), (25.0, y)],
                    closed: false,
                    width: 0.1,
                    cap: Cap::Flat,
                    join: Join::Miter,
                }),
            },
        )
    }

    #[test]
    fn clipped_shape_uses_clip_path() {
        let pdf = content_of(&write(&[clipped_line(15.0)]));
        let after = |from: usize, needle: &[u8]| {
            find(&pdf[from..], needle)
                .map(|i| from + i)
                .unwrap_or_else(|| panic!("{} after {from}", String::from_utf8_lossy(needle)))
        };
        // The page clip block comes first, then the pattern clip block nested in it.
        let page_clip = after(0, b"re\nW\nn\n");
        let save = after(page_clip, b"q\n");
        let ring = after(save, b" m\n");
        let clip = after(ring, b"h\nW*\nn\n");
        let stroke = after(clip, b"S\n");
        let restore = after(stroke, b"Q\n");
        assert!(save < ring && ring < clip && clip < stroke && stroke < restore);
    }

    #[test]
    fn clipped_shapes_with_one_outline_share_a_clip_block() {
        let first = clipped_line(11.0);
        let Shape::Clipped { rings, .. } = &first.shape else {
            panic!()
        };
        let shared = Arc::clone(rings);
        let mut lines: Vec<Primitive> = (1..5)
            .map(|i| {
                let Shape::Clipped { inner, .. } = clipped_line(11.0 + f64::from(i)).shape else {
                    panic!()
                };
                Primitive::new(
                    2,
                    Shape::Clipped {
                        rings: Arc::clone(&shared),
                        inner,
                    },
                )
            })
            .collect();
        lines.insert(0, first);
        let pdf = content_of(&write(&lines));
        let clips = pdf.windows(4).filter(|w| w == b"W*\nn").count();
        let strokes = pdf.windows(2).filter(|w| w == b"S\n").count();
        assert_eq!((clips, strokes), (1, 5));
        // Equal rings in separate `Arc`s are not compared point by point: a block each.
        let separate: Vec<Primitive> = (0..2).map(|i| clipped_line(11.0 + f64::from(i))).collect();
        let pdf = content_of(&write(&separate));
        assert_eq!(pdf.windows(4).filter(|w| w == b"W*\nn").count(), 2);
    }

    #[test]
    fn nested_clips_share_their_blocks_too() {
        // Lines of a pattern inside a pattern: clipped to the outer outline and to their own.
        // Each outline is written once, though both clips wrap every line.
        let outer: Arc<[Vec<Pt>]> = vec![vec![(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)]].into();
        let inner: Arc<[Vec<Pt>]> = vec![vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)]].into();
        let lines: Vec<Primitive> = (0..5)
            .map(|i| {
                let Shape::Clipped { inner: line, .. } = clipped_line(11.0 + f64::from(i)).shape
                else {
                    panic!()
                };
                Primitive::new(
                    2,
                    Shape::Clipped {
                        rings: Arc::clone(&outer),
                        inner: Box::new(Shape::Clipped {
                            rings: Arc::clone(&inner),
                            inner: line,
                        }),
                    },
                )
            })
            .collect();
        let pdf = content_of(&write(&lines));
        let clips = pdf.windows(4).filter(|w| w == b"W*\nn").count();
        let strokes = pdf.windows(2).filter(|w| w == b"S\n").count();
        let ops = |op: &[u8]| pdf.split(|&b| b == b'\n').filter(|l| *l == op).count();
        let (saves, restores) = (ops(b"q"), ops(b"Q"));
        assert_eq!((clips, strokes), (2, 5));
        assert_eq!((saves, restores), (3, 3), "the page and both outlines");
    }

    fn text(rotation: f64, stroke: Option<f64>) -> Primitive {
        Primitive::new(
            2,
            Shape::Text {
                origin: (10.0, 10.0),
                size: 2.5,
                bold: false,
                italic: false,
                text: "Wald".into(),
                rotation,
                stroke,
            },
        )
    }

    #[test]
    fn line_framing_strokes_the_glyphs() {
        let pdf = content_of(&write(&[text(0.0, Some(0.3)), text(0.0, None)]));
        // After the page clip block `q … re W n`.
        let body = find(&pdf, b"re\nW\nn\n").expect("page clip");
        let at = |needle: &[u8]| {
            body + find(&pdf[body..], needle)
                .unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(needle)))
        };
        // Stroke only (render mode 1), framing colour, 2 × half width, miter join with OOM's
        // text framing limit √2 [RI `TextFramingRenderable`], inside q … Q so that the fill text
        // after it is not stroked.
        let (save, mode, restore) = (at(b"q\n"), at(b"1 Tr"), at(b"Q\n"));
        assert!(save < mode && mode < restore);
        assert!(find(&pdf, b"0 0 0 1 K").is_some());
        assert!(find(&pdf, b"0.8503937 w").is_some(), "0.3 mm");
        assert!(find(&pdf, b"1.4142135 M").is_some());
        assert_eq!(pdf.windows(2).filter(|w| w == b"Tj").count(), 2);
        assert!(
            find(&pdf[restore..], b"Tr").is_none(),
            "fill text in the default mode"
        );
    }

    #[test]
    fn rotated_text_turns_its_text_matrix() {
        let pdf = content_of(&write(&[text(std::f64::consts::FRAC_PI_6, None)]));
        // Counter-clockwise on paper is counter-clockwise in PDF (y up).
        let tm = String::from_utf8_lossy(&pdf);
        let line = tm.lines().find(|l| l.ends_with("Tm")).unwrap();
        let v: Vec<f64> = line
            .split_whitespace()
            .take(4)
            .map(|x| x.parse().unwrap())
            .collect();
        let (s, c) = std::f64::consts::FRAC_PI_6.sin_cos();
        for (a, b) in v.iter().zip([c, s, -s, c]) {
            assert!((a - b).abs() < 1e-6, "{line}");
        }
    }

    #[test]
    fn content_is_compressed_and_clip_blocks_are_shared() {
        let many: Vec<Primitive> = (0..200).map(|_| square(9)).collect();
        let pdf = write(&many);
        assert!(find(&pdf, b"/FlateDecode").is_some());
        let start = find(&pdf, b"stream\n").unwrap() + 7;
        let end = find(&pdf, b"\nendstream").unwrap();
        let content = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]).unwrap();
        let clips = content.windows(5).filter(|w| w == b"re\nW\n").count();
        assert_eq!(clips, 1, "one clip block for a run of map primitives");
        assert!(find(&content, b"0 0 0 0.6 k").is_some());
    }

    fn plain_text(text: &str, bold: bool, italic: bool) -> Primitive {
        Primitive::new(
            2,
            Shape::Text {
                origin: (10.0, 10.0),
                size: 2.5,
                bold,
                italic,
                text: text.into(),
                rotation: 0.0,
                stroke: None,
            },
        )
    }

    /// Every stream of `pdf`, decompressed (all streams written here are FlateDecode).
    fn streams(pdf: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(start) = find(&pdf[from..], b"stream\n").map(|i| from + i + 7) {
            let end = start + find(&pdf[start..], b"\nendstream").expect("end of stream");
            out.push(
                miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end])
                    .expect("FlateDecode"),
            );
            from = end + 10;
        }
        out
    }

    /// The string operands of a content stream, in order (literal and hex strings).
    fn shown_strings(content: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < content.len() {
            match content[i] {
                b'(' => {
                    let (mut s, mut depth) = (Vec::new(), 1);
                    i += 1;
                    loop {
                        match content[i] {
                            b'\\' => {
                                i += 1;
                                match content[i] {
                                    b'n' => s.push(b'\n'),
                                    b'r' => s.push(b'\r'),
                                    b't' => s.push(b'\t'),
                                    b'b' => s.push(8),
                                    b'f' => s.push(12),
                                    d @ b'0'..=b'7' => {
                                        let mut v = u32::from(d - b'0');
                                        for _ in 0..2 {
                                            if matches!(content[i + 1], b'0'..=b'7') {
                                                i += 1;
                                                v = v * 8 + u32::from(content[i] - b'0');
                                            }
                                        }
                                        s.push(v as u8);
                                    }
                                    c => s.push(c),
                                }
                            }
                            b'(' => {
                                depth += 1;
                                s.push(b'(');
                            }
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                s.push(b')');
                            }
                            c => s.push(c),
                        }
                        i += 1;
                    }
                    out.push(s);
                }
                b'<' if content.get(i + 1) != Some(&b'<') => {
                    let end = i + find(&content[i..], b">").unwrap();
                    let hex: Vec<u8> = content[i + 1..end]
                        .iter()
                        .copied()
                        .filter(u8::is_ascii_hexdigit)
                        .collect();
                    out.push(
                        hex.chunks(2)
                            .map(|h| {
                                u8::from_str_radix(std::str::from_utf8(h).unwrap(), 16).unwrap()
                            })
                            .collect(),
                    );
                    i = end;
                }
                _ => {}
            }
            i += 1;
        }
        out
    }

    /// The `bfchar` entries of a ToUnicode CMap: CID → text.
    fn bf_chars(cmap: &[u8]) -> std::collections::HashMap<u16, String> {
        let text = String::from_utf8_lossy(cmap);
        let mut map = std::collections::HashMap::new();
        for block in text.split("beginbfchar").skip(1) {
            let block = block.split("endbfchar").next().unwrap();
            for line in block.lines().filter(|l| l.starts_with('<')) {
                let mut parts = line.split(['<', '>']).filter(|p| !p.trim().is_empty());
                let cid = u16::from_str_radix(parts.next().unwrap(), 16).unwrap();
                let units: Vec<u16> = parts
                    .next()
                    .unwrap()
                    .as_bytes()
                    .chunks(4)
                    .map(|h| u16::from_str_radix(std::str::from_utf8(h).unwrap(), 16).unwrap())
                    .collect();
                map.insert(cid, String::from_utf16(&units).unwrap());
            }
        }
        map
    }

    #[test]
    fn pdf_embeds_subset_fonts() {
        let pdf = write(&[plain_text("Wald", false, false)]);
        for key in [
            &b"/FontFile2"[..],
            b"/ToUnicode",
            b"/Type0",
            b"/CIDFontType2",
            b"/Identity-H",
            b"/CIDToGIDMap",
            b"/W [",
            b"+Arimo-Regular",
        ] {
            assert!(
                find(&pdf, key).is_some(),
                "{}",
                String::from_utf8_lossy(key)
            );
        }
        assert!(find(&pdf, b"/Helvetica").is_none());
        assert!(find(&pdf, b"Arimo-Bold").is_none(), "only used faces");
        assert!(pdf.len() < 60_000, "{} bytes", pdf.len());
    }

    #[test]
    fn faces_follow_bold_and_italic() {
        let pdf = write(&[
            plain_text("Wald", true, false),
            plain_text("Wald", false, true),
            plain_text("Wald", true, true),
        ]);
        for name in [&b"Arimo-Bold"[..], b"Arimo-Italic", b"Arimo-BoldItalic"] {
            assert!(
                find(&pdf, name).is_some(),
                "{}",
                String::from_utf8_lossy(name)
            );
        }
        assert!(find(&pdf, b"Arimo-Regular").is_none());
        assert_eq!(pdf.windows(10).filter(|w| w == b"/FontFile2").count(), 3);
    }

    #[test]
    fn non_ascii_text_round_trips() {
        let texts = ["„Gänsefüßchen“", "Řeka"];
        let (pdf, info) = write_info(&texts.map(|t| plain_text(t, false, false)), "Test");
        assert!(info.missing_glyphs.is_empty());
        let streams = streams(&pdf);
        let cmap = streams
            .iter()
            .find(|s| find(s, b"beginbfchar").is_some())
            .expect("ToUnicode CMap");
        let to_unicode = bf_chars(cmap);
        let shown: Vec<String> = shown_strings(&content_of(&pdf))
            .iter()
            .map(|s| {
                s.chunks(2)
                    .map(|c| to_unicode[&u16::from_be_bytes([c[0], c[1]])].as_str())
                    .collect()
            })
            .collect();
        assert_eq!(shown, texts);
        assert!(shown.iter().all(|s| !s.contains('?')));
    }

    #[test]
    fn missing_glyphs_are_reported() {
        let (pdf, info) = write_info(&[plain_text("Wald ✈ 中", false, false)], "Test");
        assert_eq!(info.missing_glyphs, ['✈', '中'].into_iter().collect());
        assert!(pdf.ends_with(b"%%EOF") || pdf.ends_with(b"%%EOF\n"));
        // .notdef stands in: CID 0 is shown.
        let shown = shown_strings(&content_of(&pdf));
        assert!(shown[0].chunks(2).any(|c| c == [0, 0]));
    }

    #[test]
    fn pdf_structure_is_valid() {
        let pdf = write_info(&[square(9)], "Strausberg Vorstadt").0;
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(find(&pdf, b"xref").is_some() && find(&pdf, b"trailer").is_some());
        assert!(pdf.ends_with(b"%%EOF") || pdf.ends_with(b"%%EOF\n"));
        assert!(find(&pdf, b"(Strausberg Vorstadt)").is_some(), "title");
        // UTF-16BE hex (the en dash is not ASCII).
        let creator: String = crate::CREDIT
            .encode_utf16()
            .map(|u| format!("{u:04X}"))
            .collect();
        assert!(find(&pdf, creator.as_bytes()).is_some(), "creator");
        assert!(
            find(&pdf, b"/MediaBox [0 0 595.2756 841.8898]").is_some(),
            "A4"
        );
    }

    #[test]
    fn multi_page_pdf_has_a_media_box_per_page() {
        let mut doc = MultiPagePdf::new();
        let a4 = page(&[square(2)]);
        let mut a3 = page(&[plain_text("Bahn", false, false)]);
        a3.page_mm = (297.0, 420.0);
        assert!(doc.is_empty());
        doc.add_page(&a4).unwrap();
        let info = doc.add_page(&a3).unwrap();
        assert!(info.missing_glyphs.is_empty());
        assert_eq!(doc.len(), 2);
        let pdf = doc.finish(&PdfMeta::new("Alle Bahnen"));
        assert!(find(&pdf, b"/Count 2").is_some());
        assert!(find(&pdf, b"/MediaBox [0 0 595.2756 841.8898]").is_some());
        assert!(find(&pdf, b"/MediaBox [0 0 841.8898 1190.5511]").is_some());
        assert!(find(&pdf, b"(Alle Bahnen)").is_some());
        assert!(pdf.ends_with(b"%%EOF") || pdf.ends_with(b"%%EOF\n"));
        // A page that cannot be written is refused and leaves the document as it was.
        let mut doc = MultiPagePdf::new();
        let mut bad = page(&[]);
        bad.page_mm = (f64::INFINITY, 1.0);
        assert!(matches!(doc.add_page(&bad), Err(PdfError::NotFinite)));
        assert!(doc.is_empty());
    }

    #[test]
    fn one_page_of_a_multi_page_pdf_matches_the_single_page_pdf_content() {
        let p = page(&[square(2), plain_text("Posten", true, false)]);
        let single = write(&[square(2), plain_text("Posten", true, false)]);
        let mut doc = MultiPagePdf::new();
        doc.add_page(&p).unwrap();
        let multi = doc.finish(&PdfMeta::new("Test"));
        assert_eq!(content_of(&single), content_of(&multi));
    }

    fn sheet_page(sheet: Vec<SheetItem>, primitives: &[Primitive]) -> Vec<u8> {
        let mut page = page(primitives);
        page.sheet = sheet;
        write_pdf(&page, &PdfMeta::new("Test")).unwrap().0
    }

    fn sheet_stroke(colour: DirectColour, y: f64) -> SheetItem {
        SheetItem::Shape {
            colour,
            shape: Shape::Stroke {
                path: vec![(150.0, y), (180.0, y)],
                closed: false,
                width: 0.5,
                cap: Cap::Flat,
                join: Join::Miter,
            },
        }
    }

    #[test]
    fn sheet_items_paint_after_map() {
        let sheet = vec![
            sheet_stroke(
                DirectColour {
                    cmyk: [0.1, 0.2, 0.3, 0.4],
                },
                200.0,
            ),
            sheet_stroke(
                DirectColour {
                    cmyk: [0.5, 0.6, 0.7, 0.8],
                },
                210.0,
            ),
        ];
        let pdf = content_of(&sheet_page(sheet, &[square(2), square(9)]));
        let pos =
            |n: &[u8]| find(&pdf, n).unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(n)));
        let (map1, map2) = (pos(b"0 0 0 1 k"), pos(b"0 0 0 0.6 k"));
        let (s1, s2) = (pos(b"0.1 0.2 0.3 0.4 K"), pos(b"0.5 0.6 0.7 0.8 K"));
        assert!(map1 < map2 && map2 < s1 && s1 < s2);
        // The map's clip block is closed before the first sheet item: q (page clip) ... Q, then
        // one q ... Q per sheet item.
        let ops = |op: &[u8]| pdf.split(|&b| b == b'\n').filter(|l| *l == op).count();
        assert_eq!((ops(b"q"), ops(b"Q")), (3, 3));
        let page_restore = find(&pdf[map2..], b"Q\n").unwrap() + map2;
        assert!(page_restore < s1, "map clip closed before the sheet");
    }

    #[test]
    fn direct_cmyk_colour() {
        let fill = SheetItem::Shape {
            colour: DirectColour {
                cmyk: [0.25, 0.5, 0.75, 0.125],
            },
            shape: Shape::Fill {
                rings: vec![vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)]],
                rule: FillRule::EvenOdd,
            },
        };
        let pdf = content_of(&sheet_page(vec![fill], &[]));
        assert!(find(&pdf, b"0.25 0.5 0.75 0.125 k").is_some());
        let pdf = content_of(&sheet_page(
            vec![sheet_stroke(DirectColour::WHITE, 5.0)],
            &[],
        ));
        assert!(find(&pdf, b"0 0 0 0 K").is_some());
        assert_eq!(DirectColour::BLACK.cmyk, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn map_clip_does_not_apply_to_sheet() {
        // The map clip is (0,0)-(100,100); the sheet stroke at x 150..180 lies outside it.
        let pdf = content_of(&sheet_page(
            vec![sheet_stroke(DirectColour::BLACK, 50.0)],
            &[square(2)],
        ));
        let clip_end = find(&pdf, b"Q\n").expect("map clip closed");
        let stroke = find(&pdf, b"S\n").expect("sheet stroke");
        assert!(clip_end < stroke);
        // Only the map's page clip exists; the sheet adds no clip.
        assert_eq!(pdf.windows(3).filter(|w| *w == b"W\nn").count(), 1);
        // A sheet without map primitives has no clip block at all.
        let pdf = content_of(&sheet_page(
            vec![sheet_stroke(DirectColour::BLACK, 50.0)],
            &[],
        ));
        assert!(find(&pdf, b"W\nn").is_none());
    }

    // --- Masks (ADR 0045) ---

    /// A square ring from (`x0`, `y0`) to (`x1`, `y1`) in page mm.
    fn ring(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    /// The overprint purple of the mask tests.
    const PURPLE: DirectColour = DirectColour {
        cmyk: [0.0, 1.0, 0.0, 0.0],
    };

    /// A page with `square(2)` and `square(9)`, a lower layer meant to lie under Black (2), an upper layer, a footer
    /// stroke on the sheet layer, and `clips`.
    fn masked_page(clips: Vec<MapClip>) -> RenderedPage {
        let mut p = page(&[square(9), square(2)]);
        p.overprint = crate::page::Overprint {
            lower: vec![Shape::Fill {
                rings: vec![ring(30.0, 30.0, 40.0, 40.0)],
                rule: FillRule::NonZero,
            }],
            upper: vec![Shape::Stroke {
                path: vec![(30.0, 50.0), (60.0, 50.0)],
                closed: false,
                width: 0.35,
                cap: Cap::Flat,
                join: Join::Miter,
            }],
            below_priority: Some(2),
            colour: PURPLE,
            cut: false,
        };
        p.sheet = vec![sheet_stroke(DirectColour::BLACK, 290.0)];
        p.map_clips = clips;
        p
    }

    /// The content stream of `page` as lines.
    fn lines(page: &RenderedPage) -> Vec<String> {
        let pdf = content_of(&write_pdf(page, &PdfMeta::new("t")).unwrap().0);
        String::from_utf8(pdf)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The index of the first line from `from` on that ends with `op`.
    fn line_of(lines: &[String], from: usize, op: &str) -> usize {
        from + lines[from..]
            .iter()
            .position(|l| l == op || l.ends_with(&format!(" {op}")))
            .unwrap_or_else(|| panic!("{op} after line {from}"))
    }

    /// One mask over the whole frame with a window: outline 0..100, hole 20..60.
    fn window_clip() -> MapClip {
        MapClip::new(
            clip(),
            [ring(-10.0, -10.0, 110.0, 110.0)],
            [ring(20.0, 20.0, 60.0, 60.0)],
        )
    }

    /// The mask blocks of a content stream as (line of `q`, line of its `Q`): blocks opened inside the page clip
    /// whose first operator draws a ring (the Multiply blocks start with `gs`).
    fn mask_blocks(l: &[String]) -> Vec<(usize, usize)> {
        let mut depth = 0;
        let mut open = None;
        let mut out = Vec::new();
        for (i, x) in l.iter().enumerate() {
            match x.as_str() {
                "q" => {
                    depth += 1;
                    if depth == 2 && l[i + 1].ends_with(" m") {
                        open = Some(i);
                    }
                }
                "Q" => {
                    if depth == 2
                        && let Some(o) = open.take()
                    {
                        out.push((o, i));
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        out
    }

    /// The deepest nesting of `q` blocks.
    fn max_depth(l: &[String]) -> i32 {
        let (mut d, mut max) = (0, 0);
        for x in l {
            match x.as_str() {
                "q" => {
                    d += 1;
                    max = max.max(d);
                }
                "Q" => d -= 1,
                _ => {}
            }
        }
        max
    }

    /// Whether the map shows at `p` (page mm) under `clips` in the frame [`clip`]: inside the frame, and filled by
    /// the nonzero rule of every clip's rings, the winding counted here from the ring edges.
    fn shows(clips: &[MapClip], (px, py): Pt) -> bool {
        let frame = clip();
        let in_frame = frame.min.0 < px && px < frame.max.0 && frame.min.1 < py && py < frame.max.1;
        in_frame
            && clips.iter().all(|c| {
                let mut winding = 0;
                for r in c.rings() {
                    for i in 0..r.len() {
                        let (a, b) = (r[i], r[(i + 1) % r.len()]);
                        let left = (b.0 - a.0) * (py - a.1) - (px - a.0) * (b.1 - a.1);
                        if a.1 <= py && b.1 > py && left > 0.0 {
                            winding += 1;
                        } else if a.1 > py && b.1 <= py && left < 0.0 {
                            winding -= 1;
                        }
                    }
                }
                winding != 0
            })
    }

    #[test]
    fn map_is_clipped_to_the_windows() {
        let l = lines(&masked_page(vec![window_clip()]));
        // The page clip, then the mask's block: q, three rings (frame, outline, hole), W n with the nonzero rule.
        let page_clip = line_of(&l, 0, "re");
        assert_eq!(l[page_clip + 1..page_clip + 3], ["W", "n"]);
        let blocks = mask_blocks(&l);
        assert_eq!(blocks.len(), 2, "before and after the lower purple: {l:?}");
        for &(open, close) in &blocks {
            let clip_op = line_of(&l, open, "W");
            assert_eq!(l[clip_op + 1], "n");
            let rings = l[open..clip_op]
                .iter()
                .filter(|x| x.ends_with(" m"))
                .count();
            let closes = l[open..clip_op].iter().filter(|x| *x == "h").count();
            assert_eq!((rings, closes), (3, 3), "frame, outline and hole");
            assert!(clip_op < close);
        }
        // The same clip path both times.
        let path_of = |(open, _): (usize, usize)| l[open..line_of(&l, open, "W")].to_vec();
        assert_eq!(path_of(blocks[0]), path_of(blocks[1]));
        // Grey (9) in the first block, the lower purple between them with Multiply, Black (2) in the second, the
        // upper purple after it: the order of a page without a mask.
        let grey = line_of(&l, 0, "0 0 0 0.6 k");
        let lower = line_of(&l, grey, "0 1 0 0 k");
        let black = line_of(&l, lower, "0 0 0 1 k");
        let upper = line_of(&l, black, "0 1 0 0 K");
        assert!(blocks[0].0 < grey && grey < blocks[0].1);
        assert!(blocks[0].1 < lower && lower < blocks[1].0);
        assert!(blocks[1].0 < black && black < blocks[1].1 && blocks[1].1 < upper);
        assert!(l[blocks[0].1..lower].contains(&"/Mul gs".to_string()));
    }

    #[test]
    fn blue_still_covers_the_lower_purple_under_a_mask() {
        // Ruling R11: Blue 100 % painted after the lower purple's place still covers it on a sheet with a mask.
        let mut p = masked_page(vec![window_clip()]);
        p.primitives = vec![square(9), square(2), square(1)];
        p.colours.push(RenderColour {
            priority: 1,
            name: "Blue 100%".into(),
            cmyk: [1.0, 0.0, 0.0, 0.0],
            rgb: [0.0, 0.6, 1.0],
            opacity: 1.0,
        });
        let l = lines(&p);
        let lower = line_of(&l, 0, "0 1 0 0 k");
        let blue_fill = line_of(&l, lower, "1 0 0 0 k");
        let blocks = mask_blocks(&l);
        assert!(
            blocks[1].0 < blue_fill && blue_fill < blocks[1].1,
            "blue under the mask"
        );
        // Without the mask, the same order.
        p.map_clips.clear();
        let l = lines(&p);
        let lower = line_of(&l, 0, "0 1 0 0 k");
        assert!(line_of(&l, lower, "1 0 0 0 k") > lower);
    }

    #[test]
    fn overlapping_white_outs_hide_the_union() {
        // Two masks (two white-outs of one course): their clips one after the other in one block (ruling R10).
        // Successive clips intersect, so the map is hidden in the union of the two outlines.
        let a = MapClip::new(clip(), [ring(10.0, 10.0, 60.0, 60.0)], []);
        let b = MapClip::new(clip(), [ring(40.0, 40.0, 90.0, 90.0)], []);
        let clips = vec![a, b];
        let l = lines(&masked_page(clips.clone()));
        let blocks = mask_blocks(&l);
        assert_eq!(blocks.len(), 2);
        for &(open, close) in &blocks {
            let first = line_of(&l, open, "W");
            let second = line_of(&l, first + 1, "W");
            assert!(second < close);
            assert!(
                !l[open + 1..second].contains(&"q".to_string()),
                "not nested"
            );
            let rings =
                |from: usize, to: usize| l[from..to].iter().filter(|x| x.ends_with(" m")).count();
            assert_eq!((rings(open, first), rings(first, second)), (2, 2));
        }
        assert_eq!(max_depth(&l), 2, "page clip and the mask block");
        // Hidden in either and in both, shown in neither.
        assert!(!shows(&clips, (20.0, 20.0)));
        assert!(!shows(&clips, (80.0, 80.0)));
        assert!(!shows(&clips, (50.0, 50.0)));
        assert!(shows(&clips, (80.0, 20.0)));
    }

    #[test]
    fn many_masks_keep_the_depth() {
        // A thousand masks still nest no deeper than one.
        let many: Vec<MapClip> = (0..1000)
            .map(|i| {
                let x = f64::from(i % 40) * 2.5;
                let y = f64::from(i / 40) * 4.0;
                MapClip::new(clip(), [ring(x, y, x + 1.0, y + 1.0)], [])
            })
            .collect();
        let l = lines(&masked_page(many));
        assert_eq!(max_depth(&l), 2);
        assert_eq!(l.iter().filter(|x| *x == "W").count(), 1 + 2 * 1000);
        assert_eq!(
            l.iter().filter(|x| *x == "q").count(),
            lines(&masked_page(vec![window_clip()]))
                .iter()
                .filter(|x| *x == "q")
                .count()
        );
    }

    #[test]
    fn window_at_the_frame_edge_is_clipped() {
        // A window that reaches past the frame: the page clip stays the outer block, so nothing paints beyond the
        // frame; the mask blocks lie inside it and are closed before the sheet layer.
        let edge = MapClip::new(
            clip(),
            [ring(-50.0, -50.0, 150.0, 150.0)],
            [ring(80.0, 40.0, 130.0, 70.0)],
        );
        let clips = vec![edge];
        // The window shows the map inside the frame and not beyond it; the outline hides it beside the window.
        assert!(shows(&clips, (90.0, 55.0)));
        assert!(!shows(&clips, (120.0, 55.0)));
        assert!(!shows(&clips, (70.0, 55.0)));
        assert!(!shows(&clips, (90.0, 30.0)));
        let p = masked_page(clips);
        let l = lines(&p);
        let page_clip = line_of(&l, 0, "re");
        let mask = line_of(&l, page_clip + 3, "W");
        assert!(page_clip < mask);
        // The frame ring of the mask is the frame itself (0..100 mm).
        assert_eq!(p.map_clips[0].rings()[0].len(), 4);
        for &(x, y) in &p.map_clips[0].rings()[0] {
            assert!((x == 0.0 || x == 100.0) && (y == 0.0 || y == 100.0));
        }
        // The page clip closes after the overprint and before the footer stroke of the sheet layer.
        let upper = line_of(&l, mask, "0 1 0 0 K");
        let page_close = line_of(&l, upper, "Q");
        let footer = line_of(&l, page_close, "S");
        assert!(page_close < footer);
        let depth = l[..footer].iter().fold(0i32, |d, x| match x.as_str() {
            "q" => d + 1,
            "Q" => d - 1,
            _ => d,
        });
        assert_eq!(
            depth, 1,
            "only the sheet item's own block is open at the footer"
        );
    }

    #[test]
    fn mask_does_not_clip_overprint_sheet_elements_or_footer() {
        let l = lines(&masked_page(vec![window_clip()]));
        let blocks = mask_blocks(&l);
        assert_eq!(blocks.len(), 2);
        for op in ["0 1 0 0 k", "0 1 0 0 K", "S"] {
            let lines: Vec<usize> = (0..l.len()).filter(|&i| l[i].ends_with(op)).collect();
            assert!(!lines.is_empty(), "{op}");
            for i in lines {
                assert!(
                    !blocks.iter().any(|&(o, c)| o < i && i < c),
                    "{op} at {i} is under the mask"
                );
            }
        }
    }

    #[test]
    fn no_mask_keeps_the_lower_purple_among_the_map() {
        // Without a mask the lower purple lies right under Black (2), between the two map colours, as before.
        let l = lines(&masked_page(Vec::new()));
        let grey = line_of(&l, 0, "0 0 0 0.6 k");
        let lower = line_of(&l, grey, "0 1 0 0 k");
        let black = line_of(&l, lower, "0 0 0 1 k");
        assert!(grey < lower && lower < black);
        assert_eq!(
            l.iter().filter(|x| *x == "W").count(),
            1,
            "the page clip only"
        );
    }

    #[test]
    fn map_clip_rings_take_their_orientation_from_the_page() {
        // Either orientation in, the same clip out: frame and holes counter-clockwise on paper (negative shoelace
        // sum, y down), outlines clockwise.
        let area = |r: &[Pt]| -> f64 {
            (0..r.len())
                .map(|i| {
                    let (p, q) = (r[i], r[(i + 1) % r.len()]);
                    p.0 * q.1 - q.0 * p.1
                })
                .sum()
        };
        let rev = |mut r: Vec<Pt>| {
            r.reverse();
            r
        };
        let o = ring(10.0, 10.0, 90.0, 90.0);
        let h = ring(20.0, 20.0, 30.0, 30.0);
        let a = MapClip::new(clip(), [o.clone()], [h.clone()]);
        let b = MapClip::new(clip(), [rev(o)], [rev(h)]);
        for c in [&a, &b] {
            let signs: Vec<bool> = c.rings().iter().map(|r| area(r) < 0.0).collect();
            assert_eq!(signs, [true, false, true]);
        }
        // A ring without area is left out.
        let c = MapClip::new(clip(), [vec![(0.0, 0.0), (1.0, 1.0)]], []);
        assert_eq!(c.rings().len(), 1);
    }

    fn rgba_image() -> Arc<RasterImage> {
        Arc::new(RasterImage::new(2, 1, vec![255, 0, 0, 0, 255, 0], Some(vec![255, 128])).unwrap())
    }

    fn logo(image: &Arc<RasterImage>) -> SheetItem {
        SheetItem::Image {
            rect: PaperBox {
                min: (10.0, 20.0),
                max: (30.0, 25.0),
            },
            image: Arc::clone(image),
        }
    }

    #[test]
    fn image_xobject_with_smask() {
        let image = rgba_image();
        let pdf = sheet_page(vec![logo(&image)], &[]);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Subtype /Image"), "image XObject");
        assert!(text.contains("/SMask"), "soft mask");
        assert!(text.contains("/ColorSpace /DeviceRGB"));
        assert!(text.contains("/ColorSpace /DeviceGray"));
        assert!(text.contains("/XObject"));
        // 20 x 5 mm at (10, 297 - 25) mm, in points.
        let content = content_of(&pdf);
        let pt = |mm: f64| format!("{}", (mm * PT_PER_MM) as f32);
        let cm = format!("{} 0 0 {} {} {} cm", pt(20.0), pt(5.0), pt(10.0), pt(272.0));
        assert!(
            find(&content, cm.as_bytes()).is_some(),
            "{cm} in {}",
            String::from_utf8_lossy(&content)
        );
        assert!(find(&content, b"/Im0 Do").is_some());
    }

    #[test]
    fn image_without_alpha_has_no_smask() {
        let image = Arc::new(RasterImage::new(1, 1, vec![1, 2, 3], None).unwrap());
        let pdf = sheet_page(vec![logo(&image)], &[]);
        assert!(!String::from_utf8_lossy(&pdf).contains("/SMask"));
    }

    #[test]
    fn identical_images_are_embedded_once() {
        let image = rgba_image();
        let pdf = sheet_page(vec![logo(&image), logo(&image)], &[]);
        let content = content_of(&pdf);
        assert_eq!(content.windows(7).filter(|w| *w == b"/Im0 Do").count(), 2);
        assert!(find(&content, b"/Im1").is_none());
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(
            text.matches("/Subtype /Image").count(),
            2,
            "image and mask once"
        );
        // A different Arc with equal pixels is a separate image.
        let other = Arc::new((*image).clone());
        let pdf = sheet_page(vec![logo(&image), logo(&other)], &[]);
        assert_eq!(
            String::from_utf8_lossy(&pdf)
                .matches("/Subtype /Image")
                .count(),
            4
        );
    }

    #[test]
    fn sheet_text_without_map_text_embeds_one_font() {
        let item = SheetItem::Shape {
            colour: DirectColour::BLACK,
            shape: Shape::Text {
                origin: (10.0, 10.0),
                size: 3.0,
                bold: false,
                italic: false,
                text: "A\u{4e2d}".into(),
                rotation: 0.0,
                stroke: None,
            },
        };
        let mut p = page(&[square(2)]);
        p.sheet = vec![item];
        let (pdf, info) = write_pdf(&p, &PdfMeta::new("T")).unwrap();
        assert!(info.missing_glyphs.contains(&'\u{4e2d}'));
        assert_eq!(
            String::from_utf8_lossy(&pdf)
                .matches("/Subtype /Type0")
                .count(),
            1
        );
    }

    #[test]
    fn sheet_text_shares_fonts_and_reports_missing_glyphs() {
        let item = SheetItem::Shape {
            colour: DirectColour::BLACK,
            shape: Shape::Text {
                origin: (10.0, 10.0),
                size: 3.0,
                bold: false,
                italic: false,
                text: "A\u{4e2d}".into(),
                rotation: 0.0,
                stroke: None,
            },
        };
        let mut p = page(&[text(0.0, None)]);
        p.sheet = vec![item];
        let (pdf, info) = write_pdf(&p, &PdfMeta::new("T")).unwrap();
        assert!(info.missing_glyphs.contains(&'\u{4e2d}'));
        assert_eq!(
            String::from_utf8_lossy(&pdf)
                .matches("/Subtype /Type0")
                .count(),
            1
        );
    }

    // --- Compact paths (ADR 0032) ---

    /// A dot of point symbols as draw.rs draws it: 48 sides around `centre`, radius 0.5 mm, the first point repeated.
    fn dot(centre: Pt) -> Vec<Pt> {
        (0..=48)
            .map(|i| {
                let a = f64::from(i) / 48.0 * std::f64::consts::TAU;
                (centre.0 + 0.5 * a.cos(), centre.1 + 0.5 * a.sin())
            })
            .collect()
    }

    /// The lines of each block `q`, `… cm`, …, `Q` of `l`.
    fn relative_blocks(l: &[String]) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        for (i, x) in l.iter().enumerate() {
            if x == "q" && l.get(i + 1).is_some_and(|c| c.ends_with(" cm")) {
                let end = line_of(l, i, "Q");
                out.push(l[i..=end].to_vec());
            }
        }
        out
    }

    #[test]
    fn a_small_shape_is_written_relative_to_its_first_point() {
        let fill = |centre| {
            Primitive::new(
                2,
                Shape::Fill {
                    rings: vec![dot(centre)],
                    rule: FillRule::NonZero,
                },
            )
        };
        let l = lines(&page(&[fill((30.0, 40.0)), fill((61.25, 17.5))]));
        let blocks = relative_blocks(&l);
        assert_eq!(blocks.len(), 2, "{l:?}");
        // A translation to the first point, (30.5, 40) mm, in points rounded to 1/1000; the ring from 0 0, every
        // point kept; the fill inside the block.
        let at = |mm: f64| (mm * PT_PER_MM * 1000.0).round() / 1000.0;
        assert_eq!(
            blocks[0][1],
            format!("1 0 0 1 {} {} cm", at(30.5) as f32, at(297.0 - 40.0) as f32)
        );
        assert_eq!(blocks[0][2], "0 0 m");
        assert_eq!(blocks[0].iter().filter(|x| x.ends_with(" l")).count(), 48);
        assert_eq!(&blocks[0][blocks[0].len() - 3..], ["h", "f", "Q"]);
        // The second dot gives the same path, so deflate stores it once.
        assert_eq!(blocks[0][2..], blocks[1][2..]);
        // A stroke of a small ring too; the stroke settings stay outside the block.
        let ring = Primitive::new(
            2,
            Shape::Stroke {
                path: dot((30.0, 40.0)),
                closed: true,
                width: 0.1,
                cap: Cap::Flat,
                join: Join::Miter,
            },
        );
        let l = lines(&page(&[ring]));
        let blocks = relative_blocks(&l);
        assert_eq!(blocks.len(), 1);
        assert_eq!(&blocks[0][blocks[0].len() - 3..], ["h", "S", "Q"]);
        assert!(line_of(&l, 0, "w") < line_of(&l, 0, "cm"));
    }

    #[test]
    fn small_means_within_a_square_of_7_mm() {
        // Dots and circles of point symbols up to a control circle of 6 mm.
        assert!(is_small(&dot((10.0, 20.0))));
        for r in [0.2, 0.5, 1.0, 3.0] {
            let circle: Vec<Pt> = (0..48)
                .map(|i| {
                    let a = f64::from(i) / 48.0 * std::f64::consts::TAU;
                    (r * a.cos(), r * a.sin())
                })
                .collect();
            assert!(is_small(&circle), "r {r}");
        }
        assert!(!is_small(&[(0.0, 0.0), (7.0, 1.0)]));
        assert!(!is_small(&[(0.0, 0.0), (1.0, 7.0)]));
        assert!(is_small(&[(0.0, 0.0), (6.99, 6.99)]));
        // A point that is not a number makes nothing small, wherever it is.
        assert!(!is_small(&[(0.0, 0.0), (f64::NAN, 0.0), (1.0, 1.0)]));
        assert!(!is_small(&[(f64::INFINITY, 0.0), (1.0, 1.0)]));
        assert!(!is_small(&[]));
    }

    #[test]
    fn large_and_short_shapes_keep_page_coordinates() {
        // A square of 10 mm and a small stroke of three points.
        let three = Primitive::new(
            2,
            Shape::Stroke {
                path: vec![(30.0, 30.0), (30.5, 30.0), (30.5, 30.5)],
                closed: false,
                width: 0.1,
                cap: Cap::Flat,
                join: Join::Miter,
            },
        );
        let l = lines(&page(&[square(2), three]));
        assert!(relative_blocks(&l).is_empty(), "{l:?}");
        // Without compact paths a dot keeps its page coordinates too.
        let p = page(&[Primitive::new(
            2,
            Shape::Fill {
                rings: vec![dot((30.0, 40.0))],
                rule: FillRule::NonZero,
            },
        )]);
        let plain = Writing {
            compact: false,
            ..Writing::FILE
        };
        let (pdf, _) = write_pdf_with(&p, &PdfMeta::new("t"), plain).unwrap();
        assert!(find(&content_of(&pdf), b" cm").is_none());
    }

    // --- Shapes that cannot show are left out (ADR 0031) ---

    /// A square of priority `priority` from (`x`, `y`), 10 mm a side.
    fn square_at(priority: i32, x: f64, y: f64) -> Primitive {
        Primitive::new(
            priority,
            Shape::Fill {
                rings: vec![ring(x, y, x + 10.0, y + 10.0)],
                rule: FillRule::EvenOdd,
            },
        )
    }

    /// How many fills the content stream of `page` paints.
    fn fills(page: &RenderedPage) -> usize {
        lines(page)
            .iter()
            .filter(|l| *l == "f*" || *l == "f")
            .count()
    }

    #[test]
    fn shapes_wholly_outside_the_frame_are_not_written() {
        // The frame is 0..100 mm; grey squares lie beside it on the page, more than the slack away, black ones inside
        // or across its edge.
        let p = page(&[
            square_at(2, 20.0, 20.0),
            square_at(9, 120.0, 20.0),
            square_at(2, 95.0, 50.0),
            square_at(9, 20.0, 150.0),
            square_at(9, -30.0, 20.0),
        ]);
        let l = lines(&p);
        assert_eq!(fills(&p), 2, "{l:?}");
        assert!(!l.iter().any(|x| x == "0 0 0 0.6 k"), "{l:?}");
        // Written without leaving anything out, all five are there.
        let uncut = Writing {
            cull: false,
            ..Writing::FILE
        };
        let (all, _) = write_pdf_with(&p, &PdfMeta::new("t"), uncut).unwrap();
        let all = String::from_utf8(content_of(&all)).unwrap();
        assert_eq!(all.lines().filter(|l| *l == "f*").count(), 5);
    }

    #[test]
    fn shapes_off_the_page_are_not_written_even_inside_the_clip() {
        // A clip larger than the page: only the page shows.
        let mut p = page(&[square_at(2, 20.0, 20.0), square_at(9, -40.0, -40.0)]);
        p.clip = PaperBox {
            min: (-100.0, -100.0),
            max: (400.0, 400.0),
        };
        assert_eq!(fills(&p), 1);
        // A clip given with its corners swapped is the same rectangle in the PDF, and culls the same.
        p.clip = PaperBox {
            min: (100.0, 100.0),
            max: (0.0, 0.0),
        };
        assert_eq!(fills(&p), 1);
    }

    #[test]
    fn the_overprint_outside_the_frame_is_left_out_and_keeps_its_place() {
        let mut p = masked_page(Vec::new());
        // A second lower shape and a second upper shape beside the frame.
        p.overprint.lower.push(Shape::Fill {
            rings: vec![ring(130.0, 30.0, 140.0, 40.0)],
            rule: FillRule::NonZero,
        });
        p.overprint.upper.push(Shape::Stroke {
            path: vec![(130.0, 50.0), (160.0, 50.0)],
            closed: false,
            width: 0.35,
            cap: Cap::Flat,
            join: Join::Miter,
        });
        let l = lines(&p);
        assert_eq!(l.iter().filter(|x| *x == "0 1 0 0 k").count(), 1, "{l:?}");
        assert_eq!(l.iter().filter(|x| *x == "0 1 0 0 K").count(), 1, "{l:?}");
        // The lower purple still lies between grey and black.
        let grey = line_of(&l, 0, "0 0 0 0.6 k");
        let lower = line_of(&l, grey, "0 1 0 0 k");
        line_of(&l, lower, "0 0 0 1 k");
        // An overprint layer with nothing left writes no Multiply block.
        p.overprint.lower = vec![p.overprint.lower.pop().unwrap()];
        p.overprint.upper.clear();
        let l = lines(&p);
        assert!(!l.iter().any(|x| x == "/Mul gs"), "{l:?}");
    }

    #[test]
    fn a_mask_run_with_nothing_left_writes_no_mask_block() {
        let mut p = masked_page(vec![window_clip()]);
        // Grey moves beside the frame; black stays.
        p.primitives = vec![square_at(9, 150.0, 20.0), square(2)];
        let l = lines(&p);
        assert_eq!(mask_blocks(&l).len(), 1, "{l:?}");
        assert!(!l.iter().any(|x| x == "0 0 0 0.6 k"));
    }

    #[test]
    fn a_text_left_out_still_reports_its_missing_glyphs_and_embeds_no_font() {
        let mut beside = plain_text("Wald ✈", false, false);
        let Shape::Text { origin, .. } = &mut beside.shape else {
            panic!()
        };
        *origin = (150.0, 50.0);
        let (pdf, info) = write_info(&[beside], "Test");
        assert_eq!(info.missing_glyphs, ['✈'].into_iter().collect());
        assert!(find(&pdf, b"/FontFile2").is_none());
        assert!(find(&content_of(&pdf), b"Tj").is_none());
    }

    #[test]
    fn a_shape_left_out_still_refuses_a_number_the_pdf_cannot_hold() {
        // Beyond the f32 range, far left of the frame.
        let far = Primitive::new(
            2,
            Shape::Fill {
                rings: vec![vec![(-1e39, 10.0), (-2e39, 10.0), (-2e39, 20.0)]],
                rule: FillRule::EvenOdd,
            },
        );
        let err = write_pdf(&page(&[far]), &PdfMeta::new("t")).unwrap_err();
        assert!(matches!(err, PdfError::NotFinite), "{err}");
        // A colour that is not a number, used only beside the frame.
        let mut p = page(&[square_at(2, 20.0, 20.0), square_at(9, 150.0, 20.0)]);
        p.colours[1].cmyk[3] = f64::NAN;
        assert!(matches!(
            write_pdf(&p, &PdfMeta::new("t")),
            Err(PdfError::NotFinite)
        ));
    }
}

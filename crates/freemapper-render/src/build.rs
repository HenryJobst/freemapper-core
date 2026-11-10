//! The page builder: from a map to the page every backend draws (ADR 0023).
//!
//! Provenance: freemapper-print's `print.rs` (`render_page_within` and its helpers), moved here with its
//! behaviour unchanged. The scale policy stays in the app: it decides the symbol factor and the geometry factor
//! for a print scale, and [`PageSpec`] takes both as plain numbers.
//!
//! One millimetre at print scale is one page millimetre, and the page is exactly the paper. The geometry is
//! multiplied by the geometry factor (map scale / print scale), the symbols by the symbol factor. Nothing is
//! fitted to the page: a map larger than the page is clipped, and the report says by how much it overflows.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use freemapper_omap::enlarge::enlarge_symbols;
use freemapper_omap::{MapCoord, MapDocument, MapObject, MapObjectKind, OmapError, SymbolSet};

use crate::bounds::{Bounds, painted_bounds};
use crate::draw::{MAX_UNITS_PER_OBJECT, Primitive, Shape, primitives_within};
use crate::fonts::substituted;
use crate::geometry::Pt;
use crate::page::{PaperBox, RenderedPage};
use crate::symbols::{RenderSymbols, Symbol};

/// White margin on each side of the page for freemapper-print's `render` command; the map is clipped to the
/// page minus this margin (mm). A layout sets its own ([`PageSpec::frame_margin_mm`]).
pub const PAGE_MARGIN_MM: f64 = 5.0;

/// Most units of work one page may use, counted as for one object (`MAX_UNITS_PER_OBJECT`): a
/// primitive costs one unit plus one per eight points it keeps, and a clip outline one per
/// eight points for each clip block the PDF writes. Lübeck (2173 objects, about 4400
/// primitives) uses about 6500 units and the forest sample (539 objects, about 1500
/// primitives) at most about 6400 at every allowed scale, so a dense real map stays far below
/// it. A file that asks for more is broken or hostile; the objects after the limit are not
/// drawn and counted ([`BuildReport::objects_not_drawn_page_limit`]).
///
/// A page keeps at most four million points (500 000 units of at most eight points each),
/// clip outlines counted once per block. Measured peak memory (release build, macOS): about
/// 200 MB on a 134 KB file of 500 000 strokes of seven points, the largest of the hostile
/// files tried (freemapper-print ADR 0019).
pub const MAX_UNITS_PER_PAGE: usize = 500_000;

/// The tag freemapper puts on the objects of a map's peripheral information.
pub const SHEET_TAG: (&str, &str) = ("freemapper:source", "sheet");

/// The tag freemapper v0.9.0 put on the scale bar objects (lines 994, texts 993) instead of `sheet`; they belong to
/// the information block all the same. Maps made by that version carry it.
pub const SCALE_BAR_TAG: (&str, &str) = ("freemapper:source", "scale-bar");

/// The tag freemapper puts on the base area of a sprint map (freemapper-print ADR 0044, freemapper ADR 0069): one
/// area in the colour "walkable area" under the whole map. It is always drawn and never counts for the page
/// centre, the overflow or the north-line bounds.
pub const BASE_TAG: (&str, &str) = ("freemapper:source", "base");

/// The map part freemapper puts the base area in. A layout cannot hide it (freemapper-print ADR 0044).
pub const BASE_PART: &str = "base";

/// Micrometres (OOM map units) per millimetre.
const UM_PER_MM: f64 = 1000.0;

/// Paper sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Paper {
    A4,
    A3,
}

impl Paper {
    /// Width and height in portrait orientation (mm).
    pub fn size_mm(self) -> (f64, f64) {
        match self {
            Paper::A4 => (210.0, 297.0),
            Paper::A3 => (297.0, 420.0),
        }
    }
}

/// Page orientation. `Auto` takes the orientation in which less of the map lies outside the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Auto,
    Portrait,
    Landscape,
}

/// How a map is put on a page. The app's scale policy supplies the two factors.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSpec {
    /// Every symbol dimension (line widths, point symbols, patterns, font sizes) is multiplied by it; 1 keeps
    /// the symbols as the file defines them.
    pub symbol_factor: f64,
    /// Every coordinate is multiplied by it: the map scale divided by the print scale, 1 at the map scale.
    pub geometry_factor: f64,
    pub paper: Paper,
    pub orientation: Orientation,
    /// The map point (in the file's map coordinates) that lands at the centre of the page; `None`
    /// takes the centre of the box everything drawn paints in, the base area of a sprint map left
    /// out (freemapper-print ADR 0044). A point far outside the map moves
    /// the map off the page: the overflow then counts from the page edge to the far side of the
    /// map and can exceed the map's own size.
    pub centre: Option<MapCoord>,
    /// Width (mm) of the white border between the page edge and the map frame, the same on every
    /// side: the map is clipped to the page minus this border, centred in what is left, and the
    /// overflow is measured against it. `None` takes [`PAGE_MARGIN_MM`] (freemapper-print's `render`
    /// command); a layout passes its passe-partout. A margin wider than half the page leaves a frame of
    /// zero width or height.
    pub frame_margin_mm: Option<f64>,
    /// Draw freemapper's information block (title, scale, scale bar, credits): the objects tagged
    /// `freemapper:source=sheet`. North lines (601) carry the tag too but belong to the map and are
    /// always drawn (freemapper-print ADR 0023).
    pub show_sheet_objects: bool,
}

impl PageSpec {
    /// A page of `paper` with the two factors, in automatic orientation, centred on what the map paints, with
    /// the margin [`PAGE_MARGIN_MM`] and without the information block.
    pub fn new(symbol_factor: f64, geometry_factor: f64, paper: Paper) -> Self {
        PageSpec {
            symbol_factor,
            geometry_factor,
            paper,
            orientation: Orientation::Auto,
            centre: None,
            frame_margin_mm: None,
            show_sheet_objects: false,
        }
    }
}

/// What a page shows and what it could not show.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildReport {
    /// Width and height of the page (mm): the paper in the chosen orientation.
    pub page_mm: (f64, f64),
    pub geometry_factor: f64,
    pub symbol_factor: f64,
    /// How far (mm) the box the drawn primitives paint in (line widths, symbols and texts
    /// included, the base area of a sprint map not) reaches beyond the clip rectangle in x and in
    /// y, both sides together; `None` when everything fits.
    pub overflow_mm: Option<(f64, f64)>,
    /// Objects not drawn because their symbol is unknown or hidden (`is_hidden` in the file).
    /// Sheet objects left out by [`PageSpec::show_sheet_objects`] do not count.
    pub skipped_objects: usize,
    /// Objects of a type the reader does not know ([`MapDocument::unknown_objects`]).
    pub unknown_type_objects: usize,
    /// Objects whose drawing a limit cut short: the per-object budget (`MAX_UNITS_PER_OBJECT`),
    /// the symbol nesting depth, a pattern's line cap, or the rest of the page budget.
    pub truncated_objects: usize,
    /// Objects not drawn at all because the page budget ([`MAX_UNITS_PER_PAGE`]) was spent.
    pub objects_not_drawn_page_limit: usize,
    /// Drawn objects whose symbol lost an area pattern when read, because the pattern broke a
    /// limit (spacing or point distance below 0.05 mm, a number that is not finite).
    pub dropped_pattern_objects: usize,
    /// Font families of drawn texts that Arimo stands in for although it does not match them.
    pub substituted_fonts: Vec<String>,
    /// Characters of drawn texts that the embedded font has no glyph for. [`build_page`] leaves it empty;
    /// an app fills it in from what [`crate::pdf::write_pdf`] reports.
    pub missing_glyphs: Vec<char>,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("could not resize the map symbols for the print scale")]
    Symbols(#[from] OmapError),
}

/// The page for `map` as `spec` sets it, in page millimetres, ready for any backend.
///
/// Templates are never drawn. Objects of hidden or unknown symbols are skipped and counted, sheet
/// objects are left out unless [`PageSpec::show_sheet_objects`] is set. The page budget is
/// [`MAX_UNITS_PER_PAGE`].
pub fn build_page(
    map: &MapDocument,
    spec: &PageSpec,
) -> Result<(RenderedPage, BuildReport), BuildError> {
    build_page_within(map, spec, MAX_UNITS_PER_PAGE)
}

/// [`build_page`] with a page budget of `page_budget` units instead of [`MAX_UNITS_PER_PAGE`]. Tests use it
/// to reach the budget with small maps; an app calls [`build_page`].
pub fn build_page_within(
    map: &MapDocument,
    spec: &PageSpec,
    page_budget: usize,
) -> Result<(RenderedPage, BuildReport), BuildError> {
    let symbol_factor = spec.symbol_factor;
    let geometry_factor = spec.geometry_factor;
    let symbols = render_symbols(&map.symbol_set, symbol_factor)?;

    let mut skipped_objects = 0;
    let mut objects = Vec::new();
    for object in map.parts.iter().flat_map(|p| &p.objects) {
        if is_sheet_object(object)
            && !spec.show_sheet_objects
            && !is_north_line(&map.symbol_set, object)
        {
            continue;
        }
        if map.symbol_set.is_hidden(object.symbol_id)
            || !symbols.symbols.contains_key(&object.symbol_id)
        {
            skipped_objects += 1;
            continue;
        }
        objects.push(scaled(object, geometry_factor));
    }

    // Draw everything in print-scale millimetres of the map, then place it on the page.
    let mut primitives = Vec::new();
    let mut page_left = page_budget;
    let mut truncated_objects = 0;
    let mut objects_not_drawn_page_limit = 0;
    let mut dropped_pattern_objects = 0;
    let mut fonts = BTreeSet::new();
    for (index, object) in objects.iter().enumerate() {
        if page_left == 0 {
            objects_not_drawn_page_limit += 1;
            continue;
        }
        let mut drawn = primitives_within(object, &symbols, page_left.min(MAX_UNITS_PER_OBJECT));
        let north_line = is_north_line(&map.symbol_set, object);
        let base = is_base_object(object);
        for p in &mut drawn.primitives {
            p.north_line = north_line;
            p.base = base;
            p.object = u32::try_from(index).unwrap_or(u32::MAX);
            p.symbol = object.symbol_id;
        }
        page_left = page_left.saturating_sub(drawn.units);
        truncated_objects += usize::from(drawn.cut);
        dropped_pattern_objects += usize::from(
            object.kind == MapObjectKind::Path
                && symbols.dropped_patterns.contains(&object.symbol_id),
        );
        primitives.append(&mut drawn.primitives);
        if let (MapObjectKind::Text { .. }, Some(Symbol::Text(def))) =
            (&object.kind, symbols.symbols.get(&object.symbol_id))
            && substituted(&def.family)
        {
            fonts.insert(def.family.trim().to_string());
        }
    }
    // Paint order: highest priority number first, priority 0 last (on top). Stable, so the
    // objects keep their order within a colour.
    primitives.sort_by_key(|p| std::cmp::Reverse(p.priority));

    // The base area stays out of the bounds: it is the map plus a margin (freemapper-print ADR 0044).
    let bounds = painted_bounds(&primitives);
    let centre = match (spec.centre, bounds) {
        (Some(c), _) => (
            f64::from(c.x) * geometry_factor / UM_PER_MM,
            f64::from(c.y) * geometry_factor / UM_PER_MM,
        ),
        (None, Some((min, max))) => ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0),
        (None, None) => (0.0, 0.0),
    };
    let margin = spec.frame_margin_mm.unwrap_or(PAGE_MARGIN_MM);
    let page_mm = page_size(spec.paper, spec.orientation, bounds, centre, margin);
    let frame = frame_box(page_mm, margin);
    // The centre of the frame is the centre of the page: the margin is the same on every side.
    let offset = (page_mm.0 / 2.0 - centre.0, page_mm.1 / 2.0 - centre.1);
    let overflow_mm = bounds
        .map(|b| overflow(b, offset, frame))
        .filter(|o| o.dx > 0.0 || o.dy > 0.0)
        .map(|o| (o.dx, o.dy));
    translate(&mut primitives, offset);

    let page = RenderedPage {
        primitives,
        clip: frame,
        page_mm,
        centre_mm: centre,
        colours: symbols.colours,
        sheet: Vec::new(),
        overprint: Default::default(),
        map_clips: Vec::new(),
    };
    let report = BuildReport {
        page_mm,
        geometry_factor,
        symbol_factor,
        overflow_mm,
        skipped_objects,
        unknown_type_objects: map.unknown_objects,
        truncated_objects,
        objects_not_drawn_page_limit,
        dropped_pattern_objects,
        substituted_fonts: fonts.into_iter().collect(),
        missing_glyphs: Vec::new(),
    };
    Ok((page, report))
}

/// The symbols of `set` with every dimension multiplied by `factor`.
fn render_symbols(set: &SymbolSet, factor: f64) -> Result<RenderSymbols, OmapError> {
    if factor == 1.0 {
        return RenderSymbols::from_set(set);
    }
    // Enlarging keeps every id, so the code table and the hidden list stay valid.
    let mut enlarged = set.clone();
    enlarged.symbols_xml = enlarge_symbols(&set.symbols_xml, factor)?;
    RenderSymbols::from_set(&enlarged)
}

/// Part of freemapper's information block, by its tag ([`SHEET_TAG`], or [`SCALE_BAR_TAG`] on maps of
/// freemapper v0.9.0).
pub fn is_sheet_object(object: &MapObject) -> bool {
    has_tag(object, SHEET_TAG) || has_tag(object, SCALE_BAR_TAG)
}

/// Part of freemapper's base area, by its tag ([`BASE_TAG`], freemapper-print ADR 0044).
pub fn is_base_object(object: &MapObject) -> bool {
    has_tag(object, BASE_TAG)
}

fn has_tag(object: &MapObject, (key, value): (&str, &str)) -> bool {
    object.tags.iter().any(|(k, v)| k == key && v == value)
}

/// A magnetic north line (601, or a variant 601.x): part of the map even where freemapper tags it
/// as a sheet object (freemapper-print ADR 0023).
pub fn is_north_line(set: &SymbolSet, object: &MapObject) -> bool {
    set.code_of(object.symbol_id)
        .is_some_and(|code| code == "601" || code.starts_with("601."))
}

/// `c` times `factor`, rounded to whole µm. The geometry factor is at most 4 (a forest map drawn
/// at 1:20000 printed at 1:5000), so a coordinate beyond ±537 m of map paper leaves the `i32`
/// range. `as i32` then saturates at ±2147 m: such a point lies kilometres off any page either
/// way, so the page and its overflow report stay as they would be (the overflow counts to the
/// saturated point).
fn scale_coord(c: MapCoord, factor: f64) -> MapCoord {
    MapCoord {
        x: (f64::from(c.x) * factor).round() as i32,
        y: (f64::from(c.y) * factor).round() as i32,
        ..c
    }
}

/// `object` with its geometry multiplied by `factor` (Bézier control points, pattern origin and
/// the box of a box text included).
fn scaled(object: &MapObject, factor: f64) -> MapObject {
    let mut object = object.clone();
    if factor == 1.0 {
        return object;
    }
    for c in &mut object.coords {
        *c = scale_coord(*c, factor);
    }
    if let Some(p) = &mut object.pattern {
        p.origin = scale_coord(p.origin, factor);
    }
    if let MapObjectKind::Text {
        box_size: Some((w, h)),
        ..
    } = &mut object.kind
    {
        *w *= factor;
        *h *= factor;
    }
    object
}

/// How far painted bounds reach beyond the clip rectangle (mm), and the area outside it (mm²).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Overflow {
    dx: f64,
    dy: f64,
    area: f64,
}

/// The map frame on a page of `page_mm`: the page minus `margin_mm` on every side. A margin of
/// more than half the page leaves a frame of zero width or height in the middle of the page.
pub fn frame_box(page_mm: (f64, f64), margin_mm: f64) -> PaperBox {
    // `max` first: a NaN margin becomes 0.
    let mx = margin_mm.max(0.0).min(page_mm.0 / 2.0);
    let my = margin_mm.max(0.0).min(page_mm.1 / 2.0);
    PaperBox {
        min: (mx, my),
        max: (page_mm.0 - mx, page_mm.1 - my),
    }
}

/// The overflow of `bounds` moved by `offset` beyond `frame`.
fn overflow((min, max): Bounds, offset: Pt, frame: PaperBox) -> Overflow {
    let axis = |min: f64, max: f64, lo: f64, hi: f64| {
        let out = (lo - min).max(0.0) + (max - hi).max(0.0);
        let inside = (max.min(hi) - min.max(lo)).max(0.0);
        (out, max - min, inside)
    };
    let (dx, w, w_in) = axis(min.0 + offset.0, max.0 + offset.0, frame.min.0, frame.max.0);
    let (dy, h, h_in) = axis(min.1 + offset.1, max.1 + offset.1, frame.min.1, frame.max.1);
    Overflow {
        dx,
        dy,
        area: w * h - w_in * h_in,
    }
}

/// The page (mm) in the requested orientation. `Auto` takes the one whose overflowing area beyond
/// the frame (the page minus `margin_mm`) is smaller, then the one with the smaller overflow
/// lengths (a map of a single straight line has almost no area), and portrait on a tie.
fn page_size(
    paper: Paper,
    orientation: Orientation,
    bounds: Option<Bounds>,
    centre: Pt,
    margin_mm: f64,
) -> (f64, f64) {
    let portrait = paper.size_mm();
    let landscape = (portrait.1, portrait.0);
    match (orientation, bounds) {
        (Orientation::Portrait, _) | (Orientation::Auto, None) => portrait,
        (Orientation::Landscape, _) => landscape,
        (Orientation::Auto, Some(b)) => {
            let cost = |page: (f64, f64)| {
                let o = overflow(
                    b,
                    (page.0 / 2.0 - centre.0, page.1 / 2.0 - centre.1),
                    frame_box(page, margin_mm),
                );
                (o.area, o.dx + o.dy)
            };
            let (p, l) = (cost(portrait), cost(landscape));
            if l.0 < p.0 || (l.0 == p.0 && l.1 < p.1) {
                landscape
            } else {
                portrait
            }
        }
    }
}

type Rings = Arc<[Vec<Pt>]>;

/// Moved outlines by the address of the original, which the entry holds so that no address
/// is reused.
type MovedOutlines = HashMap<usize, (Rings, Rings)>;

/// Moves every primitive by `d` (mm). Clipped shapes that share an outline keep sharing it,
/// at every nesting level: each outline is moved once.
fn translate(primitives: &mut [Primitive], d: Pt) {
    let mut moved = MovedOutlines::new();
    for p in primitives {
        translate_shape(&mut p.shape, d, &mut moved);
    }
}

fn translate_shape(shape: &mut Shape, d: Pt, moved: &mut MovedOutlines) {
    let shift = |p: &mut Pt| {
        p.0 += d.0;
        p.1 += d.1;
    };
    match shape {
        Shape::Fill { rings, .. } => rings.iter_mut().flatten().for_each(shift),
        Shape::Stroke { path, .. } => path.iter_mut().for_each(shift),
        Shape::Text { origin, .. } => shift(origin),
        Shape::Clipped { rings, inner } => {
            let key = Arc::as_ptr(rings).cast::<()>().addr();
            let (_, to) = moved.entry(key).or_insert_with(|| {
                let to: Rings = rings
                    .iter()
                    .map(|r| r.iter().map(|&(x, y)| (x + d.0, y + d.1)).collect())
                    .collect();
                (Arc::clone(rings), to)
            });
            *rings = Arc::clone(to);
            translate_shape(inner, d, moved);
        }
    }
}

#[cfg(test)]
mod tests;

/// Maps with freemapper's base area for tests (freemapper-print ADR 0044): the colour and symbol as freemapper's
/// `SymbolSet::with_walkable_area` appends them (its ADR 0069), and the base rectangle in its own part.
#[cfg(test)]
mod base_fixture {
    use freemapper_omap::{MapObject, MapObjectKind, MapPart, SymbolSet, closed_ring};

    /// freemapper's colour of the walkable area, the mix of "Lower brown 30%".
    const COLOUR: &str = r#"<color priority="{priority}" name="Lower brown 30% for walkable area" c="0.06" m="0.23" y="0.33" k="0" opacity="1"><spotcolors knockout="true"><component factor="0.3" spotcolor="8"/></spotcolors><cmyk method="custom"/><rgb method="cmyk" r="0.94" g="0.77" b="0.67"/></color>"#;

    /// freemapper's symbol 990 in the colour `{priority}`.
    const SYMBOL: &str = r#"<symbol type="4" id="@" code="990" name="Walkable area (base fill)"><area_symbol inner_color="{priority}" min_area="0" patterns="0"/></symbol>"#;

    /// `set` with the walkable-area colour appended behind all others (priority 32 on ISSprOM 2019) and
    /// symbol 990 in it. Existing priorities and ids stay.
    pub(crate) fn with_walkable_area(set: &SymbolSet) -> SymbolSet {
        let count = set.colors_xml.matches("<color ").count();
        let colour = COLOUR.replace("{priority}", &count.to_string());
        let closing = set.colors_xml.rfind("</colors>").unwrap();
        let mut with = set.clone();
        with.colors_xml = format!("{}{colour}\n</colors>", &set.colors_xml[..closing]).replacen(
            &format!(r#"<colors count="{count}""#),
            &format!(r#"<colors count="{}""#, count + 1),
            1,
        );
        with.with_additions(&SYMBOL.replace("{priority}", &count.to_string()))
    }

    /// ISSprOM 2019 with the walkable area: the palette of a freemapper sprint map.
    pub(crate) fn sprint_set() -> SymbolSet {
        with_walkable_area(&SymbolSet::issprom_2019())
    }

    /// The part `base` with one 990 rectangle from `min` to `max` (µm), tagged as freemapper tags it.
    pub(crate) fn base_part(set: &SymbolSet, min: (i32, i32), max: (i32, i32)) -> MapPart {
        MapPart {
            name: super::BASE_PART.into(),
            objects: vec![MapObject {
                symbol_id: set.id_for_code("990").unwrap(),
                kind: MapObjectKind::Path,
                coords: closed_ring(&[min, (max.0, min.1), max, (min.0, max.1)]),
                tags: vec![(super::BASE_TAG.0.into(), super::BASE_TAG.1.into())],
                pattern: None,
            }],
        }
    }
}

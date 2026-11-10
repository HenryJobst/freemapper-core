//! Copied from freemapper 743ce4b (ADR 0014).
//! Map objects → drawing primitives tagged with a colour priority (`docs/render/oom-render-reference.md`).
//!
//! Paper millimetres with y down.

use std::sync::Arc;

use crate::fonts::{ASCENT, FontFace, LINE_SPACING, char_width, face, text_width};
use crate::geometry::{self, Pt};
use crate::line::line;
use crate::pattern::area_pattern;
use crate::symbols::{
    Cap, Element, Framing, Join, Part as SymbolPart, PointDef, RenderSymbols, Symbol, TextDef,
};
use freemapper_omap::{MapCoord, MapObject, MapObjectKind, PatternOrigin};

/// Which points a fill covers where its rings cross or nest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    /// Inside where a ray from the point crosses the rings an odd number of times. Map areas
    /// fill this way, as in OOM.
    #[default]
    EvenOdd,
    /// Inside where the rings wind around the point a nonzero number of times, as SVG fills by
    /// default.
    NonZero,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Fill of closed rings by `rule`.
    Fill { rings: Vec<Vec<Pt>>, rule: FillRule },
    Stroke {
        path: Vec<Pt>,
        closed: bool,
        width: f64,
        cap: Cap,
        join: Join,
    },
    /// One line of text; `origin` is the start of the baseline. The baseline runs at `rotation`
    /// (radians, counter-clockwise on paper, as OOM's object rotation) about `origin`. `stroke`:
    /// the glyph outlines are stroked this wide (mm, text line framing) instead of filled.
    Text {
        origin: Pt,
        size: f64,
        bold: bool,
        italic: bool,
        text: String,
        rotation: f64,
        stroke: Option<f64>,
    },
    /// `inner` painted inside the even-odd clip of the closed `rings` (area patterns). All
    /// pattern shapes of one area share the same `rings`.
    Clipped {
        rings: Arc<[Vec<Pt>]>,
        inner: Box<Shape>,
    },
}

/// The kind of map symbol a primitive belongs to. Number placement reads it (ADR 0038): a control number may
/// not cover point and line symbols, areas only cost a little. Backends ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SymbolClass {
    Point,
    Line,
    /// Area symbols, and anything that is not a map object.
    #[default]
    Area,
    Text,
}

impl SymbolClass {
    /// The class of a symbol; `None` for a combined symbol, whose class depends on its path.
    fn of(symbol: &Symbol) -> Option<SymbolClass> {
        match symbol {
            Symbol::Point(_) => Some(SymbolClass::Point),
            Symbol::Line(_) => Some(SymbolClass::Line),
            Symbol::Area { .. } => Some(SymbolClass::Area),
            Symbol::Text(_) => Some(SymbolClass::Text),
            Symbol::Combined(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Primitive {
    /// Colour priority (= colour index); 0 is painted last, on top.
    pub priority: i32,
    pub shape: Shape,
    /// The class of the symbol (or symbol part) that drew it.
    pub class: SymbolClass,
    /// Drawn by a magnetic north line (symbol 601 or 601.x), which never counts as covered map detail.
    pub north_line: bool,
    /// Drawn by the base area under a sprint map (the tag `freemapper:source=base`). It is painted like any
    /// other primitive but stays out of the box the map paints in, so it moves neither the page centre nor the
    /// overflow nor the north lines, and it never counts as covered map detail.
    pub base: bool,
    /// Index of the map object that drew it among the objects of its page, so that the primitives of one
    /// object can count once.
    pub object: u32,
    /// Id of the map object's symbol in the map's symbol set, so that a covered symbol can be named.
    pub symbol: u32,
}

impl Primitive {
    /// A primitive of class [`SymbolClass::Area`], neither a north line nor the base area, object 0 and symbol 0.
    /// The drawing code sets the class, the page the rest.
    pub fn new(priority: i32, shape: Shape) -> Self {
        Primitive {
            priority,
            shape,
            class: SymbolClass::Area,
            north_line: false,
            base: false,
            object: 0,
            symbol: 0,
        }
    }
}

/// Segments of a circle drawn as a polygon.
const CIRCLE_SEGMENTS: usize = 48;

fn circle(centre: Pt, radius: f64) -> Vec<Pt> {
    (0..=CIRCLE_SEGMENTS)
        .map(|i| {
            let a = i as f64 / CIRCLE_SEGMENTS as f64 * std::f64::consts::TAU;
            (centre.0 + radius * a.cos(), centre.1 + radius * a.sin())
        })
        .collect()
}

/// OOM 0.001 mm → mm.
pub(crate) fn mm(v: f64) -> f64 {
    v / 1000.0
}

/// Most units of work one map object may use: one per primitive plus its kept points
/// ([`KEPT_POINTS_PER_UNIT`]), one per step that draws nothing (a pattern line or point
/// placement without output, a symbol call), one per [`POINTS_PER_UNIT`] points handled.
/// Guards against hostile or broken files (dense patterns, symbols nested into each other, a
/// combined symbol that draws the same large area many times); real objects stay far below it.
pub const MAX_UNITS_PER_OBJECT: usize = 250_000;

/// Coordinates or outline points handled per unit of budget: copying an element's coordinates
/// for a placement, or testing a point against an outline, costs one unit per this many points
/// (rounded down, so small symbols stay free).
pub(crate) const POINTS_PER_UNIT: usize = 64;

/// Points kept in the output per unit of budget. A primitive costs one unit plus one per this
/// many of its points (rounded down). A clip outline costs one unit per this many points
/// (rounded up) for each clip block the PDF writes for it ([`Sink::charge_outline`]). Kept
/// points stay in memory until the page is written, and the PDF writes each one again as text,
/// so they cost more than points that are only handled ([`POINTS_PER_UNIT`]). A unit keeps
/// fewer than eight points.
pub const KEPT_POINTS_PER_UNIT: usize = 8;

/// The points (or text bytes) `shape` keeps; a clipped shape's outline is charged separately
/// ([`Sink::charge_outline`]).
fn kept_points(shape: &Shape) -> usize {
    match shape {
        Shape::Fill { rings, .. } => rings.iter().map(Vec::len).sum(),
        Shape::Stroke { path, .. } => path.len(),
        Shape::Text { text, .. } => text.len(),
        Shape::Clipped { inner, .. } => kept_points(inner),
    }
}

/// Collects the primitives of one drawing call within a budget.
pub(crate) struct Sink {
    pub(crate) out: Vec<Primitive>,
    left: usize,
    /// Every unit asked for, also beyond the budget: what the object costs the page budget, and
    /// the proof in tests that drawing stays within its own.
    spent: usize,
    /// The budget ran out and output was dropped.
    pub(crate) cut: bool,
    /// The class every primitive pushed now gets.
    class: SymbolClass,
}

// Units asked for by all sinks of this thread, so tests also see work done in a sink other than
// the object's own.
#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Units asked for by all sinks of this thread so far.
#[cfg(any(test, feature = "test-support"))]
pub fn work_units() -> usize {
    WORK.with(std::cell::Cell::get)
}

/// How far a drawing call may ask beyond its budget: the steps in progress when the budget runs
/// out still ask for their units before the loops around them stop, at most one step per
/// nesting level (a settle, a push of 1 + n / 8 units, an outline test or element copy of
/// n / 64 units). The test objects stay well inside it; a primitive with more than 8000 points,
/// or an element or outline with more than 64 000, could exceed it.
#[cfg(any(test, feature = "test-support"))]
pub const BUDGET_SLACK: usize = 1_000;

impl Sink {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            out: Vec::new(),
            left: budget,
            spent: 0,
            cut: false,
            class: SymbolClass::Area,
        }
    }

    /// Records `units` asked for.
    fn request(&mut self, units: usize) {
        self.spent = self.spent.saturating_add(units);
        #[cfg(any(test, feature = "test-support"))]
        WORK.with(|w| w.set(w.get().saturating_add(units)));
    }

    /// Units asked for so far, also beyond the budget.
    #[cfg(test)]
    pub(crate) fn spent(&self) -> usize {
        self.spent
    }

    /// Adds `primitive` for one unit plus its kept points ([`KEPT_POINTS_PER_UNIT`]); one that
    /// costs more than is left is dropped, and the budget counts as spent.
    pub(crate) fn push(&mut self, mut primitive: Primitive) {
        primitive.class = self.class;
        let units = 1 + kept_points(&primitive.shape) / KEPT_POINTS_PER_UNIT;
        self.request(units);
        if units > self.left {
            self.left = 0;
            self.cut = true;
            return;
        }
        self.left -= units;
        self.out.push(primitive);
    }

    pub(crate) fn extend(&mut self, primitives: impl IntoIterator<Item = Primitive>) {
        for p in primitives {
            self.push(p);
        }
    }

    /// True (and marks the output cut) once the budget is spent; drawing loops stop then.
    pub(crate) fn stop(&mut self) -> bool {
        if self.left == 0 {
            self.cut = true;
        }
        self.left == 0
    }

    /// Units left.
    pub(crate) fn left(&self) -> usize {
        self.left
    }

    /// Charges `units` of work that drew nothing.
    pub(crate) fn charge(&mut self, units: usize) {
        self.request(units);
        if units > self.left {
            self.cut = true;
        }
        self.left = self.left.saturating_sub(units);
    }

    /// Charges the work of handling `points` coordinates ([`POINTS_PER_UNIT`]).
    pub(crate) fn charge_points(&mut self, points: usize) {
        self.charge(points / POINTS_PER_UNIT);
    }

    /// Charges one clip block of an outline of `points` points: one unit per
    /// [`KEPT_POINTS_PER_UNIT`] points, rounded up. False when it does not fit in the budget;
    /// the budget then counts as spent and the clipped shape must be dropped.
    pub(crate) fn charge_outline(&mut self, points: usize) -> bool {
        let units = points.div_ceil(KEPT_POINTS_PER_UNIT);
        self.request(units);
        if units > self.left {
            self.left = 0;
            self.cut = true;
            return false;
        }
        self.left -= units;
        true
    }

    /// Records that a limit (budget or repeat count) changed the output.
    pub(crate) fn mark_cut(&mut self) {
        self.cut = true;
    }

    /// Charges one unit if nothing was drawn since the output had `len` primitives.
    pub(crate) fn settle(&mut self, len: usize) {
        if self.out.len() == len {
            self.charge(1);
        }
    }
}

/// The primitives of one map object; none when its symbol is unknown or its geometry unusable.
#[cfg(test)]
pub(crate) fn primitives(object: &MapObject, symbols: &RenderSymbols) -> Vec<Primitive> {
    primitives_counted(object, symbols).0
}

/// The primitives of one map object and whether [`MAX_UNITS_PER_OBJECT`] cut them short.
#[cfg(test)]
pub(crate) fn primitives_counted(
    object: &MapObject,
    symbols: &RenderSymbols,
) -> (Vec<Primitive>, bool) {
    let drawn = primitives_within(object, symbols, MAX_UNITS_PER_OBJECT);
    (drawn.primitives, drawn.cut)
}

/// What one map object drew within its budget.
pub struct Drawn {
    pub primitives: Vec<Primitive>,
    /// A limit cut the drawing short.
    pub cut: bool,
    /// Units asked for, also beyond the budget.
    pub units: usize,
}

/// The primitives of one map object within `budget` units.
pub fn primitives_within(object: &MapObject, symbols: &RenderSymbols, budget: usize) -> Drawn {
    let mut sink = Sink::new(budget);
    draw_object(object, symbols, &mut sink);
    let Sink {
        mut out,
        cut,
        spent,
        ..
    } = sink;
    // A colour missing from the colour table is not drawn (as in OOM).
    out.retain(|p| symbols.colour(p.priority).is_some());
    Drawn {
        primitives: out,
        cut,
        units: spent,
    }
}

/// Draws one map object into `sink` (colours not yet filtered).
pub(crate) fn draw_object(object: &MapObject, symbols: &RenderSymbols, sink: &mut Sink) {
    let Some(symbol) = symbols.symbols.get(&object.symbol_id) else {
        return;
    };
    sink.class = match object.kind {
        MapObjectKind::Point { .. } => SymbolClass::Point,
        MapObjectKind::Text { .. } => SymbolClass::Text,
        // A combined symbol counts by its path, as freemapper-courseplan reads maps: closed is an area (a
        // building with its outline), open a line (a wall with its tags).
        MapObjectKind::Path => SymbolClass::of(symbol).unwrap_or_else(|| {
            match geometry::parts(&object.coords).first() {
                Some(part) if !part.closed => SymbolClass::Line,
                _ => SymbolClass::Area,
            }
        }),
    };
    match &object.kind {
        MapObjectKind::Point { rotation_rad } => {
            if let (Symbol::Point(def), Some(c)) = (symbol, object.coords.first()) {
                let r = if def.rotatable { -rotation_rad } else { 0.0 };
                point_symbol(
                    def,
                    (mm(f64::from(c.x)), mm(f64::from(c.y))),
                    r,
                    symbols,
                    sink,
                    0,
                );
            }
        }
        MapObjectKind::Path => path_symbol(
            symbol,
            &object.coords,
            object.pattern.as_ref(),
            symbols,
            sink,
            0,
        ),
        MapObjectKind::Text {
            text,
            h_align,
            v_align,
            rotation_rad,
            box_size,
        } => {
            if let (Symbol::Text(def), Some(c)) = (symbol, object.coords.first()) {
                let placement = Placement {
                    align: (*h_align, *v_align),
                    anchor: (mm(f64::from(c.x)), mm(f64::from(c.y))),
                    box_mm: box_size.map(|(w, h)| (mm(w), mm(h))),
                    rotation: *rotation_rad,
                };
                text_primitives(def, text, &placement, sink);
            }
        }
    }
}

/// Nesting depth of symbols (combined parts, point elements) beyond which drawing stops and the
/// object counts as cut short: a symbol set that refers to itself must not overflow the stack.
pub(crate) const MAX_DEPTH: u32 = 8;

/// A symbol drawn on path coordinates (map objects and point symbol elements); `pattern` is the
/// object's pattern rotation and origin.
pub(crate) fn path_symbol(
    symbol: &Symbol,
    coords: &[MapCoord],
    pattern: Option<&PatternOrigin>,
    symbols: &RenderSymbols,
    sink: &mut Sink,
    depth: u32,
) {
    if depth > MAX_DEPTH {
        sink.mark_cut();
        return;
    }
    if sink.stop() {
        return;
    }
    let len = sink.out.len();
    match symbol {
        Symbol::Area { color, patterns } => {
            sink.charge_points(coords.len());
            let parts = geometry::parts(coords);
            // The fill and the patterns' clip path need more than two points in the first part.
            if parts.first().is_some_and(|p| p.points.len() > 2) {
                let rings: Arc<[Vec<Pt>]> = parts
                    .into_iter()
                    .map(|p| p.points)
                    .filter(|r| r.len() > 2)
                    .collect();
                if *color >= 0 {
                    sink.push(Primitive::new(
                        *color,
                        Shape::Fill {
                            rings: rings.to_vec(),
                            rule: FillRule::EvenOdd,
                        },
                    ));
                }
                for p in patterns {
                    area_pattern(p, &rings, pattern, symbols, sink, depth);
                }
            }
        }
        Symbol::Line(def) => line(def, coords, symbols, sink, depth),
        Symbol::Combined(parts) => {
            for part in parts {
                let part_symbol = match part {
                    SymbolPart::Ref(id) => symbols.symbols.get(id),
                    SymbolPart::Inline(symbol) => Some(&**symbol),
                };
                if let Some(part_symbol) = part_symbol {
                    path_symbol(part_symbol, coords, pattern, symbols, sink, depth + 1);
                }
            }
        }
        // A point symbol element on a point object: drawn at its coordinate, unrotated.
        Symbol::Point(def) => {
            if let Some(c) = coords.first() {
                point_symbol(
                    def,
                    (mm(f64::from(c.x)), mm(f64::from(c.y))),
                    0.0,
                    symbols,
                    sink,
                    depth + 1,
                );
            }
        }
        Symbol::Text(_) => {}
    }
    sink.settle(len);
}

pub(crate) fn stroke(
    color: i32,
    path: Vec<Pt>,
    closed: bool,
    width: f64,
    cap: Cap,
    join: Join,
) -> Option<Primitive> {
    (color >= 0 && width > 0.0 && path.len() >= 2).then_some(Primitive::new(
        color,
        Shape::Stroke {
            path,
            closed,
            width,
            cap,
            join,
        },
    ))
}

/// The filled dot of a point symbol at `at`, if it has one.
pub(crate) fn dot(def: &PointDef, at: Pt) -> Option<Primitive> {
    def.dot.map(|(color, radius)| {
        Primitive::new(
            color,
            Shape::Fill {
                rings: vec![circle(at, mm(radius))],
                rule: FillRule::EvenOdd,
            },
        )
    })
}

/// The stroked ring of a point symbol at `at`, if it has one.
pub(crate) fn ring(def: &PointDef, at: Pt) -> Option<Primitive> {
    let (color, radius, width) = def.ring?;
    stroke(
        color,
        circle(at, mm(radius)),
        true,
        mm(width),
        Cap::Flat,
        Join::Miter,
    )
}

/// An element's coordinates rotated about the point centre `at` and moved there, back in
/// 0.001 mm; the copy is charged to `sink` ([`POINTS_PER_UNIT`]).
pub(crate) fn element_coords(
    element: &Element,
    at: Pt,
    rotation: f64,
    sink: &mut Sink,
) -> Vec<MapCoord> {
    element_coords_scaled(element, at, rotation, 1.0, sink)
}

/// [`element_coords`] with the element coordinates scaled by `scale` first
/// [PS `createRenderablesScaled`].
fn element_coords_scaled(
    element: &Element,
    at: Pt,
    rotation: f64,
    scale: f64,
    sink: &mut Sink,
) -> Vec<MapCoord> {
    sink.charge_points(element.coords.len());
    let (sin, cos) = rotation.sin_cos();
    element
        .coords
        .iter()
        .map(|c| {
            let (ox, oy) = (scale * f64::from(c.x), scale * f64::from(c.y));
            MapCoord {
                x: (ox * cos - oy * sin + at.0 * 1000.0).round() as i32,
                y: (oy * cos + ox * sin + at.1 * 1000.0).round() as i32,
                flags: c.flags,
            }
        })
        .collect()
}

pub(crate) fn point_symbol(
    def: &PointDef,
    at: Pt,
    rotation: f64,
    symbols: &RenderSymbols,
    sink: &mut Sink,
    depth: u32,
) {
    point_symbol_scaled(def, at, rotation, 1.0, symbols, sink, depth);
}

/// [`point_symbol`] with the element coordinates scaled by `scale` (dash symbols); dot and ring
/// keep their size [PS `createRenderablesScaled`].
pub(crate) fn point_symbol_scaled(
    def: &PointDef,
    at: Pt,
    rotation: f64,
    scale: f64,
    symbols: &RenderSymbols,
    sink: &mut Sink,
    depth: u32,
) {
    if depth > MAX_DEPTH {
        sink.mark_cut();
        return;
    }
    if sink.stop() {
        return;
    }
    let len = sink.out.len();
    sink.extend(dot(def, at));
    sink.extend(ring(def, at));
    for element in &def.elements {
        if sink.stop() {
            break;
        }
        let coords = element_coords_scaled(element, at, rotation, scale, sink);
        path_symbol(&element.symbol, &coords, None, symbols, sink, depth + 1);
    }
    sink.settle(len);
}

/// Where a text object goes: its alignment (OOM `h_align`, `v_align`), anchor (mm), box (mm, a
/// box text's anchor is the middle of the box) and rotation (radians).
struct Placement {
    align: (u8, u8),
    anchor: Pt,
    box_mm: Option<(f64, f64)>,
    rotation: f64,
}

/// The text primitives of a text object, laid out per reference §6: the framing first (a stroke
/// of the glyph outlines or the shadow copy, in the framing colour), then the text, one primitive
/// per line [TS `createRenderables`]. A rotatable text symbol turns the lines about the anchor by
/// the object's rotation, counter-clockwise on paper [RI `TextRenderable`].
fn text_primitives(def: &TextDef, text: &str, placement: &Placement, sink: &mut Sink) {
    let rotation = if def.rotatable {
        placement.rotation
    } else {
        0.0
    };
    let lines = text_layout(def, text, placement, sink);
    let anchor = placement.anchor;
    match def.framing {
        Some(Framing::Line { color, half_width }) => {
            let layer = Layer {
                color,
                anchor,
                stroke: Some(2.0 * mm(half_width)),
            };
            text_lines(def, &lines, layer, rotation, sink);
        }
        // The shadow offset is not turned with the text [TS `createRenderables`].
        Some(Framing::Shadow { color, dx, dy }) => {
            let layer = Layer {
                color,
                anchor: (anchor.0 + mm(dx), anchor.1 + mm(dy)),
                stroke: None,
            };
            text_lines(def, &lines, layer, rotation, sink);
        }
        None => {}
    }
    let layer = Layer {
        color: def.color,
        anchor,
        stroke: None,
    };
    text_lines(def, &lines, layer, rotation, sink);
}

/// One painting of a text: its colour, anchor and stroke width (line framing, mm).
struct Layer {
    color: i32,
    anchor: Pt,
    stroke: Option<f64>,
}

/// One line of a laid-out text: the start of its baseline in text coordinates (mm, y down,
/// relative to the anchor, before rotation) and its characters.
#[derive(Debug)]
struct TextLine<'a> {
    at: Pt,
    text: &'a str,
}

/// The lines of `text` and where they start [TO `prepareLineInfos`]. Each `\n` ends a paragraph.
/// A box text wraps a paragraph at the last space where the line still fits the box width (a
/// word wider than the box stays whole) and aligns the lines to the box edges; a single-anchor
/// text aligns them to the anchor. Each line costs one unit of `sink`, plus the characters
/// measured ([`POINTS_PER_UNIT`]), and layout stops when the budget is spent.
fn text_layout<'a>(
    def: &TextDef,
    text: &'a str,
    placement: &Placement,
    sink: &mut Sink,
) -> Vec<TextLine<'a>> {
    let (h_align, v_align) = placement.align;
    let em = mm(def.size);
    let face = face(def.bold, def.italic);
    let ascent = ASCENT * em;
    let line_step = def.line_spacing * LINE_SPACING * em;
    let paragraph_step = mm(def.paragraph_spacing);
    let (box_w, box_h) = placement.box_mm.unwrap_or((0.0, 0.0));
    let line_x = match h_align {
        0 => -box_w / 2.0,
        2 => box_w / 2.0,
        _ => 0.0,
    };
    let mut y = 0.0;
    if v_align == 0 || v_align == 1 {
        y -= box_h / 2.0;
    }
    if v_align != 0 {
        y += ascent;
    }
    let mut lines = Vec::new();
    let mut widths = Vec::new();
    let mut paragraphs = 0;
    'paragraphs: for paragraph in text.split('\n') {
        let mut rest = paragraph;
        loop {
            sink.charge(1);
            let end = match placement.box_mm {
                Some(_) => wrap_end(rest, face, em, box_w),
                None => rest.len(),
            };
            let line = &rest[..end];
            sink.charge_points(end);
            if sink.stop() {
                break 'paragraphs;
            }
            widths.push(text_width(line, face) * em);
            lines.push(TextLine {
                at: (line_x, y),
                text: line,
            });
            y += line_step;
            if end == rest.len() {
                y += paragraph_step;
                paragraphs += 1;
                break;
            }
            // The space the line broke at belongs to neither line.
            rest = &rest[end + 1..];
        }
    }
    let delta_y = match v_align {
        2 | 3 => {
            let height = ascent
                + lines.len().saturating_sub(1) as f64 * line_step
                + (paragraphs as f64 - 1.0).max(0.0) * paragraph_step;
            if v_align == 2 {
                -height / 2.0
            } else {
                -height + box_h / 2.0
            }
        }
        _ => 0.0,
    };
    for (line, width) in lines.iter_mut().zip(widths) {
        line.at.0 -= match h_align {
            1 => width / 2.0,
            2 => width,
            _ => 0.0,
        };
        line.at.1 += delta_y;
    }
    lines
}

/// Where the first line of `paragraph` ends in a box `box_w` mm wide (byte index): at the last
/// space before which the text still fits; at the first space when not even the first word fits;
/// at the end when the whole paragraph fits or has no space [TO `prepareLineInfos`]. A space at
/// the start is no break. Linear in the length of the line it returns.
fn wrap_end(paragraph: &str, face: FontFace, em: f64, box_w: f64) -> usize {
    let mut width = 0.0;
    let mut fits = None;
    for (i, c) in paragraph.char_indices() {
        if c == ' ' && i > 0 {
            if width <= box_w {
                fits = Some(i);
            } else {
                return fits.unwrap_or(i);
            }
        }
        width += char_width(c, face) * em;
    }
    if width <= box_w {
        paragraph.len()
    } else {
        fits.unwrap_or(paragraph.len())
    }
}

/// One text primitive per non-empty line of `lines`, painted as `layer`.
fn text_lines(
    def: &TextDef,
    lines: &[TextLine<'_>],
    Layer {
        color,
        anchor,
        stroke,
    }: Layer,
    rotation: f64,
    sink: &mut Sink,
) {
    if color < 0 {
        return;
    }
    // Text coordinates (x, y) → paper: turned by −rotation (y down) about the anchor.
    let (sin, cos) = rotation.sin_cos();
    for TextLine { at: (x, y), text } in lines {
        if !text.is_empty() {
            sink.push(Primitive::new(
                color,
                Shape::Text {
                    origin: (anchor.0 + x * cos + y * sin, anchor.1 - x * sin + y * cos),
                    size: mm(def.size),
                    bold: def.bold,
                    italic: def.italic,
                    text: (*text).to_string(),
                    rotation,
                    stroke,
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{Clipping, Dash, LineDef, Pattern};
    use freemapper_omap::coord::{FLAG_CLOSE, FLAG_HOLE};
    use freemapper_omap::{PatternOrigin, SymbolSet, closed_ring};

    fn set() -> (RenderSymbols, SymbolSet) {
        let set = SymbolSet::issprom_2019().with_sheet_symbols();
        (RenderSymbols::from_set(&set).unwrap(), set)
    }

    fn object(
        set: &SymbolSet,
        code: &str,
        kind: MapObjectKind,
        coords: Vec<MapCoord>,
    ) -> MapObject {
        MapObject {
            symbol_id: set.id_for_code(code).unwrap(),
            kind,
            coords,
            tags: Vec::new(),
            pattern: None,
        }
    }

    fn square(side: i32) -> Vec<MapCoord> {
        closed_ring(&[(0, 0), (side, 0), (side, side), (0, side)])
    }

    fn strokes(p: &[Primitive]) -> Vec<(i32, f64)> {
        p.iter()
            .filter_map(|p| match &p.shape {
                Shape::Stroke { width, .. } => {
                    Some((p.priority, (width * 1000.0).round() / 1000.0))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn paved_area_edge_lies_below_its_fill() {
        let (r, s) = set();
        let p = primitives(&object(&s, "501", MapObjectKind::Path, square(10_000)), &r);
        let fills: Vec<i32> = p
            .iter()
            .filter(|p| matches!(p.shape, Shape::Fill { .. }))
            .map(|p| p.priority)
            .collect();
        assert_eq!(fills, [14]);
        assert_eq!(strokes(&p), [(15, 0.2)]);
        let closed = p
            .iter()
            .any(|p| matches!(p.shape, Shape::Stroke { closed: true, .. }));
        assert!(closed, "the edge of an area is a closed line");
    }

    #[test]
    fn training_pdf_paints_water_under_paved_areas_and_buildings() {
        let (r, s) = set();
        // Painted highest priority number first, so a larger number lies lower.
        let fill_priority = |code: &str| {
            let p = primitives(&object(&s, code, MapObjectKind::Path, square(10_000)), &r);
            p.iter()
                .filter(|p| matches!(p.shape, Shape::Fill { .. }))
                .map(|p| p.priority)
                .max()
                .unwrap_or_else(|| panic!("{code} has a fill"))
        };
        let water = fill_priority("301.1");
        assert!(water > fill_priority("501.5"), "paved areas cover water");
        assert!(water > fill_priority("521.3"), "buildings cover water");
        assert!(water < fill_priority("520"), "water covers out of bounds");
        let bank = primitives(
            &object(&s, "301.6", MapObjectKind::Path, square(10_000)),
            &r,
        );
        assert!(
            bank.iter().all(|p| p.priority < water),
            "the bank line is on top of the fill"
        );
    }

    #[test]
    fn bush_is_dot_and_ring() {
        let (r, s) = set();
        let p = primitives(
            &object(
                &s,
                "418",
                MapObjectKind::Point { rotation_rad: 0.0 },
                vec![MapCoord::new(5_000, 5_000)],
            ),
            &r,
        );
        assert_eq!(p.len(), 2);
        let Shape::Fill { rings, .. } = &p.iter().find(|p| p.priority == 21).unwrap().shape else {
            panic!()
        };
        let r_dot = rings[0]
            .iter()
            .map(|q| (q.0 - 5.0).hypot(q.1 - 5.0))
            .fold(0.0, f64::max);
        assert!((r_dot - 0.1).abs() < 1e-6);
        let Shape::Stroke {
            path,
            width,
            closed,
            ..
        } = &p.iter().find(|p| p.priority == 3).unwrap().shape
        else {
            panic!()
        };
        assert!(*closed && (*width - 0.3).abs() < 1e-9);
        assert!((((path[0].0 - 5.0).hypot(path[0].1 - 5.0)) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn fence_ticks_on_the_right() {
        let (r, s) = set();
        let fence = object(
            &s,
            "518",
            MapObjectKind::Path,
            vec![MapCoord::new(0, 0), MapCoord::new(20_000, 0)],
        );
        let p = primitives(&fence, &r);
        // Main line 0.4 mm and five ticks of two lines 0.14 mm each.
        let widths = strokes(&p);
        assert_eq!(widths.iter().filter(|w| w.1 == 0.4).count(), 1);
        assert_eq!(widths.iter().filter(|w| w.1 == 0.14).count(), 10);
        for prim in &p {
            if let Shape::Stroke { path, width, .. } = &prim.shape
                && (*width - 0.14).abs() < 1e-9
            {
                assert!(
                    path.iter().all(|q| q.1 >= -1e-9),
                    "right of travel (south): {path:?}"
                );
            }
        }
    }

    #[test]
    fn building_has_fill_and_outline() {
        let (r, s) = set();
        let mut coords = square(10_000);
        coords.extend(closed_ring(&[
            (2_000, 2_000),
            (4_000, 2_000),
            (4_000, 4_000),
            (2_000, 4_000),
        ]));
        let p = primitives(&object(&s, "521", MapObjectKind::Path, coords), &r);
        let Shape::Fill { rings, .. } = &p.iter().find(|p| p.priority == 9).unwrap().shape else {
            panic!()
        };
        assert_eq!(rings.len(), 2, "outer ring and courtyard");
        assert_eq!(strokes(&p), [(2, 0.14), (2, 0.14)]);
        let _ = (FLAG_CLOSE, FLAG_HOLE);
    }

    #[test]
    fn text_alignment_uses_widths() {
        let (r, s) = set();
        let text = |h_align| {
            object(
                &s,
                "992",
                MapObjectKind::Text {
                    text: "Maßstab".into(),
                    h_align,
                    v_align: 0,
                    rotation_rad: 0.0,
                    box_size: None,
                },
                vec![MapCoord::new(50_000, 20_000)],
            )
        };
        let em = 2.5;
        let width = text_width("Maßstab", FontFace::Regular) * em;
        for (h_align, x) in [(0, 50.0), (1, 50.0 - width / 2.0), (2, 50.0 - width)] {
            let p = primitives(&text(h_align), &r);
            let Shape::Text { origin, size, .. } = &p[0].shape else {
                panic!()
            };
            assert!(
                (origin.0 - x).abs() < 1e-9 && (origin.1 - 20.0).abs() < 1e-9,
                "{h_align}: {origin:?}"
            );
            assert_eq!(*size, em);
            assert_eq!(p[0].priority, 2);
        }
    }

    const TEXT: u32 = 9_997;

    /// The primitives of `text` in a text symbol `def` at (50, 20) mm.
    fn text_with(def: TextDef, text: &str, h_align: u8, rotation_rad: f64) -> Vec<Primitive> {
        let (mut r, _) = set();
        r.symbols.insert(TEXT, Symbol::Text(def));
        let object = MapObject {
            symbol_id: TEXT,
            kind: MapObjectKind::Text {
                text: text.into(),
                h_align,
                v_align: 0,
                rotation_rad,
                box_size: None,
            },
            coords: vec![MapCoord::new(50_000, 20_000)],
            tags: Vec::new(),
            pattern: None,
        };
        primitives(&object, &r)
    }

    fn text_def(framing: Option<crate::symbols::Framing>) -> TextDef {
        TextDef {
            family: "Arial".into(),
            color: 2,
            size: 4_000.0,
            bold: false,
            italic: false,
            line_spacing: 1.0,
            paragraph_spacing: 0.0,
            rotatable: true,
            framing,
        }
    }

    /// The (origin, text) of each line of `text` in `def`, anchored at (50, 20) mm, in a box of
    /// `box_mm` when given.
    fn laid_out(
        def: TextDef,
        text: &str,
        (h_align, v_align): (u8, u8),
        box_mm: Option<(f64, f64)>,
    ) -> Vec<(Pt, String)> {
        let (mut r, _) = set();
        r.symbols.insert(TEXT, Symbol::Text(def));
        let object = MapObject {
            symbol_id: TEXT,
            kind: MapObjectKind::Text {
                text: text.into(),
                h_align,
                v_align,
                rotation_rad: 0.0,
                box_size: box_mm.map(|(w, h)| (w * 1000.0, h * 1000.0)),
            },
            coords: vec![MapCoord::new(50_000, 20_000)],
            tags: Vec::new(),
            pattern: None,
        };
        primitives(&object, &r)
            .into_iter()
            .map(|p| match p.shape {
                Shape::Text { origin, text, .. } => (origin, text),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn box_text_starts_at_the_left_and_top_edge() {
        // A 40 × 20 mm box around (50, 20): left edge x = 30, top edge y = 10 [TO
        // `prepareLineInfos`].
        let lines = laid_out(text_def(None), "Wald", (0, 1), Some((40.0, 20.0)));
        let [(origin, _)] = &lines[..] else {
            panic!("{lines:?}")
        };
        assert!(near(origin.0, 30.0), "{origin:?}");
        assert!(near(origin.1, 10.0 + ASCENT * 4.0), "{origin:?}");
        // Right and bottom: the line ends at the right edge, the baseline sits half the box below
        // the anchor less the ascent's share of the height (one line: height = ascent).
        let width = text_width("Wald", FontFace::Regular) * 4.0;
        let lines = laid_out(text_def(None), "Wald", (2, 3), Some((40.0, 20.0)));
        assert!(near(lines[0].0.0, 70.0 - width), "{lines:?}");
        assert!(near(lines[0].0.1, 30.0), "{lines:?}");
        // Baseline alignment in a box: the first baseline on the top edge.
        let lines = laid_out(text_def(None), "Wald", (1, 0), Some((40.0, 20.0)));
        assert!(near(lines[0].0.0, 50.0 - width / 2.0) && near(lines[0].0.1, 10.0));
    }

    #[test]
    fn long_box_text_wraps_inside_the_box() {
        let text = "Der Weg führt am Wasser entlang bis zur alten Brücke am Markt";
        let lines = laid_out(text_def(None), text, (0, 1), Some((40.0, 60.0)));
        assert!(lines.len() > 2, "{lines:?}");
        let step = LINE_SPACING * 4.0;
        for (k, (origin, line)) in lines.iter().enumerate() {
            assert!(near(origin.0, 30.0), "{lines:?}");
            assert!(near(origin.1, 20.0 - 30.0 + ASCENT * 4.0 + k as f64 * step));
            let width = text_width(line, FontFace::Regular) * 4.0;
            assert!(width <= 40.0, "{line}: {width}");
        }
        // Each line takes as many words as fit: the next word would not.
        for pair in lines.windows(2) {
            let first_word = pair[1].1.split(' ').next().unwrap();
            let longer = format!("{} {first_word}", pair[0].1);
            assert!(
                text_width(&longer, FontFace::Regular) * 4.0 > 40.0,
                "{longer}"
            );
        }
        let joined: Vec<&str> = lines.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(joined.join(" "), text);
        // Without a box, nothing wraps.
        assert_eq!(laid_out(text_def(None), text, (0, 1), None).len(), 1);
    }

    #[test]
    fn a_word_longer_than_the_box_stays_whole() {
        let lines = laid_out(
            text_def(None),
            "Donaudampfschifffahrt ab",
            (0, 1),
            Some((10.0, 20.0)),
        );
        let texts: Vec<&str> = lines.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(texts, ["Donaudampfschifffahrt", "ab"]);
    }

    #[test]
    fn paragraph_spacing_adds_after_each_paragraph() {
        let def = TextDef {
            paragraph_spacing: 1_500.0,
            ..text_def(None)
        };
        let lines = laid_out(def.clone(), "Alte\nMühle", (0, 0), None);
        let step = LINE_SPACING * 4.0;
        assert!(
            near(lines[0].0.1, 20.0) && near(lines[1].0.1, 20.0 + step + 1.5),
            "{lines:?}"
        );
        // A wrapped line is no paragraph end: no extra space inside the paragraph.
        let lines = laid_out(def.clone(), "aaaa bbbb\ncccc", (0, 0), Some((10.0, 40.0)));
        let ys: Vec<f64> = lines.iter().map(|(o, _)| o.1 - lines[0].0.1).collect();
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(near(ys[1], step) && near(ys[2], 2.0 * step + 1.5), "{ys:?}");
        // Bottom alignment counts the spacing between paragraphs into the height.
        let lines = laid_out(def, "Alte\nMühle", (0, 3), None);
        assert!(
            near(lines[1].0.1, 20.0),
            "last baseline on the anchor: {lines:?}"
        );
    }

    #[test]
    fn text_line_framing() {
        use crate::symbols::Framing;
        let p = text_with(
            text_def(Some(Framing::Line {
                color: 10,
                half_width: 150.0,
            })),
            "Wald",
            0,
            0.0,
        );
        assert_eq!(p.len(), 2);
        // The framing first, so that it lies below the glyphs where the colours are equal; its
        // own colour decides the order otherwise [TS `createRenderables`].
        let (
            Shape::Text {
                stroke,
                origin,
                text,
                size,
                ..
            },
            Shape::Text {
                stroke: fill,
                origin: o2,
                text: t2,
                ..
            },
        ) = (&p[0].shape, &p[1].shape)
        else {
            panic!("{p:?}")
        };
        assert_eq!((p[0].priority, p[1].priority), (10, 2));
        assert_eq!(*stroke, Some(0.3), "2 × half width");
        assert_eq!(*fill, None);
        assert_eq!((origin, text), (o2, t2));
        assert_eq!((*origin, *size), ((50.0, 20.0), 4.0));
    }

    #[test]
    fn text_shadow_framing() {
        use crate::symbols::Framing;
        let p = text_with(
            text_def(Some(Framing::Shadow {
                color: 10,
                dx: 200.0,
                dy: -100.0,
            })),
            "Wald",
            0,
            0.0,
        );
        assert_eq!(p.len(), 2);
        let Shape::Text { origin, stroke, .. } = &p[0].shape else {
            panic!()
        };
        assert_eq!(p[0].priority, 10);
        assert_eq!(*stroke, None);
        assert!((origin.0 - 50.2).abs() < 1e-9 && (origin.1 - 19.9).abs() < 1e-9);
    }

    #[test]
    fn rotatable_text_turns_about_its_anchor() {
        // Right-aligned, turned a quarter counter-clockwise: the line runs north and ends at the
        // anchor [RI `TextRenderable`].
        let quarter = std::f64::consts::FRAC_PI_2;
        let width = text_width("Wald", FontFace::Regular) * 4.0;
        let p = text_with(text_def(None), "Wald", 2, quarter);
        let Shape::Text {
            origin, rotation, ..
        } = &p[0].shape
        else {
            panic!()
        };
        assert_eq!(*rotation, quarter);
        assert!(
            (origin.0 - 50.0).abs() < 1e-9 && (origin.1 - (20.0 + width)).abs() < 1e-9,
            "{origin:?}"
        );
        // Not rotatable: the object's rotation is ignored.
        let fixed = TextDef {
            rotatable: false,
            ..text_def(None)
        };
        let p = text_with(fixed, "Wald", 2, quarter);
        let Shape::Text {
            origin, rotation, ..
        } = &p[0].shape
        else {
            panic!()
        };
        assert_eq!(*rotation, 0.0);
        assert!((origin.0 - (50.0 - width)).abs() < 1e-9 && origin.1 == 20.0);
    }

    fn closed_square(side: i32) -> Vec<MapCoord> {
        closed_ring(&[(0, 0), (side, 0), (side, side), (0, side)])
    }

    fn stroke_paths(p: &[Primitive], width: f64) -> Vec<(&Vec<Pt>, bool)> {
        p.iter()
            .filter_map(|p| match &p.shape {
                Shape::Stroke {
                    path,
                    closed,
                    width: w,
                    ..
                } if (w - width).abs() < 1e-9 => Some((path, *closed)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn fence_ring_does_not_wrap_around() {
        let (r, s) = set();
        // 80 mm ring: ticks keep e2 from the closing point (0, 0) on both sides.
        let p = primitives(
            &object(&s, "518", MapObjectKind::Path, closed_square(20_000)),
            &r,
        );
        let ticks = stroke_paths(&p, 0.14);
        assert_eq!(ticks.len() % 2, 0);
        let spots = ticks.len() / 2;
        assert_eq!(spots, 21, "n·3.75 + 4.65 closest to 80 mm");
        for (path, _) in &ticks {
            let d = path
                .iter()
                .map(|q| q.0.hypot(q.1))
                .fold(f64::INFINITY, f64::min);
            assert!(d > 1.5, "no tick at the closing point: {path:?}");
        }
        let main = stroke_paths(&p, 0.4);
        assert_eq!(main.len(), 1);
        assert!(main[0].1, "the ring itself is closed");
    }

    #[test]
    fn path_ring_borders_are_dashed_alike_on_both_sides() {
        let (r, s) = set();
        let p = primitives(
            &object(&s, "505.1", MapObjectKind::Path, closed_square(10_000)),
            &r,
        );
        let borders = stroke_paths(&p, 0.1);
        assert!(borders.len() > 4);
        assert_eq!(borders.len() % 2, 0, "one dash per side and position");
        assert!(
            borders.iter().all(|(_, closed)| !closed),
            "dashes are open strokes"
        );
    }

    #[test]
    fn short_path_ring_has_one_border_dash_per_side() {
        let (r, s) = set();
        // 2.4 mm around: two half dashes joined at the closing point and one gap (reference §2.2:
        // closed parts always have H = 2, so they are never drawn solid).
        let p = primitives(
            &object(&s, "505.1", MapObjectKind::Path, closed_square(600)),
            &r,
        );
        let borders = stroke_paths(&p, 0.1);
        assert_eq!(borders.len(), 2);
        assert!(borders.iter().all(|(_, closed)| !closed));
    }

    #[test]
    fn self_referencing_combined_symbol_stops() {
        let (mut r, s) = set();
        let id = s.id_for_code("521").unwrap();
        r.symbols
            .insert(id, Symbol::Combined(vec![SymbolPart::Ref(id)]));
        let (p, cut) =
            primitives_counted(&object(&s, "521", MapObjectKind::Path, square(10_000)), &r);
        assert!(p.is_empty());
        assert!(cut, "the depth limit counts as cut short");
    }

    #[test]
    fn end_bars_of_a_closed_line_follow_their_segments() {
        let set = SymbolSet::school_2019();
        let r = RenderSymbols::from_set(&set).unwrap();
        let bench = object(&set, "543", MapObjectKind::Path, closed_square(5_000));
        let p = primitives(&bench, &r);
        // Bars along local y, 0.8 mm (enlarged 125 % in the school set? no: school additions are
        // final sizes): the start bar is perpendicular to the first segment (east), so vertical.
        let bars: Vec<&Vec<Pt>> = p
            .iter()
            .filter_map(|p| match &p.shape {
                Shape::Stroke { path, .. } if path.len() == 2 && geometry::length(path) < 1.0 => {
                    Some(path)
                }
                _ => None,
            })
            .collect();
        assert_eq!(bars.len(), 2, "{p:?}");
        let vertical = |b: &Vec<Pt>| (b[0].0 - b[1].0).abs() < 1e-6;
        let horizontal = |b: &Vec<Pt>| (b[0].1 - b[1].1).abs() < 1e-6;
        assert!(
            bars.iter().any(|b| vertical(b)),
            "start bar ⟂ east: {bars:?}"
        );
        assert!(
            bars.iter().any(|b| horizontal(b)),
            "end bar ⟂ north: {bars:?}"
        );
    }

    #[test]
    fn tiny_dash_on_long_line_is_bounded() {
        let (mut r, _) = set();
        let dashed = |dash: f64| {
            Symbol::Line(LineDef {
                color: 2,
                width: 100.0,
                cap: Cap::Flat,
                join: Join::Miter,
                dash: Some(Dash {
                    dash,
                    gap: dash,
                    group: 1,
                    in_group_gap: 0.0,
                    half_outer: false,
                }),
                border: None,
                mid: None,
                start: None,
                end: None,
                pointed_caps: false,
                start_offset: 0.0,
                end_offset: 0.0,
                dash_symbol: None,
            })
        };
        let line_object = |coords| MapObject {
            symbol_id: PATTERNED,
            kind: MapObjectKind::Path,
            coords,
            tags: Vec::new(),
            pattern: None,
        };
        let before = work_units();
        // 1 µm dashes on 1 km of paper: 5 · 10⁸ dashes, beyond the repeat limit, drawn solid.
        r.symbols.insert(PATTERNED, dashed(1.0));
        let km = line_object(vec![MapCoord::new(0, 0), MapCoord::new(1_000_000_000, 0)]);
        let (p, cut) = primitives_counted(&km, &r);
        assert_eq!((p.len(), cut), (1, true));
        // Ten 900 mm parts with 90 000 dashes each: the budget stops them.
        r.symbols.insert(PATTERNED, dashed(5.0));
        let coords: Vec<MapCoord> = (0..10)
            .flat_map(|i| {
                [
                    MapCoord::new(0, i * 1_000),
                    MapCoord {
                        flags: FLAG_HOLE,
                        ..MapCoord::new(900_000, i * 1_000)
                    },
                ]
            })
            .collect();
        let (p, cut) = primitives_counted(&line_object(coords), &r);
        assert!(cut);
        // The rest went to one unit per dash section: the parts begun before the budget ran out.
        assert!(
            (MAX_UNITS_PER_OBJECT - 10..=MAX_UNITS_PER_OBJECT).contains(&p.len()),
            "{}",
            p.len()
        );
        // Both objects together: one budget plus the slack, plus the solid kilometre.
        let work = work_units() - before;
        assert!(work <= MAX_UNITS_PER_OBJECT + BUDGET_SLACK + 1, "{work}");
    }

    /// Ten 900 mm parts, `def` on each.
    fn ten_long_parts(r: &mut RenderSymbols, def: LineDef) -> MapObject {
        r.symbols.insert(PATTERNED, Symbol::Line(def));
        MapObject {
            symbol_id: PATTERNED,
            kind: MapObjectKind::Path,
            coords: (0..10)
                .flat_map(|i| {
                    [
                        MapCoord::new(0, i * 1_000),
                        MapCoord {
                            flags: FLAG_HOLE,
                            ..MapCoord::new(900_000, i * 1_000)
                        },
                    ]
                })
                .collect(),
            tags: Vec::new(),
            pattern: None,
        }
    }

    #[test]
    fn invalid_border_costs_nothing() {
        let (mut r, _) = set();
        // No main line, a border with 5 µm dashes (90 000 per part) but no width or no colour.
        for (color, width) in [(3, 0.0), (-1, 100.0)] {
            let def = LineDef {
                color: -1,
                width: 100.0,
                cap: Cap::Flat,
                join: Join::Miter,
                dash: None,
                border: Some(crate::symbols::Border {
                    color,
                    width,
                    shift: 0.0,
                    dash: Some(Dash {
                        dash: 5.0,
                        gap: 5.0,
                        group: 1,
                        in_group_gap: 0.0,
                        half_outer: false,
                    }),
                }),
                mid: None,
                start: None,
                end: None,
                pointed_caps: false,
                start_offset: 0.0,
                end_offset: 0.0,
                dash_symbol: None,
            };
            let object = ten_long_parts(&mut r, def);
            let before = work_units();
            let (p, cut) = primitives_counted(&object, &r);
            let work = work_units() - before;
            assert!(p.is_empty() && !cut, "{color} {width}");
            // One settle for the object that drew nothing.
            assert!(work <= 1, "{color} {width}: {work}");
        }
    }

    #[test]
    fn unknown_colours_are_left_out() {
        let (mut r, s) = set();
        let id = s.id_for_code("520").unwrap();
        r.symbols.insert(
            id,
            Symbol::Area {
                color: 99,
                patterns: Vec::new(),
            },
        );
        let p = primitives(&object(&s, "520", MapObjectKind::Path, square(10_000)), &r);
        assert!(p.is_empty());
    }

    const PATTERNED: u32 = 9_999;
    const COMBINED: u32 = 9_997;

    /// Fill and stroke points in `p`.
    fn kept(p: &[Primitive]) -> usize {
        p.iter()
            .map(|p| match &p.shape {
                Shape::Fill { rings, .. } => rings.iter().map(Vec::len).sum(),
                Shape::Stroke { path, .. } => path.len(),
                _ => 0,
            })
            .sum()
    }

    /// An object of 20 000 points whose combined symbol draws `part` on it 1000 times (final
    /// review of M1: a 260 KB file took 565 MB).
    fn thousand_parts(r: &mut RenderSymbols, part: Symbol) -> MapObject {
        r.symbols.insert(PATTERNED, part);
        r.symbols.insert(
            COMBINED,
            Symbol::Combined(vec![SymbolPart::Ref(PATTERNED); 1000]),
        );
        let zigzag: Vec<(i32, i32)> = (0..20_000)
            .map(|i| (i % 1000 * 10, i / 1000 * 10))
            .collect();
        MapObject {
            symbol_id: COMBINED,
            kind: MapObjectKind::Path,
            coords: closed_ring(&zigzag),
            tags: Vec::new(),
            pattern: None,
        }
    }

    #[test]
    fn copied_points_count_against_the_budget() {
        let line = Symbol::Line(LineDef {
            color: 2,
            width: 100.0,
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
        });
        let fill = Symbol::Area {
            color: 2,
            patterns: Vec::new(),
        };
        for part in [fill, line] {
            let (mut r, _) = set();
            let object = thousand_parts(&mut r, part);
            let before = work_units();
            let (p, cut) = primitives_counted(&object, &r);
            let work = work_units() - before;
            assert!(cut, "{} primitives", p.len());
            assert!(work <= MAX_UNITS_PER_OBJECT + BUDGET_SLACK, "{work}");
            // Each copy holds 20 000 points; the budget lets a few dozen through.
            let points = kept(&p);
            assert!(
                points <= MAX_UNITS_PER_OBJECT * KEPT_POINTS_PER_UNIT,
                "{points}"
            );
            assert!(p.len() > 10, "{}", p.len());
        }
    }

    fn patterned_square(pattern: Pattern, origin: Option<PatternOrigin>) -> Vec<Primitive> {
        let (mut r, _) = set();
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
            coords: square(10_000),
            tags: Vec::new(),
            pattern: origin,
        };
        primitives(&object, &r)
    }

    fn square_mm() -> Vec<Vec<Pt>> {
        vec![vec![
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (0.0, 0.0),
        ]]
    }

    fn lines(rotatable: bool) -> Pattern {
        Pattern::Lines {
            angle_rad: 0.0,
            rotatable,
            spacing: 1_000.0,
            offset: 0.0,
            color: 3,
            width: 100.0,
            clipping: Clipping::Clip,
        }
    }

    /// The clip rings and inner shape of every primitive, which must all be clipped.
    fn clipped(p: &[Primitive]) -> Vec<(&[Vec<Pt>], &Shape)> {
        p.iter()
            .map(|p| match &p.shape {
                Shape::Clipped { rings, inner } => (&**rings, &**inner),
                other => panic!("not clipped: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn line_pattern_fills_square() {
        let p = patterned_square(lines(false), None);
        // Canvas = square grown by half the line width: y = 0, 1, …, 10 all lie inside it.
        assert_eq!(p.len(), 11);
        assert!(p.iter().all(|p| p.priority == 3));
        let mut ys = Vec::new();
        for (rings, inner) in clipped(&p) {
            assert_eq!(rings, square_mm().as_slice());
            let Shape::Stroke {
                path, width, cap, ..
            } = inner
            else {
                panic!("{inner:?}")
            };
            assert!((width - 0.1).abs() < 1e-12 && *cap == Cap::Flat);
            assert!((path[0].1 - path[1].1).abs() < 1e-9, "horizontal: {path:?}");
            assert!(
                path[0].0 <= 0.0 && path[1].0 >= 10.0,
                "spans the square: {path:?}"
            );
            ys.push(path[0].1);
        }
        let expected: Vec<f64> = (0..=10).map(f64::from).collect();
        assert!(
            ys.iter().zip(&expected).all(|(a, b)| (a - b).abs() < 1e-9),
            "{ys:?}"
        );
    }

    #[test]
    fn pattern_follows_object_rotation() {
        let origin = PatternOrigin {
            rotation_rad: std::f64::consts::FRAC_PI_2,
            origin: MapCoord::new(0, 0),
        };
        let p = patterned_square(lines(true), Some(origin));
        assert_eq!(p.len(), 11);
        for (_, inner) in clipped(&p) {
            let Shape::Stroke { path, .. } = inner else {
                panic!()
            };
            assert!((path[0].0 - path[1].0).abs() < 1e-9, "vertical: {path:?}");
        }
        // Not rotatable: the object rotation is ignored.
        let p = patterned_square(lines(false), Some(origin));
        for (_, inner) in clipped(&p) {
            let Shape::Stroke { path, .. } = inner else {
                panic!()
            };
            assert!((path[0].1 - path[1].1).abs() < 1e-9, "horizontal: {path:?}");
        }
    }

    #[test]
    fn point_pattern_positions() {
        let dot = PointDef {
            rotatable: false,
            dot: Some((3, 100.0)),
            ring: None,
            elements: Vec::new(),
        };
        let pattern = Pattern::Points {
            angle_rad: 0.0,
            rotatable: false,
            spacing: 2_000.0,
            offset: 0.0,
            offset_along: 0.0,
            distance: 2_000.0,
            symbol: Box::new(dot),
            clipping: Clipping::Clip,
        };
        let p = patterned_square(pattern, None);
        // Canvas = square grown by the dot extent (0.1 mm): lines y = 0, 2, …, 10, points
        // x = 0, 2, …, 10 on each (reference §4.3).
        assert_eq!(p.len(), 36);
        let mut centres: Vec<(i64, i64)> = clipped(&p)
            .into_iter()
            .map(|(rings, inner)| {
                assert_eq!(rings, square_mm().as_slice());
                let Shape::Fill { rings, .. } = inner else {
                    panic!("{inner:?}")
                };
                let n = rings[0].len() as f64;
                let (sx, sy) = rings[0]
                    .iter()
                    .fold((0.0, 0.0), |(x, y), q| (x + q.0, y + q.1));
                ((sx / n).round() as i64, (sy / n).round() as i64)
            })
            .collect();
        centres.sort_unstable();
        let expected: Vec<(i64, i64)> = (0..=5)
            .flat_map(|x| (0..=5).map(move |y| (2 * x, 2 * y)))
            .collect();
        assert_eq!(centres, expected);
    }

    #[test]
    fn rotation_of_rotatable_points() {
        let (r, _) = set();
        let arrow = PointDef {
            rotatable: true,
            dot: None,
            ring: None,
            elements: vec![crate::symbols::Element {
                symbol: Symbol::Line(LineDef {
                    color: 2,
                    width: 100.0,
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
                coords: vec![MapCoord::new(0, 0), MapCoord::new(1_000, 0)],
            }],
        };
        // Rotation −π/2 (y down) turns local +x to paper up.
        let mut sink = Sink::new(MAX_UNITS_PER_OBJECT);
        let quarter = -std::f64::consts::FRAC_PI_2;
        point_symbol(&arrow, (10.0, 10.0), quarter, &r, &mut sink, 0);
        let p = sink.out;
        let Shape::Stroke { path, .. } = &p[0].shape else {
            panic!()
        };
        assert!(
            (path[1].0 - 10.0).abs() < 1e-9 && (path[1].1 - 9.0).abs() < 1e-9,
            "{path:?}"
        );
    }
}

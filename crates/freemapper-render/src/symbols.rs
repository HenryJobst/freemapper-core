//! Copied from freemapper 743ce4b (ADR 0014).
//! The OOM symbol definitions as far as the renderer needs them (`docs/render/oom-render-reference.md`).
//!
//! Lengths stay in OOM units (0.001 mm); colours are indices = priorities.

use std::collections::{HashMap, HashSet};

use freemapper_omap::{MAX_XML_DEPTH, MapCoord, OmapError, SymbolSet, too_deep};

/// A colour of the map: its priority (0 is on top), CMYK for print, RGB for a screen and its
/// opacity, all 0..1 as OOM stores them (`<color c m y k opacity><rgb r g b/>`).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderColour {
    pub priority: i32,
    /// The colour's name in the map file (`name`, empty when missing).
    pub name: String,
    pub cmyk: [f64; 4],
    /// OOM's RGB for the screen; without `<rgb>` derived from CMYK.
    pub rgb: [f64; 3],
    /// 1 is opaque (missing attribute: 1). The PDF backend does not apply it yet.
    pub opacity: f64,
}

/// One `<color>` element.
fn colour(c: &Node) -> RenderColour {
    let cmyk = [c.num("c"), c.num("m"), c.num("y"), c.num("k")];
    let rgb = match c.child("rgb") {
        Some(rgb) => [rgb.num("r"), rgb.num("g"), rgb.num("b")],
        None => {
            let [cyan, magenta, yellow, black] = cmyk;
            let channel = |v: f64| (1.0 - v) * (1.0 - black);
            [channel(cyan), channel(magenta), channel(yellow)]
        }
    };
    let opacity = c
        .attr("opacity")
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map_or(1.0, |v| v.clamp(0.0, 1.0));
    RenderColour {
        priority: c.num("priority") as i32,
        name: c.attr("name").unwrap_or_default().to_string(),
        cmyk,
        rgb,
        opacity,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    Flat,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    Bevel,
    Miter,
    Round,
}

/// Dash pattern of a line or border.
#[derive(Debug, Clone, PartialEq)]
pub struct Dash {
    /// Dash length; not above 0 on a main line: see [`Dash::is_empty`].
    pub dash: f64,
    pub gap: f64,
    pub group: u32,
    pub in_group_gap: f64,
    pub half_outer: bool,
}

impl Dash {
    /// `dashed="true"` without a dash length: OOM draws no main line, no mid symbols and no
    /// borders for such a line; dash, start and end symbols remain
    /// [LS `createSinglePathRenderables`].
    pub fn is_empty(&self) -> bool {
        self.dash.is_nan() || self.dash <= 0.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    pub color: i32,
    pub width: f64,
    pub shift: f64,
    pub dash: Option<Dash>,
}

/// Where the mid symbols of a dashed line go: OOM's `mid_symbol_placement`
/// (`LineSymbol::MidSymbolPlacement`, reference §2.5). Solid lines ignore it; 99 (`NoMidSymbols`)
/// is read as no [`Mid`] on a dashed line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // OOM's names (`CenterOfDash`, …)
pub enum MidPlacement {
    /// 0: centre of every full dash.
    CentreOfDash,
    /// 1: centre of every dash group.
    CentreOfDashGroup,
    /// 2: centre of every main gap (not the gaps inside a group).
    CentreOfGap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mid {
    pub symbol: Box<PointDef>,
    pub segment: f64,
    pub end: f64,
    pub per_spot: u32,
    pub distance: f64,
    pub at_least_one: bool,
    /// Used on dashed lines only.
    pub placement: MidPlacement,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineDef {
    /// −1: no main line.
    pub color: i32,
    pub width: f64,
    /// Stroke cap; a pointed cap (`cap_style` 3) is stroked flat (reference §2.1).
    pub cap: Cap,
    pub join: Join,
    /// `cap_style` 3: the stretches cut off by the start and end offsets are drawn as filled
    /// tapers (reference §2.7).
    pub pointed_caps: bool,
    /// Length cut off an open line at its start and end (0.001 mm, reference §2.7).
    pub start_offset: f64,
    pub end_offset: f64,
    pub dash: Option<Dash>,
    pub border: Option<Border>,
    pub mid: Option<Mid>,
    pub start: Option<Box<PointDef>>,
    pub end: Option<Box<PointDef>>,
    /// Drawn at every coordinate flagged as dash point (reference §2.7).
    pub dash_symbol: Option<DashSymbol>,
}

/// The symbol at the dash points of a line (`<dash_symbol>`, reference §2.7).
#[derive(Debug, Clone, PartialEq)]
pub struct DashSymbol {
    pub symbol: Box<PointDef>,
    /// `suppress_dash_symbol_at_ends`: none at the first and last coordinate.
    pub suppress_at_ends: bool,
    /// `scale_dash_symbol` (default true): elements scaled by 1 / cos(half the corner angle),
    /// at most 2.
    pub scale: bool,
}

/// One element of a point symbol: a symbol drawn on local coordinates (0.001 mm, y down).
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub symbol: Symbol,
    pub coords: Vec<MapCoord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PointDef {
    pub rotatable: bool,
    /// Filled dot: colour, radius.
    pub dot: Option<(i32, f64)>,
    /// Stroked ring: colour, centre radius, width.
    pub ring: Option<(i32, f64, f64)>,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextDef {
    /// Font family named by the symbol (`<font family>`); Arimo draws every family (ADR 0017).
    pub family: String,
    pub color: i32,
    /// Em size (0.001 mm).
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub line_spacing: f64,
    /// Extra space after each paragraph (0.001 mm): `paragraph_spacing`, plus the distance and
    /// width of a `<line_below>` as OOM counts them, although the line itself is not drawn
    /// (reference §6) [TO `prepareLineInfos`].
    pub paragraph_spacing: f64,
    /// `rotatable="true"`: the object's rotation applies (reference §6; version 9 files only).
    pub rotatable: bool,
    pub framing: Option<Framing>,
}

/// Text framing (`<framing>` of a text symbol, OOM `TextSymbol::FramingMode`, reference §6).
/// Mode 0 (`NoFraming`), a framing without colour and a line framing without width are read as
/// no framing.
#[derive(Debug, Clone, PartialEq)]
pub enum Framing {
    /// Mode 1: the glyph outlines stroked `2 · half_width` wide (0.001 mm).
    Line { color: i32, half_width: f64 },
    /// Mode 2: the text again in `color`, moved by `dx`, `dy` (0.001 mm, y down).
    Shadow { color: i32, dx: f64, dy: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// Another symbol of the set, by index (= id).
    Ref(u32),
    Inline(Box<Symbol>),
}

/// An area fill pattern (`<pattern>` of an area symbol, reference §4.2).
///
/// Lines run at `angle_rad` (counter-clockwise on paper from east), `spacing` apart, shifted by
/// `offset` along their normal. A point pattern places `symbol` every `distance` along those lines,
/// starting at `offset_along`. `rotatable`: the object's pattern rotation and origin apply.
/// `clipping`: how the pattern is cut at the area outline.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Lines {
        angle_rad: f64,
        rotatable: bool,
        spacing: f64,
        offset: f64,
        color: i32,
        width: f64,
        clipping: Clipping,
    },
    Points {
        angle_rad: f64,
        rotatable: bool,
        spacing: f64,
        offset: f64,
        offset_along: f64,
        distance: f64,
        symbol: Box<PointDef>,
        clipping: Clipping,
    },
}

/// How an area pattern meets the area outline: OOM's `no_clipping` attribute masked with 0x03
/// (`AreaSymbol::FillPattern::Option`, reference §4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clipping {
    /// 0, `Default`: clipped to the outline.
    Clip,
    /// 1, `NoClippingIfCompletelyInside`: unclipped points drawn only where completely inside.
    NoClippingIfCompletelyInside,
    /// 2, `NoClippingIfCenterInside`: unclipped points drawn where their centre is inside.
    NoClippingIfCentreInside,
    /// 3, `NoClippingIfPartiallyInside`: unclipped points drawn where partly inside.
    NoClippingIfPartlyInside,
}

/// Smallest `line_spacing` and `point_distance` (µm) a pattern may have: anything denser is
/// dropped when the symbols are read (OOM itself only skips values ≤ 0). 0.05 mm is far below
/// any printable pattern and keeps a broken or hostile file from asking for billions of lines
/// or points.
pub const MIN_PATTERN_STEP: f64 = 50.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Symbol {
    Line(LineDef),
    /// Fill colour (−1: none) and fill patterns, painted after the fill.
    Area {
        color: i32,
        patterns: Vec<Pattern>,
    },
    Point(PointDef),
    Text(TextDef),
    Combined(Vec<Part>),
}

/// Colours and symbols of a symbol set, ready for drawing.
#[derive(Debug, Clone)]
pub struct RenderSymbols {
    pub colours: Vec<RenderColour>,
    pub symbols: HashMap<u32, Symbol>,
    /// Ids of the symbols that lost an area pattern when read, because it broke a limit
    /// ([`MIN_PATTERN_STEP`], non-finite numbers); also combined symbols with such a part.
    pub dropped_patterns: HashSet<u32>,
}

/// Whether a `<symbol>` element holds an area pattern of a known type that [`pattern`] drops for
/// breaking a limit.
fn drops_a_pattern(node: &Node) -> bool {
    node.children.iter().any(|c| {
        (node.name == "area_symbol"
            && c.name == "pattern"
            && matches!(c.num("type") as i32, 1 | 2)
            && !pattern_within_limits(c))
            || drops_a_pattern(c)
    })
}

/// Whether a combined symbol refers to one of `ids`.
fn refers_to(symbol: &Symbol, ids: &HashSet<u32>) -> bool {
    match symbol {
        Symbol::Combined(parts) => parts.iter().any(|p| match p {
            Part::Ref(id) => ids.contains(id),
            Part::Inline(inner) => refers_to(inner, ids),
        }),
        _ => false,
    }
}

/// A parsed XML element.
#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    text: String,
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn num(&self, name: &str) -> f64 {
        self.attr(name).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    }

    fn color(&self, name: &str) -> i32 {
        self.attr(name).and_then(|v| v.parse().ok()).unwrap_or(-1)
    }

    fn flag(&self, name: &str) -> bool {
        self.attr(name) == Some("true")
    }

    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
}

/// The element tree of an XML fragment (one synthetic root holding the top-level elements). A
/// fragment nested deeper than [`MAX_XML_DEPTH`] is an error, so the tree (and dropping it) stays
/// shallow.
fn parse(xml: &str) -> Result<Node, OmapError> {
    use quick_xml::events::{BytesStart, Event};
    fn node(e: &BytesStart<'_>) -> Node {
        Node {
            name: e.name().as_ref().to_string(),
            attrs: e
                .attributes()
                .flatten()
                .map(|a| {
                    let key = a.key.as_ref().to_string();
                    let value = freemapper_omap::symbol_set::attribute(e, &key)
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    (key, value)
                })
                .collect(),
            ..Node::default()
        }
    }
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack = vec![Node::default()];
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(e) => {
                if stack.len() > MAX_XML_DEPTH {
                    return Err(too_deep());
                }
                stack.push(node(&e));
            }
            Event::Empty(e) => {
                let n = node(&e);
                stack.last_mut().expect("root").children.push(n);
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let n = stack.pop().expect("checked");
                    stack.last_mut().expect("root").children.push(n);
                }
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(t.as_ref());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    while stack.len() > 1 {
        let n = stack.pop().expect("checked");
        stack.last_mut().expect("root").children.push(n);
    }
    Ok(stack.pop().expect("root"))
}

/// `x y [flags];` coordinates of an OOM `<coords>` element.
fn coords(node: &Node) -> Vec<MapCoord> {
    node.text
        .split(';')
        .filter_map(|c| {
            let v: Vec<i32> = c
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            (v.len() >= 2).then(|| MapCoord {
                x: v[0],
                y: v[1],
                flags: v.get(2).copied().unwrap_or(0) as u8,
            })
        })
        .collect()
}

/// The dash pattern of a `dashed="true"` line, also an empty one ([`Dash::is_empty`]).
fn dash(node: &Node) -> Option<Dash> {
    node.flag("dashed").then(|| Dash {
        dash: node.num("dash_length"),
        gap: node.num("break_length"),
        group: (node.num("dashes_in_group") as u32).max(1),
        in_group_gap: node.num("in_group_break_length"),
        half_outer: node.flag("half_outer_dashes"),
    })
}

/// The point symbol nested in `<mid_symbol>`, `<start_symbol>` … (`<symbol><point_symbol>`).
fn nested_point(node: Option<&Node>, version: u32) -> Option<Box<PointDef>> {
    let point = node?.child("symbol")?.child("point_symbol")?;
    Some(Box::new(point_def(point, version)))
}

fn line_def(node: &Node, version: u32) -> LineDef {
    let cap = match node.num("cap_style") as i32 {
        1 => Cap::Round,
        2 => Cap::Square,
        _ => Cap::Flat,
    };
    let join = match node.num("join_style") as i32 {
        0 => Join::Bevel,
        2 => Join::Round,
        _ => Join::Miter,
    };
    let border = node
        .child("borders")
        .and_then(|b| b.child("border"))
        .map(|b| {
            let border = Border {
                color: b.color("color"),
                width: b.num("width"),
                shift: b.num("shift"),
                dash: (b.flag("dashed")
                    && b.num("dash_length") > 0.0
                    && b.num("break_length") > 0.0)
                    .then(|| Dash {
                        dash: b.num("dash_length"),
                        gap: b.num("break_length"),
                        group: 1,
                        in_group_gap: 0.0,
                        half_outer: false,
                    }),
            };
            (border, b)
        })
        // A dashed border without dash length is invisible [LS `LineSymbolBorder::isVisible`].
        .filter(|(b, node)| {
            b.width > 0.0
                && b.color >= 0
                && !(node.flag("dashed") && node.num("dash_length") == 0.0)
        })
        .map(|(b, _)| b);
    let dash = dash(node);
    // Solid lines need a segment length [LS `createSinglePathRenderables`]; dashed lines a
    // placement other than 99 and at least one symbol per spot [LS `createDashGroups`].
    let per_spot = node.num("mid_symbols_per_spot");
    let placement = match node.num("mid_symbol_placement") as i32 {
        0 => Some(MidPlacement::CentreOfDash),
        1 => Some(MidPlacement::CentreOfDashGroup),
        2 => Some(MidPlacement::CentreOfGap),
        _ => None,
    };
    let mid = nested_point(node.child("mid_symbol"), version)
        .filter(|s| !s.is_empty())
        .and_then(|symbol| {
            let placement = match dash {
                None => (node.num("segment_length") > 0.0).then_some(MidPlacement::CentreOfDash),
                Some(_) => placement.filter(|_| per_spot >= 1.0),
            }?;
            Some(Mid {
                symbol,
                segment: node.num("segment_length"),
                end: node.num("end_length"),
                per_spot: (per_spot as u32).max(1),
                distance: node.num("mid_symbol_distance"),
                at_least_one: node.flag("show_at_least_one_symbol"),
                placement,
            })
        });
    // Offsets since version 8; before, a pointed cap had one `pointed_cap_length` for both ends
    // [LS `loadImpl`].
    let pointed_caps = node.num("cap_style") as i32 == 3;
    let (start_offset, end_offset) =
        if node.attr("start_offset").is_some() || node.attr("end_offset").is_some() {
            (node.num("start_offset"), node.num("end_offset"))
        } else if pointed_caps {
            let length = node.num("pointed_cap_length");
            (length, length)
        } else {
            (0.0, 0.0)
        };
    let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
    let dash_symbol = nested_point(node.child("dash_symbol"), version)
        .filter(|s| !s.is_empty())
        .map(|symbol| DashSymbol {
            symbol,
            suppress_at_ends: node.flag("suppress_dash_symbol_at_ends"),
            scale: node.attr("scale_dash_symbol") != Some("false"),
        });
    LineDef {
        color: node.color("color"),
        width: node.num("line_width"),
        cap,
        join,
        pointed_caps,
        start_offset: finite(start_offset),
        end_offset: finite(end_offset),
        dash,
        border,
        mid,
        start: nested_point(node.child("start_symbol"), version),
        end: nested_point(node.child("end_symbol"), version),
        dash_symbol,
    }
}

impl PointDef {
    /// No elements, dot or ring [PS `isEmpty`]: OOM drops such symbols from lines.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.dot.is_none() && self.ring.is_none()
    }
}

fn point_def(node: &Node, version: u32) -> PointDef {
    let (inner, radius) = (node.color("inner_color"), node.num("inner_radius"));
    let (outer, width) = (node.color("outer_color"), node.num("outer_width"));
    PointDef {
        rotatable: node.flag("rotatable"),
        dot: (inner >= 0 && radius > 0.0).then_some((inner, radius)),
        ring: (outer >= 0 && width > 0.0).then_some((outer, radius + width / 2.0, width)),
        elements: node
            .children
            .iter()
            .filter(|c| c.name == "element")
            .filter_map(|e| {
                Some(Element {
                    symbol: symbol(e.child("symbol")?, version)?,
                    coords: e.child("object")?.child("coords").map(coords)?,
                })
            })
            .collect(),
    }
}

/// One `<pattern>` of an area symbol [AS `FillPattern::load`]; `None` for an unknown type, a
/// point pattern without a point symbol (OOM draws nothing for it), a non-finite number or a
/// spacing or point distance below [`MIN_PATTERN_STEP`].
fn pattern(node: &Node, version: u32) -> Option<Pattern> {
    let angle_rad = node.num("angle");
    let rotatable = node.flag("rotatable");
    let spacing = node.num("line_spacing");
    let offset = node.num("line_offset");
    let clipping = match (node.num("no_clipping") as i32) & 0x03 {
        1 => Clipping::NoClippingIfCompletelyInside,
        2 => Clipping::NoClippingIfCentreInside,
        3 => Clipping::NoClippingIfPartlyInside,
        _ => Clipping::Clip,
    };
    if !pattern_within_limits(node) {
        return None;
    }
    match node.num("type") as i32 {
        1 => {
            let width = node.num("line_width");
            Some(Pattern::Lines {
                angle_rad,
                rotatable,
                spacing,
                offset,
                color: node.color("color"),
                width,
                clipping,
            })
        }
        2 => {
            let offset_along = node.num("offset_along_line");
            let distance = node.num("point_distance");
            let symbol = Box::new(point_def(
                node.child("symbol")?.child("point_symbol")?,
                version,
            ));
            Some(Pattern::Points {
                angle_rad,
                rotatable,
                spacing,
                offset,
                offset_along,
                distance,
                symbol,
                clipping,
            })
        }
        _ => None,
    }
}

/// Whether a `<pattern>`'s numbers are finite and its spacing (and, for points, distance) at
/// least [`MIN_PATTERN_STEP`].
fn pattern_within_limits(node: &Node) -> bool {
    let finite = ["angle", "line_offset", "line_spacing"]
        .iter()
        .all(|a| node.num(a).is_finite());
    let spacing = node.num("line_spacing") >= MIN_PATTERN_STEP;
    match node.num("type") as i32 {
        1 => finite && spacing && node.num("line_width").is_finite(),
        2 => {
            let distance = node.num("point_distance");
            finite
                && spacing
                && node.num("offset_along_line").is_finite()
                && distance.is_finite()
                && distance >= MIN_PATTERN_STEP
        }
        _ => true,
    }
}

/// A `<framing>` element [TS `loadImpl`, `createRenderables`].
fn framing(node: &Node) -> Option<Framing> {
    let color = node.color("color");
    if color < 0 {
        return None;
    }
    let finite = |v: f64| v.is_finite().then_some(v);
    match node.num("mode") as i32 {
        1 => finite(node.num("line_half_width"))
            .filter(|&w| w > 0.0)
            .map(|half_width| Framing::Line { color, half_width }),
        2 => Some(Framing::Shadow {
            color,
            dx: finite(node.num("shadow_x_offset"))?,
            dy: finite(node.num("shadow_y_offset"))?,
        }),
        _ => None,
    }
}

/// The space after a paragraph (0.001 mm) [TS `loadImpl`, TO `prepareLineInfos`]; a non-finite
/// value counts as 0.
fn paragraph_spacing(text: Option<&Node>, line_below: Option<&Node>) -> f64 {
    let spacing = text.map_or(0.0, |t| t.num("paragraph_spacing"));
    let below = line_below.map_or(0.0, |l| l.num("distance") + l.num("width"));
    let total = spacing + below;
    if total.is_finite() { total } else { 0.0 }
}

/// One `<symbol>`; `version` is the file format version (defaults that changed with it).
fn symbol(node: &Node, version: u32) -> Option<Symbol> {
    match node.num("type") as u32 {
        1 => node
            .child("point_symbol")
            .map(|p| Symbol::Point(point_def(p, version))),
        2 => node
            .child("line_symbol")
            .map(|l| Symbol::Line(line_def(l, version))),
        4 => node.child("area_symbol").map(|a| Symbol::Area {
            color: a.color("inner_color"),
            patterns: a
                .children
                .iter()
                .filter(|c| c.name == "pattern")
                .filter_map(|p| pattern(p, version))
                .collect(),
        }),
        8 => {
            let t = node.child("text_symbol")?;
            let font = t.child("font");
            let text = t.child("text");
            Some(Symbol::Text(TextDef {
                family: font
                    .and_then(|f| f.attr("family"))
                    .unwrap_or("Arial")
                    .to_string(),
                color: text.map_or(-1, |t| t.color("color")),
                size: font.map_or(4000.0, |f| f.num("size")),
                bold: font.is_some_and(|f| f.flag("bold")),
                italic: font.is_some_and(|f| f.flag("italic")),
                line_spacing: text
                    .and_then(|t| t.attr("line_spacing"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1.0),
                paragraph_spacing: paragraph_spacing(text, t.child("line_below")),
                // Without the attribute, rotatable before version 9 [TS `loadImpl`].
                rotatable: match t.attr("rotatable") {
                    Some(v) => v == "true",
                    None => version < 9,
                },
                framing: t.child("framing").and_then(framing),
            }))
        }
        16 => {
            let c = node.child("combined_symbol")?;
            Some(Symbol::Combined(
                c.children
                    .iter()
                    .filter(|p| p.name == "part")
                    .filter_map(|p| {
                        if p.flag("private") {
                            p.child("symbol")
                                .and_then(|s| symbol(s, version))
                                .map(|s| Part::Inline(Box::new(s)))
                        } else {
                            p.attr("symbol")?.parse().ok().map(Part::Ref)
                        }
                    })
                    .collect(),
            ))
        }
        _ => None,
    }
}

impl RenderSymbols {
    pub fn from_set(set: &SymbolSet) -> Result<Self, OmapError> {
        let colours = parse(&set.colors_xml)?
            .children
            .iter()
            .flat_map(|c| c.children.iter().chain(std::iter::once(c)))
            .filter(|c| c.name == "color")
            .map(colour)
            .collect();
        let root = parse(&set.symbols_xml)?;
        let mut symbols = HashMap::new();
        let mut dropped_patterns = HashSet::new();
        let top_level = root
            .children
            .iter()
            .flat_map(|s| s.children.iter().chain(std::iter::once(s)))
            .filter(|s| s.name == "symbol");
        for node in top_level {
            let Some(id) = node.attr("id").and_then(|v| v.parse::<u32>().ok()) else {
                continue;
            };
            let Some(parsed) = symbol(node, set.version) else {
                continue;
            };
            if drops_a_pattern(node) {
                dropped_patterns.insert(id);
            }
            symbols.insert(id, parsed);
        }
        // A combined symbol whose parts lost a pattern lost it too (as deep as drawing goes).
        for _ in 0..=super::draw::MAX_DEPTH {
            let more: Vec<u32> = symbols
                .iter()
                .filter(|(id, s)| {
                    !dropped_patterns.contains(*id) && refers_to(s, &dropped_patterns)
                })
                .map(|(id, _)| *id)
                .collect();
            if more.is_empty() {
                break;
            }
            dropped_patterns.extend(more);
        }
        Ok(Self {
            colours,
            symbols,
            dropped_patterns,
        })
    }

    pub fn colour(&self, priority: i32) -> Option<&RenderColour> {
        self.colours.iter().find(|c| c.priority == priority)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols() -> (RenderSymbols, SymbolSet) {
        let set = SymbolSet::issprom_2019().with_sheet_symbols();
        (RenderSymbols::from_set(&set).unwrap(), set)
    }

    fn by_code<'a>(s: &'a (RenderSymbols, SymbolSet), code: &str) -> &'a Symbol {
        &s.0.symbols[&s.1.id_for_code(code).unwrap()]
    }

    fn line<'a>(s: &'a (RenderSymbols, SymbolSet), code: &str) -> &'a LineDef {
        match by_code(s, code) {
            Symbol::Line(l) => l,
            other => panic!("{code}: {other:?}"),
        }
    }

    #[test]
    fn colours_have_priority_and_cmyk() {
        let (r, _) = symbols();
        assert_eq!(r.colours.len(), 32);
        assert_eq!(r.colour(2).unwrap().cmyk, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(r.colour(8).unwrap().cmyk, [0.0, 0.56, 1.0, 0.18]);
        // Purple for the course overprint: OOM's RGB and full opacity.
        assert_eq!(r.colour(0).unwrap().rgb, [0.65, 0.15, 1.0]);
        assert_eq!(r.colour(0).unwrap().opacity, 1.0);
    }

    #[test]
    fn deep_symbol_xml_is_an_error() {
        let deep = format!("{}{}", "<a>".repeat(100_000), "</a>".repeat(100_000));
        let err = parse(&deep).unwrap_err();
        assert!(err.to_string().contains("nesting too deep"), "{err}");
        let fine = format!("{}{}", "<a>".repeat(200), "</a>".repeat(200));
        assert!(parse(&fine).is_ok());
    }

    #[test]
    fn colour_opacity_and_rgb_fallback() {
        let half = parse(
            r#"<color priority="4" c="0" m="1" y="0" k="0" opacity="0.5"><rgb r="1" g="0" b="1"/></color>"#,
        ).unwrap();
        let c = colour(&half.children[0]);
        assert_eq!((c.priority, c.rgb, c.opacity), (4, [1.0, 0.0, 1.0], 0.5));
        let bare = parse(r#"<color priority="3" c="0.2" m="0" y="1" k="0.5"/>"#).unwrap();
        let c = colour(&bare.children[0]);
        assert_eq!((c.rgb, c.opacity), ([0.4, 0.5, 0.0], 1.0));
    }

    #[test]
    fn line_with_dashed_borders() {
        let s = symbols();
        let l = line(&s, "505.1");
        assert_eq!(
            (l.color, l.width, l.cap, l.join),
            (14, 350.0, Cap::Flat, Join::Miter)
        );
        assert!(l.dash.is_none());
        let b = l.border.as_ref().unwrap();
        assert_eq!((b.color, b.width, b.shift), (15, 100.0, 50.0));
        let d = b.dash.as_ref().unwrap();
        assert_eq!((d.dash, d.gap), (2000.0, 250.0));
    }

    #[test]
    fn dashed_main_line() {
        let s = symbols();
        let d = line(&s, "506").dash.clone().unwrap();
        assert_eq!(
            (d.dash, d.gap, d.group, d.half_outer),
            (1500.0, 370.0, 1, false)
        );
        assert_eq!(line(&s, "101").cap, Cap::Round);
        assert_eq!(line(&s, "101").join, Join::Round);
    }

    #[test]
    fn mid_symbol_of_fence() {
        let s = symbols();
        let mid = line(&s, "518").mid.clone().unwrap();
        assert_eq!((mid.segment, mid.end, mid.per_spot), (3750.0, 2325.0, 1));
        assert!(!mid.at_least_one);
        assert!(mid.symbol.rotatable);
        assert!(mid.symbol.dot.is_none(), "inner colour −1");
        assert_eq!(mid.symbol.elements.len(), 2);
        assert_eq!(
            mid.symbol.elements[0].coords,
            [MapCoord::new(450, 0), MapCoord::new(825, 650)]
        );
        assert!(matches!(mid.symbol.elements[0].symbol, Symbol::Line(ref l) if l.width == 140.0));
        assert!(line(&s, "513.2").mid.as_ref().unwrap().at_least_one);
    }

    #[test]
    fn start_and_end_symbols() {
        let set = SymbolSet::school_2019();
        let s = (RenderSymbols::from_set(&set).unwrap(), set);
        let l = line(&s, "543");
        assert!(l.start.is_some() && l.end.is_some());
        assert!(l.start.as_ref().unwrap().rotatable);
        assert!(l.mid.is_none());
    }

    #[test]
    fn point_with_ring_and_elements() {
        let s = symbols();
        let Symbol::Point(bush) = by_code(&s, "418") else {
            panic!()
        };
        assert_eq!(bush.dot, Some((21, 100.0)));
        assert_eq!(bush.ring, Some((3, 250.0, 300.0)));
        assert!(!bush.rotatable);
        let Symbol::Point(minimum) = by_code(&s, "521.1") else {
            panic!()
        };
        assert!(minimum.rotatable);
        assert_eq!(minimum.elements.len(), 2);
    }

    #[test]
    fn combined_parts_resolve() {
        let s = symbols();
        let Symbol::Combined(building) = by_code(&s, "521") else {
            panic!()
        };
        assert_eq!(building, &[Part::Ref(143), Part::Ref(144)]);
        let Symbol::Combined(paved) = by_code(&s, "501") else {
            panic!()
        };
        assert_eq!(paved[0], Part::Ref(105));
        assert!(
            matches!(&paved[1], Part::Inline(p) if matches!(**p, Symbol::Line(ref l) if l.color == 15))
        );
        assert!(matches!(s.0.symbols[&143], Symbol::Area { color: 9, .. }));
    }

    #[test]
    fn parses_line_and_point_patterns() {
        let xml = r#"<symbol type="4" id="7"><area_symbol inner_color="-1" min_area="0" patterns="2">
            <pattern type="1" angle="0.785" rotatable="true" line_spacing="1000" line_offset="0" offset_along_line="0" color="3" line_width="100"/>
            <pattern type="2" angle="0" no_clipping="3" line_spacing="2000" line_offset="500" offset_along_line="250" point_distance="1500">
                <symbol type="1" code=""><point_symbol inner_radius="100" inner_color="5" outer_width="0" outer_color="-1" elements="0"/></symbol>
            </pattern>
        </area_symbol></symbol>"#;
        let Some(Symbol::Area { color, patterns }) = symbol(&parse(xml).unwrap().children[0], 9)
        else {
            panic!()
        };
        assert_eq!(color, -1);
        assert_eq!(
            patterns[0],
            Pattern::Lines {
                angle_rad: 0.785,
                rotatable: true,
                spacing: 1000.0,
                offset: 0.0,
                color: 3,
                width: 100.0,
                clipping: Clipping::Clip,
            }
        );
        let Pattern::Points {
            angle_rad,
            rotatable,
            spacing,
            offset,
            offset_along,
            distance,
            symbol,
            clipping,
        } = &patterns[1]
        else {
            panic!("{:?}", patterns[1])
        };
        assert_eq!(
            (
                *angle_rad,
                *rotatable,
                *spacing,
                *offset,
                *offset_along,
                *distance
            ),
            (0.0, false, 2000.0, 500.0, 250.0, 1500.0)
        );
        assert_eq!(symbol.dot, Some((5, 100.0)));
        assert_eq!(*clipping, Clipping::NoClippingIfPartlyInside);
    }

    /// The patterns of an area symbol holding `pattern`.
    fn patterns_of(pattern: &str) -> Vec<Pattern> {
        let xml = format!(
            r#"<symbol type="4" id="1"><area_symbol inner_color="-1" patterns="1">{pattern}</area_symbol></symbol>"#
        );
        match symbol(&parse(&xml).unwrap().children[0], 9) {
            Some(Symbol::Area { patterns, .. }) => patterns,
            other => panic!("{other:?}"),
        }
    }

    const DOT: &str = r#"<symbol type="1" code=""><point_symbol inner_radius="100" inner_color="5" outer_width="0" outer_color="-1"/></symbol>"#;

    #[test]
    fn nan_angle_pattern_dropped() {
        for angle in ["NaN", "inf", "-inf"] {
            let line = format!(
                r#"<pattern type="1" angle="{angle}" line_spacing="1000" line_offset="0" color="3" line_width="100"/>"#
            );
            assert!(patterns_of(&line).is_empty(), "{angle}");
            let points = format!(
                r#"<pattern type="2" angle="{angle}" line_spacing="1000" line_offset="0" offset_along_line="0" point_distance="1000">{DOT}</pattern>"#
            );
            assert!(patterns_of(&points).is_empty(), "{angle}");
        }
        // Other non-finite numbers too.
        let width = r#"<pattern type="1" angle="0" line_spacing="1000" line_offset="0" color="3" line_width="NaN"/>"#;
        assert!(patterns_of(width).is_empty());
        let along = format!(
            r#"<pattern type="2" angle="0" line_spacing="1000" line_offset="0" offset_along_line="inf" point_distance="1000">{DOT}</pattern>"#
        );
        assert!(patterns_of(&along).is_empty());
    }

    #[test]
    fn tiny_spacing_pattern_dropped() {
        let line = |spacing: &str| {
            patterns_of(&format!(
                r#"<pattern type="1" angle="0" line_spacing="{spacing}" line_offset="0" color="3" line_width="100"/>"#
            ))
            .len()
        };
        assert_eq!(
            (line("49"), line("50"), line("0"), line("-1000")),
            (0, 1, 0, 0)
        );
        let points = |spacing: &str, distance: &str| {
            patterns_of(&format!(
                r#"<pattern type="2" angle="0" line_spacing="{spacing}" line_offset="0" offset_along_line="0" point_distance="{distance}">{DOT}</pattern>"#
            ))
            .len()
        };
        assert_eq!(
            (
                points("50", "50"),
                points("49", "1000"),
                points("1000", "49")
            ),
            (1, 0, 0)
        );
    }

    #[test]
    fn forest_sample_patterns_parse() {
        let doc = freemapper_omap::read::read_map_file(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/maps/forest-sample.omap"
        )))
        .unwrap();
        let r = RenderSymbols::from_set(&doc.symbol_set).unwrap();
        let patterned: Vec<&Vec<Pattern>> = r
            .symbols
            .values()
            .filter_map(|s| match s {
                Symbol::Area { patterns, .. } if !patterns.is_empty() => Some(patterns),
                _ => None,
            })
            .collect();
        assert_eq!(patterned.len(), 20);
        let all: Vec<&Pattern> = patterned.into_iter().flatten().collect();
        let points = all
            .iter()
            .filter(|p| matches!(p, Pattern::Points { .. }))
            .count();
        assert_eq!((all.len(), points), (23, 9));
    }

    fn line_of(attrs: &str, children: &str) -> LineDef {
        let xml = format!(
            r#"<symbol type="2" id="1"><line_symbol color="2" line_width="300" {attrs}>{children}</line_symbol></symbol>"#
        );
        match symbol(&parse(&xml).unwrap().children[0], 9) {
            Some(Symbol::Line(l)) => l,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pointed_caps_and_offsets() {
        let l = line_of(r#"cap_style="3" start_offset="1121" end_offset="900""#, "");
        assert!(l.pointed_caps);
        assert_eq!(l.cap, Cap::Flat, "stroked flat");
        assert_eq!((l.start_offset, l.end_offset), (1121.0, 900.0));
        // Before version 8: one length for both ends of a pointed line [LS `loadImpl`].
        let l = line_of(r#"cap_style="3" pointed_cap_length="500""#, "");
        assert_eq!((l.start_offset, l.end_offset), (500.0, 500.0));
        let l = line_of(r#"cap_style="1" pointed_cap_length="500""#, "");
        assert!(!l.pointed_caps);
        assert_eq!((l.start_offset, l.end_offset), (0.0, 0.0));
    }

    #[test]
    fn dash_symbol_and_mid_placement() {
        let l = line_of(
            r#"suppress_dash_symbol_at_ends="true""#,
            &format!("<dash_symbol>{DOT}</dash_symbol>"),
        );
        let d = l.dash_symbol.unwrap();
        assert!(d.suppress_at_ends && d.scale);
        assert_eq!(d.symbol.dot, Some((5, 100.0)));
        let l = line_of(
            r#"scale_dash_symbol="false""#,
            &format!("<dash_symbol>{DOT}</dash_symbol>"),
        );
        assert!(!l.dash_symbol.unwrap().scale);
        // An empty dash symbol is dropped.
        let empty = r#"<symbol type="1" code=""><point_symbol inner_radius="0" inner_color="-1" outer_width="0" outer_color="-1"/></symbol>"#;
        assert!(
            line_of("", &format!("<dash_symbol>{empty}</dash_symbol>"))
                .dash_symbol
                .is_none()
        );
        // Mid symbols on a dashed line: placement 0..2; 99 and zero per spot mean none.
        let dashed = |placement: &str, per_spot: &str| {
            line_of(
                &format!(r#"dashed="true" dash_length="2000" break_length="500" segment_length="0" mid_symbol_placement="{placement}" mid_symbols_per_spot="{per_spot}""#),
                &format!("<mid_symbol>{DOT}</mid_symbol>"),
            )
            .mid
            .map(|m| m.placement)
        };
        assert_eq!(dashed("0", "1"), Some(MidPlacement::CentreOfDash));
        assert_eq!(dashed("1", "1"), Some(MidPlacement::CentreOfDashGroup));
        assert_eq!(dashed("2", "2"), Some(MidPlacement::CentreOfGap));
        assert_eq!(dashed("99", "1"), None);
        assert_eq!(dashed("0", "0"), None);
    }

    #[test]
    fn dashed_without_dash_length() {
        let l = line_of(r#"dashed="true" dash_length="0" break_length="500""#, "");
        assert!(l.dash.unwrap().is_empty());
        assert!(
            line_of(r#"dashed="false" dash_length="0""#, "")
                .dash
                .is_none()
        );
        // Borders: dashed with dash length 0 is invisible; without a break it is solid.
        let border = |attrs: &str| {
            line_of(
                "",
                &format!(r#"<borders><border color="3" width="100" shift="0" {attrs}/></borders>"#),
            )
            .border
        };
        assert!(border(r#"dashed="true" dash_length="0" break_length="200""#).is_none());
        let solid = border(r#"dashed="true" dash_length="1000" break_length="0""#).unwrap();
        assert!(solid.dash.is_none());
        assert!(border(r#"dashed="false" dash_length="0""#).is_some());
    }

    fn text_of(children: &str, rotatable: &str) -> TextDef {
        let xml = format!(
            r#"<symbol type="8" id="1"><text_symbol {rotatable}><font family="Arial" size="3000"/><text color="2" line_spacing="1"/>{children}</text_symbol></symbol>"#
        );
        match symbol(&parse(&xml).unwrap().children[0], 9) {
            Some(Symbol::Text(t)) => t,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn paragraph_spacing_includes_the_line_below() {
        let xml = |children: &str| {
            format!(
                r#"<symbol type="8" id="1"><text_symbol><font family="Arial" size="3000"/><text color="2" line_spacing="1" paragraph_spacing="700"/>{children}</text_symbol></symbol>"#
            )
        };
        let spacing = |xml: &str| match symbol(&parse(xml).unwrap().children[0], 9) {
            Some(Symbol::Text(t)) => t.paragraph_spacing,
            other => panic!("{other:?}"),
        };
        assert_eq!(spacing(&xml("")), 700.0);
        assert_eq!(
            spacing(&xml(
                r#"<line_below color="2" width="100" distance="200"/>"#
            )),
            1000.0
        );
        assert_eq!(text_of("", "").paragraph_spacing, 0.0);
    }

    #[test]
    fn text_symbol_keeps_its_font_family() {
        assert_eq!(text_of("", "").family, "Arial");
    }

    #[test]
    fn text_framing_and_rotation() {
        let framed = |mode: &str, color: &str| {
            text_of(
                &format!(r#"<framing color="{color}" mode="{mode}" line_half_width="150" shadow_x_offset="200" shadow_y_offset="-100"/>"#),
                "",
            )
            .framing
        };
        assert_eq!(
            framed("1", "5"),
            Some(Framing::Line {
                color: 5,
                half_width: 150.0
            })
        );
        assert_eq!(
            framed("2", "5"),
            Some(Framing::Shadow {
                color: 5,
                dx: 200.0,
                dy: -100.0
            })
        );
        assert_eq!(framed("0", "5"), None);
        assert_eq!(framed("1", "-1"), None);
        assert!(text_of("", r#"rotatable="true""#).rotatable);
        assert!(!text_of("", "").rotatable, "version 9: not rotatable");
    }

    #[test]
    fn text_rotatable_by_default_before_version_9() {
        // The same text symbol without `rotatable` in a version 8 and a version 9 file.
        let text = |version: u32| {
            let map = format!(
                r#"<map version="{version}"><georeferencing scale="10000"/><colors count="1"><color priority="0" c="0" m="0" y="0" k="1"/></colors><barrier><symbols count="1" id="x"><symbol type="8" id="0" code="1"><text_symbol><font family="Arial" size="3000"/><text color="0"/></text_symbol></symbol></symbols></barrier></map>"#
            );
            let set = SymbolSet::from_omap_xml(&map).unwrap();
            match &RenderSymbols::from_set(&set).unwrap().symbols[&0] {
                Symbol::Text(t) => t.rotatable,
                other => panic!("{other:?}"),
            }
        };
        assert!(text(8));
        assert!(!text(9));
        assert!(!text_of("", r#"rotatable="false""#).rotatable);
    }

    #[test]
    fn forest_sample_line_features_parse() {
        let doc = freemapper_omap::read::read_map_file(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/maps/forest-sample.omap"
        )))
        .unwrap();
        let r = RenderSymbols::from_set(&doc.symbol_set).unwrap();
        let lines: Vec<&LineDef> = r
            .symbols
            .values()
            .filter_map(|s| match s {
                Symbol::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        let pointed = lines.iter().filter(|l| l.pointed_caps).count();
        let offset = lines.iter().filter(|l| l.start_offset > 0.0).count();
        let dash_symbols = lines.iter().filter(|l| l.dash_symbol.is_some()).count();
        let mid_on_dashes = lines
            .iter()
            .filter(|l| l.dash.is_some() && l.mid.is_some())
            .count();
        assert_eq!((pointed, offset, dash_symbols, mid_on_dashes), (1, 1, 3, 3));
        let rotatable = r
            .symbols
            .values()
            .filter(|s| matches!(s, Symbol::Text(t) if t.rotatable))
            .count();
        assert_eq!(rotatable, 2);
    }

    #[test]
    fn text_symbol() {
        let s = symbols();
        let Symbol::Text(title) = by_code(&s, "991") else {
            panic!()
        };
        assert_eq!((title.color, title.size, title.bold), (2, 5000.0, true));
    }
}

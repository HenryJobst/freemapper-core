//! What Purple Pen needs to load a `.ppen`, as a strict check of the file text, and the writer's output held
//! against it.
//!
//! Purple Pen 4.0 refused a file of the 0.4.0 writer: `<options print-scale="4000" climb="5" />` has no
//! `description-kind`, which `EventDBUtil.ReadDescriptionKindAttribute` reads without a default. The crate's
//! own reader has a default there, so no round trip could see it. [`RULES`] is the audit of `EventDB.Load` at
//! petergolde/PurplePen `201417f` (`EventDB.cs`, `ObjectStore.cs`, `XmlInput.cs`, `SymbolDB.cs`,
//! `SpecialColor.cs`, `CircleGap.cs` in `src/PurplePenCore`): per element, by parent and name, the attributes
//! it reads and how, the children it needs and the values it accepts. [`check`] applies it to a file.
//!
//! How Purple Pen reads (`XmlInput.cs`):
//! - `GetAttributeString/Int/Float/Bool(name)` without a default throws "Missing attribute" when the attribute
//!   is absent or empty. With a default, an empty value means the default.
//! - Values go through `XmlConvert`: an int is an `Int32`, a float an XML Schema float (`INF`, `-INF`, `NaN` or a
//!   decimal number), a bool `true`, `false`, `1` or `0`. A value it cannot parse throws, required or not.
//! - `FindSubElement` skips unknown children, so the loop itself needs no child. The children a special object
//!   needs (`location`, `font` for text, `image-data` for an image) are checked after the loop.
//! - The root is `course-scribe-event`. `ObjectStore.Load` reads the stores in the order event, control, course,
//!   course-control, leg, special-object, each while the element name matches, and refuses a repeated id. The
//!   first other element ends the load without an error, so everything after an out-of-order or unknown
//!   top-level element is lost.
//!
//! In the table, `req` attributes are read without a default. `Enum` lists the values outside of which Purple
//! Pen throws; `Known` lists those outside of which it silently takes a default. The check refuses both, since a
//! writer must not rely on the fallback. Not covered: `EventDB.Validate` (run only in Purple Pen's debug
//! builds) and the base64 of `image-data`.

use std::collections::{BTreeMap, BTreeSet};

use freemapper_course::model::{CourseFilter, RawPoint, Special, Stop};
use freemapper_course::{
    Control, ControlKind, Course, CourseEvent, CourseKind, CourseSource, Deletions,
    DescriptionStandard, Leg, LineEnding, MapStandard, PatchOptions, PpenMap, RawEvent,
    ScratchOptions, SourceFormat, read_ppen, write_ppen, write_ppen_patch,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

// ---------------------------------------------------------------------------------------------------------
// The audit, as data.
// ---------------------------------------------------------------------------------------------------------

/// How Purple Pen parses an attribute value.
#[derive(Debug, Clone, Copy)]
enum Ty {
    /// `XmlConvert.ToInt32`.
    Int,
    /// `XmlConvert.ToSingle`.
    Float,
    /// `XmlConvert.ToBoolean`.
    Bool,
    /// Any text.
    Str,
    /// `Convert.ToUInt32(value, 2)`: 1 to 32 binary digits.
    Bits,
    /// `SpecialColor.Parse`: `black`, `purple`, `lower-purple` or four comma-separated numbers (CMYK).
    Color,
    /// Purple Pen throws on any other value.
    Enum(&'static [&'static str]),
    /// Purple Pen reads any other value as a default.
    Known(&'static [&'static str]),
    /// As `Known`, compared without case and surrounding space.
    KnownCi(&'static [&'static str]),
}

/// A condition on the element's own attributes.
#[derive(Debug, Clone, Copy)]
enum Cond {
    Always,
    /// The attribute has one of the values.
    If(&'static str, &'static [&'static str]),
}

struct Attr {
    name: &'static str,
    ty: Ty,
    /// `None`: read with a default.
    need: Option<Cond>,
}

enum Kid {
    /// The child must be there when the condition holds.
    Need(&'static str, Cond),
    /// The child must not be there unless the condition holds.
    OnlyIf(&'static str, Cond),
    /// When the condition holds, attribute `attr` of every child `kid` takes only `values`.
    Values {
        kid: &'static str,
        attr: &'static str,
        values: &'static [&'static str],
        when: Cond,
    },
}

/// What the text content of an element must be.
#[derive(Debug, Clone, Copy)]
enum Text {
    Any,
    /// `CircleGap.DecodeGaps`: `start:stop` pairs of numbers, comma-separated (bad pairs are dropped silently).
    CircleGaps,
    /// Old gaps: binary digits as for [`Ty::Bits`], or the new form if it has a colon.
    GapBits,
    /// At least `size * size` digits `0` or `1`.
    PunchPattern,
}

struct Rule {
    parent: &'static str,
    name: &'static str,
    attrs: &'static [Attr],
    kids: &'static [Kid],
    text: Text,
}

const fn req(name: &'static str, ty: Ty) -> Attr {
    Attr {
        name,
        ty,
        need: Some(Cond::Always),
    }
}

const fn opt(name: &'static str, ty: Ty) -> Attr {
    Attr {
        name,
        ty,
        need: None,
    }
}

const fn req_if(
    name: &'static str,
    ty: Ty,
    attr: &'static str,
    values: &'static [&'static str],
) -> Attr {
    Attr {
        name,
        ty,
        need: Some(Cond::If(attr, values)),
    }
}

const fn rule(parent: &'static str, name: &'static str, attrs: &'static [Attr]) -> Rule {
    Rule {
        parent,
        name,
        attrs,
        kids: &[],
        text: Text::Any,
    }
}

const ROOT: &str = "course-scribe-event";
/// `ObjectStore.Load` order of the top-level stores.
const STORES: [&str; 6] = [
    "event",
    "control",
    "course",
    "course-control",
    "leg",
    "special-object",
];
const DESCRIPTION_KIND: Ty = Ty::Enum(&["symbols", "text", "symbols-and-text"]);
const CONTROL_KINDS: &[&str] = &[
    "normal",
    "start",
    "map-issue",
    "finish",
    "crossing-point",
    "map-exchange",
];
/// Control kinds with one description symbol: box D to H would index past the end of `symbolIds`.
const ONE_SYMBOL: &[&str] = &["finish", "crossing-point", "map-issue"];
const SPECIAL_KINDS: &[&str] = &[
    "first-aid",
    "water",
    "optional-crossing-point",
    "forbidden-route",
    "registration-mark",
    "boundary",
    "out-of-bounds",
    "dangerous-area",
    "temporary-construction",
    "white-out",
    "text",
    "descriptions",
    "image",
    "line",
    "rectangle",
    "ellipse",
];
const PARTIAL_FLAGGING: &[&str] = &["beginning-part", "end-part", "map-issue-middle"];
const FORK_OR_LOOP: &[&str] = &["fork", "loop"];
const XY: &[Attr] = &[req("x", Ty::Float), req("y", Ty::Float)];
const DESCRIPTION_TEXT_LINE: &[Attr] = &[req("location", Ty::Enum(&["before", "after"]))];

/// The audit of `EventDB.Load`. Elements not listed here are skipped by Purple Pen (or not read at all), so
/// they carry no requirement.
static RULES: &[Rule] = &[
    // ObjectStore.Load: `id` of every store element, read without a default.
    rule(ROOT, "event", &[req("id", Ty::Int)]),
    Rule {
        parent: ROOT,
        name: "control",
        attrs: &[
            req("id", Ty::Int),
            req("kind", Ty::Enum(CONTROL_KINDS)),
            req_if("orientation", Ty::Float, "kind", &["crossing-point"]),
            opt("stretch", Ty::Float),
            opt("gaps", Ty::Bits),
            opt("all-controls-code-angle", Ty::Float),
            opt(
                "map-issue-location",
                Ty::Known(&["beginning", "middle", "end"]),
            ),
        ],
        kids: &[
            // "Only normal control points can have a code".
            Kid::OnlyIf("code", Cond::If("kind", &["normal"])),
            Kid::Values {
                kid: "description",
                attr: "box",
                values: &["all", "C"],
                when: Cond::If("kind", ONE_SYMBOL),
            },
        ],
        text: Text::Any,
    },
    rule(
        ROOT,
        "course",
        &[
            req("id", Ty::Int),
            req("kind", Ty::Enum(&["normal", "score"])),
            opt("order", Ty::Int),
        ],
    ),
    Rule {
        parent: ROOT,
        name: "course-control",
        attrs: &[
            req("id", Ty::Int),
            req("control", Ty::Int),
            opt("variation", Ty::Known(FORK_OR_LOOP)),
            req_if("variation-end", Ty::Int, "variation", FORK_OR_LOOP),
            opt("map-exchange", Ty::Bool),
            opt("map-flip", Ty::Bool),
            opt("points", Ty::Int),
        ],
        // "Non-variation course control shouldn't have variation sub-element."
        kids: &[Kid::OnlyIf(
            "variation",
            Cond::If("variation", FORK_OR_LOOP),
        )],
        text: Text::Any,
    },
    rule(
        ROOT,
        "leg",
        &[
            req("id", Ty::Int),
            req("start-control", Ty::Int),
            req("end-control", Ty::Int),
        ],
    ),
    Rule {
        parent: ROOT,
        name: "special-object",
        attrs: &[
            req("id", Ty::Int),
            req("kind", Ty::Enum(SPECIAL_KINDS)),
            req_if(
                "orientation",
                Ty::Float,
                "kind",
                &["optional-crossing-point"],
            ),
            opt("stretch", Ty::Float),
        ],
        kids: &[
            Kid::Need("location", Cond::Always),
            Kid::Need("font", Cond::If("kind", &["text"])),
            Kid::Need("image-data", Cond::If("kind", &["image"])),
        ],
        text: Text::Any,
    },
    // Event.ReadAttributesAndContent.
    rule(
        "event",
        "standards",
        &[
            opt("map", Ty::Known(&["2000", "2017", "Spr2019"])),
            opt("description", Ty::Known(&["2004", "2018"])),
        ],
    ),
    rule(
        "event",
        "map",
        &[
            req("scale", Ty::Float),
            req("kind", Ty::Enum(&["none", "OCAD", "bitmap", "PDF"])),
            req_if("dpi", Ty::Float, "kind", &["bitmap"]),
            opt("ignore-missing-fonts", Ty::Bool),
            opt("absolute-path", Ty::Str),
        ],
    ),
    rule(
        "event",
        "all-controls",
        &[
            opt("print-scale", Ty::Float),
            req("description-kind", DESCRIPTION_KIND),
        ],
    ),
    rule(
        "event",
        "numbering",
        &[opt("start", Ty::Int), opt("disallow-invertible", Ty::Bool)],
    ),
    rule(
        "event",
        "punch-card",
        &[
            opt("rows", Ty::Int),
            opt("columns", Ty::Int),
            opt("left-to-right", Ty::Bool),
            opt("top-to-bottom", Ty::Bool),
        ],
    ),
    rule(
        "event",
        "course-appearance",
        &[
            opt("control-circle-size-ratio", Ty::Float),
            opt("line-width-ratio", Ty::Float),
            opt("center-dot-diameter", Ty::Float),
            opt("number-size-ratio", Ty::Float),
            // `Enum.Parse(typeof(ItemScaling), …)` throws on another name.
            opt(
                "scale-sizes",
                Ty::Enum(&["None", "RelativeToMap", "RelativeTo15000"]),
            ),
            opt("scale-sizes-circle-gaps", Ty::Bool),
            opt("number-bold", Ty::Bool),
            opt("number-font", Ty::Known(&["Roboto", "Arial"])),
            opt("number-outline-width", Ty::Float),
            opt("auto-leg-gap-size", Ty::Float),
            opt("blend-purple", Ty::Known(&["true", "false"])),
            opt("blend-style", Ty::Known(&["blend", "layer"])),
            opt("lower-purple-layer", Ty::Int),
            opt("purple-cyan", Ty::Float),
            opt("purple-magenta", Ty::Float),
            opt("purple-yellow", Ty::Float),
            opt("purple-black", Ty::Float),
        ],
    ),
    // PrintArea.ReadAttributesAndContent: everything has a default.
    rule(
        "event",
        "print-area",
        &[
            opt("automatic", Ty::Bool),
            opt("restrict-to-page-size", Ty::Bool),
            opt("left", Ty::Float),
            opt("top", Ty::Float),
            opt("right", Ty::Float),
            opt("bottom", Ty::Float),
            opt("page-width", Ty::Int),
            opt("page-height", Ty::Int),
            opt("page-margins", Ty::Int),
            opt("page-landscape", Ty::Bool),
        ],
    ),
    rule(
        "event",
        "descriptions",
        &[
            req("lang", Ty::Str),
            opt("color", Ty::KnownCi(&["black", "purple"])),
        ],
    ),
    rule("event", "ocad", &[opt("overprint-colors", Ty::Bool)]),
    rule(
        "event",
        "custom-symbol-text",
        &[req("iof-2004-ref", Ty::Str), opt("show-key", Ty::Bool)],
    ),
    // SymbolText.ReadXml.
    rule(
        "custom-symbol-text",
        "text",
        &[
            req("lang", Ty::Str),
            opt("plural", Ty::Bool),
            opt("gender", Ty::Str),
            opt("case", Ty::Str),
            opt("modified-case", Ty::Str),
        ],
    ),
    rule("event", "livelox", &[req("importable-event-id", Ty::Str)]),
    // ControlPoint.ReadAttributesAndContent.
    rule("control", "location", XY),
    Rule {
        parent: "control",
        name: "punch-pattern",
        attrs: &[req("size", Ty::Int)],
        kids: &[],
        text: Text::PunchPattern,
    },
    Rule {
        parent: "control",
        name: "gaps",
        attrs: &[opt("scale", Ty::Int)],
        kids: &[],
        text: Text::GapBits,
    },
    Rule {
        parent: "control",
        name: "circle-gaps",
        attrs: &[opt("scale", Ty::Int)],
        kids: &[],
        text: Text::CircleGaps,
    },
    rule("control", "description-text-line", DESCRIPTION_TEXT_LINE),
    rule(
        "control",
        "description",
        &[
            req("box", Ty::Enum(&["all", "C", "D", "E", "F", "G", "H"])),
            opt("iof-2004-ref", Ty::Str),
        ],
    ),
    // Course.ReadAttributesAndContent.
    rule("course", "name", &[opt("hide-variations-on-map", Ty::Bool)]),
    rule(
        "course",
        "first",
        &[
            req("course-control", Ty::Int),
            opt("control-number", Ty::Int),
        ],
    ),
    rule(
        "course",
        "print-area",
        &[
            opt("part", Ty::Int),
            opt("automatic", Ty::Bool),
            opt("restrict-to-page-size", Ty::Bool),
            opt("left", Ty::Float),
            opt("top", Ty::Float),
            opt("right", Ty::Float),
            opt("bottom", Ty::Float),
            opt("page-width", Ty::Int),
            opt("page-height", Ty::Int),
            opt("page-margins", Ty::Int),
            opt("page-landscape", Ty::Bool),
        ],
    ),
    rule(
        "course",
        "options",
        &[
            req("print-scale", Ty::Float),
            opt("climb", Ty::Float),
            opt("load", Ty::Int),
            // EventDBUtil.ReadScoreColumnAttribute, score courses only: anything else is column A.
            opt("score-column", Ty::KnownCi(&["A", "B", "H", "none"])),
            // EventDBUtil.ReadDescriptionKindAttribute: the bug of 0.4.0.
            req("description-kind", DESCRIPTION_KIND),
            opt("hide-from-reports", Ty::Bool),
            opt("course-length", Ty::Float),
        ],
    ),
    rule(
        "course",
        "part-options",
        &[opt("part", Ty::Int), req("show-finish", Ty::Bool)],
    ),
    // Required, though any other value reads as `sequence`.
    rule(
        "course",
        "labels",
        &[req(
            "label-kind",
            Ty::Known(&[
                "sequence",
                "code",
                "sequence-and-code",
                "sequence-and-score",
                "code-and-score",
                "score",
            ]),
        )],
    ),
    rule(
        "course",
        "relay",
        &[
            opt("first-team", Ty::Int),
            opt("teams", Ty::Int),
            opt("legs", Ty::Int),
        ],
    ),
    rule(
        "course",
        "relay-branch",
        &[req("branch", Ty::Str), opt("leg", Ty::Int)],
    ),
    // CourseControl.ReadAttributesAndContent.
    rule("course-control", "next", &[req("course-control", Ty::Int)]),
    rule(
        "course-control",
        "variation",
        &[req("course-control", Ty::Int)],
    ),
    rule("course-control", "number-location", XY),
    rule(
        "course-control",
        "description-text-line",
        DESCRIPTION_TEXT_LINE,
    ),
    // Leg.ReadAttributesAndContent.
    rule(
        "leg",
        "flagging",
        &[
            req(
                "kind",
                Ty::Enum(&[
                    "none",
                    "beginning-part",
                    "end-part",
                    "map-issue-middle",
                    "all",
                ]),
            ),
            req_if("x", Ty::Float, "kind", PARTIAL_FLAGGING),
            req_if("y", Ty::Float, "kind", PARTIAL_FLAGGING),
        ],
    ),
    rule("bends", "location", XY),
    rule(
        "gaps",
        "gap",
        &[req("start", Ty::Float), req("length", Ty::Float)],
    ),
    // Special.ReadAttributesAndContent.
    rule(
        "special-object",
        "font",
        &[
            req("name", Ty::Str),
            req("bold", Ty::Bool),
            req("italic", Ty::Bool),
            opt("height", Ty::Float),
        ],
    ),
    rule("special-object", "location", XY),
    rule(
        "special-object",
        "appearance",
        &[
            opt("columns", Ty::Int),
            opt("color", Ty::Color),
            opt("line-kind", Ty::Known(&["single", "double", "dashed"])),
            opt("line-width", Ty::Float),
            opt("gap-size", Ty::Float),
            opt("dash-size", Ty::Float),
            opt("corner-radius", Ty::Float),
        ],
    ),
    rule("special-object", "courses", &[opt("all", Ty::Bool)]),
    // Read only under `courses` without `all="true"`; checked always (stricter than Purple Pen).
    rule(
        "courses",
        "course",
        &[req("course", Ty::Int), opt("part", Ty::Int)],
    ),
    rule("special-object", "image-data", &[opt("format", Ty::Str)]),
];

// ---------------------------------------------------------------------------------------------------------
// The check.
// ---------------------------------------------------------------------------------------------------------

fn trim(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
}

fn xml_float(s: &str) -> bool {
    let t = trim(s);
    matches!(t, "INF" | "-INF" | "NaN")
        || (!t.is_empty()
            && t.chars().all(|c| c.is_ascii_digit() || "+-.eE".contains(c))
            && t.parse::<f32>().is_ok())
}

fn bits(s: &str) -> bool {
    let t = trim(s);
    (1..=32).contains(&t.len()) && t.chars().all(|c| c == '0' || c == '1')
}

fn valid(ty: Ty, v: &str) -> bool {
    match ty {
        Ty::Int => trim(v).parse::<i32>().is_ok(),
        Ty::Float => xml_float(v),
        Ty::Bool => matches!(trim(v), "true" | "false" | "1" | "0"),
        Ty::Str => true,
        Ty::Bits => bits(v),
        Ty::Color => {
            matches!(v, "black" | "purple" | "lower-purple") || {
                let parts: Vec<&str> = v.split(',').collect();
                parts.len() == 4 && parts.iter().all(|p| xml_float(p))
            }
        }
        Ty::Enum(values) | Ty::Known(values) => values.contains(&v),
        Ty::KnownCi(values) => values.iter().any(|k| k.eq_ignore_ascii_case(trim(v))),
    }
}

fn holds(cond: Cond, attrs: &BTreeMap<String, String>) -> bool {
    match cond {
        Cond::Always => true,
        Cond::If(name, values) => attrs
            .get(name)
            .is_some_and(|v| values.contains(&v.as_str())),
    }
}

struct Frame {
    name: String,
    path: String,
    attrs: BTreeMap<String, String>,
    kids: Vec<(String, BTreeMap<String, String>)>,
    text: String,
    rule: Option<&'static Rule>,
}

#[derive(Default)]
struct Checker {
    stack: Vec<Frame>,
    errors: Vec<String>,
    roots: usize,
    store_rank: usize,
    ids: BTreeMap<String, BTreeSet<String>>,
}

impl Checker {
    fn open(&mut self, e: &BytesStart<'_>) {
        let name = e.local_name().as_ref().to_string();
        let mut attrs = BTreeMap::new();
        for a in e.attributes() {
            match a {
                Ok(a) => match a.normalized_value(XmlVersion::Explicit1_0) {
                    Ok(v) => {
                        attrs.insert(a.key.as_ref().to_string(), v.into_owned());
                    }
                    Err(err) => self
                        .errors
                        .push(format!("<{name}>: bad attribute value: {err}")),
                },
                Err(err) => self.errors.push(format!("<{name}>: bad attribute: {err}")),
            }
        }
        let parent = self.stack.last().map_or("", |f| f.name.as_str());
        let id = attrs
            .get("id")
            .map_or(String::new(), |i| format!("[id={i}]"));
        let path = match self.stack.last() {
            Some(f) => format!("{}/{name}{id}", f.path),
            None => name.clone(),
        };
        match self.stack.len() {
            0 => {
                self.roots += 1;
                if name != ROOT {
                    self.errors
                        .push(format!("{path}: the root must be <{ROOT}>"));
                }
            }
            1 => match STORES.iter().position(|s| *s == name) {
                None => self.errors.push(format!(
                    "{path}: unknown top-level element; Purple Pen stops loading here"
                )),
                Some(r) => {
                    if r < self.store_rank {
                        self.errors.push(format!(
                            "{path}: out of store order (event, control, course, course-control, leg, \
                             special-object); Purple Pen stops loading before it"
                        ));
                    }
                    self.store_rank = self.store_rank.max(r);
                    if let Some(i) = attrs.get("id")
                        && !self.ids.entry(name.clone()).or_default().insert(i.clone())
                    {
                        self.errors.push(format!("{path}: duplicate id"));
                    }
                }
            },
            _ => {}
        }
        let rule = RULES.iter().find(|r| r.parent == parent && r.name == name);
        if let Some(rule) = rule {
            for a in rule.attrs {
                match attrs.get(a.name).filter(|v| !v.is_empty()) {
                    None => {
                        if a.need.is_some_and(|c| holds(c, &attrs)) {
                            self.errors
                                .push(format!("{path}: missing attribute '{}'", a.name));
                        }
                    }
                    Some(v) => {
                        if !valid(a.ty, v) {
                            self.errors.push(format!(
                                "{path}: attribute '{}' has the value {v:?}, expected {:?}",
                                a.name, a.ty
                            ));
                        }
                    }
                }
            }
        }
        if let Some(f) = self.stack.last_mut() {
            f.kids.push((name.clone(), attrs.clone()));
        }
        self.stack.push(Frame {
            name,
            path,
            attrs,
            kids: Vec::new(),
            text: String::new(),
            rule,
        });
    }

    fn close(&mut self) {
        let Some(f) = self.stack.pop() else {
            self.errors.push("unbalanced end tag".into());
            return;
        };
        let Some(rule) = f.rule else { return };
        let has = |n: &str| f.kids.iter().any(|(k, _)| k == n);
        for k in rule.kids {
            match k {
                Kid::Need(n, c) => {
                    if holds(*c, &f.attrs) && !has(n) {
                        self.errors.push(format!("{}: missing child <{n}>", f.path));
                    }
                }
                Kid::OnlyIf(n, c) => {
                    if !holds(*c, &f.attrs) && has(n) {
                        self.errors
                            .push(format!("{}: child <{n}> not allowed here", f.path));
                    }
                }
                Kid::Values {
                    kid,
                    attr,
                    values,
                    when,
                } => {
                    if !holds(*when, &f.attrs) {
                        continue;
                    }
                    for (n, a) in &f.kids {
                        if n == kid
                            && let Some(v) = a.get(*attr)
                            && !values.contains(&v.as_str())
                        {
                            self.errors.push(format!(
                                "{}: <{kid} {attr}={v:?}> not allowed here, only {values:?}",
                                f.path
                            ));
                        }
                    }
                }
            }
        }
        let t = trim(&f.text);
        let ok = match rule.text {
            Text::Any => true,
            Text::CircleGaps => circle_gaps(t),
            Text::GapBits => t.is_empty() || (t.contains(':') && circle_gaps(t)) || bits(t),
            Text::PunchPattern => {
                let size = f
                    .attrs
                    .get("size")
                    .and_then(|s| trim(s).parse::<usize>().ok())
                    .unwrap_or(0);
                t.chars().filter(|c| matches!(c, '0' | '1')).count() >= size * size
            }
        };
        if !ok {
            self.errors
                .push(format!("{}: text {t:?} is not {:?}", f.path, rule.text));
        }
    }
}

fn circle_gaps(t: &str) -> bool {
    t.is_empty()
        || t.split(',').all(|pair| {
            let p: Vec<&str> = pair.split(':').collect();
            p.len() == 2 && p.iter().all(|v| xml_float(v))
        })
}

/// The ways `text` would fail to load in Purple Pen 4.0, or would load with content lost, by [`RULES`].
fn check(text: &str) -> Result<(), Vec<String>> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut reader = Reader::from_str(text);
    let mut c = Checker::default();
    loop {
        match reader.read_event() {
            Err(e) => {
                c.errors.push(format!("not well-formed: {e}"));
                break;
            }
            Ok(Event::Start(e)) => c.open(&e),
            Ok(Event::Empty(e)) => {
                c.open(&e);
                c.close();
            }
            Ok(Event::End(_)) => c.close(),
            Ok(Event::Text(t)) => {
                if let Some(f) = c.stack.last_mut() {
                    f.text.push_str(&t.xml_content(XmlVersion::Explicit1_0));
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(f) = c.stack.last_mut() {
                    f.text.push_str(&t.xml_content(XmlVersion::Explicit1_0));
                }
            }
            Ok(Event::GeneralRef(_)) => {
                if let Some(f) = c.stack.last_mut() {
                    f.text.push('&');
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
        }
    }
    if c.roots != 1 {
        c.errors
            .push(format!("{} root elements, expected 1", c.roots));
    }
    if !c.stack.is_empty() {
        c.errors.push("unclosed elements at the end".into());
    }
    if c.errors.is_empty() {
        Ok(())
    } else {
        Err(c.errors)
    }
}

#[track_caller]
fn assert_loads(what: &str, text: &str) {
    if let Err(errors) = check(text) {
        panic!(
            "{what}: Purple Pen would refuse this file:\n  {}\n\n{text}",
            errors.join("\n  ")
        );
    }
}

#[track_caller]
fn assert_refused(what: &str, text: &str, needle: &str) {
    match check(text) {
        Ok(()) => panic!("{what}: the check passed\n{text}"),
        Err(errors) => assert!(
            errors.iter().any(|e| e.contains(needle)),
            "{what}: no error mentions {needle:?}: {errors:?}"
        ),
    }
}

// ---------------------------------------------------------------------------------------------------------
// Inputs.
// ---------------------------------------------------------------------------------------------------------

const FIXTURES: [&str; 3] = ["lincoln-park.ppen", "overlapper.ppen", "sampleevent8.ppen"];

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/courses/ppen/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    String::from_utf8(std::fs::read(path).unwrap()).unwrap()
}

fn read(text: &str) -> RawEvent {
    read_ppen(text.as_bytes()).unwrap()
}

fn options(map: PpenMap, standard: DescriptionStandard) -> ScratchOptions {
    ScratchOptions {
        map,
        map_standard: MapStandard::Isom2000,
        description_standard: standard,
        description_lang: "en".into(),
        line_ending: LineEnding::Crlf,
    }
}

fn options_for(name: &str) -> ScratchOptions {
    match name {
        "lincoln-park.ppen" => options(
            PpenMap::Bitmap {
                path: "LincolnNov12.gif".into(),
                dpi: 300.0,
            },
            DescriptionStandard::Iof2004,
        ),
        "overlapper.ppen" => options(PpenMap::None, DescriptionStandard::Iof2004),
        "sampleevent8.ppen" => options(PpenMap::None, DescriptionStandard::Iof2018),
        other => panic!("no options for {other}"),
    }
}

fn scratch(ev: &RawEvent, o: &ScratchOptions) -> String {
    write_ppen(ev, o).unwrap_or_else(|e| panic!("{e}")).text
}

fn pt(x: f64, y: f64) -> RawPoint {
    RawPoint { x_mm: x, y_mm: y }
}

fn stop(code: &str) -> Stop {
    Stop {
        control: code.into(),
        kind: None,
        number: None,
        number_text_at: None,
        circle_gaps: Vec::new(),
        map_flip: false,
    }
}

fn control(code: &str, kind: ControlKind, x: f64, y: f64) -> Control<RawPoint> {
    Control {
        id: None,
        code: code.into(),
        kind,
        position: Some(pt(x, y)),
        geo: None,
        description: None,
    }
}

fn course(name: &str, kind: CourseKind, stops: &[&str]) -> Course<RawPoint> {
    Course {
        id: None,
        name: name.into(),
        family: None,
        kind,
        legs: Vec::new(),
        stops: stops.iter().map(|s| stop(s)).collect(),
        climb_m: None,
        length_m: None,
        print_scale_hint: None,
        course_obj_ratio: None,
        secondary_title: None,
    }
}

/// One special object of every kind the writer writes, for the courses `only` (and all courses).
fn every_special(only: &str) -> Vec<Special<RawPoint>> {
    let any = CourseFilter::All;
    let some = CourseFilter::Only(vec![only.into()]);
    let ring = vec![
        pt(10.0, 10.0),
        pt(20.0, 10.0),
        pt(20.0, 20.0),
        pt(10.0, 10.0),
    ];
    vec![
        Special::FirstAid {
            id: None,
            pt: pt(1.0, 2.0),
            courses: any.clone(),
        },
        Special::Water {
            id: None,
            pt: pt(3.0, 4.0),
            courses: some.clone(),
        },
        Special::ForbiddenRoute {
            id: None,
            path: vec![pt(5.0, 6.0)],
            courses: any.clone(),
        },
        Special::Boundary {
            id: None,
            path: vec![pt(1.0, 1.0), pt(9.0, 9.0)],
            courses: some.clone(),
        },
        Special::Line {
            id: None,
            path: vec![pt(2.0, 1.0), pt(8.0, 9.0), pt(3.0, 7.0)],
            courses: any.clone(),
        },
        Special::OutOfBounds {
            id: None,
            rings: vec![ring.clone()],
            courses: any.clone(),
        },
        Special::Dangerous {
            id: None,
            rings: vec![ring.clone()],
            courses: some.clone(),
        },
        Special::WhiteOut {
            id: None,
            rings: vec![ring],
            courses: any,
        },
    ]
}

/// A small event with every feature the writer has: a normal course with a crossing point, a flagged leg,
/// leg bends and gaps, circle gaps, a number location, climb and length; a map exchange with a flip; a score
/// course; a course of only start and finish; and a special object of every kind.
fn full_event() -> RawEvent {
    let controls = vec![
        control("S1", ControlKind::Start, 0.0, 0.0),
        control("31", ControlKind::Control, 10.0, 0.0),
        control("32", ControlKind::Control, 20.0, 5.0),
        control("X1", ControlKind::CrossingPoint, 25.0, 10.0),
        control("XC", ControlKind::MapExchange, 30.0, 10.0),
        control("33", ControlKind::Control, 35.0, 0.0),
        control("F1", ControlKind::Finish, 40.0, 0.0),
    ];
    let mut long = course(
        "Long",
        CourseKind::Normal,
        &["S1", "31", "32", "X1", "XC", "33", "F1"],
    );
    long.climb_m = Some(5.0);
    long.length_m = Some(4200.0);
    long.print_scale_hint = Some(4000);
    long.secondary_title = Some("Second line".into());
    long.stops[1].circle_gaps = vec![(10.0, 40.0)];
    long.stops[2].number_text_at = Some(freemapper_course::model::Offset {
        dx_mm: 1.5,
        dy_mm: -2.0,
    });
    long.stops[4].map_flip = true;
    long.legs = vec![
        Leg {
            bends: vec![pt(5.0, 3.0)],
            gaps: vec![(1.0, 2.5)],
            marked_route: false,
        },
        Leg {
            bends: Vec::new(),
            gaps: Vec::new(),
            marked_route: true,
        },
    ];
    // No course but Long uses S1 to 31 or 31 to 32: Purple Pen keeps one leg per pair of controls.
    let mut medium = course("Medium", CourseKind::Normal, &["S1", "33", "F1"]);
    medium.climb_m = Some(5.0);
    let mut exchange = course("Exchange", CourseKind::Normal, &["S1", "32", "33", "F1"]);
    exchange.stops[1].kind = Some(ControlKind::MapExchange);
    let score = course("Score", CourseKind::Score, &["S1", "32", "31", "33", "F1"]);
    let start_finish = course("Start and finish", CourseKind::Normal, &["S1", "F1"]);
    CourseEvent {
        controls,
        courses: vec![long, medium, exchange, score, start_finish],
        specials: every_special("Long"),
        source: CourseSource {
            format: SourceFormat::PurplePen,
            event_name: Some("Strict".into()),
            map_scale: Some(10000),
            item_scaling: None,
        },
        notes: Vec::new(),
    }
}

fn empty_event() -> RawEvent {
    CourseEvent {
        controls: Vec::new(),
        courses: Vec::new(),
        specials: Vec::new(),
        source: CourseSource {
            format: SourceFormat::PurplePen,
            event_name: None,
            map_scale: Some(15000),
            item_scaling: None,
        },
        notes: Vec::new(),
    }
}

fn patch(base: &str, ev: &RawEvent) -> String {
    write_ppen_patch(base.as_bytes(), ev, &PatchOptions::default())
        .unwrap_or_else(|e| panic!("{e}"))
        .text
}

// ---------------------------------------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------------------------------------

/// The check itself: Purple Pen wrote the fixtures and loads them.
#[test]
fn fixtures_pass_the_check() {
    for name in FIXTURES {
        assert_loads(name, &fixture(name));
    }
}

/// The check catches the file Purple Pen 4.0 refused, and the other kinds of refusal it encodes.
#[test]
fn check_refuses_what_purple_pen_refuses() {
    let f = fixture("lincoln-park.ppen");
    let v040 = f.replace(
        "<options print-scale=\"5000\" description-kind=\"symbols\" />",
        "<options print-scale=\"4000\" climb=\"5\" />",
    );
    assert_ne!(v040, f);
    assert_refused(
        "0.4.0 options",
        &v040,
        "course[id=1]/options: missing attribute 'description-kind'",
    );
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "empty description-kind",
            "description-kind=\"symbols\" />\r\n  </course>",
            "description-kind=\"\" />\r\n  </course>",
            "missing attribute 'description-kind'",
        ),
        (
            "bad description-kind",
            "<options print-scale=\"5000\" description-kind=\"symbols\" />",
            "<options print-scale=\"5000\" description-kind=\"Symbols\" />",
            "attribute 'description-kind'",
        ),
        (
            "labels without label-kind",
            "<labels label-kind=\"sequence\" />",
            "<labels />",
            "missing attribute 'label-kind'",
        ),
        (
            "all-controls without description-kind",
            "<all-controls print-scale=\"5000\" description-kind=\"symbols\" />",
            "<all-controls print-scale=\"5000\" />",
            "missing attribute 'description-kind'",
        ),
        (
            "bitmap map without dpi",
            " dpi=\"300\"",
            "",
            "missing attribute 'dpi'",
        ),
        (
            "descriptions without lang",
            "<descriptions lang=\"en\" />",
            "<descriptions />",
            "missing attribute 'lang'",
        ),
        (
            "bad bool",
            "disallow-invertible=\"false\"",
            "disallow-invertible=\"False\"",
            "attribute 'disallow-invertible'",
        ),
        (
            "bad float",
            "x=\"140.207184\"",
            "x=\"140,207184\"",
            "attribute 'x'",
        ),
        (
            "code at a start",
            "<control id=\"1\" kind=\"start\">\r\n",
            "<control id=\"1\" kind=\"start\">\r\n    <code>30</code>\r\n",
            "child <code> not allowed",
        ),
        (
            "box D at a finish",
            "<description box=\"all\" iof-2004-ref=\"14.3\" />",
            "<description box=\"D\" iof-2004-ref=\"14.3\" />",
            "<description box=\"D\"> not allowed",
        ),
        (
            "gap without length",
            "<gap start=\"70.4939\" length=\"15.66552\" />",
            "<gap start=\"70.4939\" />",
            "missing attribute 'length'",
        ),
        (
            "partial flagging without point",
            "end-control=\"5\">\r\n",
            "end-control=\"5\">\r\n    <flagging kind=\"end-part\" />\r\n",
            "missing attribute 'x'",
        ),
        (
            "next without course-control",
            "<next course-control=\"2\" />",
            "<next />",
            "missing attribute 'course-control'",
        ),
        (
            "course-control without control",
            "<course-control id=\"6\" control=\"5\" />",
            "<course-control id=\"6\" />",
            "missing attribute 'control'",
        ),
        (
            "duplicate id",
            "<course-control id=\"6\" control=\"5\" />",
            "<course-control id=\"3\" control=\"5\" />",
            "duplicate id",
        ),
        (
            "old gaps not binary",
            "11100000000011111111111111111111",
            "1110000000001111111111111111111x",
            "is not GapBits",
        ),
    ];
    for (what, from, to, needle) in cases {
        let changed = f.replacen(from, to, 1);
        assert_ne!(changed, f, "{what}: no change");
        assert_refused(what, &changed, needle);
    }
    // Store order: a control after the course is never read.
    let control = "  <control id=\"5\" kind=\"finish\">\r\n    <location x=\"134.321289\" y=\"35.8955536\" />\r\n    \
                   <description box=\"all\" iof-2004-ref=\"14.3\" />\r\n    \
                   <gaps scale=\"5000\">11111111111000000000011111111111</gaps>\r\n    \
                   <circle-gaps scale=\"5000\">126.475586:231.030273</circle-gaps>\r\n  </control>\r\n";
    assert!(f.contains(control));
    let moved = f.replace(control, "").replace(
        "  <course-control id=\"1\"",
        &format!("{control}  <course-control id=\"1\""),
    );
    assert_refused("control after course", &moved, "out of store order");
    let unknown = f.replace(
        "  <course id=\"1\"",
        "  <comment-object />\r\n  <course id=\"1\"",
    );
    assert_refused("unknown top level", &unknown, "unknown top-level element");
    // Special objects: a location always, a font for text, an orientation for an optional crossing point.
    let base = fixture("overlapper.ppen");
    let water = "<special-object id=\"1\" kind=\"water\">";
    assert!(base.contains(water));
    let crossing = base.replace(
        water,
        "<special-object id=\"1\" kind=\"optional-crossing-point\">",
    );
    assert_refused(
        "optional crossing point",
        &crossing,
        "missing attribute 'orientation'",
    );
    let text = base.replace(water, "<special-object id=\"1\" kind=\"text\">");
    assert_refused("text without font", &text, "missing child <font>");
    let s = base.find(water).unwrap();
    let e = s + base[s..].find("</special-object>").unwrap();
    let no_location = format!(
        "{}{water}\r\n    <courses all=\"true\" />\r\n  {}",
        &base[..s],
        &base[e..]
    );
    assert_refused(
        "special without location",
        &no_location,
        "missing child <location>",
    );
}

/// (b) Every fixture written from scratch, from what the reader made of it.
#[test]
fn scratch_output_of_the_fixtures_loads() {
    for name in FIXTURES {
        let text = scratch(&read(&fixture(name)), &options_for(name));
        assert_loads(name, &text);
    }
}

/// (b) Score course, map exchange, crossing point, flagged legs, gaps, a start-and-finish course, special
/// objects of every kind, every map kind and standard; and an empty event.
#[test]
fn scratch_output_of_every_feature_loads() {
    let maps = [
        PpenMap::None,
        PpenMap::Ocad {
            path: "map.ocd".into(),
        },
        PpenMap::Bitmap {
            path: "map.png".into(),
            dpi: 150.0,
        },
        PpenMap::Pdf {
            path: "map.pdf".into(),
        },
    ];
    for map in maps {
        for map_standard in [
            MapStandard::Isom2000,
            MapStandard::Isom2017,
            MapStandard::Issprom2019,
        ] {
            let mut o = options(map.clone(), DescriptionStandard::Iof2018);
            o.map_standard = map_standard;
            o.line_ending = LineEnding::Lf;
            assert_loads(
                &format!("{map:?} {map_standard:?}"),
                &scratch(&full_event(), &o),
            );
        }
    }
    let o = options(PpenMap::None, DescriptionStandard::Iof2004);
    assert_loads("empty event", &scratch(&empty_event(), &o));
    // Every course alone, so that each option set is the first and only `<options>`.
    let full = full_event();
    for c in &full.courses {
        let mut ev = full.clone();
        ev.courses = vec![c.clone()];
        ev.specials.clear();
        assert_loads(&c.name, &scratch(&ev, &o));
    }
}

/// (c) A patch of each fixture with an unchanged event, with changed objects, and with added and deleted ones.
#[test]
fn patch_output_loads() {
    for name in FIXTURES {
        let base = fixture(name);
        let ev = read(&base);
        assert_loads(&format!("{name} unchanged"), &patch(&base, &ev));

        // Changed: every course gets climb, length and another print scale (the `<options>` merge), the
        // first control moves. The scale doubles, so courses that share circle gaps still share a scale.
        let mut changed = ev.clone();
        for c in &mut changed.courses {
            c.climb_m = Some(5.0);
            c.length_m = Some(3100.0);
            c.print_scale_hint = Some(2 * c.print_scale_hint.unwrap_or(15000));
        }
        if let Some(p) = changed.controls[0].position.as_mut() {
            p.x_mm += 1.0;
        }
        assert_loads(&format!("{name} changed"), &patch(&base, &changed));

        // Options removed from the first course: no hint, no climb, no length (one course only, so that no
        // two courses meet at the default scale with different circle gaps).
        let mut bare = ev.clone();
        let c = &mut bare.courses[0];
        c.climb_m = None;
        c.length_m = None;
        c.print_scale_hint = None;
        assert_loads(&format!("{name} bare options"), &patch(&base, &bare));

        // Added: new controls, a new course with climb (new `<options>`), a start-and-finish course, a
        // special object of every kind.
        let mut added = ev.clone();
        let start = added
            .controls
            .iter()
            .find(|c| c.kind == ControlKind::Start)
            .unwrap()
            .code
            .clone();
        let finish = added
            .controls
            .iter()
            .find(|c| c.kind == ControlKind::Finish)
            .unwrap()
            .code
            .clone();
        added
            .controls
            .push(control("901", ControlKind::Control, 11.0, 12.0));
        added
            .controls
            .push(control("902", ControlKind::Control, 21.0, 22.0));
        added
            .controls
            .push(control("XP9", ControlKind::CrossingPoint, 31.0, 32.0));
        let mut new = course(
            "New",
            CourseKind::Normal,
            &[&start, "901", "XP9", "902", &finish],
        );
        new.climb_m = Some(5.0);
        new.legs = vec![
            Leg {
                bends: vec![pt(15.0, 15.0)],
                gaps: vec![(2.0, 3.0)],
                marked_route: true,
            },
            Leg {
                bends: Vec::new(),
                gaps: Vec::new(),
                marked_route: false,
            },
        ];
        added.courses.push(new);
        added.courses.push(course(
            "New score",
            CourseKind::Score,
            &[&start, "902", "901", &finish],
        ));
        added.courses.push(course(
            "New start and finish",
            CourseKind::Normal,
            &[&start, &finish],
        ));
        added.specials.extend(every_special("New"));
        assert_loads(&format!("{name} added"), &patch(&base, &added));

        // Deleted: the first course.
        let mut fewer = ev.clone();
        let gone = fewer.courses.remove(0);
        let delete = Deletions {
            courses: vec![gone.id.unwrap()],
            ..Deletions::default()
        };
        let text = write_ppen_patch(
            base.as_bytes(),
            &fewer,
            &PatchOptions {
                delete,
                ..PatchOptions::default()
            },
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .text;
        assert_loads(&format!("{name} deleted"), &text);
    }
}

/// (c) A patch that puts courses, controls and special objects into a base that has none of them, so every
/// store is inserted next to the `<event>` element.
#[test]
fn patch_into_an_empty_base_loads() {
    let o = options(PpenMap::None, DescriptionStandard::Iof2018);
    let base = scratch(&empty_event(), &o);
    assert_loads("empty base", &base);
    let mut ev = full_event();
    ev.source.map_scale = Some(15000);
    assert_loads("full event into an empty base", &patch(&base, &ev));
    // And into a base written from the full event, with everything changed once more.
    let base = scratch(&full_event(), &o);
    let mut ev = read(&base);
    for c in &mut ev.courses {
        c.climb_m = Some(12.0);
        c.print_scale_hint = Some(2 * c.print_scale_hint.unwrap_or(15000));
    }
    ev.specials.extend(every_special("Score"));
    assert_loads("full event patched", &patch(&base, &ev));
}

/// A file of the 0.4.0 writer gets its `description-kind` back from any patch, also one that changes nothing
/// else, and a value the base has stays.
#[test]
fn patch_repairs_a_file_of_the_0_4_0_writer() {
    let f = fixture("lincoln-park.ppen");
    let v040 = f.replace(
        "<options print-scale=\"5000\" description-kind=\"symbols\" />",
        "<options print-scale=\"4000\" climb=\"5\" />",
    );
    let ev = read(&v040);
    let text = patch(&v040, &ev);
    assert_loads("0.4.0 base, unchanged event", &text);
    assert!(
        text.contains("<options print-scale=\"4000\" climb=\"5\" description-kind=\"symbols\" />"),
        "{text}"
    );
    assert_eq!(
        text,
        v040.replace(
            " climb=\"5\" />",
            " climb=\"5\" description-kind=\"symbols\" />"
        )
    );
    // The base's own value is kept when the options change.
    let base = f.replace("description-kind=\"symbols\"", "description-kind=\"text\"");
    let mut ev = read(&base);
    ev.courses[0].climb_m = Some(7.0);
    let text = patch(&base, &ev);
    assert_loads("text descriptions", &text);
    assert!(
        text.contains("<options print-scale=\"5000\" climb=\"7\" description-kind=\"text\" />"),
        "{text}"
    );
}

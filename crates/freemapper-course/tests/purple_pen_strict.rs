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
//!
//! Purple Pen 4.0 loaded a file of the 0.4.1 writer and then crashed: `Controller.LoadInitialFile` selects the
//! All Controls view, `SelectionMgr.UpdateActiveDescription` formats its description, and
//! `DescriptionFormatter.GetDirectiveLine` looks up `symbolDB[control.symbolIds[0]]` for every finish, crossing
//! point and map issue point. The finish had no `<description>`, so the id was null and `SymbolDB`'s indexer
//! threw. [`semantics`] is the audit of what runs after the load (`DescriptionFormatter.cs`, `Textifier.cs`,
//! `SymbolDB.cs`, `symbols.xml`, `CourseView.cs`, `CourseFormatter.cs`, `QueryEvent.cs`, `SelectionMgr.cs`,
//! `Controller.cs`): the description symbols per control kind and column, checked against Purple Pen's symbol
//! table ([`SYMBOLS`]), and the references between the stores.

use std::collections::{BTreeMap, BTreeSet};

use freemapper_course::model::{CourseFilter, Description, RawPoint, Special, Stop};
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
// After the load: the symbol table and what the views look up.
// ---------------------------------------------------------------------------------------------------------

/// Purple Pen's description symbols that are in every standard, by kind (`symbols.xml` at `201417f`: the
/// `kind` attribute, without `standard`). The kind is the column the symbol belongs to; `V` to `Z` are the
/// directives that span the row. The `@` texts (course length and the like) are left out.
const SYMBOLS: &[(char, &str)] = &[
    ('A', "start"),
    (
        'C',
        "0.1N 0.2NE 0.1E 0.2SE 0.1S 0.2SW 0.1W 0.2NW 0.3 0.4 0.5",
    ),
    (
        'D',
        "1.1 1.2 1.3 1.4 1.5 1.6 1.7 1.8 1.9 1.11 1.12 1.13 1.14 1.15 1.16 2.1 2.2 2.3 2.4 2.5 2.6 2.7 2.8 \
         2.9 3.1 3.2 3.3 3.4 3.5 3.6 3.7 3.8 3.9 3.10 3.11 4.1 4.2 4.3 4.4 4.5 4.6 4.7 4.8 4.9 4.10 5.1 5.2 \
         5.3 5.4 5.5 5.6 5.7 5.8 5.9 5.10 5.11 5.12 5.13 5.14 5.15 5.16 5.17 5.18 5.19 5.20 5.23 5.24 6.1 \
         6.2",
    ),
    ('E', "8.1 8.2 8.3 8.4 8.5 8.6 8.7 8.8 8.9 8.10 8.11"),
    (
        'F',
        "10.1 10.1single 10.2 10.2single 9.1high 9.1deep 9.2 9.3high 9.3deep 9.4",
    ),
    (
        'G',
        "11.1N 11.1NE 11.1E 11.1SE 11.1S 11.1SW 11.1W 11.1NW 11.2N 11.2NE 11.2E 11.2SE 11.2S 11.2SW 11.2W \
         11.2NW 11.3N 11.3NE 11.3E 11.3SE 11.3S 11.3SW 11.3W 11.3NW 11.4N 11.4NE 11.4E 11.4SE 11.4S 11.4SW \
         11.4W 11.4NW 11.5N 11.5NE 11.5E 11.5SE 11.5S 11.5SW 11.5W 11.5NW 11.6N 11.6NE 11.6E 11.6SE 11.6S \
         11.6SW 11.6W 11.6NW 11.8N 11.8NE 11.8E 11.8SE 11.8S 11.8SW 11.8W 11.8NW 11.14N 11.14NE 11.14E \
         11.14SE 11.14S 11.14SW 11.14W 11.14NW 11.9 11.10 11.11 11.12 11.13 11.15 11.15single",
    ),
    ('H', "12.1 12.2 12.4"),
    ('V', "13.6"),
    ('W', "13.1 13.2 13.5"),
    ('X', "13.3 13.4"),
    ('Y', "13.5control"),
    ('Z', "14.1 14.2 14.3"),
];

/// The symbols of one standard only (`standard="2004"` or `"2018"` in `symbols.xml`): id, kind, standard.
/// `SymbolDB` finds them only while the event's description standard is theirs.
const SYMBOLS_OF_ONE_STANDARD: &[(&str, char, &str)] = &[
    ("1.10", 'D', "2004"),
    ("1.10", 'D', "2018"),
    ("2.10", 'D', "2018"),
    ("5.25", 'D', "2018"),
    ("5.26", 'D', "2018"),
    ("11.7", 'F', "2018"),
    ("11.7", 'G', "2004"),
    ("12.3", 'H', "2004"),
    ("15.6", 'Y', "2018"),
];

/// The kinds of `id` in the description standard `standard`; empty when `SymbolDB` does not know it there.
fn symbol_kinds(id: &str, standard: &str) -> Vec<char> {
    let mut kinds: Vec<char> = SYMBOLS
        .iter()
        .filter(|(_, ids)| ids.split_whitespace().any(|i| i == id))
        .map(|(k, _)| *k)
        .collect();
    kinds.extend(
        SYMBOLS_OF_ONE_STANDARD
            .iter()
            .filter(|(i, _, s)| *i == id && *s == standard)
            .map(|(_, k, _)| *k),
    );
    kinds
}

/// What the description of a control needs, by control kind. A finish, crossing point or map issue point has
/// one symbol, `symbolIds[0]`, which `DescriptionFormatter.GetDirectiveLine` passes to `SymbolDB`'s indexer
/// for every such control of the All Controls view (the view `Controller.LoadInitialFile` opens) and of each
/// normal course: null throws `ArgumentNullException`, an unknown id `KeyNotFoundException`, an id of the
/// other standard `InvalidOperationException`. The kind is the one Purple Pen's description popup offers
/// (`DescriptionControl`: the directive's own kind), and the id is the one `AddControlMode` gives a new
/// control. Starts, normal controls and map exchanges have six columns, which `GetRegularLine` and the
/// `Textifier` look up only after `SymbolExistsInStandard`; see [`COLUMN_KINDS`].
const DIRECTIVES: &[(&str, char, &str)] = &[
    ("finish", 'Z', "14.3"),
    ("crossing-point", 'X', "13.3"),
    ("map-issue", 'V', "13.6"),
];

/// The symbol kinds Purple Pen's description popup offers per box (`DescriptionControl`: the column's own
/// letter, and `D` besides `E` in column E). Another kind in column C or G can throw: `Textifier`
/// formats those symbols' texts with one argument, and the combination symbols (`10.1`, `9.2`, `11.15`, …)
/// have two placeholders. An id `SymbolDB` does not know in the standard is not shown at all.
const COLUMN_KINDS: &[(&str, &[char])] = &[
    ("all", &['C']),
    ("C", &['C']),
    ("D", &['D']),
    ("E", &['E', 'D']),
    ("F", &['F']),
    ("G", &['G']),
    ("H", &['H']),
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
    sem: Sem,
}

/// What [`semantics`] needs of a file: the objects with their references, and the description symbols.
#[derive(Default)]
struct Sem {
    /// `standards/@description`; Purple Pen's default is 2004.
    standard: Option<String>,
    controls: Vec<SemControl>,
    courses: Vec<SemCourse>,
    ccs: Vec<SemCc>,
    /// Path, start control, end control.
    legs: Vec<(String, Option<String>, Option<String>)>,
    /// Path and course id of each `special-object/courses/course`.
    special_courses: Vec<(String, Option<String>)>,
}

struct SemControl {
    path: String,
    id: Option<String>,
    kind: String,
    /// Box and `iof-2004-ref` of each `description`.
    descriptions: Vec<(String, Option<String>)>,
}

struct SemCourse {
    path: String,
    id: Option<String>,
    first: Option<String>,
}

struct SemCc {
    path: String,
    id: Option<String>,
    control: Option<String>,
    next: Option<String>,
    variations: Vec<String>,
    variation_end: Option<String>,
}

impl Checker {
    /// Records what [`semantics`] needs of the element that opens at `path`.
    fn collect(&mut self, name: &str, path: &str, attrs: &BTreeMap<String, String>) {
        let depth = self.stack.len();
        let parent = self
            .stack
            .last()
            .map(|f| f.name.clone())
            .unwrap_or_default();
        let top = self
            .stack
            .get(1)
            .map(|f| f.name.clone())
            .unwrap_or_default();
        let get = |k: &str| {
            attrs
                .get(k)
                .map(|v| trim(v).to_string())
                .filter(|v| !v.is_empty())
        };
        let s = &mut self.sem;
        match (depth, parent.as_str(), name) {
            (1, _, "control") => s.controls.push(SemControl {
                path: path.into(),
                id: get("id"),
                kind: get("kind").unwrap_or_default(),
                descriptions: Vec::new(),
            }),
            (2, "control", "description") => {
                if let Some(c) = s.controls.last_mut() {
                    c.descriptions
                        .push((get("box").unwrap_or_default(), get("iof-2004-ref")));
                }
            }
            (2, "event", "standards") => s.standard = get("description"),
            (1, _, "course") => s.courses.push(SemCourse {
                path: path.into(),
                id: get("id"),
                first: None,
            }),
            (2, "course", "first") => {
                if let Some(c) = s.courses.last_mut() {
                    c.first = get("course-control");
                }
            }
            (1, _, "course-control") => s.ccs.push(SemCc {
                path: path.into(),
                id: get("id"),
                control: get("control"),
                next: None,
                variations: Vec::new(),
                variation_end: get("variation-end"),
            }),
            (2, "course-control", "next") => {
                if let Some(c) = s.ccs.last_mut() {
                    c.next = get("course-control");
                }
            }
            (2, "course-control", "variation") => {
                if let (Some(c), Some(v)) = (s.ccs.last_mut(), get("course-control")) {
                    c.variations.push(v);
                }
            }
            (1, _, "leg") => s
                .legs
                .push((path.into(), get("start-control"), get("end-control"))),
            (3, "courses", "course") if top == "special-object" => {
                s.special_courses.push((path.into(), get("course")))
            }
            _ => {}
        }
    }

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
        self.collect(&name, &path, &attrs);
        let parent = self.stack.last().map_or("", |f| f.name.as_str());
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

/// What fails after the load: description symbols Purple Pen cannot look up or show, and references that do
/// not resolve. `EventDB.GetControl`, `GetCourseControl`, `GetLeg` and `GetCourse` throw on an id their store
/// does not have; `QueryEvent.EnumCourseControlIds` follows `next` until none, so a cycle never ends; and a
/// course-control in two courses makes an edit of one course change the other (`CourseControl.Validate`).
fn semantics(s: &Sem) -> Vec<String> {
    let mut errors = Vec::new();
    let standard = s.standard.as_deref().unwrap_or("2004");
    let ids = |v: Vec<Option<&String>>| -> BTreeSet<String> {
        v.into_iter().flatten().cloned().collect()
    };
    let controls = ids(s.controls.iter().map(|c| c.id.as_ref()).collect());
    let courses = ids(s.courses.iter().map(|c| c.id.as_ref()).collect());
    let ccs: BTreeMap<&str, &SemCc> = s
        .ccs
        .iter()
        .filter_map(|c| c.id.as_deref().map(|i| (i, c)))
        .collect();

    for c in &s.controls {
        let directive = DIRECTIVES.iter().find(|(k, _, _)| *k == c.kind);
        if let Some((kind, letter, _)) = directive {
            let symbol = c
                .descriptions
                .iter()
                .find(|(b, _)| b == "all" || b == "C")
                .and_then(|(_, r)| r.as_deref());
            match symbol {
                None => errors.push(format!(
                    "{}: {kind} without a description symbol; GetDirectiveLine looks up symbolDB[null]",
                    c.path
                )),
                Some(r) if !symbol_kinds(r, standard).contains(letter) => errors.push(format!(
                    "{}: {kind} with the symbol {r:?}, not a Purple Pen '{letter}' symbol in the {standard} \
                     standard",
                    c.path
                )),
                Some(_) => {}
            }
            continue;
        }
        for (b, r) in &c.descriptions {
            let Some(r) = r else { continue };
            let Some((_, allowed)) = COLUMN_KINDS.iter().find(|(x, _)| x == b) else {
                continue;
            };
            let kinds = symbol_kinds(r, standard);
            if !kinds.iter().any(|k| allowed.contains(k)) {
                errors.push(format!(
                    "{}: description box {b} has the symbol {r:?}, not a Purple Pen symbol of kind {allowed:?} \
                     in the {standard} standard",
                    c.path
                ));
            }
        }
    }

    let dangling = |errors: &mut Vec<String>,
                    path: &str,
                    what: &str,
                    id: &Option<String>,
                    store: &dyn Fn(&str) -> bool| {
        match id {
            None => errors.push(format!("{path}: {what} without an id")),
            Some(i) if !store(i) => errors.push(format!("{path}: {what} {i} does not exist")),
            Some(_) => {}
        }
    };
    let is_control = |i: &str| controls.contains(i);
    let is_course = |i: &str| courses.contains(i);
    let is_cc = |i: &str| ccs.contains_key(i);
    for cc in &s.ccs {
        dangling(&mut errors, &cc.path, "control", &cc.control, &is_control);
        if cc.next.is_some() {
            dangling(
                &mut errors,
                &cc.path,
                "next course-control",
                &cc.next,
                &is_cc,
            );
        }
        if cc.variation_end.is_some() {
            dangling(
                &mut errors,
                &cc.path,
                "variation-end course-control",
                &cc.variation_end,
                &is_cc,
            );
        }
        for v in &cc.variations {
            dangling(
                &mut errors,
                &cc.path,
                "variation course-control",
                &Some(v.clone()),
                &is_cc,
            );
        }
    }
    for c in &s.courses {
        if c.first.is_some() {
            dangling(
                &mut errors,
                &c.path,
                "first course-control",
                &c.first,
                &is_cc,
            );
        }
    }
    let mut pairs = BTreeSet::new();
    for (path, from, to) in &s.legs {
        dangling(&mut errors, path, "start control", from, &is_control);
        dangling(&mut errors, path, "end control", to, &is_control);
        if !pairs.insert((from.clone(), to.clone())) {
            errors.push(format!(
                "{path}: a second leg for the same pair of controls; QueryEvent.FindLeg uses the first"
            ));
        }
    }
    for (path, course) in &s.special_courses {
        dangling(&mut errors, path, "course", course, &is_course);
    }

    // Each course's course-controls: along `next` (and the branches of variations), each in one course only,
    // and a course without variations ends.
    let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
    for c in &s.courses {
        let Some(first) = c.first.as_deref() else {
            continue;
        };
        let mut seen = BTreeSet::new();
        let mut todo = vec![first];
        let mut variations = false;
        let mut cycle = false;
        while let Some(id) = todo.pop() {
            let Some(cc) = ccs.get(id) else { continue };
            if !seen.insert(id) {
                cycle = true;
                continue;
            }
            match owner.get(id) {
                Some(other) if *other != c.path => errors.push(format!(
                    "{}: course-control {id} belongs to {other} too",
                    c.path
                )),
                _ => {
                    owner.insert(id, &c.path);
                }
            }
            variations |= !cc.variations.is_empty() || cc.variation_end.is_some();
            todo.extend(cc.next.as_deref());
            todo.extend(cc.variations.iter().map(String::as_str));
        }
        if cycle && !variations {
            errors.push(format!(
                "{}: the course-controls run in a cycle; QueryEvent.EnumCourseControlIds never ends",
                c.path
            ));
        }
    }
    errors
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
    c.errors.extend(semantics(&c.sem));
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

/// The file Purple Pen 4.0 loaded and then crashed on: freemapper-courseplan 0.7.1's export of its Lübeck
/// sprint example, written from scratch by the 0.4.1 writer (2026-11-10). The finish has no `<description>`.
const V041_SCRATCH: &str = r#"<course-scribe-event>
  <event id="1">
    <title>Sprinttraining Lübecker Altstadt</title>
    <map kind="OCAD" scale="4000" ignore-missing-fonts="false">../../tests/fixtures/maps/luebeck.omap</map>
    <standards map="Spr2019" description="2018" />
    <all-controls print-scale="4000" description-kind="symbols" />
    <course-appearance />
    <descriptions lang="en" />
  </event>
  <control id="1" kind="start">
    <location x="-97.4" y="3.6" />
  </control>
  <control id="2" kind="finish">
    <location x="-95.8" y="-56.4" />
  </control>
  <control id="3" kind="normal">
    <code>31</code>
    <location x="-50.1" y="3.6" />
  </control>
  <control id="4" kind="normal">
    <code>32</code>
    <location x="-47.8" y="56.3" />
  </control>
  <control id="5" kind="normal">
    <code>33</code>
    <location x="-17.8" y="57.9" />
  </control>
  <control id="6" kind="normal">
    <code>34</code>
    <location x="-1.2" y="12.4" />
  </control>
  <control id="7" kind="normal">
    <code>35</code>
    <location x="-28.2" y="-10.2" />
  </control>
  <control id="8" kind="normal">
    <code>36</code>
    <location x="-52.4" y="-47.6" />
  </control>
  <control id="9" kind="normal">
    <code>37</code>
    <location x="-88.2" y="-32.9" />
  </control>
  <course id="1" kind="normal" order="1">
    <name>A</name>
    <first course-control="1" />
    <options print-scale="4000" climb="5" description-kind="symbols" />
  </course>
  <course-control id="1" control="1">
    <next course-control="2" />
  </course-control>
  <course-control id="2" control="3">
    <next course-control="3" />
    <number-location x="4.875" y="0" />
  </course-control>
  <course-control id="3" control="4">
    <next course-control="4" />
    <number-location x="0.24777585" y="5.675" />
  </course-control>
  <course-control id="4" control="5">
    <next course-control="5" />
    <number-location x="4.875" y="0.26402056" />
  </course-control>
  <course-control id="5" control="6">
    <next course-control="6" />
    <number-location x="2.0308948" y="-5.579838" />
  </course-control>
  <course-control id="6" control="7">
    <next course-control="7" />
    <number-location x="-4.3672853" y="3.8638477" />
  </course-control>
  <course-control id="7" control="8">
    <next course-control="8" />
    <number-location x="-3.2618735" y="-5.0420885" />
  </course-control>
  <course-control id="8" control="9">
    <next course-control="9" />
    <number-location x="-4.875" y="1.9993832" />
  </course-control>
  <course-control id="9" control="2" />
</course-scribe-event>"#;

const V041_FINISH: &str =
    "<control id=\"2\" kind=\"finish\">\n    <location x=\"-95.8\" y=\"-56.4\" />\n  </control>";

/// The finish of [`V041_SCRATCH`] with `description` as its one description element.
fn v041_finish_with(description: &str) -> String {
    format!(
        "<control id=\"2\" kind=\"finish\">\n    <location x=\"-95.8\" y=\"-56.4\" />\n    {description}\n  </control>"
    )
}

/// The check catches the file Purple Pen 4.0 crashed on, and the other ways a file fails after the load: a
/// symbol Purple Pen cannot look up or does not offer for the column, and references that do not resolve.
#[test]
fn check_refuses_what_fails_after_the_load() {
    assert_refused(
        "0.4.1 scratch export",
        V041_SCRATCH,
        "control[id=2]: finish without a description symbol",
    );
    let lincoln = fixture("lincoln-park.ppen");
    let overlapper = fixture("overlapper.ppen");
    let finish = "<description box=\"all\" iof-2004-ref=\"14.3\" />";
    let crossing = "<description box=\"all\" iof-2004-ref=\"13.3\" />";
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        (
            "finish without description",
            &lincoln,
            finish,
            "",
            "finish without a description symbol",
        ),
        (
            "finish box without symbol",
            &lincoln,
            finish,
            "<description box=\"all\" />",
            "finish without a description symbol",
        ),
        (
            "finish with a crossing symbol",
            &lincoln,
            finish,
            crossing,
            "finish with the symbol \"13.3\"",
        ),
        (
            "finish with an unknown symbol",
            &lincoln,
            finish,
            "<description box=\"all\" iof-2004-ref=\"14.9\" />",
            "finish with the symbol \"14.9\"",
        ),
        (
            "crossing point without description",
            &overlapper,
            crossing,
            "",
            "crossing-point without a description symbol",
        ),
        (
            "unknown symbol",
            &lincoln,
            "iof-2004-ref=\"1.10\"",
            "iof-2004-ref=\"1.99\"",
            "description box D has the symbol \"1.99\"",
        ),
        (
            "symbol of the other standard",
            &lincoln,
            "<description box=\"D\" iof-2004-ref=\"5.2\" />",
            "<description box=\"D\" iof-2004-ref=\"5.25\" />",
            "description box D has the symbol \"5.25\"",
        ),
        (
            "combination symbol in column G",
            &lincoln,
            "<description box=\"F\" iof-2004-ref=\"10.2\" />",
            "<description box=\"G\" iof-2004-ref=\"10.2\" />",
            "description box G has the symbol \"10.2\"",
        ),
        (
            "course-control of a missing control",
            &lincoln,
            "<course-control id=\"2\" control=\"2\">",
            "<course-control id=\"2\" control=\"99\">",
            "control 99 does not exist",
        ),
        (
            "next of a missing course-control",
            &lincoln,
            "<next course-control=\"2\" />",
            "<next course-control=\"92\" />",
            "next course-control 92 does not exist",
        ),
        (
            "first of a missing course-control",
            &lincoln,
            "<first course-control=\"1\" />",
            "<first course-control=\"91\" />",
            "first course-control 91 does not exist",
        ),
        (
            "leg to a missing control",
            &lincoln,
            "<leg id=\"1\" start-control=\"3\" end-control=\"5\">",
            "<leg id=\"1\" start-control=\"3\" end-control=\"55\">",
            "end control 55 does not exist",
        ),
        (
            "two legs for one pair",
            &lincoln,
            "</leg>\r\n</course-scribe-event>",
            "</leg>\r\n  <leg id=\"2\" start-control=\"3\" end-control=\"5\" />\r\n</course-scribe-event>",
            "a second leg",
        ),
        (
            "course-controls in a cycle",
            &lincoln,
            "<course-control id=\"6\" control=\"5\" />",
            "<course-control id=\"6\" control=\"5\">\r\n    <next course-control=\"1\" />\r\n  </course-control>",
            "run in a cycle",
        ),
        (
            "course-control in two courses",
            &overlapper,
            "<first course-control=\"13\" />",
            "<first course-control=\"1\" />",
            "belongs to",
        ),
        (
            "special object for a missing course",
            &overlapper,
            "<courses all=\"true\" />",
            "<courses>\r\n      <course course=\"9\" />\r\n    </courses>",
            "course 9 does not exist",
        ),
    ];
    for (what, base, from, to, needle) in cases {
        let changed = base.replacen(from, to, 1);
        assert_ne!(&changed, base, "{what}: no change");
        assert_refused(what, &changed, needle);
    }
}

fn description(standard: DescriptionStandard, columns: [Option<&str>; 6]) -> Description {
    Description {
        columns: columns.map(|c| c.map(str::to_string)),
        standard: standard.as_str().into(),
        column_f_text: None,
    }
}

/// A finish, crossing point or map issue point is written with its symbol, and with Purple Pen's symbol for a
/// new control when the event has none: 14.3 (navigate to the finish) and 13.3 (mandatory crossing point).
/// A symbol the event holds is never replaced.
#[test]
fn scratch_writes_a_symbol_for_every_finish_and_crossing_point() {
    let o = options(PpenMap::None, DescriptionStandard::Iof2018);
    let text = scratch(&full_event(), &o);
    assert_loads("full event", &text);
    assert!(
        text.contains("<description box=\"all\" iof-2004-ref=\"14.3\" />"),
        "{text}"
    );
    assert!(
        text.contains("<description box=\"all\" iof-2004-ref=\"13.3\" />"),
        "{text}"
    );

    let mut ev = full_event();
    let std = DescriptionStandard::Iof2018;
    for c in &mut ev.controls {
        c.description = match c.kind {
            ControlKind::Finish => Some(description(
                std,
                [Some("14.2"), None, None, None, None, None],
            )),
            ControlKind::CrossingPoint => Some(description(
                std,
                [Some("13.4"), None, None, None, None, None],
            )),
            _ => None,
        };
    }
    let text = scratch(&ev, &o);
    assert_loads("own symbols", &text);
    assert!(text.contains("iof-2004-ref=\"14.2\""), "{text}");
    assert!(text.contains("iof-2004-ref=\"13.4\""), "{text}");
    assert!(!text.contains("iof-2004-ref=\"14.3\""), "{text}");
    assert!(!text.contains("iof-2004-ref=\"13.3\""), "{text}");
}

/// Descriptions of starts and normal controls in both standards, with a symbol of each column, load.
#[test]
fn scratch_descriptions_of_both_standards_load() {
    for (std, d_only) in [
        (DescriptionStandard::Iof2004, "12.3"),
        (DescriptionStandard::Iof2018, "12.4"),
    ] {
        let mut ev = full_event();
        for c in &mut ev.controls {
            c.description = match c.code.as_str() {
                "S1" => Some(description(
                    std,
                    [None, Some("1.10"), None, None, None, None],
                )),
                "31" => Some(description(
                    std,
                    [
                        Some("0.1N"),
                        Some("5.2"),
                        Some("8.1"),
                        Some("10.1"),
                        Some("11.1NE"),
                        Some(d_only),
                    ],
                )),
                "32" => Some(description(
                    std,
                    [
                        None,
                        Some("5.2"),
                        Some("5.2"),
                        Some("10.2"),
                        Some("11.15"),
                        None,
                    ],
                )),
                _ => None,
            };
        }
        let text = scratch(&ev, &options(PpenMap::None, std));
        assert_loads(std.as_str(), &text);
    }
}

/// What makes Purple Pen throw after the load is refused: a finish, crossing point or map issue point symbol
/// Purple Pen does not offer for the kind, and a combination symbol in column C or G, whose two placeholders
/// `Textifier` fills with one argument.
#[test]
fn scratch_refuses_symbols_purple_pen_throws_on() {
    let std = DescriptionStandard::Iof2018;
    let o = options(PpenMap::None, std);
    let one = |id: &str| description(std, [Some(id), None, None, None, None, None]);
    let cases: &[(&str, ControlKind, Description)] = &[
        ("finish with 13.3", ControlKind::Finish, one("13.3")),
        (
            "finish with an unknown id",
            ControlKind::Finish,
            one("14.9"),
        ),
        (
            "crossing point with 14.3",
            ControlKind::CrossingPoint,
            one("14.3"),
        ),
        ("combination in column C", ControlKind::Control, one("10.1")),
        (
            "combination in column G",
            ControlKind::Control,
            description(std, [None, Some("1.1"), None, None, Some("10.2"), None]),
        ),
        (
            "one-sided between in column G",
            ControlKind::Control,
            description(
                std,
                [None, Some("1.1"), None, None, Some("11.15single"), None],
            ),
        ),
        (
            "between in column C",
            ControlKind::Start,
            description(std, [Some("11.15"), None, None, None, None, None]),
        ),
    ];
    for (what, kind, d) in cases {
        let mut ev = full_event();
        let c = ev.controls.iter_mut().find(|c| c.kind == *kind).unwrap();
        c.description = Some(d.clone());
        match write_ppen(&ev, &o) {
            Ok(w) => panic!("{what}: written\n{}", w.text),
            Err(e) => assert!(e.to_string().contains("symbol"), "{what}: {e}"),
        }
    }
}

/// A patch repairs a finish, crossing point or map issue point without a symbol, also one the event does not
/// mention, and keeps a symbol the base file has when the event has none. A symbol of the event wins.
#[test]
fn patch_repairs_symbols_of_the_0_4_1_writer() {
    // The file Purple Pen crashed on, patched with the event read from it: only the finish changes.
    let ev = read(V041_SCRATCH);
    let text = patch(V041_SCRATCH, &ev);
    assert_loads("0.4.1 base, unchanged event", &text);
    let repaired = v041_finish_with("<description box=\"all\" iof-2004-ref=\"14.3\" />");
    assert_eq!(text, V041_SCRATCH.replace(V041_FINISH, &repaired));

    // The base's own symbol stays when the event has none ...
    let flagged = v041_finish_with("<description box=\"all\" iof-2004-ref=\"14.1\" />");
    let base = V041_SCRATCH.replace(V041_FINISH, &flagged);
    // (The reader reads the leg into a 14.1 finish as a marked route; the event says nothing of it.)
    let mut ev = read(&base);
    for c in &mut ev.controls {
        c.description = None;
    }
    for l in ev.courses.iter_mut().flat_map(|c| c.legs.iter_mut()) {
        l.marked_route = false;
    }
    let text = patch(&base, &ev);
    assert_loads("base symbol", &text);
    assert_eq!(text, base);
    // ... and the event's symbol replaces it.
    let finish = ev
        .controls
        .iter_mut()
        .find(|c| c.kind == ControlKind::Finish)
        .unwrap();
    finish.description = Some(description(
        DescriptionStandard::Iof2018,
        [Some("14.2"), None, None, None, None, None],
    ));
    let text = patch(&base, &ev);
    assert_loads("event symbol", &text);
    assert_eq!(
        text,
        V041_SCRATCH.replace(
            V041_FINISH,
            &v041_finish_with("<description box=\"all\" iof-2004-ref=\"14.2\" />")
        )
    );

    // A finish and a crossing point the event does not mention, and a map issue point (read as the start).
    let extra = format!(
        "{V041_FINISH}\n  <control id=\"10\" kind=\"finish\">\n    <location x=\"1\" y=\"2\" />\n  </control>\n  \
         <control id=\"11\" kind=\"crossing-point\" orientation=\"0\">\n    <location x=\"3\" y=\"4\" />\n  </control>"
    );
    let base = V041_SCRATCH.replace(V041_FINISH, &extra).replace(
        "<control id=\"1\" kind=\"start\">",
        "<control id=\"1\" kind=\"map-issue\">",
    );
    let mut ev = read(&base);
    ev.controls.retain(|c| c.id != Some(10) && c.id != Some(11));
    let written = write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()).unwrap();
    assert_eq!(written.kept.len(), 2, "{:?}", written.kept);
    assert_loads("kept controls", &written.text);
    for (id, symbol) in [("1", "13.6"), ("2", "14.3"), ("10", "14.3"), ("11", "13.3")] {
        let at = written.text.find(&format!("<control id=\"{id}\"")).unwrap();
        let end = at + written.text[at..].find("</control>").unwrap();
        assert!(
            written.text[at..end].contains(&format!(
                "<description box=\"all\" iof-2004-ref=\"{symbol}\" />"
            )),
            "control {id}:\n{}",
            written.text
        );
    }
}

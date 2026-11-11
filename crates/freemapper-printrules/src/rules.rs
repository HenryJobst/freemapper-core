//! Print rules as data profiles (ADR 0031): the symbol size table, north-line spacing, course symbol
//! sizes, the course colour and the other print values, each with its source.
//!
//! A rule profile is a TOML file in `rules/`, embedded with `include_str!`. A base profile (such as
//! `iof-print`) holds a full rule set; an overlay names its base and adds rules, replaces base rules
//! with the same `id`, or switches them off with `enabled = false`. `rules/README.md` describes the
//! file format.
//!
//! The format, the parser, the merge and the checks of ids, units and ranges live in the shared
//! crate `freemapper-rules` (freemapper-core ADR 0011). This module holds what is specific to
//! freemapper-print: the embedded profiles, the [`SCHEMA`] with the rule keys and the values each
//! reads, the scale table check and the description cell range, and the [`RuleSet`] that resolves
//! every value for every map kind when it is loaded and checks that each map kind gets each value
//! exactly once. Every value holds at the base scale of its map kind, so a `print_scales` filter is
//! read at the base scale (sprint 1:4000, forest 1:15000, schoolyard 1:1000).

use std::ops::Deref;
use std::sync::OnceLock;

use freemapper_rules::toml::Value;
pub use freemapper_rules::{
    FORMAT, FilterArg, FilterItem, FilterKind, FilterSchema, KeySchema, LocalizedText, ProfileKind,
    Rule, RuleProfile, RulesError, Schema, SetRule, Severity, Source, SourceRef, SwitchedOff,
    TableSchema, value_number,
};

use crate::scale::MapKind;
use freemapper_course::placement::GapRange;

/// Smallest side of a description cell in mm.
pub const MIN_CELL_MM: f64 = 4.0;
/// Largest side of a description cell in mm.
pub const MAX_CELL_MM: f64 = 10.0;

/// The base profile a project uses unless it names another (`[print] rule_profile`).
pub const BASE_PROFILE: &str = "iof-print";

/// The embedded profiles: profile id and content.
const EMBEDDED: &[(&str, &str)] = &[("iof-print", include_str!("../rules/iof-print.toml"))];

/// The filter field of the map kinds a rule applies to.
pub const FIELD_MAP_KINDS: &str = "map_kinds";
/// The filter field of the print scale denominators a rule applies to.
pub const FIELD_PRINT_SCALES: &str = "print_scales";

/// The value names of `course_symbol` rules, one per field of [`CourseSymbolSizes`].
const COURSE_SYMBOL_VALUES: [&str; 18] = [
    "circle_diameter_mm",
    "line_width_mm",
    "start_side_mm",
    "finish_inner_mm",
    "finish_outer_mm",
    "number_height_mm",
    "marked_route_dash_mm",
    "marked_route_gap_mm",
    "out_of_bounds_line_mm",
    "crossing_point_mm",
    "out_of_bounds_hatch_angle_degrees",
    "out_of_bounds_hatch_second_angle_degrees",
    "out_of_bounds_hatch_spacing_mm",
    "out_of_bounds_hatch_line_mm",
    "first_aid_mm",
    "first_aid_bar_mm",
    "refreshment_mm",
    "refreshment_line_mm",
];

/// The value names of `course_colour` rules.
const COLOUR_VALUES: [&str; 4] = [
    "cyan_percent",
    "magenta_percent",
    "yellow_percent",
    "black_percent",
];

/// The filters a key whose values hold for every map kind must not carry.
const KIND_INDEPENDENT: &[&str] = &[FIELD_MAP_KINDS, FIELD_PRINT_SCALES];

/// Units a numeric value name ends in (`metres`, `spacing_metres`, `cyan_percent`).
pub const UNITS: &[&str] = &["metres", "mm", "degrees", "percent", "factor"];

/// The document key of values freemapper-print chose itself where no standard gives a number
/// (`[sources.fmp]` in a profile). Such a rule needs a note.
pub const PROPOSAL_DOC: &str = "fmp";

/// What the print profiles may contain: the rule keys this program reads, each with the value names
/// a rule of that key may give; the units; the one table value; the filters; and the checks of the
/// scale table and of single values.
pub static SCHEMA: Schema = Schema {
    keys: &[
        KeySchema {
            key: "scale_table",
            values: Some(&["rows"]),
            without_filters: &[],
        },
        KeySchema {
            key: "north_lines",
            values: Some(&["spacing_metres"]),
            without_filters: &[],
        },
        KeySchema {
            key: "contour_interval",
            values: Some(&["proposed_metres"]),
            without_filters: &[],
        },
        KeySchema {
            key: "course_symbol",
            values: Some(&COURSE_SYMBOL_VALUES),
            without_filters: &[],
        },
        KeySchema {
            key: "description_box",
            values: Some(&["cell_mm"]),
            without_filters: &[],
        },
        KeySchema {
            key: "course_colour",
            values: Some(&COLOUR_VALUES),
            without_filters: KIND_INDEPENDENT,
        },
        KeySchema {
            key: "placement_weights",
            values: Some(&[
                "black_factor",
                "brown_factor",
                "blue_factor",
                "other_factor",
            ]),
            without_filters: KIND_INDEPENDENT,
        },
        KeySchema {
            key: "number_gap",
            values: Some(&["min_gap_mm", "max_gap_mm"]),
            without_filters: KIND_INDEPENDENT,
        },
    ],
    units: UNITS,
    tables: &[TableSchema {
        key: "scale_table",
        value: "rows",
        shape: "[{ scale, size_factor, standard }, ...]: one row per scale denominator a map of the \
                kind may be drawn and printed at, ascending; size_factor is the symbol size relative \
                to the base scale, standard = false marks a reduction the standard does not foresee \
                (default true)",
        check: Some(check_scale_rows),
    }],
    filters: &[
        FilterSchema {
            field: FIELD_MAP_KINDS,
            kind: FilterKind::Text(&["sprint", "schoolyard", "forest"]),
        },
        FilterSchema {
            field: FIELD_PRINT_SCALES,
            kind: FilterKind::Int {
                min: 1,
                max: u32::MAX as i64,
            },
        },
    ],
    proposal_doc: PROPOSAL_DOC,
    value_check: Some(check_value),
};

pub use freemapper_render::overprint::CourseSymbolSizes;

/// How much a mm² of map detail under a control number counts, by colour class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacementWeights {
    pub black: f64,
    pub brown: f64,
    pub blue: f64,
    pub other: f64,
}

/// One row of the symbol size table (ADR 0018).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleEntry {
    /// Scale denominator.
    pub scale: u32,
    /// Symbol size relative to the base scale of the kind.
    pub size_factor: f64,
    /// The standard foresees this scale; `false` for a reduction below the base scale.
    pub standard: bool,
}

/// The values of one map kind, resolved when the set is loaded.
#[derive(Debug, Clone, PartialEq)]
struct KindValues {
    scales: Vec<ScaleEntry>,
    north_line_spacing_m: f64,
    contour_interval_m: Option<f64>,
    course: CourseSymbolSizes,
    description_cell_mm: f64,
}

/// A base profile merged with its overlays: the print rules a sheet uses.
///
/// It wraps the merged set of `freemapper-rules` (its rules, switched-off rules and sources, reached
/// through `Deref`) and adds the values resolved per map kind.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleSet {
    set: freemapper_rules::RuleSet,
    /// Per map kind, in the order of [`MapKind::ALL`].
    kinds: Vec<KindValues>,
    purple_cmyk: [f64; 4],
    placement_weights: PlacementWeights,
    number_gap: Option<(f64, f64)>,
    number_gap_allowed: Option<(f64, f64)>,
}

impl Deref for RuleSet {
    type Target = freemapper_rules::RuleSet;

    fn deref(&self) -> &Self::Target {
        &self.set
    }
}

impl RuleSet {
    fn kind(&self, kind: MapKind) -> &KindValues {
        let slot = match kind {
            MapKind::Sprint => 0,
            MapKind::Schoolyard => 1,
            MapKind::Forest => 2,
        };
        &self.kinds[slot]
    }

    /// Course symbol sizes in mm at the base scale of `kind`.
    pub fn course_symbol(&self, kind: MapKind) -> CourseSymbolSizes {
        self.kind(kind).course
    }

    /// Ground distance (m) between north lines (ADR 0027).
    pub fn north_line_spacing_m(&self, kind: MapKind) -> f64 {
        self.kind(kind).north_line_spacing_m
    }

    /// The contour interval (m) `init` proposes, if the set has one for `kind`.
    pub fn proposed_contour_interval_m(&self, kind: MapKind) -> Option<f64> {
        self.kind(kind).contour_interval_m
    }

    /// Side (mm) of a control description cell printed on the map.
    pub fn description_cell_mm(&self, kind: MapKind) -> f64 {
        self.kind(kind).description_cell_mm
    }

    /// The course colour as CMYK fractions from 0 to 1.
    pub fn purple_cmyk(&self) -> [f64; 4] {
        self.purple_cmyk
    }

    /// Weights of map detail under a control number.
    pub fn placement_weights(&self) -> PlacementWeights {
        self.placement_weights
    }

    /// The smallest and the largest gap (print mm at the base scale) between a control circle and its number,
    /// from the active `number_gap` rules (ruling R29). With several rules the strictest wins: the largest
    /// minimum and the smallest maximum, normalised as [`GapRange::from_limits`] does. `None` without such a
    /// rule; placement then uses a fixed gap.
    pub fn number_gap(&self) -> Option<(f64, f64)> {
        self.number_gap
    }

    /// The gaps the check allows under the active `number_gap` rules: as [`Self::number_gap`], but a side no rule
    /// gives stays open ([`GapRange::allowed`]). `None` without such a rule.
    pub fn number_gap_allowed(&self) -> Option<(f64, f64)> {
        self.number_gap_allowed
    }

    /// The symbol size table of `kind`: scale denominator and symbol size relative to the base
    /// scale, ascending by scale.
    pub fn scale_table(&self, kind: MapKind) -> Vec<(u32, f64)> {
        self.kind(kind)
            .scales
            .iter()
            .map(|e| (e.scale, e.size_factor))
            .collect()
    }

    /// The rows of the symbol size table of `kind`, ascending by scale.
    pub fn scale_entries(&self, kind: MapKind) -> &[ScaleEntry] {
        &self.kind(kind).scales
    }

    /// How large course symbols print at 1:`print_scale`, relative to the values of [`Self::course_symbol`]
    /// (ADR 0018, ruling R35): the size factor of the print scale's row of the symbol size table, as the standard
    /// sets course symbols for that scale (ISOM 2017-2 section 3.7, ISSprOM 2019-2 section 4.7). It does not
    /// depend on the scale the map is drawn at. 1.0 for a scale outside the table.
    pub fn course_factor(&self, kind: MapKind, print_scale: u32) -> f64 {
        self.scale_entry(kind, print_scale)
            .map_or(1.0, |e| e.size_factor)
    }

    /// The row of the symbol size table for `kind` at 1:`scale`.
    pub fn scale_entry(&self, kind: MapKind, scale: u32) -> Option<ScaleEntry> {
        self.kind(kind)
            .scales
            .iter()
            .find(|e| e.scale == scale)
            .copied()
    }

    /// Whether the standard foresees printing a map of `kind` at 1:`scale`.
    pub fn is_standard_scale(&self, kind: MapKind, scale: u32) -> bool {
        self.scale_entry(kind, scale).is_some_and(|e| e.standard)
    }
}

/// Why a rule profile or rule set cannot be loaded. The variants up to `UnknownRule` are those of
/// [`RulesError`] with the same texts; `Incomplete` is this program's own.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    #[error("unknown rule profile {id}; available: {available}")]
    UnknownProfile { id: String, available: String },
    #[error("rule profile {0} is an overlay, not a base profile")]
    NotABase(String),
    #[error("rule profile {0} is a base profile, not an overlay")]
    NotAnOverlay(String),
    #[error("overlay {overlay} extends {extends}, not {base}")]
    OverlayForAnotherBase {
        overlay: String,
        extends: String,
        base: String,
    },
    #[error("rule profile {profile} is malformed: {message}")]
    Malformed { profile: String, message: String },
    #[error("rule {rule} in profile {profile} has the unknown key {key}")]
    UnknownKey {
        profile: String,
        rule: String,
        key: String,
    },
    #[error("overlay {overlay} switches off rule {rule}, which the base profile does not have")]
    UnknownRule { overlay: String, rule: String },
    #[error("rule set {set} cannot be used: {message}")]
    Incomplete { set: String, message: String },
}

impl RuleError {
    /// The error of `freemapper-rules` as this program's. `set` names the rule set for
    /// `Ambiguous`, which only [`freemapper_rules::RuleSet::pick`] returns and which makes the set
    /// unusable.
    fn from_core(err: RulesError, set: &str) -> RuleError {
        match err {
            RulesError::UnknownProfile { id, available } => {
                RuleError::UnknownProfile { id, available }
            }
            RulesError::NotABase(id) => RuleError::NotABase(id),
            RulesError::NotAnOverlay(id) => RuleError::NotAnOverlay(id),
            RulesError::OverlayForAnotherBase {
                overlay,
                extends,
                base,
            } => RuleError::OverlayForAnotherBase {
                overlay,
                extends,
                base,
            },
            RulesError::Malformed { profile, message } => RuleError::Malformed { profile, message },
            RulesError::UnknownKey { profile, rule, key } => {
                RuleError::UnknownKey { profile, rule, key }
            }
            RulesError::UnknownRule { overlay, rule } => RuleError::UnknownRule { overlay, rule },
            ambiguous @ RulesError::Ambiguous { .. } => RuleError::Incomplete {
                set: set.to_owned(),
                message: ambiguous_text(&ambiguous, None),
            },
        }
    }
}

/// The ids of the embedded profiles, base profiles and overlays.
pub fn available() -> Vec<&'static str> {
    freemapper_rules::available(EMBEDDED)
}

/// The embedded profile `id`. `id` also goes to `from_core` as the set name for `Ambiguous`, which
/// these calls can never return.
pub fn profile(id: &str) -> Result<RuleProfile, RuleError> {
    freemapper_rules::profile(&SCHEMA, EMBEDDED, id).map_err(|e| RuleError::from_core(e, id))
}

/// Parses one profile file embedded as `id` against [`SCHEMA`]. `id` also goes to `from_core` as the
/// set name for `Ambiguous`, which this call can never return.
pub fn parse_profile(id: &str, text: &str) -> Result<RuleProfile, RuleError> {
    freemapper_rules::parse_profile(&SCHEMA, id, text).map_err(|e| RuleError::from_core(e, id))
}

/// Value names of `rule` that carry no unit and are no table value of its key (see
/// [`freemapper_rules::value_name_problems`]).
pub fn value_name_problems(rule: &Rule) -> Vec<String> {
    freemapper_rules::value_name_problems(&SCHEMA, rule)
}

/// The filter arguments of a map of `kind` printed at 1:`print_scale`.
fn filter_args(kind: MapKind, print_scale: u32) -> [(&'static str, FilterArg<'static>); 2] {
    [
        (FIELD_MAP_KINDS, FilterArg::Text(kind.name())),
        (FIELD_PRINT_SCALES, FilterArg::Int(i64::from(print_scale))),
    ]
}

/// Whether the filters of `rule` select a map of `kind` printed at 1:`print_scale`.
pub fn applies(rule: &Rule, kind: MapKind, print_scale: u32) -> bool {
    rule.applies(&filter_args(kind, print_scale))
}

/// The embedded base profile `iof-print` without overlays, loaded once.
pub fn base() -> &'static RuleSet {
    static BASE: OnceLock<RuleSet> = OnceLock::new();
    BASE.get_or_init(|| match load_rule_set(BASE_PROFILE, &[]) {
        Ok(set) => set,
        // The tests load every embedded profile, so this is a build mistake.
        Err(e) => panic!("the embedded rule profile {BASE_PROFILE} is invalid: {e}"),
    })
}

/// Merges the embedded base profile `base` with the overlays, in order.
pub fn load_rule_set(base: &str, overlays: &[String]) -> Result<RuleSet, RuleError> {
    load_from(EMBEDDED, base, overlays)
}

/// [`load_from`] for tests of an app, with test-only profiles.
#[cfg(feature = "test-support")]
pub fn load_for_tests(
    files: &[(&str, &str)],
    base: &str,
    overlays: &[String],
) -> Result<RuleSet, RuleError> {
    load_from(files, base, overlays)
}

/// The text of the embedded profile `id`, for tests of an app.
#[cfg(feature = "test-support")]
pub fn embedded_text(id: &str) -> &'static str {
    EMBEDDED
        .iter()
        .find(|(stem, _)| *stem == id)
        .map_or("", |(_, text)| text)
}

fn load_from(
    files: &[(&str, &str)],
    base: &str,
    overlays: &[String],
) -> Result<RuleSet, RuleError> {
    let set_name = std::iter::once(base)
        .chain(overlays.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" + ");
    let set = freemapper_rules::load_rule_set(&SCHEMA, files, base, overlays)
        .map_err(|e| RuleError::from_core(e, &set_name))?;
    let incomplete = |message: String| RuleError::Incomplete {
        set: set_name.clone(),
        message,
    };
    let mut kinds = Vec::new();
    for kind in MapKind::ALL {
        kinds.push(resolve_kind(&set, kind).map_err(incomplete)?);
    }
    let colour = |name| needed(&set, "course_colour", name, None);
    let mut purple_cmyk = [0.0; 4];
    for (slot, name) in purple_cmyk.iter_mut().zip(COLOUR_VALUES) {
        *slot = colour(name).map_err(incomplete)? / 100.0;
    }
    let weight = |name| needed(&set, "placement_weights", name, None);
    let placement_weights = PlacementWeights {
        black: weight("black_factor").map_err(incomplete)?,
        brown: weight("brown_factor").map_err(incomplete)?,
        blue: weight("blue_factor").map_err(incomplete)?,
        other: weight("other_factor").map_err(incomplete)?,
    };
    let number_gap = gap_limits(&set, GapRange::from_limits);
    let number_gap_allowed = gap_limits(&set, GapRange::allowed);
    Ok(RuleSet {
        set,
        kinds,
        purple_cmyk,
        placement_weights,
        number_gap,
        number_gap_allowed,
    })
}

/// The strictest range of the active `number_gap` rules: the largest `min_gap_mm` and the smallest
/// `max_gap_mm`, normalised by `range` ([`GapRange::from_limits`] for placement, [`GapRange::allowed`] for the
/// check; ruling R30, courseplan ADR 0032). A rule with neither value is skipped; `None` when no rule gives one.
/// Values that are not finite or not above 0 never get here: the profile loader refuses them.
fn gap_limits(
    set: &freemapper_rules::RuleSet,
    range: fn(Option<f64>, Option<f64>) -> GapRange,
) -> Option<(f64, f64)> {
    let values = |name: &str| -> Vec<f64> {
        set.rules
            .iter()
            .filter(|r| r.rule.key == "number_gap")
            .filter_map(|r| value_number(&r.rule, name))
            .collect()
    };
    let (mins, maxs) = (values("min_gap_mm"), values("max_gap_mm"));
    if mins.is_empty() && maxs.is_empty() {
        return None;
    }
    let range = range(
        mins.into_iter().reduce(f64::max),
        maxs.into_iter().reduce(f64::min),
    );
    Some((range.min, range.max))
}

/// The one active rule with `key` that gives `name` for maps of `kind` at their base scale (any
/// rule of the key when `kind` is `None`; such keys take no filter). Two such rules are an error that
/// names them and the map kind.
fn pick<'a>(
    set: &'a freemapper_rules::RuleSet,
    key: &str,
    name: &str,
    kind: Option<MapKind>,
) -> Result<Option<&'a SetRule>, String> {
    let picked = match kind {
        Some(k) => set.pick(key, name, &filter_args(k, k.base_scale())),
        None => set.pick(key, name, &[]),
    };
    picked.map_err(|e| ambiguous_text(&e, kind))
}

/// The text of an error of [`freemapper_rules::RuleSet::pick`]: for `Ambiguous`, which rules give
/// the value, with the map kind.
fn ambiguous_text(err: &RulesError, kind: Option<MapKind>) -> String {
    match err {
        RulesError::Ambiguous { key, value, rules } => {
            let (last, rest) = rules.split_last().map_or(("", &[][..]), |(l, r)| (l, r));
            let all = if rest.len() == 1 { "both" } else { "all" };
            format!(
                "rules {} and {last} {all} give {key} {value}{}",
                rest.join(", "),
                for_kind(kind)
            )
        }
        other => other.to_string(),
    }
}

fn for_kind(kind: Option<MapKind>) -> String {
    kind.map_or(String::new(), |k| format!(" for {} maps", k.name()))
}

/// The number `pick` finds, or `None` without a rule.
fn optional(
    set: &freemapper_rules::RuleSet,
    key: &str,
    name: &str,
    kind: Option<MapKind>,
) -> Result<Option<f64>, String> {
    Ok(pick(set, key, name, kind)?.and_then(|r| value_number(&r.rule, name)))
}

/// The number `pick` finds; an error without a rule.
fn needed(
    set: &freemapper_rules::RuleSet,
    key: &str,
    name: &str,
    kind: Option<MapKind>,
) -> Result<f64, String> {
    optional(set, key, name, kind)?
        .ok_or_else(|| format!("no rule gives {key} {name}{}", for_kind(kind)))
}

fn resolve_kind(set: &freemapper_rules::RuleSet, kind: MapKind) -> Result<KindValues, String> {
    let k = Some(kind);
    let rows = pick(set, "scale_table", "rows", k)?
        .and_then(|r| r.rule.values.get("rows"))
        .ok_or_else(|| format!("no rule gives scale_table rows{}", for_kind(k)))?;
    // The schema's table check passed when the profile was parsed: the rows have their shape and
    // ascend by scale.
    let scales = scale_rows(rows)?;
    // The base scale is where every other value holds, and `scale.rs` refers to it.
    let base = kind.base_scale();
    let at_base = scales
        .iter()
        .any(|e| e.scale == base && e.size_factor == 1.0 && e.standard);
    if !at_base {
        return Err(format!(
            "the scale_table rows{} have no row {{ scale = {base}, size_factor = 1.0 }} for the \
             base scale",
            for_kind(k)
        ));
    }
    let course = |name| needed(set, "course_symbol", name, k);
    Ok(KindValues {
        scales,
        north_line_spacing_m: needed(set, "north_lines", "spacing_metres", k)?,
        contour_interval_m: optional(set, "contour_interval", "proposed_metres", k)?,
        course: CourseSymbolSizes {
            circle_diameter_mm: course("circle_diameter_mm")?,
            line_width_mm: course("line_width_mm")?,
            start_side_mm: course("start_side_mm")?,
            finish_inner_mm: course("finish_inner_mm")?,
            finish_outer_mm: course("finish_outer_mm")?,
            number_height_mm: course("number_height_mm")?,
            marked_route_dash_mm: course("marked_route_dash_mm")?,
            marked_route_gap_mm: course("marked_route_gap_mm")?,
            out_of_bounds_line_mm: course("out_of_bounds_line_mm")?,
            crossing_point_mm: course("crossing_point_mm")?,
            out_of_bounds_hatch_angle_degrees: course("out_of_bounds_hatch_angle_degrees")?,
            out_of_bounds_hatch_second_angle_degrees: optional(
                set,
                "course_symbol",
                "out_of_bounds_hatch_second_angle_degrees",
                k,
            )?,
            out_of_bounds_hatch_spacing_mm: course("out_of_bounds_hatch_spacing_mm")?,
            out_of_bounds_hatch_line_mm: course("out_of_bounds_hatch_line_mm")?,
            first_aid_mm: course("first_aid_mm")?,
            first_aid_bar_mm: course("first_aid_bar_mm")?,
            refreshment_mm: course("refreshment_mm")?,
            refreshment_line_mm: course("refreshment_line_mm")?,
        },
        description_cell_mm: needed(set, "description_box", "cell_mm", k)?,
    })
}

/// The schema's check of a `scale_table` `rows` value: the shape of each row and ascending scales.
fn check_scale_rows(rows: &Value) -> Result<(), String> {
    let scales = scale_rows(rows)?;
    if scales.windows(2).any(|w| w[0].scale >= w[1].scale) {
        return Err("the rows are not in ascending order of scale".to_owned());
    }
    Ok(())
}

/// The rows of a `scale_table` value; an error names the first row that is not
/// `{ scale, size_factor, standard }`.
fn scale_rows(rows: &Value) -> Result<Vec<ScaleEntry>, String> {
    let Value::Array(rows) = rows else {
        return Err("rows is not a list of { scale, size_factor }".to_owned());
    };
    if rows.is_empty() {
        return Err("rows is empty".to_owned());
    }
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let place = format!("row {}", i + 1);
            let Value::Table(row) = row else {
                return Err(format!("{place} is not {{ scale, size_factor }}"));
            };
            if let Some(unknown) = row
                .keys()
                .find(|k| !["scale", "size_factor", "standard"].contains(&k.as_str()))
            {
                return Err(format!("{place} has the unknown field {unknown}"));
            }
            let scale = match row.get("scale") {
                Some(Value::Integer(n)) if *n > 0 && *n <= i64::from(u32::MAX) => *n as u32,
                _ => return Err(format!("{place}: scale is no scale denominator")),
            };
            let size_factor = match row.get("size_factor") {
                Some(Value::Integer(n)) => *n as f64,
                Some(Value::Float(f)) => *f,
                _ => f64::NAN,
            };
            if !(size_factor.is_finite() && size_factor > 0.0) {
                return Err(format!("{place}: size_factor is not a number above 0"));
            }
            let standard = match row.get("standard") {
                None => true,
                Some(Value::Boolean(b)) => *b,
                Some(_) => return Err(format!("{place}: standard is not true or false")),
            };
            Ok(ScaleEntry {
                scale,
                size_factor,
                standard,
            })
        })
        .collect()
}

/// The schema's check of one value, after `freemapper-rules` checked its unit range. Every value
/// name this program reads, apart from the table `rows`, is one number, so a list is refused (the
/// shared loader accepts a list of numbers, core ADR 0011). `description_box` `cell_mm` has the range
/// the project file allows for `cell_mm`.
fn check_value(key: &str, name: &str, value: &Value) -> Option<String> {
    if key == "scale_table" && name == "rows" {
        return None;
    }
    let n = match value {
        Value::Integer(n) => *n as f64,
        Value::Float(f) => *f,
        Value::Array(_) => return Some(format!("is a list, but {key} reads one number")),
        _ => return Some("is not a number".to_owned()),
    };
    if key == "description_box" && name == "cell_mm" && !(MIN_CELL_MM..=MAX_CELL_MM).contains(&n) {
        return Some(format!(
            "{n} is out of range; a description cell is {MIN_CELL_MM} to {MAX_CELL_MM} mm"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use freemapper_rules::toml::Table;

    /// A test-only overlay for the embedded base profile: replaces the sprint north-line rule and adds
    /// a rule of its own.
    const REPLACE: &str = r#"
    format = 1

    [profile]
    id = "test-replace"
    kind = "overlay"
    base = "iof-print"
    name = { de = "Test", en = "Test" }

    [sources.REG]
    title = "Regional print rules"

    [[rules]]
    id = "iofprint.north_lines.sprint"
    key = "north_lines"
    severity = "hint"
    map_kinds = ["sprint"]
    values = { spacing_metres = 100.0 }
    source = { doc = "REG", section = "§ 4" }
    "#;

    /// A test-only overlay that switches the sprint control circle rule off and gives its own.
    const SWITCH_OFF: &str = r#"
    format = 1

    [profile]
    id = "test-off"
    kind = "overlay"
    base = "iof-print"
    name = { en = "Switch off" }

    [sources.REG]
    title = "Regional print rules"

    [[rules]]
    id = "iofprint.course.control_sprint"
    key = "course_symbol"
    severity = "warning"
    enabled = false
    map_kinds = ["sprint"]
    values = { circle_diameter_mm = 6.0 }
    source = { doc = "REG", section = "§ 6" }

    [[rules]]
    id = "testoff.course.control_sprint"
    key = "course_symbol"
    severity = "warning"
    map_kinds = ["sprint"]
    values = { circle_diameter_mm = 5.5 }
    source = { doc = "REG", section = "§ 6" }
    "#;

    /// A test-only overlay that switches a rule off without a replacement: the set lacks a value.
    const LEAVES_A_GAP: &str = r#"
    format = 1

    [profile]
    id = "test-gap"
    kind = "overlay"
    base = "iof-print"
    name = { en = "Gap" }

    [sources.REG]
    title = "Regional print rules"

    [[rules]]
    id = "iofprint.course.line_forest"
    key = "course_symbol"
    severity = "warning"
    enabled = false
    map_kinds = ["forest"]
    values = { line_width_mm = 0.35 }
    source = { doc = "REG", section = "§ 7" }
    "#;

    fn files() -> Vec<(&'static str, &'static str)> {
        vec![
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", REPLACE),
            ("test-off", SWITCH_OFF),
            ("test-gap", LEAVES_A_GAP),
        ]
    }

    fn s(text: &str) -> String {
        text.to_owned()
    }

    /// A minimal base profile with `rule` as its only rule.
    fn base_with_rule(rule: &str) -> String {
        format!(
            "format = 1\n[profile]\nid = \"base-x\"\nkind = \"base\"\nname = {{ en = \"Base\" }}\n\
             [sources.DOC]\ntitle = \"Some rules\"\n[[rules]]\n{rule}"
        )
    }

    #[test]
    fn base_profile_loads_and_every_value_has_a_unit_and_source() {
        assert_eq!(available(), vec!["iof-print"]);
        let profile = profile("iof-print").unwrap();
        assert_eq!(profile.kind, ProfileKind::Base);
        for lang in ["de", "en"] {
            assert!(profile.name.0.contains_key(lang), "name has no {lang}");
        }
        assert!(!profile.rules.is_empty());
        for rule in &profile.rules {
            assert!(rule.id.starts_with("iofprint."), "{}", rule.id);
            assert!(SCHEMA.keys.iter().any(|k| k.key == rule.key), "{}", rule.id);
            assert_eq!(
                value_name_problems(rule),
                Vec::<String>::new(),
                "{}",
                rule.id
            );
            assert!(!rule.values.is_empty(), "{} has no values", rule.id);
            assert!(
                profile.sources.contains_key(&rule.source.doc),
                "{}: source {} not listed",
                rule.id,
                rule.source.doc
            );
            assert!(!rule.source.section.trim().is_empty(), "{}", rule.id);
            if rule.source.doc == PROPOSAL_DOC {
                assert!(
                    rule.note.is_some(),
                    "{}: our own call needs a note",
                    rule.id
                );
            }
            if let Some(note) = &rule.note {
                for lang in ["de", "en"] {
                    assert!(note.0.contains_key(lang), "{}: note has no {lang}", rule.id);
                }
            }
        }
        let set = base();
        assert_eq!(set.base, "iof-print");
        assert!(set.overlays.is_empty() && set.switched_off.is_empty());
        assert!(set.rules.iter().all(|r| set.source(r).is_some()));
        // Every key the program reads has a rule.
        for key in SCHEMA.keys.iter().map(|k| k.key) {
            assert!(set.rules.iter().any(|r| r.rule.key == key), "{key}");
        }
        assert_eq!(set.purple_cmyk(), [0.35, 0.85, 0.0, 0.0]);
        assert_eq!(
            set.placement_weights(),
            PlacementWeights {
                black: 10.0,
                brown: 4.0,
                blue: 4.0,
                other: 1.0
            }
        );
        for kind in MapKind::ALL {
            assert_eq!(set.description_cell_mm(kind), 6.0);
        }
        assert_eq!(set.north_line_spacing_m(MapKind::Sprint), 120.0);
        assert_eq!(set.north_line_spacing_m(MapKind::Forest), 300.0);
        assert_eq!(set.north_line_spacing_m(MapKind::Schoolyard), 37.5);
        assert_eq!(set.proposed_contour_interval_m(MapKind::Sprint), Some(2.0));
        assert_eq!(set.proposed_contour_interval_m(MapKind::Forest), Some(5.0));
        assert_eq!(set.proposed_contour_interval_m(MapKind::Schoolyard), None);
    }

    #[test]
    fn scale_table_matches_adr_0018() {
        let set = base();
        assert_eq!(
            set.scale_table(MapKind::Sprint),
            [(3000, 4.0 / 3.0), (4000, 1.0), (5000, 1.0)]
        );
        assert_eq!(
            set.scale_table(MapKind::Forest),
            [
                (5000, 3.0),
                (7500, 2.0),
                (10000, 1.5),
                (15000, 1.0),
                (20000, 1.0)
            ]
        );
        assert_eq!(set.scale_table(MapKind::Schoolyard), [(1000, 1.0)]);
        for (kind, scale, standard) in [
            (MapKind::Sprint, 3000, true),
            (MapKind::Sprint, 4000, true),
            (MapKind::Sprint, 5000, false),
            (MapKind::Schoolyard, 1000, true),
            (MapKind::Forest, 5000, true),
            (MapKind::Forest, 15000, true),
            (MapKind::Forest, 20000, false),
            (MapKind::Forest, 4000, false),
        ] {
            assert_eq!(
                set.is_standard_scale(kind, scale),
                standard,
                "{kind:?} {scale}"
            );
        }
        // The base scale of each kind is in its table at 100 %.
        for kind in MapKind::ALL {
            assert!(
                set.scale_table(kind).contains(&(kind.base_scale(), 1.0)),
                "{kind:?}"
            );
            assert!(set.is_standard_scale(kind, kind.base_scale()));
        }
    }

    /// Whether a base profile whose one rule of `key` gives `name = value` (TOML) parses.
    fn value_ok(key: &str, name: &str, value: &str) -> bool {
        let text = base_with_rule(&format!(
            "id = \"basex.x.y\"\nkey = \"{key}\"\nseverity = \"hint\"\nvalues = {{ {name} = {value} }}\n\
             source = {{ doc = \"DOC\", section = \"1\" }}\n"
        ));
        parse_profile("base-x", &text).is_ok()
    }

    #[test]
    fn degrees_take_any_finite_number() {
        let check = |name: &str, value: &str| value_ok("course_symbol", name, value);
        let angle = "out_of_bounds_hatch_angle_degrees";
        assert!(check(angle, "0"));
        assert!(check(angle, "-45"));
        assert!(check(angle, "400.0"));
        assert!(!check(angle, "nan"));
        assert!(!check(angle, "inf"));
        // Lengths stay above 0.
        assert!(!check("first_aid_mm", "0"));
    }

    #[test]
    fn description_cell_is_bounded() {
        let check = |value: &str| value_ok("description_box", "cell_mm", value);
        assert!(check("4"));
        assert!(check("6.0"));
        assert!(check("10"));
        assert!(!check("3.9"));
        assert!(!check("10.5"));
        assert!(!check("nan"));
        assert!(!check("0"));
    }

    #[test]
    fn schema_has_no_problems() {
        assert_eq!(SCHEMA.problems(), Vec::<String>::new());
        // The map kinds filter allows exactly the map kinds this program knows.
        let names: Vec<&str> = MapKind::ALL.iter().map(|k| k.name()).collect();
        let allowed = SCHEMA
            .filters
            .iter()
            .find(|f| f.field == FIELD_MAP_KINDS)
            .map(|f| f.kind);
        assert_eq!(
            allowed,
            Some(FilterKind::Text(&["sprint", "schoolyard", "forest"]))
        );
        assert_eq!(names, ["sprint", "schoolyard", "forest"]);
        // The kind-independent keys refuse both filters.
        for key in ["course_colour", "placement_weights", "number_gap"] {
            let schema = SCHEMA.keys.iter().find(|k| k.key == key).unwrap();
            assert_eq!(schema.without_filters, KIND_INDEPENDENT, "{key}");
        }
    }

    #[test]
    fn a_list_is_refused_where_one_number_is_read() {
        // freemapper-rules checks a list item by item (core ADR 0011); every value but rows is one
        // number here, so the schema's value check refuses the list.
        for (key, name) in [
            ("north_lines", "spacing_metres"),
            ("course_colour", "cyan_percent"),
            ("placement_weights", "black_factor"),
            ("course_symbol", "out_of_bounds_hatch_angle_degrees"),
            ("description_box", "cell_mm"),
            ("number_gap", "min_gap_mm"),
        ] {
            assert!(!value_ok(key, name, "[5.0, 6.0]"), "{key} {name}");
            assert!(!value_ok(key, name, "[]"), "{key} {name}: empty list");
        }
        let text = base_with_rule(
            "id = \"basex.x.y\"\nkey = \"north_lines\"\nseverity = \"hint\"\n\
             values = { spacing_metres = [100.0, 120.0] }\nsource = { doc = \"DOC\", section = \"1\" }\n",
        );
        let err = parse_profile("base-x", &text).unwrap_err().to_string();
        assert!(
            err.contains("basex.x.y")
                && err.contains("is a list, but north_lines reads one number"),
            "{err}"
        );
    }

    #[test]
    fn scale_rows_must_ascend() {
        let rows = |rows: &str| value_ok("scale_table", "rows", &format!("[{rows}]"));
        assert!(rows(
            "{ scale = 3000, size_factor = 1.5 }, { scale = 4000, size_factor = 1.0 }"
        ));
        // Since the switch to freemapper-rules the table check refuses the profile when it is parsed
        // (Malformed); before, the set was refused when it was resolved (Incomplete).
        assert!(!rows(
            "{ scale = 4000, size_factor = 1.0 }, { scale = 3000, size_factor = 1.5 }"
        ));
        assert!(!rows(
            "{ scale = 4000, size_factor = 1.0 }, { scale = 4000, size_factor = 1.0 }"
        ));
        assert!(!rows(""));
    }

    #[test]
    fn a_proposal_without_a_note_is_refused() {
        // freemapper-rules refuses it when the profile is parsed; before, only a test over the
        // embedded profile checked it.
        let text = format!(
            "{}\n[sources.fmp]\ntitle = \"Ours\"\n",
            base_with_rule(
                "id = \"basex.x.y\"\nkey = \"north_lines\"\nseverity = \"hint\"\n\
                 values = { spacing_metres = 100.0 }\nsource = { doc = \"fmp\", section = \"ADR 0031\" }\n",
            )
        );
        let err = parse_profile("base-x", &text).unwrap_err();
        assert!(matches!(err, RuleError::Malformed { .. }), "{err:?}");
        assert!(err.to_string().contains("basex.x.y"), "{err}");
        let with_note = text.replace(
            "source = { doc = \"fmp\"",
            "note = { en = \"Our call.\" }\nsource = { doc = \"fmp\"",
        );
        assert!(parse_profile("base-x", &with_note).is_ok());
    }

    #[test]
    fn three_rules_for_one_slot_are_all_named() {
        let overlay = format!(
            "{REPLACE}\n[[rules]]\nid = \"testreplace.north_lines.a\"\nkey = \"north_lines\"\n\
             severity = \"hint\"\nmap_kinds = [\"sprint\"]\nvalues = {{ spacing_metres = 90.0 }}\n\
             source = {{ doc = \"REG\", section = \"§ 4\" }}\n\
             [[rules]]\nid = \"testreplace.north_lines.b\"\nkey = \"north_lines\"\n\
             severity = \"hint\"\nvalues = {{ spacing_metres = 80.0 }}\n\
             source = {{ doc = \"REG\", section = \"§ 4\" }}\n"
        );
        let files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", overlay.as_str()),
        ];
        let err = load_from(&files, "iof-print", &[s("test-replace")]).unwrap_err();
        assert!(matches!(err, RuleError::Incomplete { .. }), "{err:?}");
        assert_eq!(
            err.to_string(),
            "rule set iof-print + test-replace cannot be used: rules iofprint.north_lines.sprint, \
             testreplace.north_lines.a and testreplace.north_lines.b all give north_lines \
             spacing_metres for sprint maps"
        );
    }

    #[test]
    fn pick_at_the_base_scale_is_the_old_applies() {
        // The per-kind resolution passes the map kind and its base scale. A rule that filters on a
        // dimension matches only when the argument is in its list; one without a filter matches any.
        let filtered = |kinds: &str, scales: &str| {
            let text = base_with_rule(&format!(
                "id = \"basex.north.x\"\nkey = \"north_lines\"\nseverity = \"hint\"\n\
                 map_kinds = {kinds}\nprint_scales = {scales}\nvalues = {{ spacing_metres = 1.0 }}\n\
                 source = {{ doc = \"DOC\", section = \"1\" }}\n"
            ));
            parse_profile("base-x", &text).unwrap().rules.remove(0)
        };
        let old_applies = |kinds: &[MapKind], scales: &[u32], kind: MapKind, scale: u32| {
            (kinds.is_empty() || kinds.contains(&kind))
                && (scales.is_empty() || scales.contains(&scale))
        };
        for (kinds, kinds_toml) in [
            (&[][..], "[]"),
            (&[MapKind::Sprint][..], "[\"sprint\"]"),
            (
                &[MapKind::Sprint, MapKind::Forest][..],
                "[\"sprint\", \"forest\"]",
            ),
        ] {
            for (scales, scales_toml) in [
                (&[][..], "[]"),
                (&[4000][..], "[4000]"),
                (&[3000, 15000][..], "[3000, 15000]"),
            ] {
                let rule = filtered(kinds_toml, scales_toml);
                for kind in MapKind::ALL {
                    let scale = kind.base_scale();
                    assert_eq!(
                        applies(&rule, kind, scale),
                        old_applies(kinds, scales, kind, scale),
                        "{kinds_toml} {scales_toml} {kind:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn sprint_course_symbols() {
        // ISSprOM 2019-2 section 4.7, at 1:4000; Purple Pen's Spr2019 sizes agree.
        assert_eq!(
            base().course_symbol(MapKind::Sprint),
            CourseSymbolSizes {
                circle_diameter_mm: 6.0,
                line_width_mm: 0.35,
                start_side_mm: 7.0,
                finish_inner_mm: 5.0,
                finish_outer_mm: 7.0,
                number_height_mm: 4.0,
                marked_route_dash_mm: 2.0,
                marked_route_gap_mm: 0.5,
                out_of_bounds_line_mm: 1.0,
                crossing_point_mm: 3.0,
                out_of_bounds_hatch_angle_degrees: 45.0,
                out_of_bounds_hatch_second_angle_degrees: Some(135.0),
                out_of_bounds_hatch_spacing_mm: 1.2,
                out_of_bounds_hatch_line_mm: 0.2,
                first_aid_mm: 4.0,
                first_aid_bar_mm: 1.33,
                refreshment_mm: 3.5,
                refreshment_line_mm: 0.4,
            }
        );
        // Schoolyard maps take the sprint values (an assumption the rule's note states).
        assert_eq!(
            base().course_symbol(MapKind::Schoolyard),
            base().course_symbol(MapKind::Sprint)
        );
        // The assumption is our call, so the note check of PROPOSAL_DOC applies.
        let schoolyard = &base().rule("iofprint.course.schoolyard").unwrap().rule;
        assert_eq!(schoolyard.source.doc, PROPOSAL_DOC);
        assert!(schoolyard.note.is_some());
    }

    #[test]
    fn forest_course_symbols() {
        // ISOM 2017-2 section 3.7, at 1:15000; Purple Pen's ISOM 2017 sizes agree.
        assert_eq!(
            base().course_symbol(MapKind::Forest),
            CourseSymbolSizes {
                circle_diameter_mm: 5.0,
                line_width_mm: 0.35,
                start_side_mm: 6.0,
                finish_inner_mm: 4.0,
                finish_outer_mm: 6.0,
                number_height_mm: 4.0,
                marked_route_dash_mm: 2.0,
                marked_route_gap_mm: 0.5,
                out_of_bounds_line_mm: 0.7,
                crossing_point_mm: 3.0,
                out_of_bounds_hatch_angle_degrees: 45.0,
                out_of_bounds_hatch_second_angle_degrees: Some(135.0),
                out_of_bounds_hatch_spacing_mm: 1.2,
                out_of_bounds_hatch_line_mm: 0.2,
                first_aid_mm: 4.0,
                first_aid_bar_mm: 1.33,
                refreshment_mm: 3.5,
                refreshment_line_mm: 0.4,
            }
        );
    }

    /// An overlay with `rules` (TOML `[[rules]]` entries) for the embedded base.
    fn overlay_with(rules: &str) -> String {
        format!(
            "format = 1\n[profile]\nid = \"test-gaps\"\nkind = \"overlay\"\nbase = \"iof-print\"\n\
             name = {{ en = \"Gaps\" }}\n[sources.REG]\ntitle = \"Regional print rules\"\n{rules}"
        )
    }

    fn gap_rule(id: &str, values: &str) -> String {
        format!(
            "[[rules]]\nid = \"{id}\"\nkey = \"number_gap\"\nseverity = \"warning\"\nvalues = {{ {values} }}\n\
             source = {{ doc = \"REG\", section = \"§ 9\" }}\n"
        )
    }

    fn gap_set(rules: &str) -> Result<RuleSet, RuleError> {
        let overlay = overlay_with(rules);
        let files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-gaps", overlay.as_str()),
        ];
        load_from(&files, "iof-print", &[s("test-gaps")])
    }

    #[test]
    fn number_gap_from_the_profile_strictest_wins() {
        // iof-print: 0.5 to 5 mm (ruling R29).
        assert_eq!(base().number_gap(), Some((0.5, 5.0)));
        let rule = base().rule("iofprint.number.gap").unwrap();
        assert_eq!(rule.rule.severity, Severity::Warning);
        // A second rule: the largest minimum and the smallest maximum.
        let set = gap_set(&gap_rule(
            "reg.number.gap",
            "min_gap_mm = 0.8, max_gap_mm = 8.0",
        ))
        .unwrap();
        assert_eq!(set.number_gap(), Some((0.8, 5.0)));
        let set = gap_set(&gap_rule("reg.number.gap", "max_gap_mm = 2.0")).unwrap();
        assert_eq!(set.number_gap(), Some((0.5, 2.0)));
        // Switched off: no rule, placement takes its fixed gap.
        let off = "[[rules]]\nid = \"iofprint.number.gap\"\nkey = \"number_gap\"\nseverity = \"warning\"\n\
                   enabled = false\nvalues = { min_gap_mm = 0.5 }\nsource = { doc = \"REG\", section = \"§ 9\" }\n";
        assert_eq!(gap_set(off).unwrap().number_gap(), None);
        // Ruling R30, courseplan ADR 0032: a maximum below the minimum is raised to it; a missing side stays
        // open (minimum 0.5, maximum 10); both are held to 0.01..10 and rounded inwards to 0.01 mm.
        let set = gap_set(&gap_rule("reg.number.gap", "min_gap_mm = 6.0")).unwrap();
        assert_eq!(set.number_gap(), Some((6.0, 6.0)));
        let only = |values: &str| {
            gap_set(&format!("{off}{}", gap_rule("reg.number.gap", values)))
                .unwrap()
                .number_gap()
        };
        assert_eq!(only("max_gap_mm = 12.0"), Some((0.5, 10.0)));
        // Only a minimum: placement keeps to it, as freemapper-courseplan does; the check leaves the top open.
        assert_eq!(only("min_gap_mm = 0.8"), Some((0.8, 0.8)));
        let allowed = |values: &str| {
            gap_set(&format!("{off}{}", gap_rule("reg.number.gap", values)))
                .unwrap()
                .number_gap_allowed()
        };
        assert_eq!(allowed("min_gap_mm = 0.8"), Some((0.8, 10.0)));
        assert_eq!(allowed("max_gap_mm = 2.0"), Some((0.01, 2.0)));
        assert_eq!(
            allowed("min_gap_mm = 1.0, max_gap_mm = 3.0"),
            Some((1.0, 3.0))
        );
        assert_eq!(base().number_gap_allowed(), Some((0.5, 5.0)));
        assert_eq!(
            only("min_gap_mm = 0.123, max_gap_mm = 3.456"),
            Some((0.13, 3.45))
        );
        assert_eq!(only("min_gap_mm = 0.001"), Some((0.01, 0.01)));
        // A rule with neither value is skipped.
        assert_eq!(only(""), None);
        // Values that are not above 0 or not finite still stop the profile from loading.
        for bad in [
            "min_gap_mm = 0.0",
            "max_gap_mm = -1.0",
            "min_gap_mm = nan",
            "max_gap_mm = inf",
        ] {
            assert!(
                gap_set(&format!("{off}{}", gap_rule("reg.number.gap", bad))).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn overlay_replaces_rule_by_id() {
        let set = load_from(&files(), "iof-print", &[s("test-replace")]).unwrap();
        let rule = set.rule("iofprint.north_lines.sprint").unwrap();
        assert_eq!(rule.profile, "test-replace");
        assert_eq!(set.source(rule).unwrap().title, "Regional print rules");
        assert_eq!(set.north_line_spacing_m(MapKind::Sprint), 100.0);
        assert_eq!(set.north_line_spacing_m(MapKind::Forest), 300.0);
        // The replaced rule keeps its place in the set.
        let position = |set: &RuleSet| {
            set.rules
                .iter()
                .position(|r| r.rule.id == "iofprint.north_lines.sprint")
        };
        assert_eq!(position(&set), position(base()));
        assert_eq!(set.overlays, vec![s("test-replace")]);
        assert!(set.switched_off.is_empty());
    }

    #[test]
    fn overlay_disables_rule() {
        let set = load_from(&files(), "iof-print", &[s("test-off")]).unwrap();
        assert!(set.rule("iofprint.course.control_sprint").is_none());
        assert_eq!(set.switched_off.len(), 1);
        let off = &set.switched_off[0];
        assert_eq!(off.rule.id, "iofprint.course.control_sprint");
        assert_eq!(off.profile, "iof-print");
        assert_eq!(
            off.by, "test-off",
            "the set names the profile that switched it off"
        );
        assert_eq!(set.course_symbol(MapKind::Sprint).circle_diameter_mm, 5.5);
        assert_eq!(set.course_symbol(MapKind::Forest).circle_diameter_mm, 5.0);
        // Both overlays in order.
        let both = load_from(&files(), "iof-print", &[s("test-replace"), s("test-off")]).unwrap();
        assert_eq!(both.north_line_spacing_m(MapKind::Sprint), 100.0);
        assert_eq!(both.switched_off[0].by, "test-off");
        // Switching off a rule the base does not have is a mistake in the overlay.
        let wrong = SWITCH_OFF.replace(
            "id = \"iofprint.course.control_sprint\"",
            "id = \"iofprint.course.nope\"",
        );
        let files = [("iof-print", EMBEDDED[0].1), ("test-off", wrong.as_str())];
        assert_eq!(
            load_from(&files, "iof-print", &[s("test-off")]),
            Err(RuleError::UnknownRule {
                overlay: s("test-off"),
                rule: s("iofprint.course.nope"),
            })
        );
    }

    #[test]
    fn scale_table_keeps_the_base_scale() {
        // An overlay whose sprint table drops 1:4000 would make every base-scale project unusable.
        let drops = REPLACE
            .replace("key = \"north_lines\"", "key = \"scale_table\"")
            .replace("iofprint.north_lines.sprint", "iofprint.scale.sprint")
            .replace(
                "values = { spacing_metres = 100.0 }",
                "values = { rows = [{ scale = 3000, size_factor = 1.3333333333333333 }, \
                 { scale = 5000, size_factor = 1.0, standard = false }] }",
            );
        let files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", drops.as_str()),
        ];
        let err = load_from(&files, "iof-print", &[s("test-replace")]).unwrap_err();
        assert!(matches!(err, RuleError::Incomplete { .. }), "{err:?}");
        assert!(err.to_string().contains("scale = 4000"), "{err}");
        // A base-scale row that is not at 100 %, or not standard, is refused too.
        for row in [
            "{ scale = 4000, size_factor = 1.1 }",
            "{ scale = 4000, size_factor = 1.0, standard = false }",
        ] {
            let text = drops.replace("{ scale = 5000, size_factor = 1.0, standard = false }", row);
            let files = [
                ("iof-print", EMBEDDED[0].1),
                ("test-replace", text.as_str()),
            ];
            assert!(
                load_from(&files, "iof-print", &[s("test-replace")]).is_err(),
                "{row}"
            );
        }
        // With the base row the overlay loads.
        let keeps = drops.replace(
            "{ scale = 5000, size_factor = 1.0, standard = false }",
            "{ scale = 4000, size_factor = 1.0 }",
        );
        let files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", keeps.as_str()),
        ];
        let set = load_from(&files, "iof-print", &[s("test-replace")]).unwrap();
        assert_eq!(
            set.scale_table(MapKind::Sprint),
            [(3000, 4.0 / 3.0), (4000, 1.0)]
        );
    }

    #[test]
    fn a_set_without_a_value_is_refused() {
        let err = load_from(&files(), "iof-print", &[s("test-gap")]).unwrap_err();
        assert!(matches!(err, RuleError::Incomplete { .. }), "{err:?}");
        let text = err.to_string();
        assert!(
            text.contains("line_width_mm") && text.contains("forest"),
            "{text}"
        );
        // Two rules that give the same value for a kind are refused too.
        let twice = REPLACE.replace(
            "id = \"iofprint.north_lines.sprint\"",
            "id = \"testreplace.north_lines.sprint\"",
        );
        let files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", twice.as_str()),
        ];
        let err = load_from(&files, "iof-print", &[s("test-replace")]).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("iofprint.north_lines.sprint")
                && text.contains("testreplace.north_lines.sprint"),
            "{text}"
        );
    }

    #[test]
    fn unknown_value_key_is_an_error() {
        // A rule key the program does not implement.
        let unknown = base_with_rule(
            "id = \"basex.x.y\"\nkey = \"no_such_rule\"\nseverity = \"hint\"\nsource = { doc = \"DOC\", section = \"1\" }\n",
        );
        assert_eq!(
            parse_profile("base-x", &unknown),
            Err(RuleError::UnknownKey {
                profile: s("base-x"),
                rule: s("basex.x.y"),
                key: s("no_such_rule"),
            })
        );
        for (values, ok) in [
            ("{ spacing_metres = 120.0 }", true),
            // A number without its unit.
            ("{ spacing = 120.0 }", false),
            ("{ spacing_metresx = 120.0 }", false),
            // A value with a unit that the key does not read: a typo, not an extension.
            ("{ spacing_mm = 120.0 }", false),
            ("{ spacing_metres = 120.0, extra_metres = 1.0 }", false),
        ] {
            let text = base_with_rule(&format!(
                "id = \"basex.north.x\"\nkey = \"north_lines\"\nseverity = \"hint\"\nvalues = {values}\nsource = {{ doc = \"DOC\", section = \"1\" }}\n"
            ));
            let result = parse_profile("base-x", &text);
            assert_eq!(result.is_ok(), ok, "{values}: {result:?}");
            if !ok {
                assert!(
                    matches!(result, Err(RuleError::Malformed { .. })),
                    "{values}: {result:?}"
                );
            }
        }
        // A scale table row with an unknown field.
        let row = base_with_rule(
            "id = \"basex.scale.x\"\nkey = \"scale_table\"\nseverity = \"error\"\nvalues = { rows = [{ scale = 4000, size = 1.0 }] }\nsource = { doc = \"DOC\", section = \"1\" }\n",
        );
        assert!(matches!(
            parse_profile("base-x", &row),
            Err(RuleError::Malformed { .. })
        ));
    }

    #[test]
    fn profile_format_follows_courseplan() {
        let good = base_with_rule(
            "id = \"basex.north.x\"\nkey = \"north_lines\"\nseverity = \"hint\"\nvalues = { spacing_metres = 1.0 }\nsource = { doc = \"DOC\", section = \"1\" }\n",
        );
        assert!(parse_profile("base-x", &good).is_ok());
        for text in [
            good.replacen("format = 1", "", 1),
            good.replacen("format = 1", "format = 2", 1),
            good.replacen("format = 1", "format = \"1\"", 1),
            // A typo in a field name.
            good.replacen("severity", "severty", 1),
            good.replacen("key = ", "map_kind = [\"sprint\"]\nkey = ", 1),
            // Disciplines are courseplan's filter, not a print one.
            good.replacen("key = ", "disciplines = [\"sprint\"]\nkey = ", 1),
            // An unknown map kind.
            good.replacen("key = ", "map_kinds = [\"moon\"]\nkey = ", 1),
            // A source that [sources] does not list, and no source at all.
            good.replacen("doc = \"DOC\"", "doc = \"NOPE\"", 1),
            good.replacen("source = { doc = \"DOC\", section = \"1\" }", "", 1),
            // An id that is not <profile>.<family>.<name> in lower case.
            good.replacen("basex.north.x", "basex.North", 1),
            good.replacen("severity = \"hint\"", "severity = \"fatal\"", 1),
        ] {
            assert!(
                matches!(
                    parse_profile("base-x", &text),
                    Err(RuleError::Malformed { .. })
                ),
                "{text}"
            );
        }
        // The id in the file must be the profile id it is embedded under.
        assert!(matches!(
            parse_profile("base-y", &good),
            Err(RuleError::Malformed { .. })
        ));
    }

    #[test]
    fn unknown_profile_is_refused() {
        let expected = |id: &str| {
            Err(RuleError::UnknownProfile {
                id: s(id),
                available: s("iof-print"),
            })
        };
        assert_eq!(load_rule_set("iof-2030", &[]), expected("iof-2030"));
        assert_eq!(load_rule_set("iof-print", &[s("nope")]), expected("nope"));
        assert_eq!(
            load_from(&files(), "test-off", &[]),
            Err(RuleError::NotABase(s("test-off")))
        );
        assert_eq!(
            load_from(&files(), "iof-print", &[s("iof-print")]),
            Err(RuleError::NotAnOverlay(s("iof-print")))
        );
        let foreign = REPLACE.replace("base = \"iof-print\"", "base = \"other\"");
        let files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", foreign.as_str()),
        ];
        assert_eq!(
            load_from(&files, "iof-print", &[s("test-replace")]),
            Err(RuleError::OverlayForAnotherBase {
                overlay: s("test-replace"),
                extends: s("other"),
                base: s("iof-print"),
            })
        );
    }

    #[test]
    fn print_scale_filter_is_read_at_the_base_scale() {
        // Values hold at the base scale of the kind, so a print_scales filter selects by it.
        let overlay = REPLACE.replace(
            "map_kinds = [\"sprint\"]",
            "map_kinds = [\"sprint\"]\nprint_scales = [3000]",
        );
        let files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", overlay.as_str()),
        ];
        let err = load_from(&files, "iof-print", &[s("test-replace")]).unwrap_err();
        assert!(
            err.to_string().contains("spacing_metres"),
            "the replaced rule no longer applies at 1:4000: {err}"
        );
        let overlay = REPLACE.replace(
            "map_kinds = [\"sprint\"]",
            "map_kinds = [\"sprint\"]\nprint_scales = [4000]",
        );
        let files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", overlay.as_str()),
        ];
        let set = load_from(&files, "iof-print", &[s("test-replace")]).unwrap();
        assert_eq!(set.north_line_spacing_m(MapKind::Sprint), 100.0);
        let rule = &set.rule("iofprint.north_lines.sprint").unwrap().rule;
        assert!(applies(rule, MapKind::Sprint, 4000));
        assert!(!applies(rule, MapKind::Sprint, 3000));
        assert!(!applies(rule, MapKind::Forest, 4000));
    }

    /// The filters of `rule` as `map_kinds = [...] print_scales = [...]`, for the snapshot.
    fn rule_filters(rule: &Rule) -> String {
        let kinds: Vec<&str> = rule
            .filter(FIELD_MAP_KINDS)
            .iter()
            .filter_map(|item| match item {
                FilterItem::Text(kind) => Some(kind.as_str()),
                FilterItem::Int(_) => None,
            })
            .collect();
        let scales: Vec<u32> = rule
            .filter(FIELD_PRINT_SCALES)
            .iter()
            .filter_map(|item| match item {
                FilterItem::Int(scale) => u32::try_from(*scale).ok(),
                FilterItem::Text(_) => None,
            })
            .collect();
        format!("map_kinds = {kinds:?} print_scales = {scales:?}")
    }

    /// A TOML value in a stable text form: floats with `{:?}`, tables in key order.
    fn value_text(value: &Value) -> String {
        match value {
            Value::Integer(n) => n.to_string(),
            Value::Float(f) => format!("{f:?}"),
            Value::String(s) => format!("{s:?}"),
            Value::Boolean(b) => b.to_string(),
            Value::Datetime(d) => d.to_string(),
            Value::Array(items) => format!(
                "[{}]",
                items.iter().map(value_text).collect::<Vec<_>>().join(", ")
            ),
            Value::Table(table) => table_text(table),
        }
    }

    fn table_text(table: &Table) -> String {
        let mut entries: Vec<(&String, &Value)> = table.iter().collect();
        entries.sort_by_key(|(name, _)| *name);
        let entries: Vec<String> = entries
            .into_iter()
            .map(|(name, value)| format!("{name} = {}", value_text(value)))
            .collect();
        format!("{{ {} }}", entries.join(", "))
    }

    /// Every accessor of `set`, one `key = value` line each, prefixed with `label`.
    fn resolved_text(label: &str, set: &RuleSet, out: &mut String) {
        use std::fmt::Write;
        let mut line = |key: String, value: String| {
            let _ = writeln!(out, "{label}.{key} = {value}");
        };
        line(s("base"), format!("{:?}", set.base));
        line(s("overlays"), format!("{:?}", set.overlays));
        for (i, r) in set.rules.iter().enumerate() {
            let rule = &r.rule;
            line(
                format!("rule[{i}]"),
                format!(
                    "{} from {} key {} severity {} enabled {} {} values {} source {:?} {:?} note {:?}",
                    rule.id,
                    r.profile,
                    rule.key,
                    rule.severity.as_str(),
                    rule.enabled,
                    rule_filters(rule),
                    table_text(&rule.values),
                    rule.source.doc,
                    rule.source.section,
                    rule.note.as_ref().map(|n| &n.0),
                ),
            );
        }
        for (i, off) in set.switched_off.iter().enumerate() {
            line(
                format!("switched_off[{i}]"),
                format!("{} from {} by {}", off.rule.id, off.profile, off.by),
            );
        }
        for (profile, docs) in &set.sources {
            for (doc, source) in docs {
                line(
                    format!("source.{profile}.{doc}"),
                    format!("{:?} url {:?}", source.title, source.url),
                );
            }
        }
        for kind in MapKind::ALL {
            let k = kind.name();
            for e in set.scale_entries(kind) {
                line(
                    format!("{k}.scale.{}", e.scale),
                    format!(
                        "size_factor {:?} standard {} course_factor {:?} is_standard {}",
                        e.size_factor,
                        e.standard,
                        set.course_factor(kind, e.scale),
                        set.is_standard_scale(kind, e.scale)
                    ),
                );
            }
            line(
                format!("{k}.scale_table"),
                format!("{:?}", set.scale_table(kind)),
            );
            // A scale outside every table.
            line(
                format!("{k}.scale.12345"),
                format!(
                    "{:?} course_factor {:?} is_standard {}",
                    set.scale_entry(kind, 12345),
                    set.course_factor(kind, 12345),
                    set.is_standard_scale(kind, 12345)
                ),
            );
            line(
                format!("{k}.north_line_spacing_m"),
                format!("{:?}", set.north_line_spacing_m(kind)),
            );
            line(
                format!("{k}.proposed_contour_interval_m"),
                format!("{:?}", set.proposed_contour_interval_m(kind)),
            );
            line(
                format!("{k}.description_cell_mm"),
                format!("{:?}", set.description_cell_mm(kind)),
            );
            let c = set.course_symbol(kind);
            for (name, value) in [
                ("circle_diameter_mm", c.circle_diameter_mm),
                ("line_width_mm", c.line_width_mm),
                ("start_side_mm", c.start_side_mm),
                ("finish_inner_mm", c.finish_inner_mm),
                ("finish_outer_mm", c.finish_outer_mm),
                ("number_height_mm", c.number_height_mm),
                ("marked_route_dash_mm", c.marked_route_dash_mm),
                ("marked_route_gap_mm", c.marked_route_gap_mm),
                ("out_of_bounds_line_mm", c.out_of_bounds_line_mm),
                ("crossing_point_mm", c.crossing_point_mm),
                (
                    "out_of_bounds_hatch_angle_degrees",
                    c.out_of_bounds_hatch_angle_degrees,
                ),
                (
                    "out_of_bounds_hatch_spacing_mm",
                    c.out_of_bounds_hatch_spacing_mm,
                ),
                ("out_of_bounds_hatch_line_mm", c.out_of_bounds_hatch_line_mm),
                ("first_aid_mm", c.first_aid_mm),
                ("first_aid_bar_mm", c.first_aid_bar_mm),
                ("refreshment_mm", c.refreshment_mm),
                ("refreshment_line_mm", c.refreshment_line_mm),
            ] {
                line(format!("{k}.course.{name}"), format!("{value:?}"));
            }
            line(
                format!("{k}.course.out_of_bounds_hatch_second_angle_degrees"),
                format!("{:?}", c.out_of_bounds_hatch_second_angle_degrees),
            );
        }
        line(s("purple_cmyk"), format!("{:?}", set.purple_cmyk()));
        let w = set.placement_weights();
        line(
            s("placement_weights"),
            format!(
                "black {:?} brown {:?} blue {:?} other {:?}",
                w.black, w.brown, w.blue, w.other
            ),
        );
        line(s("number_gap"), format!("{:?}", set.number_gap()));
        line(
            s("number_gap_allowed"),
            format!("{:?}", set.number_gap_allowed()),
        );
    }

    /// The snapshot of every resolved value (`tests/fixtures/rules/resolved.txt`, README there).
    /// `FMP_BLESS_RULES=1` writes the file from the current loader instead of comparing.
    #[test]
    fn resolved_values_unchanged() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rules/resolved.txt"
        );
        let gap_overlay = overlay_with(&gap_rule(
            "reg.number.gap",
            "min_gap_mm = 0.8, max_gap_mm = 8.0",
        ));
        let at_4000 = REPLACE.replace(
            "map_kinds = [\"sprint\"]",
            "map_kinds = [\"sprint\"]\nprint_scales = [4000]",
        );
        let mut all = files();
        all.push(("test-gaps", gap_overlay.as_str()));
        let at_4000_files = [
            ("iof-print", EMBEDDED[0].1),
            ("test-replace", at_4000.as_str()),
        ];
        let mut text = String::new();
        resolved_text("base", base(), &mut text);
        for (label, overlays) in [
            ("replace", vec![s("test-replace")]),
            ("off", vec![s("test-off")]),
            ("replace_off", vec![s("test-replace"), s("test-off")]),
            ("gaps", vec![s("test-gaps")]),
        ] {
            let set = load_from(&all, "iof-print", &overlays).unwrap();
            resolved_text(label, &set, &mut text);
        }
        let set = load_from(&at_4000_files, "iof-print", &[s("test-replace")]).unwrap();
        resolved_text("replace_at_4000", &set, &mut text);
        if std::env::var_os("FMP_BLESS_RULES").is_some() {
            std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
            std::fs::write(path, &text).unwrap();
        }
        let expected = std::fs::read_to_string(path).unwrap();
        assert_eq!(text, expected);
    }

    /// The name of the variant of `err`.
    fn variant(err: &RuleError) -> &'static str {
        match err {
            RuleError::UnknownProfile { .. } => "UnknownProfile",
            RuleError::NotABase(_) => "NotABase",
            RuleError::NotAnOverlay(_) => "NotAnOverlay",
            RuleError::OverlayForAnotherBase { .. } => "OverlayForAnotherBase",
            RuleError::Malformed { .. } => "Malformed",
            RuleError::UnknownKey { .. } => "UnknownKey",
            RuleError::UnknownRule { .. } => "UnknownRule",
            RuleError::Incomplete { .. } => "Incomplete",
        }
    }

    /// Every refusal the loader had before the switch to freemapper-rules: the case, the files, the base and
    /// overlays, the variant and the texts the message must contain (the rule id where the case has one, else
    /// the profile or the value).
    #[test]
    fn refused_profiles_still_refused() {
        let north = |values: &str| {
            base_with_rule(&format!(
                "id = \"basex.north.x\"\nkey = \"north_lines\"\nseverity = \"hint\"\nvalues = {values}\n\
                 source = {{ doc = \"DOC\", section = \"1\" }}\n"
            ))
        };
        let keyed = |key: &str, extra: &str, values: &str| {
            base_with_rule(&format!(
                "id = \"basex.x.y\"\nkey = \"{key}\"\nseverity = \"hint\"\n{extra}values = {values}\n\
                 source = {{ doc = \"DOC\", section = \"1\" }}\n"
            ))
        };
        let good = north("{ spacing_metres = 1.0 }");
        let sprint_table = |rows: &str| {
            REPLACE
                .replace("key = \"north_lines\"", "key = \"scale_table\"")
                .replace("iofprint.north_lines.sprint", "iofprint.scale.sprint")
                .replace(
                    "values = { spacing_metres = 100.0 }",
                    &format!("values = {{ rows = [{rows}] }}"),
                )
        };
        let base_only = |text: String| (vec![(s("base-x"), text)], s("base-x"), Vec::new());
        let on_iof = |id: &str, text: String| {
            (
                vec![(s("iof-print"), s(EMBEDDED[0].1)), (s(id), text)],
                s("iof-print"),
                vec![s(id)],
            )
        };
        type Setup = (Vec<(String, String)>, String, Vec<String>);
        let cases: Vec<(&str, Setup, &str, Vec<&str>)> = vec![
            (
                "unknown rule key",
                base_only(keyed("no_such_rule", "", "{}")),
                "UnknownKey",
                vec!["basex.x.y", "no_such_rule"],
            ),
            (
                "number without a unit",
                base_only(north("{ spacing = 120.0 }")),
                "Malformed",
                vec!["basex.north.x", "spacing"],
            ),
            (
                "misspelt unit",
                base_only(north("{ spacing_metresx = 120.0 }")),
                "Malformed",
                vec!["basex.north.x", "spacing_metresx"],
            ),
            (
                "unknown value name with a unit",
                base_only(north("{ spacing_mm = 120.0 }")),
                "Malformed",
                vec!["basex.north.x", "spacing_mm"],
            ),
            (
                "extra value name",
                base_only(north("{ spacing_metres = 120.0, extra_metres = 1.0 }")),
                "Malformed",
                vec!["basex.north.x", "extra_metres"],
            ),
            (
                "scale table row with an unknown field",
                base_only(keyed(
                    "scale_table",
                    "",
                    "{ rows = [{ scale = 4000, size = 1.0 }] }",
                )),
                "Malformed",
                vec!["basex.x.y", "size"],
            ),
            (
                "format missing",
                base_only(good.replacen("format = 1", "", 1)),
                "Malformed",
                vec!["base-x", "format"],
            ),
            (
                "format newer",
                base_only(good.replacen("format = 1", "format = 2", 1)),
                "Malformed",
                vec!["base-x", "format"],
            ),
            (
                "format as text",
                base_only(good.replacen("format = 1", "format = \"1\"", 1)),
                "Malformed",
                vec!["base-x", "format"],
            ),
            (
                "misspelt rule field",
                base_only(good.replacen("severity", "severty", 1)),
                "Malformed",
                vec!["basex.north.x", "severty"],
            ),
            (
                "map_kind instead of map_kinds",
                base_only(good.replacen("key = ", "map_kind = [\"sprint\"]\nkey = ", 1)),
                "Malformed",
                vec!["basex.north.x", "map_kind"],
            ),
            (
                "courseplan's disciplines filter",
                base_only(good.replacen("key = ", "disciplines = [\"sprint\"]\nkey = ", 1)),
                "Malformed",
                vec!["basex.north.x", "disciplines"],
            ),
            (
                "unknown map kind",
                base_only(good.replacen("key = ", "map_kinds = [\"moon\"]\nkey = ", 1)),
                "Malformed",
                vec!["basex.north.x", "moon"],
            ),
            (
                "print scale that is no scale",
                base_only(good.replacen("key = ", "print_scales = [0]\nkey = ", 1)),
                "Malformed",
                vec!["basex.north.x"],
            ),
            (
                "source not listed",
                base_only(good.replacen("doc = \"DOC\"", "doc = \"NOPE\"", 1)),
                "Malformed",
                vec!["basex.north.x", "NOPE"],
            ),
            (
                "no source",
                base_only(good.replacen("source = { doc = \"DOC\", section = \"1\" }", "", 1)),
                "Malformed",
                vec!["basex.north.x", "source"],
            ),
            (
                "rule id not dotted lower case",
                base_only(good.replacen("basex.north.x", "basex.North", 1)),
                "Malformed",
                vec!["basex.North"],
            ),
            (
                "unknown severity",
                base_only(good.replacen("severity = \"hint\"", "severity = \"fatal\"", 1)),
                "Malformed",
                vec!["basex.north.x", "fatal"],
            ),
            (
                "rule defined twice",
                base_only(format!(
                    "{good}[[rules]]\nid = \"basex.north.x\"\nkey = \"north_lines\"\nseverity = \"hint\"\n\
                     values = {{ spacing_metres = 2.0 }}\nsource = {{ doc = \"DOC\", section = \"1\" }}\n"
                )),
                "Malformed",
                vec!["basex.north.x"],
            ),
            (
                "profile id differs from the file",
                (vec![(s("base-y"), good.clone())], s("base-y"), Vec::new()),
                "Malformed",
                vec!["base-x", "base-y"],
            ),
            (
                "not a number: nan",
                base_only(north("{ spacing_metres = nan }")),
                "Malformed",
                vec!["basex.north.x", "spacing_metres"],
            ),
            (
                "not a number: inf",
                base_only(north("{ spacing_metres = inf }")),
                "Malformed",
                vec!["basex.north.x", "spacing_metres"],
            ),
            (
                "text under a unit name",
                base_only(north("{ spacing_metres = \"120\" }")),
                "Malformed",
                vec!["basex.north.x", "spacing_metres"],
            ),
            (
                "list under a name read as one number",
                base_only(north("{ spacing_metres = [100.0, 120.0] }")),
                "Malformed",
                vec!["basex.north.x", "spacing_metres"],
            ),
            (
                "length of 0",
                base_only(north("{ spacing_metres = 0.0 }")),
                "Malformed",
                vec!["basex.north.x", "spacing_metres"],
            ),
            (
                "negative length",
                base_only(keyed("number_gap", "", "{ max_gap_mm = -1.0 }")),
                "Malformed",
                vec!["basex.x.y", "max_gap_mm"],
            ),
            (
                "percent above 100",
                base_only(keyed("course_colour", "", "{ cyan_percent = 101 }")),
                "Malformed",
                vec!["basex.x.y", "cyan_percent"],
            ),
            (
                "negative factor",
                base_only(keyed("placement_weights", "", "{ black_factor = -1.0 }")),
                "Malformed",
                vec!["basex.x.y", "black_factor"],
            ),
            (
                "description cell below 4 mm",
                base_only(keyed("description_box", "", "{ cell_mm = 3.9 }")),
                "Malformed",
                vec!["basex.x.y", "cell_mm"],
            ),
            (
                "description cell above 10 mm",
                base_only(keyed("description_box", "", "{ cell_mm = 10.5 }")),
                "Malformed",
                vec!["basex.x.y", "cell_mm"],
            ),
            (
                "map_kinds on a kind-independent key",
                base_only(keyed(
                    "course_colour",
                    "map_kinds = [\"sprint\"]\n",
                    "{ cyan_percent = 35 }",
                )),
                "Malformed",
                vec!["basex.x.y", "course_colour"],
            ),
            (
                "print_scales on a kind-independent key",
                base_only(keyed(
                    "number_gap",
                    "print_scales = [4000]\n",
                    "{ min_gap_mm = 0.5 }",
                )),
                "Malformed",
                vec!["basex.x.y", "number_gap"],
            ),
            (
                "scale table without the base row",
                on_iof(
                    "test-replace",
                    sprint_table(
                        "{ scale = 3000, size_factor = 1.3333333333333333 }, \
                         { scale = 5000, size_factor = 1.0, standard = false }",
                    ),
                ),
                "Incomplete",
                vec!["scale = 4000", "sprint"],
            ),
            (
                "base row not at 100 %",
                on_iof(
                    "test-replace",
                    sprint_table("{ scale = 4000, size_factor = 1.1 }"),
                ),
                "Incomplete",
                vec!["scale = 4000", "sprint"],
            ),
            (
                "base row not standard",
                on_iof(
                    "test-replace",
                    sprint_table("{ scale = 4000, size_factor = 1.0, standard = false }"),
                ),
                "Incomplete",
                vec!["scale = 4000", "sprint"],
            ),
            (
                "two rules for one slot",
                on_iof(
                    "test-replace",
                    REPLACE.replace(
                        "id = \"iofprint.north_lines.sprint\"",
                        "id = \"testreplace.north_lines.sprint\"",
                    ),
                ),
                "Incomplete",
                vec![
                    "iofprint.north_lines.sprint",
                    "testreplace.north_lines.sprint",
                    "spacing_metres",
                    "sprint",
                ],
            ),
            (
                "a value no rule gives",
                on_iof("test-gap", s(LEAVES_A_GAP)),
                "Incomplete",
                vec!["line_width_mm", "forest"],
            ),
            (
                "print_scales filter read at the base scale",
                on_iof(
                    "test-replace",
                    REPLACE.replace(
                        "map_kinds = [\"sprint\"]",
                        "map_kinds = [\"sprint\"]\nprint_scales = [3000]",
                    ),
                ),
                "Incomplete",
                vec!["spacing_metres", "sprint"],
            ),
            (
                "overlay switches off an unknown rule",
                on_iof(
                    "test-off",
                    SWITCH_OFF.replace(
                        "id = \"iofprint.course.control_sprint\"",
                        "id = \"iofprint.course.nope\"",
                    ),
                ),
                "UnknownRule",
                vec!["iofprint.course.nope", "test-off"],
            ),
            (
                "unknown base profile",
                (Vec::new(), s("iof-2030"), Vec::new()),
                "UnknownProfile",
                vec!["iof-2030"],
            ),
            (
                "unknown overlay",
                (
                    vec![(s("iof-print"), s(EMBEDDED[0].1))],
                    s("iof-print"),
                    vec![s("nope")],
                ),
                "UnknownProfile",
                vec!["nope", "iof-print"],
            ),
            (
                "overlay as base",
                (
                    vec![(s("test-off"), s(SWITCH_OFF))],
                    s("test-off"),
                    Vec::new(),
                ),
                "NotABase",
                vec!["test-off"],
            ),
            (
                "base as overlay",
                (
                    vec![(s("iof-print"), s(EMBEDDED[0].1))],
                    s("iof-print"),
                    vec![s("iof-print")],
                ),
                "NotAnOverlay",
                vec!["iof-print"],
            ),
            (
                "overlay for another base",
                on_iof(
                    "test-replace",
                    REPLACE.replace("base = \"iof-print\"", "base = \"other\""),
                ),
                "OverlayForAnotherBase",
                vec!["test-replace", "other", "iof-print"],
            ),
        ];
        for (case, (files, base, overlays), expected, texts) in cases {
            let files: Vec<(&str, &str)> = files
                .iter()
                .map(|(id, text)| (id.as_str(), text.as_str()))
                .collect();
            let err = match load_from(&files, &base, &overlays) {
                Ok(_) => panic!("{case}: loaded"),
                Err(err) => err,
            };
            assert_eq!(variant(&err), expected, "{case}: {err}");
            let message = err.to_string();
            for text in texts {
                assert!(message.contains(text), "{case}: {text} not in {message}");
            }
        }
    }
}

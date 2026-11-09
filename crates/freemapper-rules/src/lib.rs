//! Rule profiles: check rules as TOML files, validated against a schema the app supplies.
//!
//! A profile is a TOML file that an app embeds with `include_str!` and hands over as a list of
//! file stems and contents. A base profile holds a full rule set. An overlay names its base and
//! adds rules, replaces base rules with the same `id`, or switches them off with
//! `enabled = false`. [`load_rule_set`] merges one base and its overlays into a [`RuleSet`] that
//! remembers where each rule came from.
//!
//! The crate knows the file format but no check. The app describes its checks in a [`Schema`],
//! usually a `static`: the rule keys and the value names each key reads, the units, the table
//! values, the filter dimensions and optional validators.
//!
//! ```
//! use freemapper_rules::{FilterArg, FilterKind, FilterSchema, KeySchema, Schema, load_rule_set};
//!
//! static SCHEMA: Schema = Schema {
//!     keys: &[KeySchema { key: "min_distance", values: Some(&["metres"]), without_filters: &[] }],
//!     units: &["metres"],
//!     tables: &[],
//!     filters: &[FilterSchema {
//!         field: "map_kinds",
//!         kind: FilterKind::Text(&["sprint", "forest"]),
//!     }],
//!     proposal_doc: "",
//!     value_check: None,
//! };
//!
//! const BASE: &str = r#"
//! format = 1
//!
//! [profile]
//! id = "demo"
//! kind = "base"
//! name = { en = "Demo" }
//!
//! [sources.DOC]
//! title = "Some rules"
//!
//! [[rules]]
//! id = "demo.distance.sprint"
//! key = "min_distance"
//! severity = "error"
//! map_kinds = ["sprint"]
//! values = { metres = 15 }
//! source = { doc = "DOC", section = "1.1" }
//! "#;
//!
//! assert_eq!(SCHEMA.problems(), Vec::<String>::new());
//! let set = load_rule_set(&SCHEMA, &[("demo", BASE)], "demo", &[]).unwrap();
//! let sprint = [("map_kinds", FilterArg::Text("sprint"))];
//! let rule = set.pick("min_distance", "metres", &sprint).unwrap().unwrap();
//! assert_eq!(rule.rule.id, "demo.distance.sprint");
//! let forest = [("map_kinds", FilterArg::Text("forest"))];
//! assert_eq!(set.pick("min_distance", "metres", &forest), Ok(None));
//! ```
//!
//! # What the loader checks
//!
//! Besides the file format, the parser checks each rule against the schema:
//!
//! - its key is one of [`Schema::keys`], and its value names are the ones the key lists, if it
//!   lists any;
//! - a number or a list of numbers has a name that is a unit or ends in `_<unit>`, unless the name
//!   is a table value of the key ([`value_name_problems`]);
//! - such a number is finite and in the range of its unit: `mm` and `metres` above 0, `factor` 0
//!   or more, `percent` from 0 to 100, `degrees` and any other unit any finite number;
//! - a filter list holds only allowed values, and a key marked independent of a filter dimension
//!   does not carry that filter;
//! - a table value passes its [`TableSchema::check`], and every value passes
//!   [`Schema::value_check`];
//! - a rule that cites [`Schema::proposal_doc`] has a note.

use std::collections::BTreeMap;

pub use toml;
use toml::Table;

mod parse;

pub use parse::{parse_profile, value_name_problems};

/// The profile file format this crate reads.
pub const FORMAT: i64 = 1;

/// What an app's profiles may contain. All fields are `'static`, so an app can write its schema
/// as a `static`.
#[derive(Debug, Clone, Copy)]
pub struct Schema {
    /// The rule keys the app implements, one per check.
    pub keys: &'static [KeySchema],
    /// The units a numeric value name ends in (`metres`, `min_length_metres`).
    pub units: &'static [&'static str],
    /// Value names that hold a table or a list instead of a number with a unit.
    pub tables: &'static [TableSchema],
    /// The filter dimensions a rule may carry, such as `map_kinds`.
    pub filters: &'static [FilterSchema],
    /// The `[sources]` key of values the app proposes itself where no source gives a number. A
    /// rule citing it needs a note. Empty if the app has no such source.
    pub proposal_doc: &'static str,
    /// Checks one value of a rule: rule key, value name and value. `Some` is the problem.
    pub value_check: Option<ValueCheck>,
}

/// The app's check of one value: rule key, value name and value. `Some` is the problem.
pub type ValueCheck = fn(&str, &str, &toml::Value) -> Option<String>;

/// The app's check of a table value's shape. `Err` is the problem.
pub type TableCheck = fn(&toml::Value) -> Result<(), String>;

/// A rule key and the value names a rule of that key may give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySchema {
    pub key: &'static str,
    /// The value names the check reads. `None` accepts any name.
    pub values: Option<&'static [&'static str]>,
    /// Filter fields a rule of this key must not carry, because its values hold for every value
    /// of that dimension.
    pub without_filters: &'static [&'static str],
}

/// A value name of a rule key that holds a table or a list instead of a number with a unit.
#[derive(Debug, Clone, Copy)]
pub struct TableSchema {
    pub key: &'static str,
    pub value: &'static str,
    /// The shape of the value, for people who write a profile.
    pub shape: &'static str,
    /// Checks the shape when the profile is parsed; `Err` is the problem.
    pub check: Option<TableCheck>,
}

/// A filter dimension: the rule field and the values it may list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSchema {
    /// The rule field, such as `map_kinds`.
    pub field: &'static str,
    pub kind: FilterKind,
}

/// The values a filter field may list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// Texts out of this list.
    Text(&'static [&'static str]),
    /// Integers from `min` to `max`, both included.
    Int { min: i64, max: i64 },
}

/// One value of a rule's filter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FilterItem {
    Text(String),
    Int(i64),
}

/// The value a caller passes for one filter dimension, see [`Rule::applies`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterArg<'a> {
    Text(&'a str),
    Int(i64),
}

impl FilterItem {
    fn matches(&self, arg: FilterArg<'_>) -> bool {
        match (self, arg) {
            (FilterItem::Text(item), FilterArg::Text(arg)) => item == arg,
            (FilterItem::Int(item), FilterArg::Int(arg)) => *item == arg,
            _ => false,
        }
    }
}

/// A text in several languages, keyed by language code. English is always present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedText(pub BTreeMap<String, String>);

impl LocalizedText {
    /// The text in `lang` (a language code such as `de`), else the English text.
    pub fn get(&self, lang: &str) -> &str {
        self.0
            .get(lang)
            .or_else(|| self.0.get("en"))
            .map(String::as_str)
            .unwrap_or("")
    }
}

/// How serious a finding of a rule is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

impl Severity {
    /// The name used in profile files.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Hint => "hint",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Severity> {
        [Severity::Error, Severity::Warning, Severity::Hint]
            .into_iter()
            .find(|s| s.as_str() == text)
    }
}

/// Whether a profile stands alone or extends a base profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileKind {
    Base,
    Overlay,
}

/// A document a profile cites, listed under `[sources.<doc>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub title: String,
    pub url: Option<String>,
}

/// Where a rule comes from: the document key of the profile's `[sources]` and the section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    pub doc: String,
    pub section: String,
}

/// One check rule of a profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// Stable dotted id, `<profile>.<family>.<name>`.
    pub id: String,
    /// The check that implements the rule, one of the schema's keys.
    pub key: String,
    pub severity: Severity,
    /// `false` in an overlay switches the base rule with the same id off.
    pub enabled: bool,
    /// The filters by field, such as `map_kinds`. A field the rule does not filter on is absent,
    /// never an empty list.
    pub filters: BTreeMap<String, Vec<FilterItem>>,
    /// The values the check reads; names carry their unit.
    pub values: Table,
    pub source: SourceRef,
    /// Shown with the finding.
    pub note: Option<LocalizedText>,
}

impl Rule {
    /// Whether the rule's filters select the case described by `args`: one argument per filter
    /// dimension, by field name.
    ///
    /// Each dimension the rule filters on must contain the argument for it. A dimension the
    /// rule does not filter on matches anything, also a missing argument. A dimension the rule
    /// filters on but `args` leaves out does not match. Arguments for other dimensions are
    /// ignored.
    pub fn applies(&self, args: &[(&str, FilterArg<'_>)]) -> bool {
        self.filters.iter().all(|(field, items)| {
            args.iter()
                .find(|(name, _)| name == field)
                .is_some_and(|(_, arg)| items.iter().any(|item| item.matches(*arg)))
        })
    }

    /// The filter on `field`; empty if the rule does not filter on it.
    pub fn filter(&self, field: &str) -> &[FilterItem] {
        self.filters.get(field).map_or(&[], Vec::as_slice)
    }

    /// Sets the filter on `field`; an empty list removes it. Meant for tests in the app.
    pub fn set_filter(&mut self, field: &str, items: Vec<FilterItem>) {
        if items.is_empty() {
            self.filters.remove(field);
        } else {
            self.filters.insert(field.to_owned(), items);
        }
    }
}

/// A number of a rule's values, an integer or a float.
pub fn value_number(rule: &Rule, name: &str) -> Option<f64> {
    rule.values
        .get(name)
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
}

/// One parsed profile file.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleProfile {
    /// The file stem, such as `wkb-2026`.
    pub id: String,
    pub kind: ProfileKind,
    /// The base profile an overlay extends.
    pub base: Option<String>,
    pub name: LocalizedText,
    /// The date the rules apply from, as written in the file.
    pub valid_from: Option<String>,
    pub sources: BTreeMap<String, Source>,
    pub rules: Vec<Rule>,
}

/// A rule of a rule set with the profile it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct SetRule {
    pub rule: Rule,
    pub profile: String,
}

/// A rule that a profile switched off with `enabled = false`.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchedOff {
    /// The rule as it was before it was switched off.
    pub rule: Rule,
    /// The profile the switched-off rule came from.
    pub profile: String,
    /// The profile that switched it off.
    pub by: String,
}

/// A base profile merged with its overlays: the rules a check runs.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleSet {
    pub base: String,
    pub overlays: Vec<String>,
    /// Active rules in file order: base rules first, then rules the overlays add.
    pub rules: Vec<SetRule>,
    pub switched_off: Vec<SwitchedOff>,
    /// The `[sources]` of every profile in the set, by profile id.
    pub sources: BTreeMap<String, BTreeMap<String, Source>>,
}

impl RuleSet {
    /// The active rule with this id.
    pub fn rule(&self, id: &str) -> Option<&SetRule> {
        self.rules.iter().find(|r| r.rule.id == id)
    }

    /// The document a rule cites, from the profile the rule came from.
    pub fn source(&self, rule: &SetRule) -> Option<&Source> {
        self.sources.get(&rule.profile)?.get(&rule.rule.source.doc)
    }

    /// The one active rule with `key` that gives the value `name` and applies to `args` (see
    /// [`Rule::applies`]). `None` if no rule does; [`RulesError::Ambiguous`] if several do.
    pub fn pick(
        &self,
        key: &str,
        name: &str,
        args: &[(&str, FilterArg<'_>)],
    ) -> Result<Option<&SetRule>, RulesError> {
        let found: Vec<&SetRule> = self
            .rules
            .iter()
            .filter(|r| {
                r.rule.key == key && r.rule.values.contains_key(name) && r.rule.applies(args)
            })
            .collect();
        match found.as_slice() {
            [] => Ok(None),
            [one] => Ok(Some(one)),
            several => Err(RulesError::Ambiguous {
                key: key.to_owned(),
                value: name.to_owned(),
                rules: several.iter().map(|r| r.rule.id.clone()).collect(),
            }),
        }
    }
}

/// Why a rule profile or rule set cannot be loaded, or a value not picked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RulesError {
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
    #[error("more than one active rule gives {key} {value}: {}", .rules.join(", "))]
    Ambiguous {
        key: String,
        value: String,
        rules: Vec<String>,
    },
}

/// The file stems of `files`, base profiles and overlays, in their order.
pub fn available<'a>(files: &[(&'a str, &str)]) -> Vec<&'a str> {
    files.iter().map(|(id, _)| *id).collect()
}

/// The profile `id` out of `files` (file stem and content).
pub fn profile(
    schema: &Schema,
    files: &[(&str, &str)],
    id: &str,
) -> Result<RuleProfile, RulesError> {
    match files.iter().find(|(stem, _)| *stem == id) {
        Some((stem, text)) => parse_profile(schema, stem, text),
        None => Err(RulesError::UnknownProfile {
            id: id.to_owned(),
            available: available(files).join(", "),
        }),
    }
}

/// Merges the base profile `base` out of `files` with the overlays, in order.
pub fn load_rule_set(
    schema: &Schema,
    files: &[(&str, &str)],
    base: &str,
    overlays: &[String],
) -> Result<RuleSet, RulesError> {
    let base_profile = profile(schema, files, base)?;
    if base_profile.kind != ProfileKind::Base {
        return Err(RulesError::NotABase(base.to_owned()));
    }
    let mut set = RuleSet {
        base: base.to_owned(),
        overlays: overlays.to_vec(),
        rules: Vec::new(),
        switched_off: Vec::new(),
        sources: BTreeMap::new(),
    };
    merge(&mut set, base_profile)?;
    for overlay in overlays {
        let profile = profile(schema, files, overlay)?;
        if profile.kind != ProfileKind::Overlay {
            return Err(RulesError::NotAnOverlay(overlay.clone()));
        }
        let extends = profile.base.clone().unwrap_or_default();
        if extends != base {
            return Err(RulesError::OverlayForAnotherBase {
                overlay: overlay.clone(),
                extends,
                base: base.to_owned(),
            });
        }
        merge(&mut set, profile)?;
    }
    Ok(set)
}

/// Adds the rules of `profile` to `set`: a rule with a known id replaces the rule, a new id is
/// added, and `enabled = false` switches the rule off.
fn merge(set: &mut RuleSet, profile: RuleProfile) -> Result<(), RulesError> {
    let from = profile.id.clone();
    set.sources.insert(from.clone(), profile.sources);
    for rule in profile.rules {
        let active = set.rules.iter().position(|r| r.rule.id == rule.id);
        let off = set.switched_off.iter().position(|r| r.rule.id == rule.id);
        if rule.enabled {
            let entry = SetRule {
                rule,
                profile: from.clone(),
            };
            match active {
                Some(i) => set.rules[i] = entry,
                None => {
                    if let Some(i) = off {
                        set.switched_off.remove(i);
                    }
                    set.rules.push(entry);
                }
            }
        } else if let Some(i) = active {
            let old = set.rules.remove(i);
            set.switched_off.push(SwitchedOff {
                rule: old.rule,
                profile: old.profile,
                by: from.clone(),
            });
        } else if off.is_some() {
            // Already switched off by an earlier profile.
        } else if profile.kind == ProfileKind::Base {
            set.switched_off.push(SwitchedOff {
                rule,
                profile: from.clone(),
                by: from.clone(),
            });
        } else {
            return Err(RulesError::UnknownRule {
                overlay: from,
                rule: rule.id,
            });
        }
    }
    Ok(())
}

impl Schema {
    /// Defects of the schema itself, one line each: names listed twice, tables and filters that
    /// name unknown keys or fields, a filter named like a rule field, an empty filter range.
    /// Meant for a test in the app that asserts the list is empty.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (i, key) in self.keys.iter().enumerate() {
            let k = key.key;
            for (j, value) in key.values.unwrap_or(&[]).iter().enumerate() {
                if key.values.unwrap_or(&[])[..j].contains(value) {
                    problems.push(format!("key {k} lists the value {value} twice"));
                }
            }
            for field in key.without_filters {
                if !self.filters.iter().any(|f| f.field == *field) {
                    problems.push(format!(
                        "key {k} takes no {field} filter, but the schema has no filter {field}"
                    ));
                }
            }
            if self.keys[..i].iter().any(|other| other.key == k) {
                problems.push(format!("key {k} is listed twice"));
            }
        }
        for (i, unit) in self.units.iter().enumerate() {
            if unit.is_empty() {
                problems.push("a unit is empty".to_owned());
            } else if self.units[..i].contains(unit) {
                problems.push(format!("unit {unit} is listed twice"));
            }
        }
        for (i, table) in self.tables.iter().enumerate() {
            let name = format!("table {}.{}", table.key, table.value);
            match self.keys.iter().find(|k| k.key == table.key) {
                None => problems.push(format!("{name} belongs to the unknown key {}", table.key)),
                Some(key) => {
                    if key
                        .values
                        .is_some_and(|values| !values.contains(&table.value))
                    {
                        problems.push(format!("{name} is a value key {} does not read", key.key));
                    }
                }
            }
            if self.tables[..i]
                .iter()
                .any(|t| t.key == table.key && t.value == table.value)
            {
                problems.push(format!("{name} is listed twice"));
            }
        }
        for (i, filter) in self.filters.iter().enumerate() {
            let f = filter.field;
            if parse::RULE_FIELDS.contains(&f) {
                problems.push(format!("filter {f} has the name of a rule field"));
            }
            if self.filters[..i].iter().any(|other| other.field == f) {
                problems.push(format!("filter {f} is listed twice"));
            }
            match filter.kind {
                FilterKind::Text([]) => problems.push(format!("filter {f} allows no value")),
                FilterKind::Int { min, max } if min > max => {
                    problems.push(format!("filter {f} has min {min} above max {max}"));
                }
                _ => {}
            }
        }
        problems
    }
}

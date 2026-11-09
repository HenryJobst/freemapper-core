//! The loader against fixture profiles and a schema shaped like the apps' schemas.
//!
//! This file is an outside crate, so `SCHEMA` also shows that an app can build its schema as a
//! `static`.

use freemapper_rules::toml::Value;
use freemapper_rules::{
    FORMAT, FilterArg, FilterItem, FilterKind, FilterSchema, KeySchema, ProfileKind, Rule, RuleSet,
    RulesError, Schema, Severity, TableSchema, available, load_rule_set, parse_profile, profile,
    value_name_problems, value_number,
};

const BASE: &str = include_str!("fixtures/base-x.toml");
const OVERLAY: &str = include_str!("fixtures/over-x.toml");
const SWITCH_OFF: &str = include_str!("fixtures/off-x.toml");
const SWITCH_OFF_AGAIN: &str = include_str!("fixtures/off-again-x.toml");
const BACK_ON: &str = include_str!("fixtures/back-x.toml");
const OTHER_BASE: &str = include_str!("fixtures/other-base.toml");
const FOREIGN_OVERLAY: &str = include_str!("fixtures/foreign-x.toml");

/// A `scale_table` row list: non-empty, each row with a positive integer `scale`.
fn scale_rows(rows: &Value) -> Result<(), String> {
    let Value::Array(rows) = rows else {
        return Err("rows is not a list".to_owned());
    };
    if rows.is_empty() {
        return Err("rows is empty".to_owned());
    }
    match rows
        .iter()
        .position(|row| !matches!(row.get("scale"), Some(Value::Integer(n)) if *n > 0))
    {
        Some(i) => Err(format!("row {} has no scale", i + 1)),
        None => Ok(()),
    }
}

/// The description cell must be 4 to 10 mm.
fn cell_range(key: &str, name: &str, value: &Value) -> Option<String> {
    if key != "description_box" || name != "cell_mm" {
        return None;
    }
    let n = value_as_f64(value)?;
    (!(4.0..=10.0).contains(&n)).then(|| format!("{n} is not from 4 to 10 mm"))
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
}

static SCHEMA: Schema = Schema {
    keys: &[
        KeySchema {
            key: "min_distance",
            values: None,
            without_filters: &[],
        },
        KeySchema {
            key: "map_edge",
            values: None,
            without_filters: &[],
        },
        KeySchema {
            key: "code_range",
            values: None,
            without_filters: &[],
        },
        KeySchema {
            key: "code_pairs",
            values: None,
            without_filters: &[],
        },
        KeySchema {
            key: "number_gap",
            values: Some(&["min_gap_mm", "max_gap_mm"]),
            without_filters: &["map_kinds", "print_scales"],
        },
        KeySchema {
            key: "scale_table",
            values: Some(&["rows"]),
            without_filters: &[],
        },
        KeySchema {
            key: "description_box",
            values: Some(&["cell_mm"]),
            without_filters: &[],
        },
    ],
    units: &["metres", "mm", "degrees", "percent", "factor"],
    tables: &[
        TableSchema {
            key: "code_range",
            value: "range",
            shape: "[min, max]: the lowest and the highest allowed code number",
            check: None,
        },
        TableSchema {
            key: "code_pairs",
            value: "pairs",
            shape: "[[a, b], ...]: two codes that must not both be used",
            check: None,
        },
        TableSchema {
            key: "map_edge",
            value: "offsets_mm",
            shape: "[x, y]: an offset that may be negative",
            check: None,
        },
        TableSchema {
            key: "scale_table",
            value: "rows",
            shape: "[{ scale }, ...]: one row per scale",
            check: Some(scale_rows),
        },
    ],
    filters: &[
        FilterSchema {
            field: "map_kinds",
            kind: FilterKind::Text(&["sprint", "schoolyard", "forest"]),
        },
        FilterSchema {
            field: "disciplines",
            kind: FilterKind::Text(&["sprint", "middle", "long", "ultralong", "training"]),
        },
        FilterSchema {
            field: "print_scales",
            kind: FilterKind::Int {
                min: 1,
                max: u32::MAX as i64,
            },
        },
    ],
    proposal_doc: "claude",
    value_check: Some(cell_range),
};

fn files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("base-x", BASE),
        ("over-x", OVERLAY),
        ("off-x", SWITCH_OFF),
        ("off-again-x", SWITCH_OFF_AGAIN),
        ("back-x", BACK_ON),
        ("other-base", OTHER_BASE),
        ("foreign-x", FOREIGN_OVERLAY),
    ]
}

fn s(text: &str) -> String {
    text.to_owned()
}

fn parse(id: &str, text: &str) -> Result<freemapper_rules::RuleProfile, RulesError> {
    parse_profile(&SCHEMA, id, text)
}

fn load(base: &str, overlays: &[&str]) -> Result<RuleSet, RulesError> {
    let overlays: Vec<String> = overlays.iter().map(|o| s(o)).collect();
    load_rule_set(&SCHEMA, &files(), base, &overlays)
}

/// `BASE` with its rules replaced by `rules`.
fn base_with_rule(rules: &str) -> String {
    let head = BASE.split("[[rules]]").next().unwrap();
    format!("{head}[[rules]]\n{rules}")
}

/// A rule of `BASE` with this id, key and further lines.
fn rule_text(id: &str, key: &str, more: &str) -> String {
    format!(
        "id = \"{id}\"\nkey = \"{key}\"\nseverity = \"error\"\n{more}\nsource = {{ doc = \"DOC\", section = \"1\" }}\n"
    )
}

/// The one rule of `BASE` with this key and further lines, parsed.
fn one_rule(key: &str, more: &str) -> Result<Rule, RulesError> {
    parse(
        "base-x",
        &base_with_rule(&rule_text("basex.x.y", key, more)),
    )
    .map(|mut p| p.rules.remove(0))
}

/// The message of a `Malformed` error, else a panic.
fn malformed(result: Result<impl std::fmt::Debug, RulesError>) -> String {
    match result {
        Err(RulesError::Malformed { profile, message }) => {
            assert_eq!(profile, "base-x");
            message
        }
        other => panic!("not malformed: {other:?}"),
    }
}

fn text(field: &str) -> FilterItem {
    FilterItem::Text(s(field))
}

#[test]
fn fixture_profiles_parse() {
    assert_eq!(FORMAT, 1);
    assert_eq!(
        available(&files()),
        [
            "base-x",
            "over-x",
            "off-x",
            "off-again-x",
            "back-x",
            "other-base",
            "foreign-x"
        ]
    );
    for id in available(&files()) {
        let p = profile(&SCHEMA, &files(), id).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(p.id, id);
    }
    let base = profile(&SCHEMA, &files(), "base-x").unwrap();
    assert_eq!(base.kind, ProfileKind::Base);
    assert_eq!(base.base, None);
    assert_eq!(base.name.get("de"), "Basis");
    // A language the text lacks falls back to English.
    assert_eq!(base.name.get("fr"), "Base");
    assert_eq!(base.valid_from.as_deref(), Some("2026-03-01"));
    let doc = &base.sources["DOC"];
    assert_eq!(doc.title, "Some rules");
    assert_eq!(doc.url.as_deref(), Some("https://example.org/rules.pdf"));
    assert_eq!(base.rules.len(), 2);
    let range = &base.rules[1];
    assert_eq!(range.id, "basex.code.range");
    assert_eq!(range.key, "code_range");
    assert_eq!(range.severity, Severity::Error);
    assert!(range.enabled);
    assert!(range.filters.is_empty());
    assert_eq!(range.source.doc, "DOC");
    assert_eq!(range.source.section, "1.2");
    assert_eq!(
        range.note.as_ref().unwrap().get("de"),
        "Codes von 31 bis 599"
    );
    assert_eq!(base.rules[0].note, None);

    let overlay = profile(&SCHEMA, &files(), "over-x").unwrap();
    assert_eq!(overlay.kind, ProfileKind::Overlay);
    assert_eq!(overlay.base.as_deref(), Some("base-x"));
    assert_eq!(overlay.valid_from, None);
    assert_eq!(overlay.sources["REG"].url, None);
    // A date may also be written as text.
    let text_date = BASE.replacen("valid_from = 2026-03-01", "valid_from = \"2026-03\"", 1);
    assert_eq!(
        parse("base-x", &text_date).unwrap().valid_from.as_deref(),
        Some("2026-03")
    );
}

#[test]
fn rule_ids_keys_and_sources_are_checked() {
    assert_eq!(
        one_rule("no_such_check", ""),
        Err(RulesError::UnknownKey {
            profile: s("base-x"),
            rule: s("basex.x.y"),
            key: s("no_such_check"),
        })
    );
    let unlisted = base_with_rule(
        "id = \"basex.x.y\"\nkey = \"map_edge\"\nseverity = \"hint\"\nsource = { doc = \"NOPE\", section = \"1\" }\n",
    );
    assert_eq!(
        malformed(parse("base-x", &unlisted)),
        "rule basex.x.y cites source NOPE, which [sources] does not list"
    );
    let no_source = base_with_rule("id = \"basex.x.y\"\nkey = \"map_edge\"\nseverity = \"hint\"\n");
    assert_eq!(
        malformed(parse("base-x", &no_source)),
        "rule basex.x.y has no source = { doc, section }"
    );
    for id in [
        "basex.x",
        "Basex.x.y",
        "basex..y",
        "basex.x.y-z",
        "basex.x.ü",
    ] {
        let text = base_with_rule(&rule_text(id, "map_edge", ""));
        assert_eq!(
            malformed(parse("base-x", &text)),
            format!("rule id {id} is not of the form <profile>.<family>.<name> in lower case")
        );
    }
    assert!(
        parse(
            "base-x",
            &base_with_rule(&rule_text("a.b_2.c3.d", "map_edge", ""))
        )
        .is_ok()
    );
    let twice = base_with_rule(&format!(
        "{}[[rules]]\n{}",
        rule_text("basex.x.y", "map_edge", ""),
        rule_text("basex.x.y", "min_distance", "")
    ));
    assert_eq!(
        malformed(parse("base-x", &twice)),
        "rule basex.x.y is defined twice"
    );
}

#[test]
fn severities_enabled_and_notes() {
    for (name, severity) in [
        ("error", Severity::Error),
        ("warning", Severity::Warning),
        ("hint", Severity::Hint),
    ] {
        let text = base_with_rule(
            &rule_text("basex.x.y", "map_edge", "").replace("\"error\"", &format!("\"{name}\"")),
        );
        assert_eq!(parse("base-x", &text).unwrap().rules[0].severity, severity);
        assert_eq!(severity.as_str(), name);
    }
    assert!(Severity::Error < Severity::Warning && Severity::Warning < Severity::Hint);
    let fatal = base_with_rule(&rule_text("basex.x.y", "map_edge", "").replace("error", "fatal"));
    assert_eq!(
        malformed(parse("base-x", &fatal)),
        "rule basex.x.y: severity fatal is not error, warning or hint"
    );
    assert_eq!(
        malformed(one_rule("map_edge", "enabled = \"no\"")),
        "rule basex.x.y: enabled is not true or false"
    );
    assert!(!one_rule("map_edge", "enabled = false").unwrap().enabled);
    let note = one_rule("map_edge", "note = { en = \"Look\", de = \"Schau\" }").unwrap();
    assert_eq!(note.note.unwrap().get("de"), "Schau");
    assert_eq!(
        malformed(one_rule("map_edge", "note = { de = \"Schau\" }")),
        "rule basex.x.y note has no English text (en)"
    );
    assert_eq!(
        malformed(one_rule("map_edge", "note = { en = \" \" }")),
        "rule basex.x.y note: the en text is empty or no text"
    );
    assert_eq!(
        malformed(one_rule("map_edge", "note = \"Look\"")),
        "rule basex.x.y note is not { en = \"...\", de = \"...\" }"
    );
}

#[test]
fn profile_format_must_be_one() {
    assert!(parse("base-x", BASE).is_ok());
    for (text, message) in [
        (BASE.replacen("format = 1", "", 1), "format = 1 is missing"),
        (
            BASE.replacen("format = 1", "format = 2", 1),
            "format 2 is newer than this program reads (1)",
        ),
        (
            BASE.replacen("format = 1", "format = \"1\"", 1),
            "format \"1\" is not 1",
        ),
    ] {
        assert_eq!(malformed(parse("base-x", &text)), message, "{text}");
    }
    // The id in the file must be the file stem.
    assert_eq!(
        parse("base-y", BASE),
        Err(RulesError::Malformed {
            profile: s("base-y"),
            message: s("[profile] id base-x differs from the file name base-y"),
        })
    );
    // Unknown fields are typos, not extensions.
    assert_eq!(
        malformed(one_rule("map_edge", "map_kind = [\"sprint\"]")),
        "rule basex.x.y has the unknown field map_kind"
    );
    for (from, to, message) in [
        (
            "format = 1",
            "format = 1\nextra = 1",
            "the file has the unknown field extra",
        ),
        (
            "kind = \"base\"",
            "kind = \"base\"\nauthor = \"me\"",
            "[profile] has the unknown field author",
        ),
        (
            "kind = \"base\"",
            "kind = \"other\"",
            "kind other is neither base nor overlay",
        ),
        (
            "kind = \"base\"",
            "kind = \"base\"\nbase = \"x\"",
            "a base profile has no base",
        ),
        (
            "valid_from = 2026-03-01",
            "valid_from = 3",
            "[profile] valid_from is not a date",
        ),
        (
            "url = \"https://example.org/rules.pdf\"",
            "link = \"x\"",
            "[sources.DOC] has the unknown field link",
        ),
        ("id = \"base-x\"", "", "[profile] has no id"),
        (
            "name = { de = \"Basis\", en = \"Base\" }",
            "name = { de = \"Basis\" }",
            "[profile] name has no English text (en)",
        ),
    ] {
        let text = BASE.replacen(from, to, 1);
        assert_eq!(malformed(parse("base-x", &text)), message, "{to}");
    }
    let overlay = OVERLAY.replacen("base = \"base-x\"\n", "", 1);
    assert_eq!(
        parse("over-x", &overlay),
        Err(RulesError::Malformed {
            profile: s("over-x"),
            message: s("an overlay needs a base"),
        })
    );
    let no_head = "format = 1\n";
    assert_eq!(
        malformed(parse("base-x", no_head)),
        "the file has no [profile]"
    );
    let broken = malformed(parse("base-x", "format = "));
    assert!(!broken.is_empty());
}

#[test]
fn overlay_replaces_base_rule() {
    let set = load("base-x", &["over-x"]).unwrap();
    let rule = set.rule("basex.distance.min").unwrap();
    assert_eq!(rule.profile, "over-x");
    assert_eq!(rule.rule.severity, Severity::Warning);
    assert_eq!(rule.rule.values.get("metres"), Some(&Value::Float(50.0)));
    assert_eq!(set.source(rule).unwrap().title, "Regional rules");
    assert_eq!(rule.rule.source.section, "§ 4");
    // The replaced rule keeps its place; a new rule comes after the base rules.
    let ids: Vec<&str> = set.rules.iter().map(|r| r.rule.id.as_str()).collect();
    assert_eq!(
        ids,
        ["basex.distance.min", "basex.code.range", "overx.map.edge"]
    );
    assert_eq!(set.rule("basex.code.range").unwrap().profile, "base-x");
    assert_eq!(set.base, "base-x");
    assert_eq!(set.overlays, vec![s("over-x")]);
    assert!(set.switched_off.is_empty());
    assert_eq!(
        set.sources.keys().collect::<Vec<_>>(),
        [&s("base-x"), &s("over-x")]
    );
    assert!(set.rules.iter().all(|r| set.source(r).is_some()));
}

#[test]
fn overlay_can_switch_a_rule_off() {
    let set = load("base-x", &["off-x"]).unwrap();
    assert!(set.rule("basex.code.range").is_none());
    assert!(set.rule("basex.distance.min").is_some());
    assert_eq!(set.switched_off.len(), 1);
    let off = &set.switched_off[0];
    assert_eq!(off.rule.id, "basex.code.range");
    assert_eq!(off.profile, "base-x");
    assert_eq!(off.by, "off-x");
    // The switched-off rule is kept as it was before.
    assert_eq!(off.rule.severity, Severity::Error);
    assert!(off.rule.enabled);
    // Switching off a rule the base does not have is a mistake in the overlay.
    let wrong = SWITCH_OFF.replace("basex.code.range", "basex.code.nope");
    let files = [("base-x", BASE), ("off-x", wrong.as_str())];
    assert_eq!(
        load_rule_set(&SCHEMA, &files, "base-x", &[s("off-x")]),
        Err(RulesError::UnknownRule {
            overlay: s("off-x"),
            rule: s("basex.code.nope"),
        })
    );
}

#[test]
fn repeated_switch_off_is_ignored() {
    let set = load("base-x", &["off-x", "off-again-x"]).unwrap();
    assert!(set.rule("basex.code.range").is_none());
    assert_eq!(set.switched_off.len(), 1);
    assert_eq!(set.switched_off[0].by, "off-x");
    assert_eq!(set.switched_off[0].profile, "base-x");
}

#[test]
fn a_later_overlay_switches_a_rule_back_on() {
    let set = load("base-x", &["off-x", "back-x"]).unwrap();
    assert!(set.switched_off.is_empty());
    let ids: Vec<&str> = set.rules.iter().map(|r| r.rule.id.as_str()).collect();
    // The rule comes back at the end, as a new rule would.
    assert_eq!(ids, ["basex.distance.min", "basex.code.range"]);
    let back = set.rule("basex.code.range").unwrap();
    assert_eq!(back.profile, "back-x");
    assert_eq!(back.rule.severity, Severity::Warning);
}

#[test]
fn a_base_profile_may_ship_a_rule_switched_off() {
    let base = BASE.replacen(
        "id = \"basex.code.range\"",
        "id = \"basex.code.range\"\nenabled = false",
        1,
    );
    let files = [("base-x", base.as_str()), ("back-x", BACK_ON)];
    let set = load_rule_set(&SCHEMA, &files, "base-x", &[]).unwrap();
    assert!(set.rule("basex.code.range").is_none());
    assert_eq!(set.switched_off.len(), 1);
    assert_eq!(set.switched_off[0].profile, "base-x");
    assert_eq!(set.switched_off[0].by, "base-x");
    assert!(!set.switched_off[0].rule.enabled);
    let set = load_rule_set(&SCHEMA, &files, "base-x", &[s("back-x")]).unwrap();
    assert!(set.switched_off.is_empty());
    assert_eq!(set.rule("basex.code.range").unwrap().profile, "back-x");
}

#[test]
fn overlay_for_another_base_is_refused() {
    assert_eq!(
        load("base-x", &["foreign-x"]),
        Err(RulesError::OverlayForAnotherBase {
            overlay: s("foreign-x"),
            extends: s("other-base"),
            base: s("base-x"),
        })
    );
    assert_eq!(load("over-x", &[]), Err(RulesError::NotABase(s("over-x"))));
    assert_eq!(
        load("base-x", &["other-base"]),
        Err(RulesError::NotAnOverlay(s("other-base")))
    );
}

#[test]
fn unknown_profile_is_refused() {
    let expected = |id: &str| RulesError::UnknownProfile {
        id: s(id),
        available: s("base-x, over-x, off-x, off-again-x, back-x, other-base, foreign-x"),
    };
    assert_eq!(load("base-y", &[]), Err(expected("base-y")));
    assert_eq!(load("base-x", &["over-y"]), Err(expected("over-y")));
    assert_eq!(
        profile(&SCHEMA, &files(), "nope").map(|p| p.id),
        Err(expected("nope"))
    );
}

#[test]
fn filters_select_rules() {
    let rule = one_rule(
        "min_distance",
        "map_kinds = [\"sprint\", \"schoolyard\"]\ndisciplines = [\"sprint\", \"middle\"]\nprint_scales = [3000, 4000]\nvalues = { metres = 15.0 }",
    )
    .unwrap();
    assert_eq!(
        rule.filter("map_kinds"),
        [text("sprint"), text("schoolyard")]
    );
    assert_eq!(rule.filter("disciplines"), [text("sprint"), text("middle")]);
    assert_eq!(
        rule.filter("print_scales"),
        [FilterItem::Int(3000), FilterItem::Int(4000)]
    );
    assert_eq!(rule.filter("no_such_filter"), []);
    let course = |kind, discipline, scale| {
        [
            ("map_kinds", FilterArg::Text(kind)),
            ("disciplines", FilterArg::Text(discipline)),
            ("print_scales", FilterArg::Int(scale)),
        ]
    };
    assert!(rule.applies(&course("sprint", "sprint", 4000)));
    assert!(rule.applies(&course("schoolyard", "middle", 3000)));
    assert!(!rule.applies(&course("sprint", "sprint", 5000)));
    assert!(!rule.applies(&course("forest", "sprint", 4000)));
    assert!(!rule.applies(&course("sprint", "long", 4000)));
    // An argument of the wrong kind does not match.
    assert!(!rule.applies(&[
        ("map_kinds", FilterArg::Text("sprint")),
        ("disciplines", FilterArg::Text("sprint")),
        ("print_scales", FilterArg::Text("4000")),
    ]));
    // Arguments may come in any order, and arguments for unknown dimensions do not matter.
    assert!(rule.applies(&[
        ("print_scales", FilterArg::Int(3000)),
        ("colour", FilterArg::Text("blue")),
        ("disciplines", FilterArg::Text("middle")),
        ("map_kinds", FilterArg::Text("sprint")),
    ]));
    // Empty filters select everything.
    let all = one_rule("min_distance", "map_kinds = []\nvalues = { metres = 15.0 }").unwrap();
    assert!(all.filters.is_empty());
    assert!(all.applies(&course("forest", "training", 15000)));
    assert!(all.applies(&[]));
}

#[test]
fn applies_with_a_missing_argument_matches_only_unfiltered_rules() {
    let filtered = one_rule("min_distance", "disciplines = [\"long\"]").unwrap();
    let open = one_rule("min_distance", "").unwrap();
    let no_discipline = [
        ("map_kinds", FilterArg::Text("forest")),
        ("print_scales", FilterArg::Int(10000)),
    ];
    assert!(!filtered.applies(&no_discipline));
    assert!(open.applies(&no_discipline));
    assert!(filtered.applies(&[("disciplines", FilterArg::Text("long"))]));
}

#[test]
fn set_filter_changes_the_filters() {
    let mut rule = one_rule("min_distance", "").unwrap();
    rule.set_filter("disciplines", vec![text("long")]);
    assert_eq!(rule.filter("disciplines"), [text("long")]);
    assert!(rule.applies(&[("disciplines", FilterArg::Text("long"))]));
    assert!(!rule.applies(&[("disciplines", FilterArg::Text("middle"))]));
    // An empty list removes the filter, as in a profile.
    rule.set_filter("disciplines", vec![]);
    assert!(rule.filters.is_empty());
    assert!(rule.applies(&[("disciplines", FilterArg::Text("middle"))]));
}

#[test]
fn filter_values_are_checked() {
    for (more, message) in [
        (
            "map_kinds = [\"park\"]",
            "rule basex.x.y: unknown map_kinds value park; allowed: sprint, schoolyard, forest",
        ),
        (
            "map_kinds = [1]",
            "map_kinds in rule basex.x.y is not a list of texts",
        ),
        (
            "map_kinds = \"sprint\"",
            "map_kinds in rule basex.x.y is not a list",
        ),
        (
            "print_scales = [0]",
            "rule basex.x.y: print_scales value 0 is not a whole number from 1 to 4294967295",
        ),
        (
            "print_scales = [4294967296]",
            "rule basex.x.y: print_scales value 4294967296 is not a whole number from 1 to 4294967295",
        ),
        (
            "print_scales = [\"4000\"]",
            "rule basex.x.y: print_scales value \"4000\" is not a whole number from 1 to 4294967295",
        ),
        (
            "print_scales = 4000",
            "print_scales in rule basex.x.y is not a list",
        ),
    ] {
        assert_eq!(malformed(one_rule("min_distance", more)), message, "{more}");
    }
}

#[test]
fn int_filter_refuses_floats() {
    assert_eq!(
        malformed(one_rule("min_distance", "print_scales = [4000.0]")),
        "rule basex.x.y: print_scales value 4000.0 is not a whole number from 1 to 4294967295"
    );
}

#[test]
fn value_names_carry_units() {
    for (values, ok) in [
        ("{ metres = 15.0 }", true),
        ("{ min_length_metres = 30.0, degrees = 45 }", true),
        ("{ tolerance_percent = 6, climb_factor = 10 }", true),
        ("{ he_length_percent = [63, 67] }", true),
        (
            "{ impassable = [\"520\", \"708\"], groups = [[\"111\", \"112\"]] }",
            true,
        ),
        (
            "{ symbol = \"520\", taped = true, from = 2026-01-01 }",
            true,
        ),
        ("{ distance = 15.0 }", false),
        ("{ metresx = 15.0 }", false),
        ("{ courses = [{ lkm = 2.1 }] }", false),
        ("{ ranges = { a = 1 } }", false),
    ] {
        let parsed = one_rule("min_distance", &format!("values = {values}"));
        assert_eq!(parsed.is_ok(), ok, "{values}: {parsed:?}");
    }
    assert_eq!(
        malformed(one_rule(
            "min_distance",
            "values = { distance = 15.0, metres = 1, b = { a = 1 } }"
        )),
        "rule basex.x.y: value names without a unit: b, distance"
    );
    // A documented table is fine for its own key only.
    assert!(one_rule("code_pairs", "values = { pairs = [[66, 99]] }").is_ok());
    assert!(one_rule("map_edge", "values = { pairs = [[66, 99]] }").is_err());
    let mut rule = one_rule("code_pairs", "values = { pairs = [[66, 99]] }").unwrap();
    assert_eq!(value_name_problems(&SCHEMA, &rule), Vec::<String>::new());
    rule.key = s("map_edge");
    assert_eq!(value_name_problems(&SCHEMA, &rule), [s("pairs")]);
}

#[test]
fn a_key_with_a_value_list_refuses_other_names() {
    assert!(one_rule("number_gap", "values = { min_gap_mm = 0.5 }").is_ok());
    assert_eq!(
        malformed(one_rule(
            "number_gap",
            "values = { min_gap_mm = 0.5, colour_mm = 1.0 }"
        )),
        "rule basex.x.y does not read colour_mm; it reads min_gap_mm, max_gap_mm"
    );
    // A key without a list reads any name.
    assert!(one_rule("min_distance", "values = { other_metres = 3 }").is_ok());
}

#[test]
fn numbers_are_range_checked_by_unit() {
    for (values, problem) in [
        ("{ metres = 15 }", None),
        (
            "{ metres = 0 }",
            Some("metres: 0 is out of range for a value in metres"),
        ),
        (
            "{ min_metres = -1.5 }",
            Some("min_metres: -1.5 is out of range for a value in metres"),
        ),
        (
            "{ mm = 0.0 }",
            Some("mm: 0 is out of range for a value in mm"),
        ),
        ("{ metres = nan }", Some("metres: is not a finite number")),
        ("{ metres = inf }", Some("metres: is not a finite number")),
        ("{ metres = \"30\" }", Some("metres: is not a number")),
        ("{ factor = 0 }", None),
        (
            "{ climb_factor = -0.1 }",
            Some("climb_factor: -0.1 is out of range for a value in factor"),
        ),
        ("{ percent = 0, max_percent = 100 }", None),
        (
            "{ percent = 100.5 }",
            Some("percent: 100.5 is out of range for a value in percent"),
        ),
        (
            "{ percent = -1 }",
            Some("percent: -1 is out of range for a value in percent"),
        ),
        ("{ degrees = -170, max_degrees = 400 }", None),
        (
            "{ degrees = -inf }",
            Some("degrees: is not a finite number"),
        ),
        ("{ he_length_percent = [63, 67] }", None),
        (
            "{ he_length_percent = [63, 101] }",
            Some("he_length_percent: 101 is out of range for a value in percent"),
        ),
        (
            "{ min_length_metres = [10, 0] }",
            Some("min_length_metres: 0 is out of range for a value in metres"),
        ),
        ("{ degrees = [] }", None),
    ] {
        let result = one_rule("min_distance", &format!("values = {values}"));
        match problem {
            None => assert!(result.is_ok(), "{values}: {result:?}"),
            Some(problem) => assert_eq!(
                malformed(result),
                format!("rule basex.x.y: {problem}"),
                "{values}"
            ),
        }
    }
}

#[test]
fn table_values_are_exempt_from_range_checks() {
    assert!(one_rule("map_edge", "values = { offsets_mm = [-1.0, 0.0] }").is_ok());
    assert!(one_rule("min_distance", "values = { offsets_mm = [-1.0, 0.0] }").is_err());
    assert!(one_rule("code_range", "values = { range = [0, -5] }").is_ok());
}

#[test]
fn a_table_check_of_the_app_runs_at_load() {
    assert!(one_rule("scale_table", "values = { rows = [{ scale = 4000 }] }").is_ok());
    assert_eq!(
        malformed(one_rule("scale_table", "values = { rows = [] }")),
        "rule basex.x.y: rows: rows is empty"
    );
    assert_eq!(
        malformed(one_rule(
            "scale_table",
            "values = { rows = [{ scale = 4000 }, { size = 1 }] }"
        )),
        "rule basex.x.y: rows: row 2 has no scale"
    );
}

#[test]
fn a_value_check_of_the_app_runs_after_the_range_check() {
    assert!(one_rule("description_box", "values = { cell_mm = 6 }").is_ok());
    assert_eq!(
        malformed(one_rule("description_box", "values = { cell_mm = 12 }")),
        "rule basex.x.y: cell_mm: 12 is not from 4 to 10 mm"
    );
    assert_eq!(
        malformed(one_rule("description_box", "values = { cell_mm = -1 }")),
        "rule basex.x.y: cell_mm: -1 is out of range for a value in mm"
    );
}

#[test]
fn a_key_without_a_filter_refuses_it() {
    assert_eq!(
        malformed(one_rule(
            "number_gap",
            "map_kinds = [\"sprint\"]\nvalues = { min_gap_mm = 0.5 }"
        )),
        "rule basex.x.y: number_gap takes no map_kinds filter"
    );
    assert_eq!(
        malformed(one_rule("number_gap", "print_scales = [4000]")),
        "rule basex.x.y: number_gap takes no print_scales filter"
    );
    // An empty list is no filter; other dimensions stay allowed.
    assert!(one_rule("number_gap", "map_kinds = []\ndisciplines = [\"long\"]").is_ok());
}

#[test]
fn proposals_of_the_program_need_a_note() {
    let proposal = |note: &str| {
        let rule = format!(
            "id = \"basex.x.y\"\nkey = \"map_edge\"\nseverity = \"hint\"\n{note}\nsource = {{ doc = \"claude\", section = \"-\" }}\n"
        );
        parse("base-x", &base_with_rule(&rule))
    };
    assert!(proposal("note = { en = \"Our proposal\" }").is_ok());
    assert_eq!(
        malformed(proposal("")),
        "rule basex.x.y cites claude, the program's own proposals, and has no note"
    );
}

/// A rule set with these `number_gap` rules: id and further lines.
fn gap_set(rules: &[(&str, &str)]) -> RuleSet {
    let text: Vec<String> = rules
        .iter()
        .map(|(id, more)| rule_text(id, "number_gap", more))
        .collect();
    let base = base_with_rule(&text.join("[[rules]]\n"));
    load_rule_set(&SCHEMA, &[("base-x", base.as_str())], "base-x", &[]).unwrap()
}

#[test]
fn pick_finds_the_one_rule_that_applies() {
    let set = gap_set(&[
        (
            "basex.gap.long",
            "disciplines = [\"long\"]\nvalues = { min_gap_mm = 1.0 }",
        ),
        (
            "basex.gap.middle",
            "disciplines = [\"middle\"]\nvalues = { min_gap_mm = 0.8, max_gap_mm = 2.0 }",
        ),
    ]);
    let long = [("disciplines", FilterArg::Text("long"))];
    let middle = [("disciplines", FilterArg::Text("middle"))];
    let picked = set
        .pick("number_gap", "min_gap_mm", &long)
        .unwrap()
        .unwrap();
    assert_eq!(picked.rule.id, "basex.gap.long");
    assert_eq!(
        set.pick("number_gap", "min_gap_mm", &middle)
            .unwrap()
            .unwrap()
            .rule
            .id,
        "basex.gap.middle"
    );
    // Only rules that give the value count.
    assert_eq!(set.pick("number_gap", "max_gap_mm", &long), Ok(None));
    assert_eq!(set.pick("map_edge", "metres", &long), Ok(None));
    // A missing argument leaves only unfiltered rules.
    assert_eq!(set.pick("number_gap", "min_gap_mm", &[]), Ok(None));
}

#[test]
fn pick_refuses_two_matching_rules() {
    let set = gap_set(&[
        ("basex.gap.all", "values = { min_gap_mm = 1.0 }"),
        (
            "basex.gap.long",
            "disciplines = [\"long\"]\nvalues = { min_gap_mm = 0.8 }",
        ),
    ]);
    let long = [("disciplines", FilterArg::Text("long"))];
    let error = set.pick("number_gap", "min_gap_mm", &long).unwrap_err();
    assert_eq!(
        error,
        RulesError::Ambiguous {
            key: s("number_gap"),
            value: s("min_gap_mm"),
            rules: vec![s("basex.gap.all"), s("basex.gap.long")],
        }
    );
    assert_eq!(
        error.to_string(),
        "more than one active rule gives number_gap min_gap_mm: basex.gap.all, basex.gap.long"
    );
    // Where only one of them applies, it is picked.
    let middle = [("disciplines", FilterArg::Text("middle"))];
    assert_eq!(
        set.pick("number_gap", "min_gap_mm", &middle)
            .unwrap()
            .unwrap()
            .rule
            .id,
        "basex.gap.all"
    );
}

#[test]
fn errors_keep_their_texts() {
    let cases = [
        (
            RulesError::UnknownProfile {
                id: s("a"),
                available: s("b, c"),
            },
            "unknown rule profile a; available: b, c",
        ),
        (
            RulesError::NotABase(s("a")),
            "rule profile a is an overlay, not a base profile",
        ),
        (
            RulesError::NotAnOverlay(s("a")),
            "rule profile a is a base profile, not an overlay",
        ),
        (
            RulesError::OverlayForAnotherBase {
                overlay: s("o"),
                extends: s("e"),
                base: s("b"),
            },
            "overlay o extends e, not b",
        ),
        (
            RulesError::Malformed {
                profile: s("p"),
                message: s("m"),
            },
            "rule profile p is malformed: m",
        ),
        (
            RulesError::UnknownKey {
                profile: s("p"),
                rule: s("r"),
                key: s("k"),
            },
            "rule r in profile p has the unknown key k",
        ),
        (
            RulesError::UnknownRule {
                overlay: s("o"),
                rule: s("r"),
            },
            "overlay o switches off rule r, which the base profile does not have",
        ),
    ];
    for (error, display) in cases {
        assert_eq!(error.to_string(), display);
    }
}

#[test]
fn value_number_reads_integers_and_floats() {
    let rule = one_rule(
        "min_distance",
        "values = { metres = 15, max_metres = 2.5, symbol = \"520\" }",
    )
    .unwrap();
    assert_eq!(value_number(&rule, "metres"), Some(15.0));
    assert_eq!(value_number(&rule, "max_metres"), Some(2.5));
    assert_eq!(value_number(&rule, "symbol"), None);
    assert_eq!(value_number(&rule, "missing"), None);
}

#[test]
fn the_test_schema_has_no_problems() {
    assert_eq!(SCHEMA.problems(), Vec::<String>::new());
}

static BROKEN: Schema = Schema {
    keys: &[
        KeySchema {
            key: "a",
            values: Some(&["x_mm", "x_mm"]),
            without_filters: &["colour"],
        },
        KeySchema {
            key: "a",
            values: None,
            without_filters: &[],
        },
    ],
    units: &["mm", "mm", ""],
    tables: &[
        TableSchema {
            key: "b",
            value: "rows",
            shape: "[...]",
            check: None,
        },
        TableSchema {
            key: "a",
            value: "rows",
            shape: "[...]",
            check: None,
        },
    ],
    filters: &[
        FilterSchema {
            field: "values",
            kind: FilterKind::Text(&["x"]),
        },
        FilterSchema {
            field: "scales",
            kind: FilterKind::Int { min: 5, max: 1 },
        },
        FilterSchema {
            field: "scales",
            kind: FilterKind::Text(&[]),
        },
    ],
    proposal_doc: "",
    value_check: None,
};

#[test]
fn schema_problems_name_each_defect() {
    assert_eq!(
        BROKEN.problems(),
        [
            "key a lists the value x_mm twice",
            "key a takes no colour filter, but the schema has no filter colour",
            "key a is listed twice",
            "unit mm is listed twice",
            "a unit is empty",
            "table b.rows belongs to the unknown key b",
            "table a.rows is a value key a does not read",
            "filter values has the name of a rule field",
            "filter scales has min 5 above max 1",
            "filter scales is listed twice",
            "filter scales allows no value",
        ]
    );
}

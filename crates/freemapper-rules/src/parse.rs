//! Parsing one profile file against a schema.

use std::collections::BTreeMap;

use toml::{Table, Value};

use crate::{
    FORMAT, FilterItem, FilterKind, FilterSchema, KeySchema, LocalizedText, ProfileKind, Rule,
    RuleProfile, RulesError, Schema, Severity, Source, SourceRef,
};

/// The fields of a rule besides the schema's filter fields.
pub(crate) const RULE_FIELDS: &[&str] = &[
    "id", "key", "severity", "enabled", "values", "source", "note",
];

/// Parses one profile file whose file stem is `id`.
pub fn parse_profile(schema: &Schema, id: &str, text: &str) -> Result<RuleProfile, RulesError> {
    let bad = |message: String| RulesError::Malformed {
        profile: id.to_owned(),
        message,
    };
    let doc: Table = text
        .parse()
        .map_err(|e: toml::de::Error| bad(e.to_string()))?;
    only_fields(&doc, &["format", "profile", "sources", "rules"], "the file").map_err(bad)?;
    match doc.get("format") {
        Some(Value::Integer(FORMAT)) => {}
        Some(Value::Integer(n)) if *n > FORMAT => {
            return Err(bad(format!(
                "format {n} is newer than this program reads ({FORMAT})"
            )));
        }
        Some(other) => return Err(bad(format!("format {other} is not {FORMAT}"))),
        None => return Err(bad(format!("format = {FORMAT} is missing"))),
    }

    let head = table(&doc, "profile", "the file").map_err(bad)?;
    only_fields(
        head,
        &["id", "kind", "base", "name", "valid_from"],
        "[profile]",
    )
    .map_err(bad)?;
    let file_id = string(head, "id", "[profile]").map_err(bad)?;
    if file_id != id {
        return Err(bad(format!(
            "[profile] id {file_id} differs from the file name {id}"
        )));
    }
    let kind = match string(head, "kind", "[profile]").map_err(bad)?.as_str() {
        "base" => ProfileKind::Base,
        "overlay" => ProfileKind::Overlay,
        other => return Err(bad(format!("kind {other} is neither base nor overlay"))),
    };
    let base = optional_string(head, "base", "[profile]").map_err(bad)?;
    match (kind, &base) {
        (ProfileKind::Overlay, None) => return Err(bad("an overlay needs a base".to_owned())),
        (ProfileKind::Base, Some(_)) => {
            return Err(bad("a base profile has no base".to_owned()));
        }
        _ => {}
    }
    let name = localized(head.get("name"), "[profile] name").map_err(bad)?;
    let valid_from = match head.get("valid_from") {
        None => None,
        Some(Value::Datetime(date)) => Some(date.to_string()),
        Some(Value::String(date)) => Some(date.clone()),
        Some(_) => return Err(bad("[profile] valid_from is not a date".to_owned())),
    };

    let mut sources = BTreeMap::new();
    if let Some(value) = doc.get("sources") {
        let Value::Table(entries) = value else {
            return Err(bad("[sources] is not a table".to_owned()));
        };
        for (doc_key, entry) in entries {
            let place = format!("[sources.{doc_key}]");
            let Value::Table(entry) = entry else {
                return Err(bad(format!("{place} is not a table")));
            };
            only_fields(entry, &["title", "url"], &place).map_err(bad)?;
            sources.insert(
                doc_key.clone(),
                Source {
                    title: string(entry, "title", &place).map_err(bad)?,
                    url: optional_string(entry, "url", &place).map_err(bad)?,
                },
            );
        }
    }

    let mut rules: Vec<Rule> = Vec::new();
    let entries = match doc.get("rules") {
        None => Vec::new(),
        Some(Value::Array(entries)) => entries.clone(),
        Some(_) => return Err(bad("rules is not a list of [[rules]]".to_owned())),
    };
    for (index, entry) in entries.iter().enumerate() {
        let Value::Table(entry) = entry else {
            return Err(bad(format!("rule {} is not a table", index + 1)));
        };
        let rule = parse_rule(schema, id, entry, index)?;
        if rules.iter().any(|r| r.id == rule.id) {
            return Err(bad(format!("rule {} is defined twice", rule.id)));
        }
        if !sources.contains_key(&rule.source.doc) {
            return Err(bad(format!(
                "rule {} cites source {}, which [sources] does not list",
                rule.id, rule.source.doc
            )));
        }
        rules.push(rule);
    }

    Ok(RuleProfile {
        id: id.to_owned(),
        kind,
        base,
        name,
        valid_from,
        sources,
        rules,
    })
}

fn parse_rule(
    schema: &Schema,
    profile: &str,
    entry: &Table,
    index: usize,
) -> Result<Rule, RulesError> {
    let bad = |message: String| RulesError::Malformed {
        profile: profile.to_owned(),
        message,
    };
    let place = format!("rule {}", index + 1);
    let id = string(entry, "id", &place).map_err(bad)?;
    let place = format!("rule {id}");
    let fields: Vec<&str> = RULE_FIELDS
        .iter()
        .copied()
        .chain(schema.filters.iter().map(|f| f.field))
        .collect();
    only_fields(entry, &fields, &place).map_err(bad)?;
    let well_formed = id.split('.').count() >= 3
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        });
    if !well_formed {
        return Err(bad(format!(
            "rule id {id} is not of the form <profile>.<family>.<name> in lower case"
        )));
    }
    let key = string(entry, "key", &place).map_err(bad)?;
    let Some(key_schema) = schema.keys.iter().find(|k| k.key == key) else {
        return Err(RulesError::UnknownKey {
            profile: profile.to_owned(),
            rule: id,
            key,
        });
    };
    let severity_text = string(entry, "severity", &place).map_err(bad)?;
    let severity = Severity::parse(&severity_text).ok_or_else(|| {
        bad(format!(
            "{place}: severity {severity_text} is not error, warning or hint"
        ))
    })?;
    let enabled = match entry.get("enabled") {
        None => true,
        Some(Value::Boolean(enabled)) => *enabled,
        Some(_) => return Err(bad(format!("{place}: enabled is not true or false"))),
    };
    let mut filters = BTreeMap::new();
    for filter in schema.filters {
        let items = filter_items(entry, filter, &place).map_err(bad)?;
        if items.is_empty() {
            continue;
        }
        if key_schema.without_filters.contains(&filter.field) {
            return Err(bad(format!(
                "{place}: {key} takes no {} filter",
                filter.field
            )));
        }
        filters.insert(filter.field.to_owned(), items);
    }
    let values = match entry.get("values") {
        None => Table::new(),
        Some(Value::Table(values)) => values.clone(),
        Some(_) => return Err(bad(format!("{place}: values is not a table"))),
    };
    let source = match entry.get("source") {
        Some(Value::Table(source)) => {
            let where_ = format!("{place} source");
            only_fields(source, &["doc", "section"], &where_).map_err(bad)?;
            SourceRef {
                doc: string(source, "doc", &where_).map_err(bad)?,
                section: string(source, "section", &where_).map_err(bad)?,
            }
        }
        _ => return Err(bad(format!("{place} has no source = {{ doc, section }}"))),
    };
    let note = match entry.get("note") {
        None => None,
        value => Some(localized(value, &format!("{place} note")).map_err(bad)?),
    };
    let rule = Rule {
        id,
        key,
        severity,
        enabled,
        filters,
        values,
        source,
        note,
    };
    let problems = value_name_problems(schema, &rule);
    if !problems.is_empty() {
        return Err(bad(format!(
            "rule {}: value names without a unit: {}",
            rule.id,
            problems.join(", ")
        )));
    }
    check_values(schema, key_schema, &rule).map_err(bad)?;
    if !schema.proposal_doc.is_empty()
        && rule.source.doc == schema.proposal_doc
        && rule.note.is_none()
    {
        return Err(bad(format!(
            "rule {} cites {}, the program's own proposals, and has no note",
            rule.id, schema.proposal_doc
        )));
    }
    Ok(rule)
}

/// The items of the filter field `filter` in a rule; empty if the field is missing or empty.
fn filter_items(
    entry: &Table,
    filter: &FilterSchema,
    place: &str,
) -> Result<Vec<FilterItem>, String> {
    let field = filter.field;
    match filter.kind {
        FilterKind::Text(allowed) => strings(entry, field, place)?
            .into_iter()
            .map(|text| {
                if allowed.contains(&text.as_str()) {
                    Ok(FilterItem::Text(text))
                } else {
                    Err(format!(
                        "{place}: unknown {field} value {text}; allowed: {}",
                        allowed.join(", ")
                    ))
                }
            })
            .collect(),
        FilterKind::Int { min, max } => match entry.get(field) {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| match item {
                    Value::Integer(n) if (min..=max).contains(n) => Ok(FilterItem::Int(*n)),
                    other => Err(format!(
                        "{place}: {field} value {other} is not a whole number from {min} to {max}"
                    )),
                })
                .collect(),
            Some(_) => Err(format!("{field} in {place} is not a list")),
        },
    }
}

/// Checks the values of a parsed rule: the names the key reads, the range of each number by its
/// unit, the app's table checks and the app's value check.
fn check_values(schema: &Schema, key: &KeySchema, rule: &Rule) -> Result<(), String> {
    if let Some(known) = key.values {
        let unread: Vec<&str> = rule
            .values
            .keys()
            .map(String::as_str)
            .filter(|name| !known.contains(name))
            .collect();
        if !unread.is_empty() {
            let reads = if known.is_empty() {
                "no values".to_owned()
            } else {
                known.join(", ")
            };
            return Err(format!(
                "rule {} does not read {}; it reads {reads}",
                rule.id,
                unread.join(", ")
            ));
        }
    }
    for (name, value) in &rule.values {
        let problem = match table_of(schema, &rule.key, name) {
            Some(table) => table.check.and_then(|check| check(value).err()),
            None => range_problem(schema, name, value),
        };
        let problem = problem.or_else(|| {
            schema
                .value_check
                .and_then(|check| check(&rule.key, name, value))
        });
        if let Some(problem) = problem {
            return Err(format!("rule {}: {name}: {problem}", rule.id));
        }
    }
    Ok(())
}

/// The table value `name` of `key`, if the schema documents one.
fn table_of<'s>(schema: &'s Schema, key: &str, name: &str) -> Option<&'s crate::TableSchema> {
    schema
        .tables
        .iter()
        .find(|t| t.key == key && t.value == name)
}

/// The unit a value name carries: the name itself or its `_<unit>` suffix.
fn unit_of<'s>(schema: &'s Schema, name: &str) -> Option<&'s str> {
    schema
        .units
        .iter()
        .find(|unit| name == **unit || name.ends_with(&format!("_{unit}")))
        .copied()
}

/// The problem of a value whose name carries a unit: it must be a number or a list of numbers,
/// each finite and in the range of the unit. Values without a unit have no range.
fn range_problem(schema: &Schema, name: &str, value: &Value) -> Option<String> {
    let unit = unit_of(schema, name)?;
    let numbers: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        single => vec![single],
    };
    for item in numbers {
        let n = match item {
            Value::Integer(i) => *i as f64,
            Value::Float(f) => *f,
            _ => return Some("is not a number".to_owned()),
        };
        if !n.is_finite() {
            return Some("is not a finite number".to_owned());
        }
        let fits = match unit {
            "mm" | "metres" => n > 0.0,
            "factor" => n >= 0.0,
            "percent" => (0.0..=100.0).contains(&n),
            // An angle may point anywhere; other units have no range of their own.
            _ => true,
        };
        if !fits {
            return Some(format!("{n} is out of range for a value in {unit}"));
        }
    }
    None
}

/// Value names of `rule` that carry no unit and are no documented table.
///
/// Text, true/false and lists of texts need no unit. A number or a list of numbers needs a name
/// that is a unit or ends in `_<unit>` ([`Schema::units`]). Anything else must be listed in
/// [`Schema::tables`] for the rule's key.
pub fn value_name_problems(schema: &Schema, rule: &Rule) -> Vec<String> {
    fn is_text(value: &Value) -> bool {
        match value {
            Value::String(_) | Value::Boolean(_) | Value::Datetime(_) => true,
            Value::Array(items) => items.iter().all(is_text),
            _ => false,
        }
    }
    fn is_number(value: &Value) -> bool {
        match value {
            Value::Integer(_) | Value::Float(_) => true,
            Value::Array(items) => items
                .iter()
                .all(|v| matches!(v, Value::Integer(_) | Value::Float(_))),
            _ => false,
        }
    }
    rule.values
        .iter()
        .filter(|(name, value)| {
            !(is_text(value)
                || (is_number(value) && unit_of(schema, name).is_some())
                || table_of(schema, &rule.key, name).is_some())
        })
        .map(|(name, _)| name.clone())
        .collect()
}

pub(crate) fn only_fields(table: &Table, allowed: &[&str], place: &str) -> Result<(), String> {
    match table.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(unknown) => Err(format!("{place} has the unknown field {unknown}")),
        None => Ok(()),
    }
}

fn table<'a>(table: &'a Table, name: &str, place: &str) -> Result<&'a Table, String> {
    match table.get(name) {
        Some(Value::Table(inner)) => Ok(inner),
        Some(_) => Err(format!("{name} in {place} is not a table")),
        None => Err(format!("{place} has no [{name}]")),
    }
}

fn string(table: &Table, name: &str, place: &str) -> Result<String, String> {
    optional_string(table, name, place)?.ok_or_else(|| format!("{place} has no {name}"))
}

fn optional_string(table: &Table, name: &str, place: &str) -> Result<Option<String>, String> {
    match table.get(name) {
        None => Ok(None),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.clone())),
        Some(_) => Err(format!("{name} in {place} is not a text")),
    }
}

fn strings(table: &Table, name: &str, place: &str) -> Result<Vec<String>, String> {
    match table.get(name) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::String(text) => Ok(text.clone()),
                _ => Err(format!("{name} in {place} is not a list of texts")),
            })
            .collect(),
        Some(_) => Err(format!("{name} in {place} is not a list")),
    }
}

fn localized(value: Option<&Value>, place: &str) -> Result<LocalizedText, String> {
    let Some(Value::Table(texts)) = value else {
        return Err(format!("{place} is not {{ en = \"...\", de = \"...\" }}"));
    };
    let mut map = BTreeMap::new();
    for (lang, text) in texts {
        match text {
            Value::String(text) if !text.trim().is_empty() => {
                map.insert(lang.clone(), text.clone());
            }
            _ => return Err(format!("{place}: the {lang} text is empty or no text")),
        }
    }
    if !map.contains_key("en") {
        return Err(format!("{place} has no English text (en)"));
    }
    Ok(LocalizedText(map))
}

# freemapper-rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The program-independent part of the rule-profile loader becomes the crate `freemapper-rules` in
freemapper-core. freemapper-courseplan uses it, and freemapper-print can switch to it after its M3.

**Architecture:**
- The crate parses, validates and merges profiles against an app-supplied `Schema`. The schema carries:
  - the known keys and their value names;
  - units and table values;
  - filter dimensions;
  - the proposal document;
  - optional validators.
- The crate returns a `RuleSet`.
- Rule filters are generic named dimensions. The app turns its domain values (map kind, discipline, print
  scale) into filter arguments.
- Typed accessors and checks stay in the apps.

**Tech Stack:** Rust 2024; `toml` and `thiserror` at the versions in freemapper-courseplan's lock (ADR 0004).

**Spec:**
- freemapper-courseplan ADR 0025 (the profile format) and `crates/freemapper-courseplan/src/rules.rs`;
- freemapper-print's differences (its `rules.rs` on branch m3-courses, ADR 0031 there), summarized below;
- core ADR 0001.

## Global Constraints

- **Unchanged from courseplan (byte-for-byte semantics):**
  - `FORMAT = 1`;
  - `[profile]` fields `id`/`kind`/`base`/`name`/`valid_from`, and `[sources.<doc>]` with `title`/`url`;
  - unknown fields are refused, the id must equal the file stem, and base and overlay must be consistent;
  - the rule id format, severities, `source { doc, section }`, and the localized `note` with mandatory `en`;
  - the unit-suffix rule (`value_name_problems`);
  - `merge()`:
    - the same id replaces a rule in place, and a new id is appended;
    - `enabled = false` moves the rule to `switched_off` with `by`, and repeating it is ignored;
    - an overlay switching off an unknown id gives `UnknownRule`;
  - the `RulesError` variants and their English `Display` texts.
- **Adopted from freemapper-print (agreed with freemapper-print, 2026-11-09):**
  - A key may list the value names it reads. Any other name is an error: "rule R does not read X; it reads A, B".
    A key without a list accepts any name, which is courseplan's current behaviour.
  - Range checks at load time, by unit:
    - non-number or non-finite values are refused;
    - `mm` and `metres` must be > 0;
    - `factor` must be ≥ 0;
    - `percent` must be in 0..=100;
    - `degrees` may be any finite number.
    - Lists are checked element by element. Values under a table name are exempt.
  - Keys can be marked independent of a filter dimension. A rule of such a key that carries that filter is
    refused.
  - Per-table shape validators supplied by the app: `fn(&toml::Value) -> Result<(), String>`.
  - An optional per-value validator supplied by the app: `fn(key, value_name, &toml::Value) -> Option<String>`.
  - `RuleSet::pick(key, value_name, &args)` returns the single active rule that applies and has the value. Two
    such rules give an error naming both ids.
- **Filters:**
  - The schema declares dimensions, each with a field name and either `Text { allowed }` or `Int { min, max }`.
  - Courseplan uses `map_kinds` (sprint, schoolyard, forest), `disciplines` (its `Discipline` names) and
    `print_scales` (1..=u32::MAX).
  - freemapper-print uses `map_kinds` and `print_scales`.
  - An empty or missing filter means all.
  - `Rule::applies(&[(&str, FilterArg)])`, with `FilterArg::Text(&str) | Int(i64)`, is true when every
    dimension that the rule filters on contains the argument for it.
  - A dimension the caller does not pass counts as not matching if the rule filters on it, and as matching if
    it does not.
- **Not in the crate** (app-specific):
  - `KNOWN_KEYS` content and the embedded profiles;
  - completeness rules (print's `Incomplete`);
  - per-kind resolution of typed values;
  - `gap_range`/`number_gap`;
  - `MapKind` and `Discipline` themselves.
  - MapKind moves to `freemapper-omap` later. That is print's crate, agreed with freemapper-print.
- Lints: `unsafe_code = "forbid"`, clippy `-D warnings`. Cargo.lock: no version of a package that both locks
  have may differ from freemapper-courseplan's lock.
- Prose (ADR, README, changelog) goes through `unslop` and `humanizer`. German needs correct umlauts.

## Review Focus

1. A courseplan profile that passed before and now fails the new range checks.
   - Test (Task 2): both embedded courseplan profiles still load, and `load_rule_set` gives the same rule ids,
     values and switched-off list as before. Compare against a snapshot taken before the switch.
2. A filter argument the caller forgets.
   - Test: `applies_with_a_missing_argument_matches_only_unfiltered_rules`.
3. Two overlays switching off the same rule.
   - Test: `repeated_switch_off_is_ignored`, ported.
4. An integer filter value given as a float (`print_scales = [4000.0]`) is refused with a message.
   - Test: `int_filter_refuses_floats`.
5. `pick` with two matching rules names both ids.
   - Test: `pick_refuses_two_matching_rules`.

---

### Task 1: The crate `freemapper-rules` in freemapper-core

**Files:**
- Create: `crates/freemapper-rules/Cargo.toml`, `src/lib.rs`, and more modules if `lib.rs` would grow past
  about 800 lines (e.g. `parse.rs`, `merge.rs`).
- Create: `crates/freemapper-rules/tests/fixtures/`: a small base profile, an overlay, and broken variants as
  needed.
- Modify: workspace `Cargo.toml`, `Cargo.lock`, READMEs (status), changelogs, glossary.
- Create: `docs/adr/0011-rule-profiles-validated-against-an-app-schema.md` and its index row.
- Also in this task: remove `Localize` from `freemapper-text`. No app uses it (orphan rule; both apps agreed).
  Update its doc, ADR 0010 (a "Superseded in part" note), and the changelog under Removed.

**Interfaces (Produces):**
- `pub struct Schema`, all `&'static` slices and fn pointers so an app can write a `static`:
  - `keys: &'static [KeySchema]`;
  - `units: &'static [&'static str]`;
  - `tables: &'static [TableSchema]`;
  - `filters: &'static [FilterSchema]`;
  - `proposal_doc: &'static str`;
  - `value_check: Option<fn(&str, &str, &toml::Value) -> Option<String>>`.
- `pub struct KeySchema { pub key: &'static str, pub values: Option<&'static [&'static str]>, pub without_filters: &'static [&'static str] }`.
- `pub struct TableSchema { pub key: &'static str, pub value: &'static str, pub shape: &'static str, pub check: Option<fn(&toml::Value) -> Result<(), String>> }`.
- `pub struct FilterSchema { pub field: &'static str, pub kind: FilterKind }` and
  `pub enum FilterKind { Text(&'static [&'static str]), Int { min: i64, max: i64 } }`.
- `pub enum FilterItem { Text(String), Int(i64) }` and `pub enum FilterArg<'a> { Text(&'a str), Int(i64) }`.
- `pub struct Rule`. It has the fields of courseplan's `Rule`, with `map_kinds`/`disciplines`/`print_scales`
  replaced by `pub filters: BTreeMap<String, Vec<FilterItem>>`. Methods:
  - `applies(&self, args: &[(&str, FilterArg)]) -> bool`;
  - `filter(&self, field) -> &[FilterItem]` (empty when absent);
  - `set_filter(&mut self, field, items)`, for app tests.
- Moved unchanged from courseplan: `RuleProfile`, `ProfileKind`, `Source`, `SourceRef`, `LocalizedText`,
  `Severity`, `SetRule`, `SwitchedOff`, `RuleSet` (`rule`, `source`, plus the new `pick`), `RulesError`, and
  `FORMAT`.
- Free functions, parameterised by schema and file list:
  - `parse_profile(schema, id, text) -> Result<RuleProfile, RulesError>`;
  - `profile(schema, files: &[(&str, &str)], id)`;
  - `load_rule_set(schema, files, base, overlays: &[String]) -> Result<RuleSet, RulesError>`;
  - `available(files) -> Vec<&str>`;
  - `value_name_problems(schema, rule) -> Vec<String>`;
  - `value_number(rule, name) -> Option<f64>`.
- `RuleSet::pick(&self, key, value_name, args) -> Result<Option<&SetRule>, RulesError>`. A new variant
  `Ambiguous { key, value, rules: Vec<String> }`, with its Display text in English.

- [ ] **Step 1:** Port the generic tests of courseplan's `rules.rs` to fixture profiles.
  - Covered: parsing, header, sources, ids, severities, notes, units, merge and overlays, errors.
  - Leave out tests that read courseplan's real profiles; those stay in the app.
  - Add tests for the adopted print features and for Review Focus 2, 4 and 5. Show that they fail.
- [ ] **Step 2:** Implement by moving courseplan's code and generalising the filters and the key list.
  - Courseplan's `TABLE_VALUES` triples become `TableSchema`s without `check`.
  - Print's range checks follow its description above.
- [ ] **Step 3:** Remove `Localize` from freemapper-text.
- [ ] **Step 4:** Docs:
  - ADR 0011: the schema; what moved from which app; the filter semantics; the adopted print checks; what
    stays in the apps; `MapKind` later in `freemapper-omap`.
  - Glossary rows: Regelprofil/rule profile/`RuleProfile`, Regelsatz/rule set/`RuleSet`, Schema/schema/`Schema`,
    Filter/filter/`FilterSchema`.
  - README status rows and the changelog entry (Added freemapper-rules; Removed `Localize`).
- [ ] **Step 5:** Set the workspace version to 0.2.0. Run fmt, clippy and test. Commit
  `feat(rules): rule profiles validated against an app schema` and `refactor(text)!: drop Localize`.

### Task 2: freemapper-courseplan uses freemapper-rules

Runs in freemapper-courseplan after core tag `v0.2.0`.

**Files:** freemapper-courseplan:
- `Cargo.toml`: the dependency `freemapper-rules`, and `freemapper-text` at tag `v0.2.0`;
- `crates/freemapper-courseplan/src/rules.rs`: it keeps `EMBEDDED`, the `Schema` static, `PROPOSAL_DOC`, the
  wrappers `available`, `profile`, `load_rule_set`, `parse_profile`, a helper
  `applies(rule, map_kind, discipline, print_scale)`, `print_scale_of`, and the tests over the real profiles;
- the call sites in `check/mod.rs`, `numbers.rs` and the test files (the `disciplines`/`map_kinds` field
  writes become `set_filter`);
- `text.rs`: `Localize for RulesError` keeps working through the local trait;
- changelogs, under Changed.

- [ ] **Step 1:** Before switching, write the snapshot test `embedded_rule_sets_are_unchanged`. For each base
  and overlay combination of the embedded profiles, it lists rule ids, keys, severities, values and switched-off
  ids. Commit it.
- [ ] **Step 2:** Switch to the crate. The snapshot test, all check tests and the CLI golden outputs stay green
  unchanged.
- [ ] **Step 3:** Commit `refactor(rules): use freemapper-rules from freemapper-core`.

# 0011. Rule profiles are validated against an app schema

Date: 2026-11-09
Status: Accepted

## Context

freemapper-courseplan keeps its check rules in TOML profiles: a base profile per rule book, and overlays that
replace, add or switch off rules (courseplan ADR 0012 and 0025). freemapper-print copied the loader in its M3
and changed it (print ADR 0031). It lists the value names each key reads, checks number ranges when it loads a
profile, has keys that take no filter, a validator for its scale table, and a `pick` that refuses two rules for
the same value. Courseplan filters on map kind, discipline and print scale. Print filters on map kind and print
scale only. Both copies share the file format and the merge logic line for line. Without a shared crate, two
loaders would drift apart.

## Decision

- `freemapper-rules` holds the format, the parser, the merge and the checks that hold for any app. The app
  describes its checks in a `Schema`, usually a `static`, since all its fields are `'static` slices and
  function pointers:
  - `keys`: the rule keys, each with the value names it reads (`None` accepts any name) and the filter fields
    its rules must not carry;
  - `units`: the units a number's name ends in;
  - `tables`: value names that hold a table, each with its shape for people who write a profile and an
    optional shape check;
  - `filters`: the filter dimensions, by rule field, each either a text list or an integer range;
  - `proposal_doc`: the `[sources]` key of the app's own proposals;
  - `value_check`: an optional check of any value.
- Taken from freemapper-courseplan without change in meaning: `FORMAT = 1`, the `[profile]` and
  `[sources.<doc>]` fields, the refusal of unknown fields, id equal to the file stem, base and overlay rules,
  the rule id format, severities, `source = { doc, section }`, the note with a required English text, the
  unit suffix rule, the merge, and the `RulesError` variants with their English texts. `TABLE_VALUES` became
  `Schema::tables`, `KNOWN_KEYS` became `Schema::keys`.
- Taken from freemapper-print *(agreed with freemapper-print)*: the value names per key ("rule R does not read
  X; it reads A, B"), the range checks, keys without a filter, the table checks, the value check and
  `RuleSet::pick`. If `pick` finds more than one rule, it returns the new error `Ambiguous`, which names all
  matching ids.
- The range checks apply to a value whose name carries a unit and is no table value of its key. The value must
  be a number or a list of numbers, each finite. `mm` and `metres` must be above 0, `factor` 0 or more,
  `percent` from 0 to 100. `degrees` and any other unit take any finite number. A list is checked item by
  item. Any other value under a unit name is refused: a text, a boolean, a date or a list of texts, because
  the check that reads it expects a number.
- Filters are named dimensions. A rule stores them as `filters: BTreeMap<String, Vec<FilterItem>>`. An empty
  list counts as no filter and is not stored. A rule changed by hand with an empty list stored reads the
  same: `filter`, `admits` and `applies` treat it as no filter. `Rule::applies` takes one argument per dimension by field name
  and is true when every dimension the rule filters on contains the argument for it. A dimension the rule
  filters on and the caller leaves out does not match. A dimension the rule does not filter on matches
  anything. An integer filter refuses a float with a message that names the field, so `print_scales = [4000.0]`
  stays an error, as it was in courseplan.
- The loader refuses a rule that cites `proposal_doc` and has no note. Both apps checked this in a test over
  their embedded profiles; the loader now checks it for every profile.
- `Schema::problems` lists defects of the schema itself, such as a name listed twice or a table of an unknown
  key, for a test in the app, as `Catalog::problems` does for catalogs.
- In the apps stay the list of keys, the embedded profiles, typed access to values, completeness rules (print's
  `Incomplete`), resolution per map kind, `gap_range` and `number_gap`, and the types `MapKind` and
  `Discipline`. The app turns them into filter arguments. `MapKind` moves to `freemapper-omap` later, which
  freemapper-print brings in after its M3 *(agreed with freemapper-print)*.

## Consequences

- Courseplan switches first (plan Task 2) and keeps a wrapper per function, so its call sites change little.
  Print switches after its M3. Its scale table check and its description cell range become a `TableSchema`
  check and a `value_check`. The loader accepts a list under a value name that print reads as one number,
  because it checks lists item by item. Print can refuse such a list in the same `value_check`.
- A profile that passed courseplan's loader can now fail on the range checks, on a text, boolean, date or
  list of texts under a unit name, or on a proposal without a note. Some `Malformed` messages read differently
  than in courseplan: those for filter values, print scales and a filter that is not a list. Both embedded courseplan profiles load unchanged. Courseplan's snapshot test in
  Task 2 compares the merged sets before and after the switch.
- A new filter dimension or key is a change to the app's schema, not to this crate.
- `RulesError` has a new variant, so an app that matches on it exhaustively, as courseplan's translated error
  texts do, needs a text for `Ambiguous`.
- Cost if wrong: if a third app needs a check the schema cannot express, such as a range that depends on two
  values, it writes a `value_check` or checks after loading. If that happens often, the schema needs a new
  field and this crate a minor version.

Source: freemapper-courseplan ADR 0012 and 0025, freemapper-print ADR 0031, plan
`docs/superpowers/plans/2026-11-09-freemapper-rules.md`.

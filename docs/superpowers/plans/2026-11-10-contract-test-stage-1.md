# Contract Test, Stage 1: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A crate `freemapper-contract` in core that holds the contract between freemapper's output and its two
readers as fixtures, expected values, a vocabulary and rules, with a producer test in freemapper and reader
tests in print and courseplan.

**Architecture:** The crate parses a map and a sidecar from text with its private `freemapper-omap`, sorts every
object into one of four classes and derives three things from that: a census (counts and boxes), a vocabulary
(the words of the file) and a list of rule violations. Its public API is text and plain data only. Fixtures are
embedded behind a feature. Each app tests itself against the crate; no app sees another app's code.

**Tech Stack:** Rust 2024, `freemapper-omap` (private), `toml`, `serde`, `thiserror`; `tempfile` in tests.

**Spec:** `docs/superpowers/specs/2026-11-10-contract-test-design.md`

## Global Constraints

- Core rules (`CLAUDE.md`): `unsafe_code = "forbid"`, `cargo fmt --all --check`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` green on
  Linux, macOS and Windows. No network. GPL-3.0-or-later.
- Third-party versions come from `[workspace.dependencies]`; this plan adds none.
- Public API of the crate: `&str`, `String`, integers, std collections and the crate's own types. No type of
  `freemapper-omap` in a public signature.
- Map coordinates are OOM's: 1/1000 mm, `i32`, y south. A box is `[min_x, min_y, max_x, max_y]`.
- Contract values (copied from the spec): reserved sources `sheet`, `base`, `north-lines`; legacy source
  `scale-bar`; role key `freemapper:sheet`; parts `peripheral information` and `base`; allowed scales
  schoolyard `[1000]`, sprint `[4000, 5000]`; CRS codes `25832`, `25833`; CRS text `+init=epsg:<code>`;
  sidecar contract keys `map_kind`, `title`, `scale_denominator`, `crs`.
- Source code and this plan are English. ADR, changelog, README and doc comments longer than a sentence go
  through the `unslop` and `humanizer` skills. New terms get their glossary row with the code.
- Git: branch `contract-test` in core, worktree `.worktrees/contract-test`; Conventional Commits with scope
  `contract`; merge `--no-ff`; never force-push. Run `cargo clean` in a worktree before removing it.
- Tasks 8 and 9 belong to the print and courseplan sessions. This session sends them the task text and does
  not edit their repositories.

## Review Focus

1. A map without a part `peripheral information` (older project, or the mapper deleted it in OOM): `census` and
   `Vocabulary::of` succeed with zero furniture. Test in Task 1 and Task 2.
2. A map without any object: every box is `None`, and `expected.toml` without a `[bbox]` entry parses. Test in
   Task 1.
3. An object the mapper drew by hand in a reserved part, without tags: no panic; `check` reports it under
   `SheetPartOnlyFurniture`. Test in Task 3.
4. A sidecar with CRLF line endings, a byte-order mark or a title of blanks only: the first two parse, the
   third violates `TitlePresent`. Test in Task 3.
5. A sidecar with an unknown `map_kind` or without the contract keys: `check` returns violations, not an error;
   `Vocabulary::of` lists only the keys present. Test in Task 2 and Task 3.

---

## File Structure

Core, `crates/freemapper-contract/`:

| File | Responsibility |
|---|---|
| `Cargo.toml` | Crate manifest; feature `fixtures` |
| `src/lib.rs` | Module list, re-exports, `CONTRACT_VERSION`, `ContractError` |
| `src/class.rs` | Reserved words, `ObjectClass`, `classify` |
| `src/census.rs` | `Census`, `ClassCounts`, `ClassBoxes`, `Bbox`, `census` |
| `src/sidecar.rs` | Reading the contract keys of `freemapper.toml` (private) |
| `src/vocabulary.rs` | `Vocabulary` |
| `src/check.rs` | `Rule`, `Violation`, `check` |
| `src/expected.rs` | `Expected` |
| `src/fixtures.rs` | `Fixture`, `fixtures`, `fixture` (feature `fixtures`) |
| `src/testing.rs` | Test-only builder for small map XML |
| `fixtures/v1/<name>/{map.omap,freemapper.toml,expected.toml,vocabulary.toml}` | The fixtures |
| `tests/fixtures.rs` | The fixtures agree with census, vocabulary and rules |

freemapper: tests `contract_*` in the test module of `crates/freemapper/src/project.rs`, where the offline
helpers live. print and courseplan: one test module each, named by their sessions.

---

### Task 1: Crate, classes and census

**Files:**
- Create: `crates/freemapper-contract/Cargo.toml`, `src/lib.rs`, `src/class.rs`, `src/census.rs`, `src/testing.rs`
- Modify: `Cargo.toml` (workspace dependency `freemapper-contract = { path = "crates/freemapper-contract" }`)

**Interfaces:**
- Produces:
  ```rust
  pub const CONTRACT_VERSION: u32 = 1;
  #[derive(Debug, thiserror::Error)]
  pub enum ContractError { #[error("the map cannot be read: {0}")] Map(String),
                           #[error("freemapper.toml cannot be read: {0}")] Sidecar(String) }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ObjectClass { Furniture, Base, NorthLines, Content }
  pub(crate) fn classify(tags: &[(String, String)]) -> ObjectClass;
  pub type Bbox = [i32; 4];
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
  pub struct ClassCounts { pub furniture: usize, pub base: usize, pub north_lines: usize, pub content: usize }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
  pub struct ClassBoxes { pub content: Option<Bbox>, pub furniture: Option<Bbox> }   // None is left out of TOML
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]   // the writer's census.toml
  pub struct Census { pub objects: ClassCounts, #[serde(default)] pub bbox: ClassBoxes }
  pub fn census(omap_xml: &str) -> Result<Census, ContractError>;
  // test only, src/testing.rs:
  pub(crate) struct TestObject { pub tags: Vec<(&'static str, &'static str)>, pub coords: Vec<(i32, i32)> }
  pub(crate) fn test_map(scale: u32, crs_spec: &str, parts: &[(&str, Vec<TestObject>)]) -> String;
  ```
- `classify`: `freemapper:source` equal to `sheet` or `scale-bar` gives `Furniture`, `base` gives `Base`,
  `north-lines` gives `NorthLines`, everything else (also no tag) `Content`. The class comes from the tag alone;
  the part plays no role here.
- `ClassBoxes::content` covers the classes `Content` and `NorthLines`; `furniture` covers `Furniture`. A box is
  taken over every coordinate of an object, control points of curves included.
- `test_map` writes OOM XML version 9 that `freemapper_omap::read_map` accepts; model it on `map_with` in the
  tests of `crates/freemapper-omap/src/read.rs`.

- [x] **Step 1: Write the failing tests** in `src/census.rs`:
  - `classes_come_from_the_source_tag`: one object per source value `sheet`, `scale-bar`, `base`,
    `north-lines`, `osm`, and one without tags; counts are furniture 2, base 1, north_lines 1, content 2.
  - `boxes_cover_content_and_furniture_apart`: content at (0,0)-(10_000,5_000), north line from (0,-2_000) to
    (0,7_000), furniture at (0,20_000)-(4_000,21_000); `bbox.content == Some([0, -2_000, 10_000, 7_000])`,
    `bbox.furniture == Some([0, 20_000, 4_000, 21_000])`.
  - `a_map_without_the_sheet_part_has_no_furniture`: only a default part; furniture 0, `bbox.furniture == None`.
  - `a_map_without_objects_has_no_boxes`: both boxes `None`; `toml::to_string(&ClassBoxes::default())` is empty
    and `toml::from_str::<ClassBoxes>("")` gives the default.
  - `an_unreadable_map_is_an_error`: `census("not xml")` is `Err(ContractError::Map(_))`.
- [x] **Step 2: Run** `cargo test -p freemapper-contract` — fails to compile (no crate yet).
- [x] **Step 3: Implement** the manifest, `lib.rs`, `class.rs`, `census.rs`, `testing.rs` with the signatures
  above. Reserved words are `pub(crate) const` in `class.rs`.
- [x] **Step 4: Run** `cargo test -p freemapper-contract` — 5 passed.
- [x] **Step 5: Commit** `feat(contract): object classes and census of a map`.

### Task 2: Vocabulary

**Files:**
- Create: `src/sidecar.rs`, `src/vocabulary.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `classify`, `test_map`, `ContractError`.
- Produces:
  ```rust
  // sidecar.rs, private to the crate
  pub(crate) struct Sidecar { pub map_kind: Option<String>, pub title: Option<String>,
                              pub scale_denominator: Option<i64>, pub crs: Option<i64>,
                              pub key_types: BTreeMap<String, String> }
  pub(crate) fn read_sidecar(toml_text: &str) -> Result<Sidecar, ContractError>;
  // vocabulary.rs
  #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
  pub struct Vocabulary {
      pub parts: BTreeSet<String>,
      pub sheet_part_sources: BTreeSet<String>,
      pub base_part_sources: BTreeSet<String>,
      pub reserved_part_tag_keys: BTreeSet<String>,
      pub sheet_roles: BTreeSet<String>,
      pub crs_form: String,
      pub symbol_set: String,
      pub sidecar: BTreeMap<String, String>,
  }
  impl Vocabulary {
      pub fn of(omap_xml: &str, sidecar_toml: &str) -> Result<Vocabulary, ContractError>;
      pub fn from_toml(text: &str) -> Result<Vocabulary, ContractError>;
      pub fn to_toml(&self) -> String;
      pub fn differences(&self, other: &Vocabulary) -> Vec<String>;
  }
  ```
- `read_sidecar` strips a leading byte-order mark. A contract key of the wrong type (say `title = 3`) leaves its
  field `None` and still records its type in `key_types`. `key_types` holds only the four contract keys that are
  present, with TOML's type names (`string`, `integer`, …). Text that is no TOML is `ContractError::Sidecar`.
- `reserved_part_tag_keys`: the `freemapper:*` keys on objects in the parts `peripheral information` and `base`.
  Keys in other parts are left out; that set is open.
- `sheet_roles`: every value of `freemapper:sheet`, in any part.
- `crs_form`: the map's CRS text with every digit replaced by `N` (`+init=epsg:NNNNN`); empty without one.
- `differences`: one line per field that differs, naming the field and the words only one side has. Empty when
  equal.

- [x] **Step 1: Write the failing tests** in `src/vocabulary.rs`:
  - `lists_the_words_of_a_map`: parts `default`, `peripheral information`, `base`; furniture tagged `sheet`, one
    of them also `freemapper:sheet=scale-bar`; base tagged `base`; content tagged `freemapper:source=osm` and
    `freemapper:osm=x`; CRS `+init=epsg:25833`; sidecar with the four keys plus `buffer_m = 20.0`. Expect
    `sheet_part_sources == {sheet}`, `base_part_sources == {base}`,
    `reserved_part_tag_keys == {freemapper:sheet, freemapper:source}`, `sheet_roles == {scale-bar}`,
    `crs_form == "+init=epsg:NNNNN"`, `sidecar == {crs: integer, map_kind: string, scale_denominator: integer,
    title: string}`.
  - `the_old_scale_bar_tag_changes_the_vocabulary`: the same map with the scale bar tagged
    `freemapper:source=scale-bar`; `differences` against the first names `sheet_part_sources` and `scale-bar`.
  - `content_tags_do_not_count`: a further content object with `freemapper:alkis=y` leaves the vocabulary equal.
  - `works_without_the_sheet_part_and_without_contract_keys`: only a default part, sidecar `buffer_m = 20.0`;
    the three reserved sets and `sidecar` are empty.
  - `round_trips_through_toml`: `Vocabulary::from_toml(&v.to_toml()) == v`.
  - `a_sidecar_with_bom_and_crlf_parses`: `"\u{feff}title = \"A\"\r\nmap_kind = \"sprint\"\r\n"` gives two keys.
- [x] **Step 2: Run** `cargo test -p freemapper-contract vocabulary` — fails to compile.
- [x] **Step 3: Implement** `sidecar.rs` and `vocabulary.rs`.
- [x] **Step 4: Run** `cargo test -p freemapper-contract` — all pass.
- [x] **Step 5: Commit** `feat(contract): vocabulary of a map and its sidecar`.

### Task 3: Rules

**Files:**
- Create: `src/check.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `classify`, `read_sidecar`, `test_map`.
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Rule { KindKnown, TitlePresent, ScaleAllowed, ScaleMatchesMap, CrsKnown, CrsText,
                  SheetPartOnlyFurniture, FurnitureOnlyInSheetPart, BasePartOnlyBase, BaseOnlyInBasePart,
                  NoLegacySource, RoleOnlyOnFurniture }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Violation { pub rule: Rule, pub detail: String }
  pub fn check(omap_xml: &str, sidecar_toml: &str) -> Result<Vec<Violation>, ContractError>;
  ```
- Rules, each violated at most once per object or key:

  | Rule | Holds when |
  |---|---|
  | `KindKnown` | `map_kind` is `sprint` or `schoolyard` |
  | `TitlePresent` | `title` is a string with a non-blank character |
  | `ScaleAllowed` | `scale_denominator` is in the kind's table (skipped when the kind is unknown) |
  | `ScaleMatchesMap` | `scale_denominator` equals the map's scale |
  | `CrsKnown` | `crs` is 25832 or 25833 |
  | `CrsText` | the map's CRS text is `+init=epsg:<crs>` |
  | `SheetPartOnlyFurniture` | every object in `peripheral information` is `Furniture` |
  | `FurnitureOnlyInSheetPart` | no `Furniture` object lies in another part |
  | `BasePartOnlyBase` | every object in `base` is `Base` |
  | `BaseOnlyInBasePart` | no `Base` object lies in another part |
  | `NoLegacySource` | no object carries `freemapper:source=scale-bar` |
  | `RoleOnlyOnFurniture` | `freemapper:sheet` occurs only on `Furniture` objects |

- `detail` names the part and the object's index in it, or the key and its value.
- A missing part is no violation. An unreadable map or sidecar is `Err`, everything else a `Violation`.

- [x] **Step 1: Write the failing tests** in `src/check.rs`. A helper `good()` returns the map and sidecar of
  `lists_the_words_of_a_map` (kind `schoolyard`, scale 1000, title `Clara-Zetkin-Schule`, crs 25833).
  - `a_map_as_freemapper_writes_it_has_no_violation`.
  - One test per rule, each changing one thing in `good()` and asserting that exactly this rule is reported:
    kind `gym`; title `"  "`; scale 2000 in sidecar and map; sidecar scale 1000 with map scale 4000; crs 4326 in the
    sidecar with `+init=epsg:4326` in the map; map CRS text `EPSG:25833`; an untagged object in `peripheral information`; a `sheet` object in the default
    part; an `osm` object in `base`; a `base` object in the default part; `freemapper:source=scale-bar` on the
    bar (reports `NoLegacySource` only); `freemapper:sheet=scale-bar` on a content object.
  - `sprint_at_3000_is_not_allowed` and `sprint_at_5000_is_allowed`.
  - `a_sidecar_without_contract_keys_gives_violations_not_an_error`: `KindKnown`, `TitlePresent`,
    `ScaleMatchesMap`, `CrsKnown` and `CrsText` are reported; the result is `Ok`.
- [x] **Step 2: Run** `cargo test -p freemapper-contract check` — fails to compile.
- [x] **Step 3: Implement** `check.rs`. The scale table is a private `fn allowed_scales(kind: &str) ->
  Option<&'static [u32]>`.
- [x] **Step 4: Run** `cargo fmt --all --check && cargo clippy --locked --workspace --all-targets -- -D warnings
  && cargo test --locked --workspace` — green.
- [x] **Step 5: Commit** `feat(contract): rules for what freemapper writes`.

### Task 4: Fixture writer in freemapper

**Files:**
- Modify: `crates/freemapper/Cargo.toml` (dev-dependency), `Cargo.toml` (temporary `[patch]`, not committed),
  `crates/freemapper/src/project.rs` (test module)

**Interfaces:**
- Consumes: `freemapper_contract::{Vocabulary, census, check}`; freemapper's test helpers `derived_school`,
  `sprint_with_ways`, `export_starter_project`, `map_path`, `PROJECT_FILE_NAME`, `MapFetcher`.
- Produces: a test `contract_write_fixtures`, marked `#[ignore]`, that writes
  `<out>/<name>/{map.omap,freemapper.toml,vocabulary.toml,census.toml}` for the names `schoolyard-1000` and
  `sprint-4000`, with `<out>` from the environment variable `FREEMAPPER_CONTRACT_OUT`; and a private helper
  `fn contract_project(kind: MapKind) -> (String, String)` that returns map XML and sidecar text.

- [ ] **Step 1:** Check `df -h .`; a freemapper worktree build needs several GB. Create the worktree
  `.claude/worktrees/contract-test` on a branch `contract-test`. Add
  `[patch."https://github.com/HenryJobst/freemapper-core"]` pointing at core's `.worktrees/contract-test`, and
  the dev-dependency on `freemapper-contract` (git, the tag of Task 6; until then only the patch resolves it).
- [ ] **Step 2: Write `contract_project`.** Schoolyard: `derived_school`, then export. Sprint: the project of
  `sprint_with_ways`. Before the export set `title = Some("Clara-Zetkin-Schule")` in the project file (the
  default would be the temporary folder's name). Read the map from `map_path(dir)` and the sidecar from
  `PROJECT_FILE_NAME`.
- [ ] **Step 3: Write a first test** `contract_output_breaks_no_rule`: for both kinds `check(...)` is empty; on
  failure print every violation.
- [ ] **Step 4: Run** `cargo test -p freemapper contract_output_breaks_no_rule`. A violation here is a finding:
  either freemapper breaks its contract (fix freemapper, on its own commit) or the rule is wrong (fix the rule
  in core and say so in the ADR). Do not weaken a rule to make the test pass without one of the two.
- [ ] **Step 5: Write `contract_write_fixtures`** and run it:
  `FREEMAPPER_CONTRACT_OUT=<core worktree>/crates/freemapper-contract/fixtures/v1 cargo test -p freemapper
  contract_write_fixtures -- --ignored`. Each `map.omap` stays under 400 kB.
- [ ] **Step 6: Commit** in freemapper (without the `[patch]`):
  `test(contract): fixture writer and rule check for the suite contract`.

### Task 5: Fixtures in core

**Files:**
- Create: `fixtures/v1/{schoolyard-1000,sprint-4000}/{map.omap,freemapper.toml,expected.toml,vocabulary.toml}`,
  `src/expected.rs`, `src/fixtures.rs`, `tests/fixtures.rs`
- Modify: `Cargo.toml` of the crate (`[features] fixtures = []`; `include` with `fixtures/**`;
  `[[test]] name = "fixtures"` with `required-features = ["fixtures"]`), `src/lib.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
  pub struct Expected { pub contract: u32, pub producer: String, pub map_kind: String, pub title: String,
                        pub scale_denominator: u32, pub crs: u32,
                        pub objects: ClassCounts, #[serde(default)] pub bbox: ClassBoxes }
  impl Expected { pub fn from_toml(text: &str) -> Result<Expected, ContractError>; }
  // feature "fixtures"
  pub struct Fixture { pub name: &'static str, pub omap_xml: &'static str, pub sidecar_toml: &'static str,
                       pub expected_toml: &'static str, pub vocabulary_toml: &'static str }
  impl Fixture {
      pub fn expected(&self) -> Expected;
      pub fn vocabulary(&self) -> Vocabulary;
      /// Writes `<name>.omap` and `freemapper.toml` into `dir` and returns the map's path.
      pub fn write_into(&self, dir: &Path) -> std::io::Result<PathBuf>;
  }
  pub fn fixtures() -> &'static [Fixture];
  pub fn fixture(name: &str) -> Option<&'static Fixture>;
  ```
- `expected.toml`: `contract = 1`, `producer = "freemapper <version> (<commit>)"`, kind, title, scale and crs
  from the sidecar, `[objects]` and `[bbox]` from the writer's `census.toml`. Before committing, open each
  fixture in a text editor and count the objects of the parts `peripheral information` and `base` by hand
  against `[objects]`; say in the commit message that this was done. `census.toml` is not committed.

- [ ] **Step 1: Write the failing tests** in `tests/fixtures.rs`, each looping over `fixtures()`:
  - `there_are_two_fixtures_named_as_the_spec_says`: names `schoolyard-1000`, `sprint-4000`.
  - `every_fixture_breaks_no_rule`: `check` is empty.
  - `every_vocabulary_file_matches_its_map`: `Vocabulary::of(...) == fixture.vocabulary()`.
  - `every_expected_file_matches_its_map`: `census(...)` equals `expected.objects` and `expected.bbox`;
    `expected.contract == CONTRACT_VERSION`; kind, title, scale and crs equal the sidecar's values.
  - `the_boxes_keep_the_furniture_outside_the_content`: `bbox.furniture` reaches further south than
    `bbox.content` (the scale bar lies below the map).
  - `write_into_puts_map_and_sidecar_side_by_side`: in a `tempfile::tempdir()` the returned path is
    `<dir>/<name>.omap`, and `<dir>/freemapper.toml` holds the sidecar text.
  - `every_map_stays_under_400_kb`.
- [ ] **Step 2: Run** `cargo test -p freemapper-contract --features fixtures --test fixtures` — fails to compile.
- [ ] **Step 3: Implement** `expected.rs` and `fixtures.rs` (`include_str!`), write the two `expected.toml`.
- [ ] **Step 4: Run** the three checks of Task 3 Step 4, plus
  `cargo test --locked -p freemapper-contract --features fixtures`. Add `--features fixtures` for this crate to
  the test step of `.github/workflows/ci.yml` (`cargo test --locked --workspace --all-features`).
- [ ] **Step 5: Commit** `feat(contract): fixtures schoolyard-1000 and sprint-4000 with expected values`.

### Task 6: Documents and release of core

**Files:**
- Create: `docs/adr/00NN-the-contract-crate.md` (next free number), `crates/freemapper-contract/README.md`
- Modify: `docs/adr/README.md`, `docs/glossary.md`, `CHANGELOG.md`, `CHANGELOG.en.md`, `README.md`,
  `README.en.md`, `CLAUDE.md` (owner table: `freemapper-contract` | freemapper),
  `docs/superpowers/specs/2026-11-10-contract-test-design.md` (anything Task 4 Step 4 changed)

- [ ] **Step 1: ADR.** Decision: the contract lives in core as fixtures, expected values, vocabulary and rules;
  text-only API; the producer compares vocabulary and rules, not coordinates; readers assert their own
  derivations against `expected.toml`. Cost if wrong. `Source:` the user's approval of 2026-11-10 and the
  review of freemapper `0f25841`. Copy the two contract tables from the spec.
- [ ] **Step 2: Glossary rows** (German and English): Vertrag/contract, Erzeuger/producer, Leser/reader,
  Fixture/fixture, Vokabular/vocabulary (`Vocabulary`), Bestandsaufnahme/census (`Census`),
  Randangaben/sheet furniture (`ObjectClass::Furniture`), Objektklasse/object class (`ObjectClass`). Where an
  app's glossary already has the term, take its row unchanged.
- [ ] **Step 3: Changelog** under "Unveröffentlicht"/"Unreleased", crate `freemapper-contract`: what it is and
  how an app uses it (dev-dependency with the feature `fixtures`). READMEs: one row in the crate table.
- [ ] **Step 4: Run `unslop` and `humanizer`** over ADR, changelog entries, README rows and the crate README.
- [ ] **Step 5: Merge and release** as `CLAUDE.md` says: merge `--no-ff` into `main`, next minor version
  (v0.8.0 while `main` is at v0.7.x), `cargo update -w`, date the changelog, annotated tag, push `main` and the
  tag to both remotes, watch CI on the three systems. `cargo clean` in the worktree, then remove it.

### Task 7: Producer test in freemapper

**Files:**
- Modify: `crates/freemapper/Cargo.toml` (dev-dependency pinned to the tag of Task 6, features `fixtures`),
  `crates/freemapper/src/project.rs` (test module), `CHANGELOG.md`, `CHANGELOG.en.md`,
  `docs/adr/00NN-contract-test-against-core.md`, `docs/adr/README.md`

**Interfaces:**
- Consumes: `freemapper_contract::{fixture, Vocabulary, check}`, `contract_project` from Task 4.

- [ ] **Step 1: Write the failing test** `contract_vocabulary_matches_the_fixture`: for (`Schoolyard`,
  `schoolyard-1000`) and (`Sprint`, `sprint-4000`), `Vocabulary::of(map, sidecar)` equals
  `fixture(name).unwrap().vocabulary()`; on failure print `differences` and the sentence "A change to these
  words is a change to the suite contract: refresh the fixtures in freemapper-core first (core ADR 00NN)."
- [ ] **Step 2: Run** it with the `[patch]` removed and the tag pinned — passes. Then prove it bites: change
  `SCALE_BAR_ROLE` handling locally so the bar is tagged `freemapper:source=scale-bar`, run, see both
  `contract_vocabulary_matches_the_fixture` and `contract_output_breaks_no_rule` fail, revert.
- [ ] **Step 3: ADR and changelog** (through `unslop` and `humanizer`): freemapper's output is checked against
  core's contract in every test run; how to refresh the fixtures.
- [ ] **Step 4: Run** freemapper's full checks (`cargo fmt --all --check`, clippy `-D warnings`,
  `cargo test --locked --workspace`).
- [ ] **Step 5: Commit, merge, push**; watch CI on the three systems. `cargo clean` in the worktree, remove it.

### Task 8: Reader test in freemapper-print (print session)

**Interfaces:**
- Consumes: `freemapper_contract::{fixtures, Fixture::write_into, Fixture::expected}` as a dev-dependency with
  the feature `fixtures`; print's own `infer_kind`, `read_sidecar_title`, `is_sheet_object`, `is_base_object`,
  `symbol_factor`, `painted_bounds`.

- [ ] Tests in a `#[cfg(test)]` module inside the crate (the two `is_*` functions are private), every test name
  starting with `contract_`, each looping over `fixtures()` after `write_into` a temporary folder and reading
  the map with print's own reader:
  - `contract_kind`: `infer_kind(&map, &path)` names `expected.map_kind`.
  - `contract_title`: `read_sidecar_title(&path)` is `expected.title`.
  - `contract_classes`: the number of objects `is_sheet_object` accepts is `expected.objects.furniture`, the
    number `is_base_object` accepts is `expected.objects.base`.
  - `contract_scale`: `symbol_factor(kind, expected.scale_denominator, expected.scale_denominator)` is `Ok`.
  - `contract_bounds`: the painted bounds of a layout without sheet objects end north of
    `expected.bbox.furniture`'s southern edge by more than the bar's height, and lie inside
    `expected.bbox.content` grown by a margin the session sets for symbol sizes.
- [ ] Changelog entry, commit, CI green.

### Task 9: Reader test in freemapper-courseplan (courseplan session)

**Interfaces:**
- Consumes: the same crate items; courseplan's own `map_kind_for_file`, `title_from_config`,
  `is_sheet_furniture`, `MapIndex::from_document`, `MapIndex::extent`.

- [ ] Tests in a `#[cfg(test)]` module inside the crate, names starting with `contract_`, looping over
  `fixtures()`:
  - `contract_kind`: `map_kind_for_file(&doc, &path)` names `expected.map_kind`.
  - `contract_title`: `title_from_config(dir)` is `Some(expected.title)`.
  - `contract_classes`: the number of objects `is_sheet_furniture` accepts is
    `expected.objects.furniture + expected.objects.base`.
  - `contract_extent`: `MapIndex::from_document(&doc).extent()` lies inside `expected.bbox.content`.
- [ ] Changelog entry, commit, CI green.

---

## Progress

- 2026-11-10: Tasks 1 to 3 done in one commit (the modules depend on each other): 35 tests, fmt, clippy and
  the workspace tests green on macOS. Next: Task 4, which needs a freemapper worktree build.

## After stage 1

Stage 2 (the gate in core's CI) starts with the spike the spec names. The names `contract_*` are what the gate
will run: `cargo test -p <crate> contract_`.

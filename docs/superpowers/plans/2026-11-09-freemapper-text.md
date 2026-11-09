# freemapper-text Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The program-independent part of freemapper-courseplan's `text.rs` becomes the crate `freemapper-text`
in freemapper-core, and freemapper-courseplan uses it instead of its own copy.

**Architecture:** The crate knows nothing about an app's messages. An app hands it its embedded `.ftl` sources
as a `Catalog`. The crate provides language detection, lookup with English fallback, the `Localize` trait,
and checks an app runs in its tests (same ids and variables in every language, every message formats).

**Tech Stack:** Rust 2024; `fluent-bundle =0.16.0`, `fluent-syntax =0.12.0`, `unic-langid =0.9.6` (the versions
freemapper-courseplan uses, ADR 0004).

**Spec:** freemapper-core ADR 0001 and CLAUDE.md; freemapper-courseplan ADR 0022 (Fluent) and the module doc of
`crates/freemapper-courseplan/src/text.rs` (detection order, fallback, what is not translated).

## Global Constraints

- Behaviour stays exactly as in freemapper-courseplan v0.4.0:
  - Detection order: `--lang`, `FREEMAPPER_LANG`, `LC_ALL`, `LC_MESSAGES`, `LANG`, else English.
  - Only the language part counts, case-insensitively. Empty values, `C` and `POSIX` are skipped.
  - English is the fallback, then the id itself.
  - Fluent isolation marks are off.
- The crate has no app-specific ids, words or error types.
- Lints: `unsafe_code = "forbid"`, clippy `-D warnings`.
- Doc comments are English. README and changelog lines are bilingual and go through `unslop` and `humanizer`.
- `Cargo.lock` starts from freemapper-courseplan's lock (copy it, then `cargo build`), so no transitive version
  younger than two weeks slips in.

## Review Focus

1. Two catalogs in one process (two apps' tests, or a test catalog next to the app's): each keeps its own bundles.
   - Test: `two_catalogs_do_not_share_bundles`.
2. A source with a Fluent syntax error: `problems()` names the language and the error.
   - Test: `problems_report_syntax_errors`.
3. A catalog whose first entry is not English: refused by `problems()`. `text` still falls back to the first
   entry.
   - Test: `problems_require_english_first`.
4. `detect_lang` with an env closure that panics on unknown names. Only the five names are read.
   - Test: covered by `detects_language_in_order`, using the exact names.

---

### Task 1: The crate `freemapper-text` in freemapper-core

**Files:**
- Create: `crates/freemapper-text/Cargo.toml`, `crates/freemapper-text/src/lib.rs`.
- Create: `crates/freemapper-text/tests/fixtures/{en,de}.ftl`, small test catalogs.
- Modify: `Cargo.toml` (`[workspace.dependencies]`), `Cargo.lock`, `README.md`/`README.en.md` (status row:
  "enthalten"/"included"), `CHANGELOG.md`/`CHANGELOG.en.md`, `docs/glossary.md`.
- Create: `docs/adr/0008-fluent-catalogs-supplied-by-the-app.md`, plus its index row.

**Interfaces (Produces):**
- `pub use fluent_bundle::FluentValue;`
- `pub struct Lang(pub unic_langid::LanguageIdentifier)` with `english()` and `code() -> String` (derive
  `Debug, Clone, PartialEq, Eq`).
- `pub struct Catalog`:
  - `pub const fn new(sources: &'static [(&'static str, &'static str)]) -> Catalog`. A source is a language
    code and the `.ftl` content, English first. Bundles are built lazily in a `OnceLock`, so an app writes
    `static TEXTS: Catalog = Catalog::new(&[("en", include_str!(…)), …]);`.
  - `pub fn available(&self) -> Vec<Lang>`.
  - `pub fn parse_lang_arg(&self, arg: &str) -> Result<Lang, Vec<Lang>>`.
  - `pub fn detect_lang(&self, arg: Option<&str>, env: &dyn Fn(&str) -> Option<String>) -> Lang`.
  - `pub fn text(&self, lang: &Lang, id: &str, args: &[(&str, FluentValue)]) -> String`.
  - `pub fn problems(&self, sample_args: &[(&str, FluentValue)]) -> Vec<String>`, for app tests. It reports:
    - a syntax error in any source;
    - English not first;
    - message ids that differ from English;
    - variables of a message that differ from English;
    - a message that fails to format with `sample_args` or formats empty;
    - isolation marks in output.
  - `pub fn message_ids(&self, code: &str) -> BTreeSet<String>`.
- `pub trait Localize { fn localize(&self, lang: &Lang) -> String; }`.
- `pub fn ids_in_source(source: &str, prefixes: &[&str]) -> BTreeSet<String>`. It finds the string literals
  above the first `#[cfg(test)]` that start with one of `prefixes`, consist of `[a-z-]` and do not end in `-`.
  Apps use it to test that every id their code names exists.

- [ ] **Step 1:** Write the failing tests, ported from freemapper-courseplan `text.rs`, against the fixture
  catalogs:
  - `detects_language_in_order`
  - `unknown_lang_argument_is_refused`
  - `missing_message_falls_back_to_english`, plus an unknown id coming back as the id
  - `two_catalogs_do_not_share_bundles`
  - `problems_report_syntax_errors`
  - `problems_report_missing_ids_and_variables`
  - `problems_require_english_first`
  - `clean_catalog_has_no_problems`
  - `ids_in_source_skips_tests_and_partial_ids`
- [ ] **Step 2:** Implement by moving the code. Function bodies stay as in `text.rs` where they exist.
- [ ] **Step 3:** ADR 0008: apps supply the catalogs; the crate holds no texts; why `problems()` is a function
  and not a test. Add the glossary rows "Textkatalog | catalog | `Catalog`" and "Sprache | language | `Lang`".
- [ ] **Step 4:** Run fmt, clippy, test. Commit `feat(text): Fluent catalogs, language detection and catalog
  checks`.

### Task 2: freemapper-courseplan uses freemapper-text

Runs in freemapper-courseplan after core's tag `v0.1.0`.

**Files:**
- Modify: freemapper-courseplan `Cargo.toml` (workspace dependency
  `freemapper-text = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.1.0" }`) and
  `crates/freemapper-courseplan/Cargo.toml`. Drop `fluent-bundle`/`unic-langid` if they are no longer used
  directly. Keep `fluent-syntax` only if a remaining test needs it.
- Modify: `crates/freemapper-courseplan/src/text.rs`:
  - It keeps `static TEXTS: Catalog` with the embedded files, the `Localize` impls, `word`, `place_*` and the
    app-specific tests.
  - It re-exports `Lang`, `FluentValue` and `Localize`, and keeps the free functions `text`, `available`,
    `parse_lang_arg` and `detect_lang` as one-line wrappers. Callers stay unchanged.
  - The generic tests are replaced by one test that asserts `TEXTS.problems(&sample_args())` is empty.
    `ids_used_in_code_exist` uses `freemapper_text::ids_in_source`.
- Modify: the module doc. Provenance now says the generic part lives in freemapper-core.
- Modify: `docs/adr/0006-dependencies-at-least-two-weeks-old.md`. A line exempts the suite's own crates,
  following core ADR 0004.
- Modify: changelogs, under Changed: "texts come from freemapper-text", no user-visible change.

- [ ] **Step 1:** Switch the dependency. During development, use a `[patch]` to `../freemapper-core`, and remove
  it before merge.
- [ ] **Step 2:** Run `cargo test --workspace`. Every text test and the CLI golden outputs stay green unchanged.
- [ ] **Step 3:** Commit `refactor(text): use freemapper-text from freemapper-core`.

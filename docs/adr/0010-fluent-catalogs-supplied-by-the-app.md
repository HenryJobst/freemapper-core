# 0010. Fluent catalogs are supplied by the app

Date: 2026-11-09
Status: Accepted

## Context

freemapper-courseplan keeps its program texts in Fluent files, one per language, and has code for language
detection, lookup with an English fallback and tests that compare the catalogs. freemapper and freemapper-print
need the same, and the first draft was a copy of that file. The texts themselves differ per app: each has its
own messages and its own words.

## Decision

- `freemapper-text` holds no texts and no message ids. An app embeds its `.ftl` files with `include_str!` and
  builds a `Catalog` from them, usually in a `static`. English comes first and is the reference and fallback.
- A `Catalog` builds its bundles on first use, so `Catalog::new` is a `const fn`. The bundles are the
  concurrent kind, which makes the catalog `Sync`. Two catalogs in one program do not share state.
- Detection order, fallback to English and then to the id, and switched-off isolation marks stay as they were
  in freemapper-courseplan 0.4.0.
- The catalog checks are a function, `Catalog::problems`, not a test in this crate. The crate cannot know the
  app's catalogs, and a function lets each app run the checks in its own test, with sample values for its own
  variables, and print every defect at once. It returns plain English lines that name the language and the
  message id. `ids_in_source` supports the second half of that test: it lists the ids named in the app's code,
  and the app checks that each one exists.
- The fallback goes to the source with the code `en`, as in freemapper-courseplan. A catalog without such a
  source has no fallback, and `text` returns the id for a message the requested language lacks. `problems`
  requires English as the first source.
- An invalid language code does not panic, as the first draft did: `available()` skips it and `problems`
  reports it, together with codes that are not in canonical form (`DE`) and codes that appear twice.
  `message_ids` tolerates syntax errors and returns the ids of the valid messages; `problems` reports the errors
  with their line numbers.

## Consequences

- Each app writes one short test that calls `problems` and asserts that the list is empty. A catalog that the
  test does not cover is not checked.
- An app that adds a language adds a file and a line in its source list; this crate does not change.
- `Localize` stays a trait here, but the implementations for an app's error types live in the app.
- The crate depends on `fluent-bundle`, `fluent-syntax` and `unic-langid`, pinned to the versions in
  freemapper-courseplan's lock file.

Source: freemapper-courseplan ADR 0022.

# Contract test for the suite: design

Date: 2026-11-10
Status: Approved by the user on 2026-11-10, with the four points under "For the user to decide" as proposed.
Plan for stage 1: `docs/superpowers/plans/2026-11-10-contract-test-stage-1.md`.
Author: session freemapper, after the code review of freemapper `0f25841` (v0.9.0+3).

## Why

freemapper writes a map (`<name>.omap`) and a sidecar (`freemapper.toml`). freemapper-print and
freemapper-courseplan read both. What the readers may rely on is written down in no single place. It lives in
ADRs of three repositories and in messages between the sessions, and no test checks it.

The review of 2026-11-10 found what that costs:

- v0.9.0 changed the scale bar's tag from `freemapper:source=sheet` to `scale-bar`, one day after `sheet` was
  agreed as stable. print printed the bar as map content, courseplan counted it into the map extent. Each
  repository's own tests stayed green.
- `MapKind` is defined three times (freemapper: 2 variants, print and courseplan: 3 each), with three ways to
  decide the kind of a given map.
- The map title was derived in three different ways.
- freemapper accepted scales print refuses.
- `luebeck.omap` is a test fixture in core, print and courseplan, and the three copies have three different
  checksums.

All of these were fixed the same day (freemapper v0.9.1, courseplan v0.7.3, print `5367708`). Nothing keeps
them from coming back. The contract test is that guard.

Prior art in this repository: the shared number placement cases of `freemapper-course` (ADR 0016). They do for
courseplan and print what this design does for the map and the sidecar.

## Goals

1. One written contract for the map and the sidecar, in core, in a form a test can run.
2. A change in freemapper's output that touches the contract fails in freemapper's own CI.
3. A contract change that breaks a reader fails before it is released, not in a later review.
4. Readers keep reading files that older freemapper versions wrote.
5. A new value such as `map_kind = "gym"` does not break a reader that does not know it yet.

## Non-goals

- No byte-for-byte golden of a whole map. Map content changes every week; that must not need a core release.
- No statement about map quality (whether the right objects are on the map).
- The course formats between courseplan and print (.ppen, IOF XML) stay with `freemapper-course`.
- OCAD and PDF output of freemapper are not part of this contract; no other program of the suite reads them.

## Anti-goals

- A contract so wide that a symbol tweak in freemapper forces a core release.
- A reader test that imports another reader's code.
- Core types leaking through the contract crate into an app that pins another core tag (see "Text-only API").
- A megabyte of fixture XML compiled into the release build of an app.

## The contract, version 1

This is what the review and the fixes of 2026-11-10 establish. The crate turns each line into a rule or an
expected value.

### Sidecar `freemapper.toml`, next to the map

| Key | Written by freemapper | Readers |
|---|---|---|
| `map_kind` | `"sprint"` or `"schoolyard"` | Take it before any inference. Also know `"forest"`. An unknown value counts as absent and is no error. |
| `title` | Always, never empty (since v0.9.1) | Default title. Fall back to the map's file stem only when the key is missing (older projects). |
| `scale_denominator` | One of the allowed scales of the kind: schoolyard 1000; sprint 4000, 5000 | Equal to the scale in the map's georeferencing. |
| `crs` | EPSG code, 25832 or 25833 | Informative. |
| every other key | freemapper's own records | Ignored. An unknown key is never an error. |

### Map file

| Fact | Meaning for readers |
|---|---|
| `freemapper:source=sheet` | Sheet objects (frame, title block, scale bar, credit line). Not map content, not part of the map extent. |
| `freemapper:sheet=<role>` | Role of a sheet object, today only `scale-bar`. Readers never need it to recognise a sheet object. |
| `freemapper:source=base` | The base area. Its own class: printed, but no feature. |
| `freemapper:source=north-lines` | North lines. Map content. |
| any other `freemapper:source` | Identifier of a data source. Map content. The set is open. |
| part `peripheral information` | Holds the sheet objects and nothing else. |
| part `base` | Holds the base area and nothing else. |
| CRS text | `+init=epsg:<code>` |

### Legacy

Files written by freemapper v0.9.0 carry `freemapper:source=scale-bar` on the scale bar. Readers treat it as
sheet objects. Such files exist (the examples of v0.9.0), so this stays in the contract as a legacy fixture.

## Design

### A new crate `freemapper-contract`

Owner: the freemapper session, because freemapper produces the format (ADR 0005). Dependencies: `freemapper-omap`
(private), `toml`, `serde`, `thiserror`. No network (ADR 0007).

It has four parts.

**1. Fixtures.** Under `fixtures/v1/<name>/`: `map.omap`, `freemapper.toml`, `expected.toml`, `vocabulary.toml`.

| Fixture | Content |
|---|---|
| `schoolyard-1000` | Current freemapper output for a small school area |
| `sprint-4000` | Current freemapper output, same area, kind sprint |
| `legacy-0.9.0-scale-bar` | A v0.9.0 map with the old scale bar tag |
| `future-kind` | `map_kind = "gym"`, one unknown sidecar key, one unknown `freemapper:sheet` role |

The area is small (a few dozen objects). The symbol set makes up most of a file: about 260 kB. Budget: 400 kB
per fixture.

**2. Expected values.** `expected.toml` says what every reader must find. Its counts and boxes come from the
crate's own census of the fixture and are checked by hand against the file before they are committed. The
numbers below only show the form. Boxes are in map coordinates (1/1000 mm, y south).

```toml
contract = 1
producer = "freemapper 0.9.1 (513fc54)"
map_kind = "schoolyard"      # "unknown" in the fixture future-kind
title = "Clara-Zetkin-Schule"
scale_denominator = 1000
crs = 25833

[objects]                    # by class, counted over all parts
sheet = 9
base = 1
north_lines = 14
content = 212

[bbox]                       # min_x, min_y, max_x, max_y; content includes the north lines
content = [-61200, -48000, 61200, 47500]
sheet = [-66000, -55000, 66000, 74900]
```

**3. Vocabulary.** `vocabulary.toml` lists the words of a file, without its content:

- the part names,
- per part `peripheral information` and `base`: the `freemapper:source` values and the `freemapper:*` tag
  keys found there,
- every value of `freemapper:sheet`,
- the form of the CRS text and the symbol set name,
- for the sidecar: the contract keys present and their types.

`Vocabulary::of(omap_xml, sidecar_toml)` computes it for any file. Tags of objects in content parts are left
out, since that set is open and belongs to freemapper alone.

**4. Rules.** `check(omap_xml, sidecar_toml) -> Vec<Violation>` holds the table above as code: the sidecar
parses, the kind is known to the producer, the scale is allowed for the kind and equals the map's, the title is
present, every object in `peripheral information` is tagged `sheet`, no object outside it is, and so on. A
`Violation` names the rule and the object.

`check` judges what freemapper writes today. The fixtures `schoolyard-1000` and `sprint-4000` must pass it in
core's own tests. `legacy-0.9.0-scale-bar` and `future-kind` are for the readers only; they break rules on
purpose and are exempt.

### Text-only API

The public API takes and returns text and plain data: `&str` for the map and the sidecar, `Expected`,
`Vocabulary`, `Violation`. It never exposes a type of `freemapper-omap`.

Reason: the apps pin different core tags (today freemapper v0.5.1, print v0.6.0, courseplan v0.7.0). A reader
must parse the fixture with the reader it ships, and the CI gate below builds an app against a newer contract
crate than the rest of its core crates. With core types in the API that would not compile.

The fixtures sit behind a cargo feature `fixtures` and are embedded with `include_str!`, so a git dependency
carries them and a release build never does.

### Three tests

**Producer, in freemapper.** Builds a project offline with the existing test helpers (`derived_school` with a
mocked fetcher), writes map and sidecar, then asserts:

1. `check(...)` returns no violation.
2. `Vocabulary::of(...)` equals the `vocabulary.toml` of the matching fixture.

It compares no coordinates and no object counts. Map content may change freely. A new tag key, a new reserved
source value, a renamed part or a new sidecar key changes the vocabulary and fails the test. The v0.9.0 change
would have failed here: the part `peripheral information` would have listed `sheet` and `scale-bar`.

An ignored test (or a small example binary) writes the four fixture files into a folder named by an
environment variable. That is how a fixture is refreshed.

**Reader, in print** (a test module inside the crate, test names starting with `contract_`). For every fixture: read the map with print's own reader and assert
against `expected.toml`:

- the map kind and the title print derives,
- the number of objects `is_sheet_object` and `is_base_object` accept,
- that the painted bounds of a layout without sheet objects stay inside `bbox.content` plus a margin for
  symbol sizes, and do not reach the scale bar,
- that every scale the contract allows for the kind is accepted by print's rule profile.

**Reader, in courseplan** (a test module inside the crate, names starting with `contract_`). The same idea with its own functions: `map_kind_for_file`,
the title, `is_sheet_furniture` counts (sheet objects plus base), and `MapIndex::extent()` inside
`bbox.content`.

Each reader asserts what it derives itself. The contract does not ask the two readers to compute the same
extent, only to agree with the classes and boxes in `expected.toml`.

### The gate in core's CI

A reader's test runs in the reader's CI only when the reader raises its core pin. Until then a fixture change
is unchecked. A job in core closes that gap:

- It runs when files under `crates/freemapper-contract/` change, on Linux only.
- It checks out `main` of print and of courseplan, patches `freemapper-contract` to the pull request's checkout
  (`cargo test --config 'patch."https://github.com/HenryJobst/freemapper-core".freemapper-contract.path=…' contract_`)
  and runs the reader tests. The tests sit inside the crates because they use private functions.
- Red means: this fixture or rule change breaks a reader as it is on `main`.

freemapper's producer test runs in freemapper's CI on every push, as today's tests do.

### How a contract change travels

1. freemapper changes its output on a branch. Its producer test fails.
2. The session refreshes the fixtures with the ignored test and opens a branch in core with the new fixtures,
   vocabulary, rules and expected values.
3. Core's gate runs the readers against it.
   - Green: the change is additive for the readers. Merge, tag core, raise the pin in freemapper, merge there.
   - Red: a reader must learn the new form first. The reader's session adds it and merges; then the gate turns
     green. Readers first, producer second.
4. A change that removes or renames something readers use raises `contract` to 2. The v1 fixtures stay in the
   tree, and readers keep passing them for as long as files of that version are in use.

The agreement between sessions becomes a branch in core that a test accepts or refuses. Messages stay useful
for announcing; they stop being the only record.

## Stages

| Stage | Content | Touches |
|---|---|---|
| 1 | Crate with `schoolyard-1000`, `sprint-4000`, expected values, vocabulary, `check`; producer test; reader tests; ADR, glossary rows, changelog | core, then the three apps (tests and a dev-dependency only) |
| 2 | Gate job in core's CI | core |
| 3 | Fixtures `legacy-0.9.0-scale-bar` and `future-kind`; print stops treating an unknown `map_kind` as an unusable sidecar | core, print |
| 4 | Shared types in the crate (`MapKind` with an unknown variant, sidecar reader, tag constants, scale table) replace the three definitions | core, all three apps |

Stage 1 changes no behaviour of any app. Stage 3 changes print: today an unknown `map_kind` makes the whole
sidecar unusable, so the kind is inferred and the title is lost. Stage 4 is the larger step and can wait for
freemapper's move to the core crates (its plan stage B2); it should be done before the gym map adds a kind.

## What each of today's findings would have met

| Finding | Caught by |
|---|---|
| Scale bar tagged `scale-bar` | Producer test (vocabulary); reader tests (counts, bounds) |
| Kind decided differently in courseplan | Reader tests (`map_kind`) |
| Three titles | Rule "title present" in the producer; reader tests (`title`) |
| Scales print refuses | Rule "scale allowed"; print's test "every allowed scale is accepted" |
| Symbols at 1:3000 | The scale table: 3000 is not in it |
| Unknown `map_kind` breaks the sidecar in print | Fixture `future-kind` |
| Empty polygon panic, dedup chain, repeated document builds, language choice | Not contract matters. They stay with freemapper's own tests. |

## My calls

Decided here without asking; each can be overruled.

- The crate is named `freemapper-contract` and owned by the freemapper session.
- The API is text-only; core types stay private to the crate.
- The producer test compares vocabulary and rules, not coordinates.
- `expected.toml` is filled from the crate's census and checked by hand, and a test in core keeps the two equal.
- The gate runs on Linux only and only for changes to the crate.
- Fixture layout `fixtures/v<contract>/<name>/`, embedded, behind the feature `fixtures`.

## For the user to decide

1. **The gate in core's CI.** It builds print and courseplan in core's CI for every contract change (a few
   minutes each). Without it stage 1 still helps, but a breaking fixture change shows only when a reader
   raises its pin.
2. **Readers first.** A reader that must learn a new form releases before freemapper releases the new output.
   That makes freemapper wait for a reader's release in the rare breaking case.
3. **Unknown `map_kind` in print (stage 3).** Proposed: treat it as absent and keep the title. The alternative
   is a clear error that names the value and asks for a newer print.
4. **When stage 4 happens.** Proposed: together with freemapper's move to the core crates, before the gym map.

## Left to the builder

- A spike for the gate before stage 2 is planned in detail: patching one crate of a git source while the app
  takes the others from a tag is untested here. Core pins its third-party versions to the apps' lock files,
  which should let the versions resolve; the job runs without `--locked`. If the patch does not work, the job
  rewrites the dependency line in the checked-out app instead.
- The exact margins for symbol sizes in the readers' bounds assertions.
- Whether the fixture refresh is an ignored test or an example binary in freemapper.
- Whether a sprint project can come from the same offline helper as the school project, or needs its own
  mocked inputs.
- The wording of the `Violation` messages (through `unslop` and `humanizer` once they are longer than a phrase).
- The ADR (next free number in `docs/adr/`), the glossary rows (contract, fixture, vocabulary, sheet objects,
  reader, producer) and the changelog entries, written in the same branch as stage 1.
- Whether the three copies of `luebeck.omap` are replaced by one in core. It is a course fixture, not a
  contract fixture, and belongs to `freemapper-course`'s owner.

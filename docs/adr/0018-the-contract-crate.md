# 0018. The contract crate

Date: 2026-11-10
Status: Accepted

## Context
freemapper writes a map (`<name>.omap`) and a sidecar file (`freemapper.toml`). freemapper-print and
freemapper-courseplan read both. What the readers may rely on was written down nowhere, and each repository
tested only itself. A review of freemapper `0f25841` (v0.9.0) found the result: freemapper had tagged the
scale bar `freemapper:source=scale-bar` while both readers knew only `sheet`, so neither recognised the bar as
a sheet object. The same review found a title only freemapper knew, a `map_kind` courseplan guessed from the
file name, and scales on which freemapper and print disagreed. Each was fixed in its own repository. No test
would have caught the next one.

The apps pin different tags of core (freemapper v0.5.1, print v0.6.0, courseplan v0.7.0), and freemapper
writes maps with its own writer, without `freemapper-omap`.

## Decision
- `freemapper-contract` holds contract version 1 (`CONTRACT_VERSION`). freemapper owns it (ADR 0005), because
  freemapper decides what it writes.
- The crate sorts every map object into an object class by its tag `freemapper:source` alone: sheet object,
  base area, north line or content. From that it derives a census (counts per class, boxes of content and of
  sheet objects in map coordinates), a vocabulary and the violations of twelve rules (`check`).
- The vocabulary is the set of words in the file without its content: part names, the `freemapper:source`
  values and `freemapper:*` tag keys in the two reserved parts, the roles of sheet objects, the form of the
  CRS text, the symbol set and the contract keys of the sidecar with their types. Tags of map content are left
  out. That set grows with every data source and concerns no reader.
- The API takes text and returns plain data. No type of `freemapper-omap` appears in it, so an app can read
  the same text with the reader of the core tag it pins.
- With the feature `fixtures` the crate embeds two maps exactly as freemapper 0.9.2 wrote them,
  `schoolyard-1000` and `sprint-4000`, each with `expected.toml` (what a reader must derive) and
  `vocabulary.toml` (what the producer must keep). An ignored test in freemapper writes them.
- freemapper tests its output in every test run: `check` finds no violation, and the vocabulary equals the
  fixture's. It compares no coordinates, so new map content never fails the test.
- Each reader tests itself: it writes a fixture into a folder, reads it with its own code and compares kind,
  title, class counts and extent with `expected.toml`. The tests are named `contract_*`.
- A contract change starts here: new rule or fixture, release of core, then the apps move their pin.

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
| `freemapper:source=sheet` | Sheet object (title, scale, scale bar, credit line). Not map content, not part of the map extent. |
| `freemapper:sheet=<role>` | Role of a sheet object, today only `scale-bar`. Readers never need it to recognise a sheet object. |
| `freemapper:source=base` | The base area. Printed, but no map content and not part of the map extent. |
| `freemapper:source=north-lines` | North lines. Map content. |
| any other `freemapper:source` | Identifier of a data source. Map content. The set is open. |
| part `peripheral information` | Holds the sheet objects and nothing else. |
| part `base` | Holds the base area and nothing else. Only sprint maps have it. |
| CRS text | `+init=epsg:<code>` |

Maps written by freemapper v0.9.0 carry `freemapper:source=scale-bar` on the scale bar. Readers treat it as a
sheet object. `check` reports it (`NoLegacySource`), because freemapper must not write it again.

## Consequences
- freemapper 0.9.2 passed all twelve rules on the first run. No rule was changed to get there.
- The fixtures name templates (`templates/orthophoto.jpg` and others) that are not shipped. A reader must
  cope with missing templates, as it must for any map that was moved without them.
- The two fixtures add about 540 kB to the repository. Most of it is the symbol set inside each map. An app
  that does not ask for the feature compiles none of it.
- An app that takes the crate at a newer tag than its other core crates builds `freemapper-omap` twice in its
  tests. That is the price of the text-only API.
- Cargo resolves a `[patch]` of this crate only when the pinned tag exists in the repository. The tag may
  predate the crate. The gate planned for core's CI (stage 2 of the design) has to work with that.
- Cost if wrong: a rule that is too strict blocks a freemapper release until core is released again. A rule
  that is too loose lets a mismatch through, as before. The reader tests cover two fixtures and no map from
  v0.9.0 yet. A legacy fixture and one with an unknown `map_kind` follow in stage 3.

Source: the user's approval of the design on 2026-11-10, the review of freemapper `0f25841`,
`docs/superpowers/specs/2026-11-10-contract-test-design.md`, freemapper ADR 0085 and 0086.

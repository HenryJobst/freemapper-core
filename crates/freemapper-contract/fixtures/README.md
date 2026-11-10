# Fixtures of the suite contract

Maps and sidecar files as freemapper writes them, for the tests of freemapper, freemapper-print and
freemapper-courseplan (ADR 0018). Each folder under `v1/` holds `map.omap`, `freemapper.toml`, `expected.toml`
(what a reader must derive) and `vocabulary.toml` (the words of the two files).

All four show the same small area, freemapper's offline test project "Clara-Zetkin-Schule". Its objects are test
data built in freemapper's tests; no data was downloaded. The embedded symbol set is ISSprOM 2019 from
OpenOrienteering Mapper (GPL-3.0). Licence of the fixtures: GPL-3.0-or-later, as the rest of this repository.
The maps name templates (`templates/…`) that are not shipped.

| Fixture | Written by | Date | Edited by hand |
|---|---|---|---|
| `schoolyard-1000` | freemapper 0.9.2 (`728fef5`), test `contract_write_fixtures` | 2026-11-10 | no |
| `sprint-4000` | freemapper 0.9.2 (`728fef5`), test `contract_write_fixtures` | 2026-11-10 | no |
| `legacy-0.9.0-scale-bar` | freemapper 0.9.0 (`8004542`), the same export in a checkout of the tag | 2026-11-10 | the title text in the map: the name of the temporary folder became `Clara-Zetkin-Schule` |
| `future-kind` | copy of `sprint-4000` | 2026-11-10 | sidecar: `map_kind = "gym"` and a key `future_key`; map: one sheet object got `freemapper:sheet=legend` |

`legacy-0.9.0-scale-bar` and `future-kind` break rules of `check` on purpose; `breaks` in their `expected.toml`
names the rules. They are for the readers: a reader must cope with a map from v0.9.0 (scale bar tagged
`freemapper:source=scale-bar`, no `title` in the sidecar) and with a kind, a key and a role it does not know.
In `future-kind` the expected kind is `unknown`: the reader takes no kind from the sidecar and reports no error.

To refresh the first two, see freemapper ADR 0089.

# freemapper-core

[Deutsche Version](README.md)

<!-- claude-cost:start -->
[![AI effort (Claude API): $0](https://img.shields.io/badge/AI%20effort%20%28Claude%20API%29-%240-8A2BE2)](docs/ai-cost/history.csv)
<!-- claude-cost:end -->

The libraries were built mostly in the sessions of the apps, above all freemapper and freemapper-courseplan, and that effort is counted there; this badge counts only sessions started directly in freemapper-core.

Shared libraries of the freemapper suite. Code that more than one of the apps needs is kept here once instead of
as a copy in each:

- [freemapper](https://github.com/HenryJobst/freemapper) – maps from open data,
- [freemapper-courseplan](https://github.com/HenryJobst/freemapper-courseplan) – course setting,
- [freemapper-print](https://github.com/HenryJobst/freemapper-print) – printing maps and courses.

## Status

The project is just starting. So far it contains `freemapper-contract`, `freemapper-course`, `freemapper-fs`, `freemapper-georef`,
`freemapper-omap`, `freemapper-render`, `freemapper-rules` and `freemapper-text`; the other libraries are planned:

| Library | Content | comes from | Status |
|---|---|---|---|
| `freemapper-text` | program texts with Fluent, language detection | freemapper-courseplan | included |
| `freemapper-rules` | rule profiles for checks, read and validated against an app schema | freemapper-courseplan, freemapper-print | included |
| `freemapper-georef` | coordinates and coordinate systems: UTM projection (Transverse Mercator) for ETRS89 and WGS84, EPSG:25832 and 25833, forward and inverse, bounding boxes, tile names; no dependencies | freemapper, freemapper-print | included |
| `freemapper-fs` | writing, copying and removing inside the project folder without following a link | freemapper | included |
| `freemapper-omap` | map model, reading .omap, embedded symbol sets ISSprOM 2019 and school map 2019; no writer yet | freemapper, freemapper-print | included |
| `freemapper-render` | map rendering: drawing OOM symbols, the page model, PDF output, the embedded Arimo font | freemapper-print | included |
| `freemapper-contract` | contract between freemapper and its readers: object classes, census, vocabulary and rules for map and sidecar file; with the feature `fixtures`, four maps written by freemapper with their expected values for the apps' tests | freemapper | included |
| `freemapper-course` | courses, reading .ppen and IOF XML, fitting to the map, number placement for a whole course, control description symbols; also writes .ppen, new or as a patch of an existing file | freemapper-print, freemapper-courseplan | included |

## Use

The apps include the libraries they need through git with a fixed version:

```toml
freemapper-text = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.5.1" }
freemapper-rules = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.5.1" }
```

To change an app and a library together, check freemapper-core out next to the app (`../freemapper-core`) and
point the app's `Cargo.toml` at the local folder for the time being:

```toml
[patch."https://github.com/HenryJobst/freemapper-core"]
freemapper-text = { path = "../freemapper-core/crates/freemapper-text" }
freemapper-rules = { path = "../freemapper-core/crates/freemapper-rules" }
```

The entry is for development only. It comes out again before the app is merged, once the change is released in
freemapper-core and the app pins the new version (decision 0006).

Decisions are in [`docs/adr/`](docs/adr/README.md), changes in the [changelog](CHANGELOG.en.md).

## Licence

GPL-3.0-or-later, see [LICENSE](LICENSE).

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

The project is just starting. So far it contains `freemapper-fs` and `freemapper-text`; the other libraries are
planned:

| Library | Content | comes from |
|---|---|---|
| `freemapper-text` | program texts with Fluent, language detection | freemapper-courseplan |
| `freemapper-rules` | format of the rule profiles for checks | freemapper-courseplan |
| `freemapper-georef` | georeferencing, declination, UTM | freemapper |
| `freemapper-fs` | writing, copying and removing inside the project folder without following a link | freemapper |
| `freemapper-omap` | reading and writing .omap, symbol sets | freemapper, freemapper-print |
| `freemapper-render` | map rendering, fonts, PDF | freemapper-print |
| `freemapper-course` | courses, reading .ppen and IOF XML, number placement | freemapper-print, freemapper-courseplan |

## Use

The apps include a library through git with a fixed version:

```toml
freemapper-text = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.1.0" }
```

To change an app and a library together, check freemapper-core out next to the app (`../freemapper-core`) and
point the app's `Cargo.toml` at the local folder for the time being:

```toml
[patch."https://github.com/HenryJobst/freemapper-core"]
freemapper-text = { path = "../freemapper-core/crates/freemapper-text" }
```

The entry is for development only. It comes out again before the app is merged, once the change is released in
freemapper-core and the app pins the new version (decision 0006).

Decisions are in [`docs/adr/`](docs/adr/README.md), changes in the [changelog](CHANGELOG.en.md).

## Licence

GPL-3.0-or-later, see [LICENSE](LICENSE).

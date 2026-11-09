# freemapper-core

[Deutsche Version](README.md)

Shared libraries of the freemapper suite. Code that more than one of the apps needs is kept here once instead of
as a copy in each:

- [freemapper](https://github.com/HenryJobst/freemapper) – maps from open data,
- [freemapper-courseplan](https://github.com/HenryJobst/freemapper-courseplan) – course setting,
- [freemapper-print](https://github.com/HenryJobst/freemapper-print) – printing maps and courses.

## Status

The project is just starting. So far it contains `freemapper-fs`; the other libraries are planned:

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

Decisions are in [`docs/adr/`](docs/adr/README.md), changes in the [changelog](CHANGELOG.en.md).

## Licence

GPL-3.0-or-later, see [LICENSE](LICENSE).

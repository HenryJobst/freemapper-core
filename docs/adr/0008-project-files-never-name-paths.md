# 0008. Project files never name paths

Date: 2026-11-09
Status: Accepted

Moved from freemapper ADR 0059.

## Context

freemapper stores each extra Berlin layer in `data/berlin/<file>.geojson` and records it in `freemapper.toml`
under `berlin_layers`, keyed by the file name. Up to 0.6.0, `fetch` deleted stored layers by the keys it read
from the project file. A project file is plain text that people share and download. A key like `../../../x`
made `fetch` delete `x.geojson` outside the project folder.

The other records in freemapper's project file do not reach a path. Tile names are parsed into numbers and the
path is built from them, template paths are constants, and the map file is named after the project folder.
Names from servers that end up in cache paths (Schleswig-Holstein, North Rhine-Westphalia, Rhineland-Palatinate,
Hamburg, Hesse) are already checked against strict patterns or replaced by names freemapper builds itself.
The other apps read project files and course files that people share in the same way.

## Decision

- The apps build a file path only from names they know. In freemapper these are, for the Berlin layers, the
  `file` names of the static table `berlin::EXTRA_LAYERS`. A record key is first looked up in the table, and
  only the table's name goes into the path. `berlin_layer_path` takes a table entry, not a string, so the
  compiler holds this.
- A name read from a record or a server goes into a path only after a lookup in a fixed table or a strict
  pattern check. `freemapper_fs::is_plain_name` is the common minimum: one plain file name, so no path
  separator, no `..`, no drive prefix, not absolute, no NUL or other control character, not empty.
- Loading a freemapper project drops every `berlin_layers` entry whose key is not a plain file name or not a
  layer of the table. The rest of the project loads. `fetch` and `derive` print
  `warning: freemapper.toml: ignored the Berlin layer entry "…": …` for each, with the key escaped. The next
  save leaves the entry out.
- Before writing or removing a file a project names, the apps resolve its folder
  (`freemapper_fs::stays_inside`). If a link leads it out of the project folder, nothing is written or removed.
  In freemapper a Berlin layer write then ends as
  `warning: Berlin … unavailable (data/berlin leads out of the project folder); layer skipped`.

## Consequences

- A crafted project file can no longer make an app delete or write files outside the project.
- A file left from a key that a later version renames or drops stays on disk. The app no longer deletes it,
  since it does not know the name. No Berlin layer has been renamed so far.
- A freemapper project whose `data/berlin` folder is a link to another drive gets no Berlin layers. That setup
  is unusual, and the warning names the reason.

Source: freemapper ADR 0059 (security hotfix after freemapper v0.6.0, branch `hotfix-layer-paths`).

# 0039. Views of a finished map

Date: 2026-11-20
Status: Accepted

## Context

Trainers use a finished training map with a class of symbols left out: an Ultimate map without paths, contour
orienteering with brown symbols only, a map without north lines. freemapper (ADR 0160 there) writes such variants
as files; freemapper-print applies the same selection when it prints any `.omap`. Both need the same rule, and
neither should copy it.

## Decision

`freemapper-omap` gets the module `view`:

- `View` is a rule on the code and the colours of a symbol: the presets `no-paths`, `brown-only` and
  `no-north-lines`, and the lists `no:<codes>` and `only:<codes>`, combined with `+`. `View::parse` returns a
  `ViewError` that carries the cause as data; each program writes its own messages. `Display` is the canonical
  form (presets in a fixed order, codes sorted), so file names agree between programs.
- `View::keeps(code, colours)` answers for a code without an object too, so freemapper-print can ask for 601 before
  it generates north lines.
- `SymbolColours::of(&SymbolSet)` reads the colour names each symbol is drawn in from the symbol set.
  "Brown only" means every colour is of the family `Brown …`: the lower browns of the paved areas do not count.
- `View::keeps_frame(part_name, tags)` recognises the frame (the part "peripheral information", the tag
  `freemapper:source=sheet`) in a function of its own; no view leaves it out.
- The module writes no file and knows no object model.

## Consequences

- A change to a preset changes freemapper and freemapper-print alike; the minor release of the library carries it.
- The paths of `no-paths` are a list of codes (502 to 507, 501.6 to 501.8, 532), not read from the set. A set with
  other path codes needs the list extended.

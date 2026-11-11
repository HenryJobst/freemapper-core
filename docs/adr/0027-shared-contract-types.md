# 0027. Shared types of the contract

Date: 2026-11-11
Status: Accepted

## Context
The contract crate (ADR 0018) tested what freemapper writes and what the readers derive, but each program
still had its own words for it. The map kind existed three times: in freemapper (`sprint`, `schoolyard`), in
`freemapper-printrules` and in freemapper-courseplan (both with `forest`). The sidecar `freemapper.toml` was
read by three pieces of code with three rules for a broken file. The tag `freemapper:source`, its reserved
values and the part names were string literals in every repository. The drawing scales of a kind stood in
freemapper and again in the rule `ScaleAllowed` here. A new kind such as the gym map would have to be added
in each place, and the tests of ADR 0018 would notice a miss only after the fact.

## Decision
- `freemapper-contract` holds the shared types, and the three apps use them instead of their own:
  - `MapKind` (`sprint`, `schoolyard`, `forest`) with `name`, `from_name`, `base_scale` and
    `drawing_scales`: the scales freemapper writes a kind at, empty for a kind it does not write. The rule
    `ScaleAllowed` reads the same table.
  - `Sidecar` with `parse` (text) and `next_to` (the file beside a map), `SidecarKind` and `SidecarError`.
  - `ObjectClass::of` for an object's tags and `is_map_content`.
  - The module `tags`: tag keys, reserved sources, the role `scale-bar`, the two part names, the sidecar's
    file name and its contract keys.
- An unknown kind is not a variant of `MapKind`. The design named "`MapKind` with an unknown variant"; a
  variant that carries a name would end `Copy` and the exhaustive `match` the apps rely on. The sidecar
  reader returns `SidecarKind::Unknown(name)` instead, so the name survives for a message and no code can
  mistake it for a kind.
- Print scales stay in `freemapper-printrules`. A sprint map drawn at 1:4000 is printed at 1:3000; that
  table belongs to the print rules, and the contract only says which scales a map file may carry.
- The shared types need no map reader. Census, vocabulary and rules move behind the feature `check`, on by
  default, and `fixtures` turns it on. An app that links the types into its program sets
  `default-features = false` and keeps `freemapper-omap` out; its tests take `fixtures` as before.
- The crate's API still names no type of `freemapper-omap`. Tags go in as `&[(String, String)]`.
- A fixture test here reads every sidecar with `Sidecar::parse` and compares with `expected.toml`, so the
  shared reader is tested like a reader of an app.
- The apps move in this order: freemapper (writes with the words of `tags`, takes names and scales from
  `MapKind`), then `freemapper-printrules` re-exports `MapKind`, then print and courseplan drop their sidecar
  readers and their own kind. The last two steps belong to the owners of those crates (ADR 0005).

## Consequences
- This release only adds. The private names `classify` and `read_sidecar` became `ObjectClass::of` and
  `Sidecar::parse`; nothing public was removed, and `Expected` is unchanged, so the gate (ADR 0021) stays
  green without a change in a reader.
- A sidecar number that does not fit `u32` (a negative scale) now counts as absent. `check` reports it as
  "missing or of another type" instead of printing the number.
- A new kind is one new variant here. The compiler then names every `match` in the suite that must decide
  about it.
- freemapper keeps an enum of the kinds it writes, because its code branches on exactly those; it maps each
  to a `MapKind` and a test keeps the two lists equal.
- Until printrules re-exports the type, two enums named `MapKind` exist in core, with the same variants and
  names.
- Cost if wrong: the types are additive. Removing them restores v0.17.0; the apps would return to their own
  definitions.

Source: `docs/superpowers/specs/2026-11-10-contract-test-design.md` (stage 4), the user's go of 2026-11-11,
ADR 0018, ADR 0021.

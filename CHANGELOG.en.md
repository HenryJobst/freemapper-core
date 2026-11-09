# Changes

[Deutsche Version](CHANGELOG.md)

This file lists all notable changes to freemapper-core, newest first, each with the library it affects. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] - 2026-11-09

### Added

- freemapper-rules: rule profiles for checks as TOML files, read and merged into a rule set. The app describes
  in a `Schema` which rule keys, value names, units, tables and filters exist. Moved from
  freemapper-courseplan 0.4.0 with the same file format, the same merge and the same `RulesError` texts. From
  freemapper-print come value names per key, range checks of numbers by unit, keys that take no filter, the
  app's own checks for tables and values, and `RuleSet::pick`, which stops with the new error `Ambiguous` when
  more than one rule matches. Filters are now named by their field (`map_kinds`, `disciplines`, `print_scales`) instead of
  being fixed fields of a rule. A rule that cites the program's own proposals needs a note. Decision 0011.

### Changed

- freemapper-text 0.2.0 no longer has the trait `Localize`. This breaks code written for 0.1.0: an app that
  imports it defines the trait itself, as freemapper-courseplan and freemapper-print already do.

### Removed

- freemapper-text: the trait `Localize`. No app could use it, because the orphan rule forbids implementing it
  for error types from a third crate. The apps keep their own trait; code that imports
  `freemapper_text::Localize` defines it itself (amendment to decision 0010).

## [0.1.0] - 2026-11-09

### Added

- Skeleton: workspace, CI on Linux, macOS and Windows, decisions 0001 to 0007.
- freemapper-text: program texts from Fluent catalogs. The app brings its own `.ftl` files; the library detects
  the language (`--lang`, `FREEMAPPER_LANG`, `LC_ALL`, `LC_MESSAGES`, `LANG`), looks texts up with a fallback
  to English and checks catalogs with `Catalog::problems`. Moved from freemapper-courseplan 0.4.0 with the
  same behaviour; decision 0010.
- freemapper-fs: writes, copies and removals inside the project folder that never follow a link. Each file is
  written as a new partial file and then renamed over the target, so a link at the target is replaced and its
  target stays untouched. A path whose folder leads out of the project folder through a link is refused. Copies
  read only files of the project itself. Moved from freemapper 0.6.2 with the same behaviour, except that the
  error texts name no app and `remove_stale_parts_inside` returns a refused folder as an error instead of
  printing a warning itself. Decisions 0008 and 0009 come along (formerly freemapper 0059 and 0060).

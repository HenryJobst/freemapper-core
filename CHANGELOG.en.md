# Changes

[Deutsche Version](CHANGELOG.md)

This file lists all notable changes to freemapper-core, newest first, each with the library it affects. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Skeleton: workspace, CI on Linux, macOS and Windows, decisions 0001 to 0007.
- freemapper-fs: writes, copies and removals inside the project folder that never follow a link. Each file is
  written as a new partial file and then renamed over the target, so a link at the target is replaced and its
  target stays untouched. A path whose folder leads out of the project folder through a link is refused. Copies
  read only files of the project itself. Moved from freemapper 0.6.2 with unchanged behaviour, together with
  decisions 0008 and 0009 (formerly freemapper 0059 and 0060).

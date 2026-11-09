# 0006. Apps pin a tag and patch locally

Date: 2026-11-09
Status: Accepted

## Context
The apps build on GitHub CI and on the user's Mac. Core is not published on crates.io.

## Decision
An app depends on core with a git dependency pinned to a release tag, for example
`freemapper-text = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.1.0" }`. While
developing against an unreleased change, the app adds a `[patch."https://github.com/HenryJobst/freemapper-core"]`
entry with a path to the local checkout, and removes it before its own merge. The workspace has one version and
one tag per release *(agreed with freemapper and freemapper-print)*.

## Consequences
- CI of an app builds against a fixed, released core.
- An app never merges with a local path in its manifest.
- Cost if wrong: publishing on crates.io later needs free crate names and a publishing step.

Source: agreement of the three app sessions, 2026-11-09.

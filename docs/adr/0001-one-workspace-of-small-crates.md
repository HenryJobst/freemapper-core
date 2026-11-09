# 0001. One workspace of small crates

Date: 2026-11-09
Status: Accepted

## Context
The apps of the freemapper suite copied code from each other. freemapper-print copied freemapper's renderer and
.omap reader. freemapper-print and freemapper copied or planned to copy freemapper-courseplan's Fluent text
core and rule-profile format. Placement, georeferencing and safe file writing exist two or three times. The
copies drift apart. The user decided that shared code goes into a common library rather than being copied, and
suggested this project *(user, 2026-11-09)*.

## Decision
One repository, one Cargo workspace, several small crates under `crates/`, one per topic: texts, rule
profiles, georeferencing, file writing, .omap, rendering, courses *(agreed with freemapper and
freemapper-print)*. A crate depends only on the crates it needs, so an app that needs texts does not build the
PDF writer.

## Consequences
- An app adds only the crates it uses.
- A change that spans crates is one commit in one repository.
- Cost if wrong: if the crates turn out too fine, merging two later is a breaking change for the apps.

Source: user decision and agreement of the three app sessions, 2026-11-09.

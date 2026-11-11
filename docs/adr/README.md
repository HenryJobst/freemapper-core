# Architecture Decision Records

An ADR records one decision that shapes freemapper-core: what was decided, why, and what it costs if it turns
out wrong. Each ADR ends with a `Source:` line. An ADR that moved here from an app names the app's ADR there.

Numbers start at 0001, follow the order of the decisions and never repeat. The file name is
`NNNN-short-kebab-title.md`. A decision that replaces an older one gets a new ADR, and the old one changes its
status to `Superseded by NNNN`.

| ADR | Decision |
|---|---|
| [0001](0001-one-workspace-of-small-crates.md) | One workspace of small crates |
| [0002](0002-gpl-3.0-or-later.md) | GPL-3.0-or-later |
| [0003](0003-toolchain-pinned-with-the-apps.md) | Toolchain pinned with the apps |
| [0004](0004-dependencies-at-least-two-weeks-old.md) | Dependencies at least two weeks old, own crates exempt |
| [0005](0005-the-contributing-session-owns-a-crate.md) | The contributing session owns a crate |
| [0006](0006-apps-pin-a-tag-and-patch-locally.md) | Apps pin a tag and patch locally |
| [0007](0007-no-network-access-in-core.md) | No network access in core |
| [0008](0008-project-files-never-name-paths.md) | Project files never name paths |
| [0009](0009-writes-never-follow-links.md) | Writes never follow links |
| [0010](0010-fluent-catalogs-supplied-by-the-app.md) | Fluent catalogs are supplied by the app |
| [0011](0011-rule-profiles-validated-against-an-app-schema.md) | Rule profiles are validated against an app schema |
| [0012](0012-the-omap-crate.md) | The omap crate |
| [0013](0013-the-course-crate.md) | The course crate |
| [0014](0014-the-ppen-writer.md) | The .ppen writer |
| [0015](0015-the-georef-crate.md) | The georef crate |
| [0016](0016-course-number-placement-shared-with-courseplan.md) | Course number placement shared with courseplan |
| [0017](0017-description-symbols-in-core.md) | Description symbols in core |
| [0018](0018-the-contract-crate.md) | The contract crate |
| [0019](0019-the-render-crate.md) | The render crate |
| [0020](0020-local-web-pages-shell.md) | The shell for local web pages |
| [0021](0021-the-contract-gate.md) | The contract gate |
| [0022](0022-svg-backend-and-hit-test.md) | SVG backend and hit test |
| [0023](0023-page-builder-in-core.md) | The page builder moves to core |
| [0024](0024-course-layer-in-render.md) | The course overprint moves to the render crate |
| [0025](0025-print-rules-in-core.md) | The print rules move to core |
| [0026](0026-the-update-crate.md) | The update crate |
| [0027](0027-shared-contract-types.md) | Shared types of the contract |

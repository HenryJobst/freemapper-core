# freemapper-core: working rules

Shared libraries of the freemapper suite. The apps are freemapper (`../freemapper`), freemapper-courseplan
(`../freemapper-courseplan`) and freemapper-print (`../freemapper-print`). Code that more than one app needs
lives here instead of being copied *(user, 2026-11-09)*. Read `docs/adr/README.md` and `docs/glossary.md`
before changing code.

- Rust, edition 2024, one Cargo workspace with small crates under `crates/` (ADR 0001). A crate holds one
  topic, so an app that needs texts does not build a PDF writer. No crate opens network connections; fetching
  stays in the apps (ADR 0007).
- Owners *(user, 2026-11-09)*: the session that brings a crate in maintains it (ADR 0005). A change to a crate
  another session owns is agreed with that session over SendMessage first.

  | Crate | Owner |
  |---|---|
  | `freemapper-text`, `freemapper-rules` | freemapper-courseplan |
  | `freemapper-georef`, `freemapper-fs`, `freemapper-contract` | freemapper |
  | `freemapper-omap`, `freemapper-render`, `freemapper-course` | freemapper-print (after its M3) |
  | `freemapper-web` | freemapper-courseplan |

- Source code (identifiers, comments), ADRs and plans are English. READMEs and changelogs are bilingual, German
  and English. With the user, speak German.
- Glossary first: a new domain term gets its row in `docs/glossary.md` before or with its code. Terms the apps'
  glossaries define keep their names and code identifiers.
- User-facing text goes through the `unslop` and `humanizer` skills before it is committed: READMEs,
  changelogs, ADRs, doc comments of public items longer than a sentence.
- Every significant decision gets an ADR in `docs/adr/` in the same branch. ADRs that move here from an app keep
  their content and name the app's ADR in their source line. Every merged change gets an entry under
  "Unveröffentlicht"/"Unreleased" in `CHANGELOG.md` and `CHANGELOG.en.md`, named by crate.
- Licence GPL-3.0-or-later for every crate (ADR 0002). Toolchain pinned in `rust-toolchain.toml`, moved
  together with the apps (ADR 0003). Third-party dependencies at least two weeks old; the suite's own crates
  are exempt (ADR 0004).
- `unsafe_code = "forbid"` and clippy `-D warnings` for the whole workspace. `cargo fmt`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` and
  `cargo test --workspace --all-features` must pass; CI runs them on Linux, macOS and Windows.
- Public API: a crate is unstable (0.x) until its first release in an app; breaking changes then need a minor
  version step and a changelog entry under "Geändert"/"Changed" that says what the apps must change.
- Apps depend on core through git dependencies pinned to a tag, and during development through a `[patch]` to
  the local checkout (ADR 0006). One version for the whole workspace, one tag `vX.Y.Z` per release.
- Git: `main`; one branch and worktree under `.worktrees/<branch>` per change; Conventional Commits in English
  with the crate as scope (`feat(text): …`); merge with `--no-ff` after review; never force-push.
- Remotes: `origin` is the private GitLab (https://gitlab.jobst-server.de/henry/freemapper-core, HTTPS),
  `github` the public GitHub repository (https://github.com/HenryJobst/freemapper-core, SSH). CI runs on
  GitHub. Push `main` to both after a merge.
- Releases: set `[workspace.package] version`, `cargo update -w`, date the changelog sections, commit,
  annotated tag, push `main` and the tag to both remotes, and tell the other sessions which version to pin.
- Disk space is shared by the suite: run `cargo clean` in a worktree before removing it.

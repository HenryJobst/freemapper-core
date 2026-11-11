# 0025. The update crate

Date: 2026-11-11
Status: Accepted

## Context
The suite's apps ship as archives with a bare executable. Releases come every few days, and a person has to
download, unpack and register again by hand. freemapper decided on a self-update from signed releases
(freemapper ADR 0094); freemapper-print and freemapper-courseplan will want the same. The checks that keep an
update safe (manifest, signature, versions, host rule, swap, rollback) are identical in every app and should not
be written three times. Core takes no network access (ADR 0007), so the crate cannot fetch anything itself.

## Decision
- `freemapper-update` holds the pure parts. freemapper owns it (ADR 0005).
  - The update manifest `<app>-update.json`: version, release notes, the lowest version that may update itself,
    and per platform the asset URL, size, SHA-256 of the download and SHA-256 of the unpacked executable. It is
    parsed only after the signature has been checked.
  - The minisign check against a list of accepted public keys, so a key can be rotated. Only keys with the id of
    the signature are tried; legacy (non-prehashed) signatures are refused.
  - Versions (`x.y.z` with the usual order), platform names, and the manifest URL of the latest or a named
    release.
  - The host rule: `check_url` accepts `https` URLs without user info, on port 443, whose host is on
    `ALLOWED_HOSTS` (github.com and the two GitHub download hosts). Apps run every URL through it, redirects
    included.
  - The check record (when the last check ran and what it found) and the mode setting (off, notify, install),
    both written through a temp file next to the target and a link-safe rename (`freemapper-fs`).
  - The swap: the running executable moves to `<name>.previous`, the new file takes its place, and the program
    must print its version before the swap counts. A rollback reverses it. An older previous file becomes a
    `stale` file that a later start removes. Every step is a rename in the executable's folder, which Windows
    allows for a running program.
  - `install`: download the `.gz` next to the executable, size and SHA-256 check, unpack, check the
    executable's SHA-256, run the new program with `--version`, and only then
    swap. Nothing unverified reaches the executable's path. An exclusive lock file `<name>.update-lock` keeps
    two installs (CLI and MCP server) apart; a lock older than 15 minutes counts as left by a crash and is taken
    over.
- The apps supply:
  - `UpdateSource`: `get` and `download_to`, with their own HTTP client, size limits and redirect handling.
    The crate adds no HTTP client.
  - `App`: name, repository URL, the list of public keys and the running version.
  - Optionally a `VersionRunner`. `ProcessRunner` runs `<exe> --version` with a 10 second timeout; tests use a
    stub.
- Test keys live in `crates/freemapper-update/tests/keys` and are used by tests only. An app's release build
  lists its own public key and none of these, so a fixture signed with a test key is never accepted.
- Dependencies are `serde`, `serde_json`, `toml`, `thiserror`, `sha2`, `flate2` and `minisign-verify`, plus the path dependency
  `freemapper-fs`, pinned exactly at versions at least two weeks old (ADR 0004 and 0012).

## Consequences
- An app adopts the crate with an `UpdateSource` and a key list. It decides itself where the check runs and
  what it tells the person.
- The key pairs are per app or shared; the crate takes a list, so both work. Whoever signs a release holds the
  secret key; the crate never sees one.
- A stale lock blocks updates for up to 15 minutes after a crash. The lock's age is not refreshed during a very
  slow download.
- Cost if wrong: the manifest format is a contract with the release workflow of every app. A change there
  needs a new crate minor and a matching app release.

Source: freemapper ADR 0094, the self-update plan of 2026-11-11, the user's request of 2026-11-11.

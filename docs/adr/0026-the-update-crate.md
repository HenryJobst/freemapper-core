# 0026. The update crate

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
  - The update manifest `<app>-update.json`: version, release date, release notes link, the lowest version
    that may update itself, and per platform the asset URL, size, SHA-256 of the download and SHA-256 of the unpacked executable. It is
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
  - A check gives `Outcome::Available` with version, release date, notes link and the asset (its size), what
    the app shows before it asks. A release the running version may not update to gives
    `TooOldToSelfUpdate` with the needed version and the release's notes link for the manual step.
    `Outcome` and `UpdateError` are `non_exhaustive`.
  - The swap (`swap_in`): the running executable moves to `<name>.previous` and the new file takes its place.
    An older previous file becomes a `stale` file that a later start removes. Every step is a rename in the
    executable's folder, which Windows allows for a running program; a rename refused with
    `PermissionDenied` (a virus scanner) is tried a few times. `swap_in` runs nothing itself.
  - The version check happens in `install` and `rollback`, before their swap: `install` runs the new program,
    `rollback` the previous one, and both refuse it unless it reports the expected version.
  - `install`: download the `.gz` next to the executable, size and SHA-256 check, unpack, check the
    executable's SHA-256, run the new program with `--version`, and only then swap. Nothing unverified reaches
    the executable's path. The temp files carry the install's process id and start time
    (`<name>.update-new-<pid>-<nanos>`, `<name>.update-download-<pid>-<nanos>.gz`).
  - An exclusive lock file `<name>.update-lock` keeps two installs or rollbacks (CLI and MCP server) apart. It
    holds a token of its holder (process id, time, counter). A lock older than 15 minutes counts as left by a
    crash and is taken over. A slow install can lose its lock that way, so `install` and `rollback` read the
    token again right before the swap and end with `Busy` if it is no longer theirs, and the lock guard
    removes only its own lock. With the lock held, temp files older than 15 minutes are swept.
- The apps supply:
  - `UpdateSource`: `get` and `download_to`, with their own HTTP client, size limits and redirect handling.
    The crate adds no HTTP client.
  - `App`: name, repository URL, the list of public keys and the running version.
  - A `VersionRunner`, which `install` and `rollback` require. The crate provides `ProcessRunner::new(app.name)`:
    it runs `<exe> --version` with stdin closed and a 10 second timeout and expects the first line to be
    `<name> x.y.z`, with the app's name, whatever the file is called. That is the `--version` contract every
    app keeps. Tests use a stub.
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
  slow download; a second install may then take it over, and the first one stops with `Busy` before its swap.
- Cost if wrong: the manifest format is a contract with the release workflow of every app. A change there
  needs a new crate minor and a matching app release.

Source: freemapper ADR 0094, the self-update plan of 2026-11-11, the user's request of 2026-11-11.

# 0003. Toolchain pinned with the apps

Date: 2026-11-09
Status: Accepted

## Context
The apps pin the same Rust toolchain, so a newer clippy never surprises one pipeline. Core is built inside each
app.

## Decision
`rust-toolchain.toml` pins the toolchain the apps use (1.99.1 at the start). It moves only together with the
apps, with whatever the newer clippy reports fixed in all four repositories.

## Consequences
- A lint that passes here passes in the apps.
- Cost if wrong: moving the toolchain takes four coordinated changes.

Source: freemapper-courseplan ADR 0007, freemapper-print and freemapper toolchain ADRs.

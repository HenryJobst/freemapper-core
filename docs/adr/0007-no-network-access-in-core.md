# 0007. No network access in core

Date: 2026-11-09
Status: Accepted

## Context
freemapper fetches map data with limits and TLS pinning. The other apps work offline.

## Decision
No core crate opens a network connection or depends on an HTTP client. Fetching stays in the apps
*(agreed with freemapper)*.

## Consequences
- Core builds and tests offline.
- Cost if wrong: a second app that needs fetching would copy freemapper's fetch code or ask for a new decision.

Source: freemapper session, 2026-11-09.

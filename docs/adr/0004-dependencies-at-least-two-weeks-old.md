# 0004. Dependencies at least two weeks old, own crates exempt

Date: 2026-11-09
Status: Accepted

## Context
The apps use only crate versions that are at least two weeks old. Core releases will often be younger than
that when an app moves onto them.

## Decision
Every third-party dependency version is at least two weeks old; where a newer release is younger, the
workspace pins the older version exactly. The suite's own crates are exempt: an app may pin a core release on
the day it appears *(agreed with freemapper and freemapper-print)*.

## Consequences
- The apps need the same exemption in their own dependency ADR.
- Cost if wrong: a fault in a fresh core release reaches the apps at once; the apps' tests are the guard.

Source: freemapper-courseplan ADR 0006, freemapper-print ADR 0006, freemapper ADR 0012.

# 0005. The contributing session owns a crate

Date: 2026-11-09
Status: Accepted

## Context
Each app is developed by its own Claude session. Core needs someone who answers for each crate.

## Decision
The session that brings a crate in maintains it *(user, 2026-11-09)*. Another session that needs a change
asks the owner over SendMessage and agrees on it before it changes the crate. The owner table is in
`CLAUDE.md`.

## Consequences
- Every crate has one place where its decisions are made.
- Cost if wrong: an owner that is busy delays the others; they can then change the crate after the owner
  agreed in a message.

Source: user decision, 2026-11-09.

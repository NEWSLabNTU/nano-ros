---
id: 243
title: "Platform board-trait family duplicated during transition: nros-platform::board::{Board,BoardInit,BoardEntry} vs legacy nros-board-common::board_init::{…} both live"
status: open
type: tech-debt
severity: low
area: platform
related: [rfc-0034]
---

## Finding (RMW/platform API audit, 2026-07-21)

Two parallel board-lifecycle trait families are live simultaneously:

- **New** (Phase 212.N.1): `nros-platform::board::{Board, BoardInit,
  BoardEntry, BoardPrint, BoardExit, TransportBringup, NetworkWait, …}`
  (`packages/core/nros-platform/src/board/mod.rs:38-96`), where config
  moved off `BoardInit` into `RuntimeCtx` (`board/init.rs:11-21`).
- **Legacy**: `nros-board-common::board_init::{Board, BoardInit, …}` — kept
  "live during transition" (`board/mod.rs:40-48`), plus a
  `NodeRuntime` → `NodeDispatchRuntime` deprecated alias
  (`mod.rs:83-86`).

Duplicated trait surfaces during a migration are fine SHORT-term, but this
one has no recorded end-state or tracking, so it risks becoming permanent
two-of-everything (the exact antipattern the API audit flags): a new board
author has to know which `BoardInit` to implement, and downstream code
picks one arbitrarily.

## Direction
Record the convergence plan: which board crates still implement the legacy
family, what blocks their move to `nros-platform::board`, and a target for
deleting `nros-board-common::board_init` + the `NodeRuntime` alias. Then
either finish the migration or, if the legacy family is intentionally
permanent for some layer, document why and stop calling it "transition".

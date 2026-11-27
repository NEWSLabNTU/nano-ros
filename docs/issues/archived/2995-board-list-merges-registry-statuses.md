---
id: 2995
title: "`nros board list` put every registry row sharing a board's name in one cell, so `linux` read \"Verified on host; Not supported; Infrastructure\""
status: resolved
resolved: 2026-11-27
resolved_in: "the change that adds this line"
type: bug
severity: low
area: [cli, boards]
related: [phase-577, phase-522]
found: 2026-11-27
---

## What happened

Found by the book work on main (CLI at db1840edff). `nros board list` showed:

- `linux`: status "Verified on host (daily); Not supported; Infrastructure",
  cadence "daily; -; -";
- `freertos`: crate `nros-board-mps2-an385-freertos`, status
  "Infrastructure".

A reader cannot tell which status belongs to which key or package, and
`freertos`'s real evidence (its package's emulator row) is not shown at all.

## Cause

`board list` chose a key's registry rows with `Status::rows_named`, which
matches a row by its registry key, its crate, its crate without the
`nros-board-` prefix, or its evidence page's name. That is a lookup for a
NAME a user typed, not for a board key:

- `linux` answers to three rows: `nros-board-linux` (crate without prefix,
  the host's row), the same crate's `linux-aarch64-musl` row, and the
  descriptor directory `linux` (an `infra` row).
- `freertos` answers to an `infra` row by its page name, so the fallback to
  the package's rows never ran.

The lookup for a board key already exists: `Status::rows_for_board`, the rows
that claim the key in `board_keys`, else the rows of the package its
descriptor resolves to. `nros doctor` and `nros rmw` use it; `board list` and
`board show <key>` did not.

## Fix

`board list` and `board show <board key>` use `rows_for_board`. `board show`
given a package or registry key keeps the name lookup, which is what that
input means.

Measured with the fixed binary, `nros board list`:

```
BOARD     CRATE                           PLATFORM  STATUS                        CADENCE
freertos  nros-board-mps2-an385-freertos  freertos  Verified in emulator (daily)  daily
linux     linux                           posix     Verified on host (daily)      daily
```

## Regression

`each_board_key_lists_only_its_own_registry_rows` (in `cmd/board.rs`)
lists the in-tree catalog against the registry and asserts `linux` has one
status and one cadence, neither key shows `Infrastructure`, and `freertos`
has its package's row. Mutation-checked: with `rows_named` put back it fails.

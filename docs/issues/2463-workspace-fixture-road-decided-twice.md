---
id: 2463
title: "A workspace fixture row's road (cargo or cmake) is decided in two places, and the bash copy broke every row without an authored builder"
status: open
type: tech-debt
area: [testing, build]
severity: medium
found: 2026-11-12
related: [2377, 2433]
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md)** W6, which
added the authored `builder` column (10d35dc209).

## Measured

A `[[workspace_fixture]]` row is built down the cargo road or the cmake road.
Since 10d35dc209 that is the row's authored `builder` (15th record column)
when it states one, and its language otherwise. Two places compute it:

- `scripts/build/workspace-fixture-units.py`:
  `road = f[14] or ("cargo" if lang == "rust" else "cmake")`.
- `scripts/build/workspace-fixtures-build.sh` `build_workspace`, a hand-written
  `if/elif/else` that tested `"$road" = "cargo"` (the variable it was assigning)
  instead of the language.

Under `set -u` every row with no `builder` stopped at
`line 215: road: unbound variable`, so every Rust workspace row and every
C/C++ row except the ten that author `builder = "cargo"` built nothing. Without
`set -u` the same line would have sent every Rust row down the cmake road.
Measured on origin/main with `workspace-fixtures-build.sh <plat> --id <id>` for
nuttx-c-zenoh-typed-carrier, threadx-riscv64-c-zenoh-typed-carrier,
threadx-riscv64-c-zenoh-errno-isolation and threadx-linux-c-zenoh-talker
(each rc 1). The one-line fix is PR "fix(scripts): a workspace row with no
authored builder takes its road from its language".

No gate noticed: `just ci gate` builds no fixture, and the two copies agreed on
the ten rows the commit itself exercised.

## Class

One decision with two spellings, which can drift (CLAUDE.md "Fix the CLASS").
The test locator (`LeafRow::builder`) is a third reader of the same column.

## Remedy

Make the road one decision. Either the manifest emits the resolved road in the
record (so bash, the units script and the locator all read it instead of
deriving it), or a fast-line check runs both derivations over every
`[[workspace_fixture]]` row and fails on a disagreement, with a negative
control that feeds it a row with no `builder` for each language.

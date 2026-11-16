---
id: 2463
title: "A workspace fixture row's road (cargo or cmake) is decided in two places, and the bash copy broke every row without an authored builder"
status: resolved
type: tech-debt
area: [testing, build]
severity: medium
found: 2026-11-12
resolved: 2026-11-17
related: [2377, 2433]
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md)** W6, which
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

## Resolution (2026-11-17)

The road is one decision in each language, and a fast-line check holds the
script twins together:

- **bash:** `nros_ws_row_road` in `scripts/lib/fixture-road.sh`, the only
  spelling. `workspace-fixtures-build.sh` calls it instead of its own
  `if/elif/else`.
- **Python:** `row_road` in `scripts/build/workspace-fixture-units.py`,
  named so the check can call it.
- **Rust:** `road` in `nros_tests::fixtures::migrated_leaf`, with a unit test
  (`road_tests`) over the same table.

`check-workspace-fixture-road` (fast line) runs every manifest row (467) and
a fixed table of each shape through the bash and Python twins, and fails on
any disagreement. The table covers each language with no builder, plus an
authored builder on each. It self-tests every run with a twin that sends a
builder-less Rust row to cmake. Negative control: breaking the bash
function's language test turns it red, naming the rows.

The record was NOT changed to carry the resolved road. `workspace_record`
keeps `builder` authored-only because the record is hashed into the fixture
signature, so emitting the road would re-stale every fixture.

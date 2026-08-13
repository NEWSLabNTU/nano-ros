---
id: 535
title: "74 west-built fixtures have no manifest row, so they have no coordinate — the lane predicate cannot see them in either direction"
status: open
type: tech-debt
area: build, testing
related: [issue-0482, issue-0509, issue-0517, issue-0536, issue-0539, phase-340, phase-344]
---

## Problem

`examples/fixtures.toml` is the fixture SSoT: 250 `[[fixture]]` + 93
`[[workspace_fixture]]` + 36 `[[compile_check_fixture]]` = **379 rows**, and
`row_coord()` in `fixtures-manifest.py` is the ONE computation that gives a row
its `(platform, lang, rmw)` coordinate. Phase-340 W3 made that coordinate the
single predicate for both halves of a lane:

```
BUILD  skips row R  ⟺  row_coord(R) ∉ lane_coords     (--coords-from)
RUN    skips row R  ⟺  row_coord(R) ∉ lane_coords     (fixtures::lane)
```

**74 west-built fixtures are outside that manifest entirely**, in two scripts
that each carry their own matrix:

| source | count | matrix lives in |
| --- | --- | --- |
| `scripts/build/zephyr-fixture-leaves.sh` | **70** | `scripts/build/fixture-matrix.sh` (`nros_fixture_langs`, `nros_fixture_roles`) + inline `fixture_rmws=(zenoh xrce cyclonedds)` |
| `scripts/build/west-fixtures.sh` | **4** | `WEST_FIXTURES=()` / `SELF_PKG_FIXTURES=()` bash arrays |

Reproduce the 70:

```sh
bash scripts/build/zephyr-fixture-leaves.sh --emit records \
  --include-logging-smoke --include-workspace-entry | wc -l   # 70
```

Shape: 3 langs × 6 roles × 3 rmws = 54, plus 12 `ws-*-entry` leaves, 3 mps2
talkers, 1 logging-smoke.

## Consequences

1. **The build half cannot narrow.** `NROS_FIXTURE_COORDS` has **zero** hits in
   `just/zephyr-ci.just`, `zephyr-fixture-leaves.sh` and
   `zephyr-fixture-make-driver.sh`. A lane can only include or exclude the
   zephyr MODULE wholesale (`nros_lane_modules`), never a coordinate inside it.
   So `lane=tier2` — which is 1-wise over platform and therefore contains
   zephyr — pulls all 70.
2. **The run half cannot attribute.** `fixtures/lane.rs:67` already names these
   as handled "module-level rather than by coordinate", and `:381` records that
   the leaves are unattributable by path. Per CLAUDE.md the staleness probe
   therefore never skips them — correct given the missing fact, but it means no
   coordinate-scoped run can ever include or exclude them on merit.
3. **The SSoT claim is false for ~16 % of the surface.** 74 of 453 fixture
   builds (379 + 74) answer to a bash array instead of `row_coord()`. That is
   the same shape as issue 0482 (two computations of one coordinate), except
   here the second computation is not a disagreeing copy — it is an ABSENCE, so
   nothing can even report the divergence.
4. **`just fixture-staleness` and the coverage gates are blind to them.**
   `examples_fixture_coverage.rs` compensates by hardcoding the role matrix a
   THIRD time (`ZEPHYR_LANGS` × `ZEPHYR_ROLES`, `examples_fixture_coverage.rs:52`)
   so the `examples/zephyr/**` dirs read as covered. Three spellings of one
   matrix, none of them the SSoT.

## Why it matters now

Issue 0509 measured the lane at **40 min for 68 leaves**, serial-added to every
full sweep because zephyr is an order-only prerequisite of every other platform.
Its last direction reads: "Question whether all 68 leaves must be in `lane=all`
at their current granularity, or whether the coordinate cover (phase-340 W3) can
retire some." **That question cannot be asked of a fixture with no coordinate.**
This issue is the structural prerequisite for 0509's cheapest lever.

Phase-329 W8.d is blocked on the same fact from the other side: a
coordinate-scoped tier-2 RUN needs every test cell-bound, and a cell cannot bind
to a fixture the coordinate system cannot name.

## Direction

Give all 74 rows in `examples/fixtures.toml` and make the two scripts CONSUME
the manifest (`--coords-from`, as `fixtures-build.sh` and
`workspace-fixtures-build.sh` already do) rather than declare their own matrix.
The manifest already models a non-cargo builder (`builder = "cmake"`,
`"cross-build"`, `"cmake-configure"`), so a `builder = "west"` row is the
existing seam, not a new concept.

Fix the CLASS: the three matrix spellings (`fixture-matrix.sh`,
`west-fixtures.sh` arrays, `examples_fixture_coverage.rs` constants) must
collapse to one, and `examples_fixture_coverage.rs` must then read the rows
instead of restating them — otherwise this is the sizes-header mirror again.

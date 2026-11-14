---
id: 2548
title: "A merge shard's run refuses what its build correctly did not make, and its build makes host work it does not own: build and run scope a shard by different predicates"
status: open
type: bug
area: [ci, build, testing]
severity: medium
found: 2026-11-14
related: [2510, 2475, 0828, phase-559]
---

**Home: [phase-559](../roadmap/phase-559-ci-workflow-organization.md) W5b**
(the per-board hosted tier-1 shards).

## Measured

`merge-runtime.yml` run 43551581652 (`lane=tier1`, main 28a9b3ebba).

**threadx_linux.** `just build tier1` passed. It started with 105 G free after
reclaim and ended with 43 G free, so the shard fits. `just ci tier1 run` then
failed in `check::tier-preconditions` after 44 s, with no test run:

- `a leaf names a sync-written path that does not exist yet`: 76 path deps
  into `generated/`, across 64 manifests, all in Linux examples. phase-535 W10
  correctly did not generate these for a threadx shard, but the probe ran
  `leaf-config-includes.py` unscoped.
- `test fixtures not ready for this lane`:
  - `workspace-c-native-robot2` was missing. It is a `linux` row that SERVES
    `threadx-linux,c,zenoh` (issue 2139), so the run requires it. But a shard
    scheduled its own module only (`nros_lane_modules`), and no module built it.
  - 11 compile-check rows were "missing or stale". The probe took every row
    except west rows in a `linux` scope, and on a coordinate scope it took all
    of them, west rows included.
- The refusal printed 6 lines of each probe's detail, so it named one
  workspace row and none of the 11 compile-check rows.

**zephyr.** The disk balloon stopped the build at 2 G free (05:24:54Z). It had
~90 G free after reclaim. The leaves had finished by 05:03 with ~16 G left;
the compile-check stage then used the rest. `du` at exhaustion:

| dir | size |
| --- | --- |
| build/zephyr-workspace-builds | 44 G |
| build/corrosion-cargo | 12 G |
| build/cmake-fixtures | 10 G |
| build/sizes-probe | 7.0 G |
| build/compile-check-fixtures | 5.5 G |
| ~/.nros | 2.8 G |
| build/metadata-probe | 2.7 G |
| target | 1.8 G |
| build/west-fixtures | 1.4 G |

The Zephyr builds took 44 G. About 35 G went to host work the zephyr shard
does not own: every compile-check row with no stated platform is in every
lane (issue 2510's rule), so a zephyr shard built every host compile-check.

## Cause: one question, several answers

Build and run each decided "which rows does this lane/shard hold?", and they
disagreed in three places:

1. **Compile-check rows.** The build kept a row with no `platform` in every
   lane. The run's staleness probe applied no platform at all. The test
   resolver kept them too. The west resolver asked the Zephyr setup scope.
2. **Served rows on a shard.** The run required a row in the lane through a
   coordinate it serves. The build scheduled only the shard's own module.
3. **Codegen leaves.** The build checked the lane's leaves
   (`_require-leaf-includes <lane>`). The run's precondition checked every
   leaf.

## Fix

ONE predicate per question, read by both sides:

- `compile_check_platform` (`fixtures-manifest.py`): the stated `platform`,
  else `zephyr` for a `west-*` row, else `linux` (a host build). The build
  (`list-compile-checks --coords-from`, `lane-dirs`) and the staleness probe
  (`check-fixtures-stale.sh` under `coords` scope) use it directly. The test
  resolvers (`require_compile_check{,_bin}`, `require_compile_verdict`,
  `require_west_fixture`) read it through `compile-check-platforms`. There is
  no second spelling.
- `lane-coords <lane> --shard-modules <module>`: the shard's module plus the
  module of every row serving one of its coordinates (issue 2139's rule, which
  `--modules` already applied to a whole lane). `nros_lane_modules` uses it
  for a shard.
- `check-tier-preconditions.sh` scopes its leaf probe with
  `nros_codegen_scope_export "$NROS_FIXTURE_LANE"`, the build's own call, and
  prints every line of a refusal.

Bound by `lane_build_covers_run`:

- `every_lane_and_shard_builds_exactly_the_fixture_rows_its_run_requires`:
  for every coordinate lane, whole and per merge shard, the rows the scheduled
  modules build equal the rows the staleness probe requires, both directions.
  Mutation: a `--shard-modules` that drops served modules fails it on
  `tier1/threadx_linux` with exactly `workspace-c-native-robot2`.
- `every_lane_builds_exactly_the_compile_check_rows_its_run_reads`: now over
  every row's effective platform, west rows included.

## Done when

A `merge-runtime.yml` tier-1 dispatch runs tests on the threadx_linux shard
(preconditions pass). On the zephyr shard it reports the disk the build uses
with host compile-checks out of scope: within budget, or the measured
shortfall goes to the user (phase-559 W5b).

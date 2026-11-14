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

## Second round, run 43568845036 (main 36f665bedc, with #3805)

- **zephyr:** `just build tier1` PASSED with 6.2 G left of ~95 G after
  reclaim. The balloon never fired; the lowest point was 4.2 G free.
  `build/zephyr-workspace-builds` 44 G, `build/corrosion-cargo` 23 G (up
  11 G: the Linux rows that serve zephyr coordinates, which the run needs),
  `build/sizes-probe` 6.3 G, `build/metadata-probe` 3.0 G; compile-check
  fixtures dropped to ~0 (4 rows) and `build/cmake-fixtures` to nothing. Then
  `just ci tier1 run` failed in `rust-rtos-link-check`. That step is
  unscoped: it regenerated all 199 codegen units and linked the NuttX talker,
  whose cross GCC the zephyr image does not carry.
- **threadx_linux:** codegen narrowed to 2 units, so nothing wrote the
  gitignored central `nros-patch.toml`, and `_require-leaf-includes` refused
  the `include` of two QEMU test bins (`nros-bench/wcet-cycles-qemu`,
  `nros-tests/bins/cdr-roundtrip-qemu`) that the lane never builds. The
  include arm of `leaf-config-includes.py` was the one arm W10 did not scope.

Fixed together (fix-2548-run-scope-2):

- `leaf-config-includes.py` scopes its `include` arm like its path-dep arm:
  under a lane narrowing, it checks only configs whose leaf is nested with a
  scoped unit or a lane row dir (`codegen_scope`). Both arms now list every
  miss instead of 5, and the text no longer names issue ids.
- `rust-rtos-link-check` runs `_codegen <lane>` instead of an unscoped
  `_codegen`. It builds a leaf only when the run holds a coordinate on its
  platform (`nros_lane_wants_platform`, the fixture builders' predicate),
  and names each leaf it leaves out.

## Third round, run 43582883501 (main 1a54770f56, with #3816)

- **threadx_linux:** preconditions PASSED and the tests RAN: 3614 tests, 3037
  passed, 558 deselected out of lane, 19 real failures. That meets this
  issue's threadx half. The failures were three more places where a test
  asked the host before it asked the lane:
  - 11 XRCE tests (plus 4 in the `xrce-wake-test` feature lane):
    `XRCE Agent not found`. The shard holds `threadx-linux,c,zenoh` only, and
    `require_xrce_agent` probed before any lane check.
  - 9 Linux cmake-template tests: `Test fixture binary MISSING for an in-lane
    coordinate`. `require_cmake_fixture` was the one compile-check resolver
    that did not ask `require_compile_check_row_in_lane`, so a row the build
    correctly omitted read as missing.
  - The `custom_transport_loopback` feature lane ran nothing (its only test
    is Linux), and `check-skip-budget`'s zero-ran floor failed it.
  - `every_lane_codegen_scope_covers_its_fixture_build` hit the 60 s ceiling
    (9.1 s solo); `every_lane_and_shard_builds_exactly…` passed at 58 s
    (7.5 s solo). CPU contention on 4 vCPUs under the whole suite.
- **zephyr:** `just build tier1` PASSED again, lowest point 4.0 G free, 6.4 G
  left. The run step was not under the balloon. It died about 4 minutes in,
  right after the `loan_e2e` feature lane, where threadx_linux spent 5
  minutes compiling the next feature lane's test binaries. The job's logs are
  gone (`BlobNotFound`), which is what a full disk did on run 43534848324.
  Not proved: no `du` exists for the run step.

Fixed together (fix-2548-run-scope-3):

- `require_xrce_agent` asks `lane::require_rmw_in_lane("xrce", …)` first: a
  lane holding no `xrce` coordinate deselects before the probe. A lane holding
  one, on any platform, still probes.
- `require_cmake_fixture` asks the row predicate. A source scan,
  `every_compile_check_resolver_asks_the_row_predicate`, now requires it of
  every resolver that reads a compile-check build root. Mutation: dropping the
  call from `require_cmake_fixture` fails it, naming that function.
- `check-skip-budget.py`: a feature lane (`NROS_SKIP_BUDGET_SUBLANE`, set by
  `feature-lanes.py run`) whose every skip is a lane deselection reports and
  passes. The zero-ran floor stays on the enclosing run and on any standalone
  run, and an unmet skip still fails.
- The two lane-contract scans get a third 60 s period (no retry).
- `just ci <lane>` runs under the disk balloon too, so the zephyr run step
  stops at 2 G free with its `du` instead of losing the job.

## Done when

A `merge-runtime.yml` tier-1 dispatch runs tests on the threadx_linux shard
(preconditions pass). On the zephyr shard it reports the disk the build uses
with host compile-checks out of scope: within budget, or the measured
shortfall goes to the user (phase-559 W5b).

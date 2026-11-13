---
id: 2510
title: "Every lane runs the FreeRTOS cross-build compile-check rows, but `build_sources` leaves out `freertos-kernel` as cross-only, so a shard that provisions no FreeRTOS fails `build-compile-check-fixtures`, and the aggregate does not name the row"
status: open
type: bug
area: [ci, build, testing]
severity: medium
found: 2026-11-14
related: [2475, 1651, phase-535, phase-559]
---

**Home: [phase-559](../roadmap/phase-559-ci-workflow-organization.md) W5b**
(the `merge-runtime.yml` re-measure). Found by issue 2475's after-run.

## Measured

`merge-runtime.yml` run 43438754804 (`lane=tier1`, branch
`p535-lane-scoped-codegen` at 7bff8a9356), shard `threadx_linux`. The job's
last step was `just build tier1`, which failed at 15:32:44Z:

```
error: recipe `build-compile-check-fixtures` failed on line 2023 with exit code 1
error: recipe `build-test-fixtures` failed with exit code 1
```

Every compile-check unit that printed its one-row summary had built. Two units
printed a heading and no summary, `cross-build: freertos_firmware` and
`cross-build: orch_sched_tasks_freertos`, and both died in the same build
script:

```
thread 'main' panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:3240:13:
zenoh port row `[port.freertos]` include_paths `{source:freertos-kernel}/include`: freertos-kernel: not provisioned anywhere this build looks:
  arg        -
  env        $FREERTOS_DIR unset
  project    no [sources] row in the project's system.toml
  local-edit checkout copy absent or at the pin
  store      /github/home/.nros/sources/freertos-kernel/10.6.2+0adc196d absent
  checkout   /__w/nano-ros/nano-ros/third-party/freertos/kernel not initialised
run: nros setup --source freertos-kernel
```

The before-run (43420012895, on main) failed at the same recipe with the same
exit. That earlier diagnosis read "cause not visible", for the reason below.

## Why

Two facts disagree:

1. `build-compile-check-fixtures` runs for every lane, unscoped:
   `fixtures-manifest.py list-compile-checks` is not narrowed by coordinate,
   and phase-535 W10's coverage test reads it unscoped for that reason. Its
   `cross-build` rows include two `thumbv7m-none-eabi` FreeRTOS images.
2. `nros-sdk-index.toml`'s `build_sources` comment calls the FreeRTOS,
   ThreadX and NuttX kernels "cross-only trees ... which only a target build
   for that board pulls in", so `nros setup --build-sources` does not
   provision `freertos-kernel`.

A lane provisioned for one board (`just setup threadx_linux`, then
`nros setup --build-sources`) therefore reaches a row that needs another
board's kernel. Issue 1651 hit this class with `px4-autopilot`, and fixed it
by adding that one tree to `build_sources`. Fix the class here: either the
cross-build compile-check rows narrow to the lane's coordinates like every
other fixture family, or the kernels they reach join `build_sources`. Also
check that the self-hosted tier1 runner only passes because of a provisioned
store.

## Secondary: the failure is not attributed

`compile-check-fixtures.sh` fans its rows out as one pool unit each. A unit
that dies under errexit outside `run_fixture` prints no summary. The parent
skips its aggregate on a nonzero pool status and exits with no line naming
the failed row. So the only evidence is a panic among ~14,000 interleaved log
lines. The parent should name every unit that exited nonzero.

## Fix (fix-2510-compile-check-lane)

Coordinator's choice (2026-11-14): narrow the rows to the lane, as every
other fixture family is narrowed, rather than provision every kernel
everywhere.

- A `[[compile_check_fixture]]` row may state `platform`. The two FreeRTOS
  cross-builds state `freertos`; `borrowed_e2e` and `cyclone_slirp_guest`,
  whose tests were already linux-scoped, state `linux`. A token no fixture row
  uses is refused by `validate-compile-checks`.
- Build: `fixtures-manifest.py list-compile-checks --coords-from` drops a row
  whose platform the lane holds no coordinate on (`compile_check_in_coords`),
  `compile-check-fixtures.sh` passes the lane's `NROS_FIXTURE_COORDS` (unless
  one row is named), and `lane-dirs` uses the same predicate for the codegen
  scope.
- Run: `require_compile_check`, `require_compile_check_bin` and
  `require_compile_verdict` read the same `platform` and deselect through
  `require_platform_in_lane`. One field answers for both sides.
- Bound by `lane_build_covers_run::
  every_lane_builds_exactly_the_compile_check_rows_its_run_reads`: build set
  vs run set, row by row, both directions, every coordinate lane whole and per
  merge shard. Mutation: a predicate that keeps every row fails it on tier1
  (`freertos_firmware` built but skipped by the run).
- Attribution: a failed pool unit prints `compile-check: row <id> FAILED`, and
  the stage ends `compile-check: FAILED — N row(s) did not build: <ids>`.
  Measured locally with `FREERTOS_DIR` pointed at nothing.

Verification on a hosted shard waits for the next `merge-runtime.yml` tier1
dispatch, which also carries issue 2511's image fix.

## Done when

- A `merge-runtime.yml` `threadx_linux` shard passes
  `build-compile-check-fixtures`, without `freertos-kernel` provisioned by
  hand.
- A failing compile-check unit is named in the stage's own failure line.

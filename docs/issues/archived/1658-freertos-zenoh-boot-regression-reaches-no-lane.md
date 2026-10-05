---
id: 1658
title: "A boot regression in a cmake-built zenoh FreeRTOS image reaches no lane —
  issue 1657 sat on main for two days because the two lanes that hold the
  coordinate never got to their cells"
status: resolved
type: tech-debt
area: [ci, testing, freertos]
severity: medium
found: 2026-10-03
resolved_in: 2026-10-03
related: [1657, 1158, 1038, 968, 1670, 1671]
---

## What

Issue 1657 made every cmake-built zenoh C/C++ FreeRTOS image on mps2-an385
deadlock in session open. `realtime_tiers_e2e` catches it in 8 s
(`freertos/c` and `freertos/cpp`: "tier `ctrl` never published"), and nothing
ran it between `0ba0600e5c` landing and the hand reproduction.

Two lanes hold the coordinate, and neither reached it:

- **nightly `freertos`** — the lane that installs `ros-humble-rmw-zenoh-cpp` for
  exactly these cells. Run `36977810939` (2026-10-02) went red in
  `build-examples`: `freertos_entry section '.bss' will not fit in region 'RAM'
  … overflowed by 151016 bytes`, so no cell ran. (That run predates
  `fba1b024a9`; every FreeRTOS row links on main `093dbb4694` in this worktree.)
- **tier 2** (`run-matrix.yml`) — its coordinate file contains
  `freertos,c,zenoh`, but run `36973805253` reports `NO VERDICT: stopped in the
  build`, and the self-hosted image has no `/opt/ros/humble`, so a zenoh
  FreeRTOS cell would skip on a missing router even if it got there.

The merge-gating lanes boot nothing by design (`ci gate` is compile + unit, no
fixtures; the PR context is the fast line plus compile smoke).

## Decision taken in 1657

A QEMU + `rmw_zenohd` boot on a merge-gating lane was judged NOT affordable:
the `ci-base` image carries no zenoh router, and the PR/queue lanes are
fixture-free on purpose (RFC-0061 / phase-395). What 1657 put on the fast line
is the CLASS guard (`check-freertos-config-single-carrier`), which would have
refused the `-D` before issue 1598 could expose it.

## Remaining scope

1. Get one of the two lanes to a verdict on `freertos/{c,cpp}` realtime cells:
   the nightly `freertos` lane is the cheaper one (it already provisions the
   router); tier 2 needs `rmw_zenohd` in its runner image (an IMAGE fix, never a
   host `apt install` — issues 1457/1482).
2. Consider a nightly-only smoke that boots `workspace-c-freertos` for 30 s and
   asserts one `[talker_pkg] sent:` — in-image delivery needs no host observer,
   so it costs one QEMU and one router.

## Resolution

**The nightly `freertos` job now reaches the cells, and selects the workspace
road too.** Two parts, one measured and one changed:

1. **The lane reaches its cells on main.** The red that stopped run
   `36977810939` was one row, `examples/workspaces/realtime-cpp`'s
   `freertos_entry` (`.bss` over `RAM` by 151,016 B, job log line 7868), which
   `fba1b024a9` fixed. Running the job's own recipe in a fresh worktree off
   `origin/main` (`9c4156da82`): `just freertos build-fixtures` exits 0, and
   `just freertos test` runs.
2. **`just freertos test` now also selects `realtime_tiers_e2e`.** It selected
   only `rtos_e2e`'s `test(Freertos)` cells; the cmake-built WORKSPACE entries
   (`workspace-{c,cpp,rust}-freertos-realtime`, which `build-examples` already
   builds in this job) were consumed by a target no scheduled job ran.
   `_nextest-platform` takes several targets (space-separated) so both run in
   ONE nextest invocation — one junit, one skip budget — with the filter
   `test(Freertos) | binary(=realtime_tiers_e2e)`. `realtime_tiers` is one test
   iterating every platform's rows, so rows this job did not build report as
   skipped in its own summary; a FreeRTOS row that boots and does not tick
   fails it.

The job already provisions `ros-humble-rmw-zenoh-cpp` (`nros setup --system
--sudo`, issue 1038), so no workflow change was needed.

### Measured (store QEMU 11.0.0-nros2, `/opt/ros/humble` `rmw_zenohd`)

`ROS_DISTRO=humble just freertos test`, the step the nightly runs:

| tree | result |
| --- | --- |
| `origin/main` + this branch | `10 tests run: 10 passed (2 slow), 24 skipped` — 9 `rtos_e2e` FreeRTOS cells (action/pubsub/service × Rust/C/C++) + `realtime_tiers` (`18 row(s) ran, 15 skipped`: freertos rust/c/cpp ran, every other platform skipped for an unbuilt fixture) |
| same, 1657's fix reverted in the working tree (`git show 72f3fc298f -- cmake packages/boards/nros-board-freertos/config/FreeRTOSConfig.h \| git apply -R`), rebuilt by the recipe | `10 tests run: 4 passed, 6 failed` — all six C and C++ `rtos_e2e` cells red on every retry (pubsub ~110 s, service/action ~90 s); the Rust cells and `realtime_tiers` green |

So the lane, as it now runs, fails on the pre-#1624 behaviour and passes after.

### Not established

- **The `realtime_tiers` FreeRTOS C/C++ rows PASSED with 1657's fix reverted**
  on today's tree, although 1657 measured them red on `28ffc826b5`. The split
  is present in that build (`-DconfigUSE_TRACE_FACILITY=1` in the realtime-c
  leaf's `flags.make`, the header without it); why those images no longer
  reach the recursive re-take was not investigated. The cells that catch the
  class on today's tree are `rtos_e2e`'s C/C++ cells, which the lane already
  ran — so on the night it went red, the build red was the whole reason
  nothing saw 1657.
- The remaining-scope item 2 smoke (`workspace-c-freertos` `[talker_pkg]
  sent:`) was not added; `rtos_e2e`'s C/C++ pubsub cells and the realtime rows
  boot the same cmake road against the router.
- Not run under `act` (none installed); the recipe the step runs was run
  instead. No nightly has executed this branch yet.

### Filed

- [issue 1670](../1670-tier2-runner-has-no-zenoh-router.md) — tier 2's runner
  has no `rmw_zenohd`, the other lane holding the coordinate.
- [issue 1671](1671-nightly-platform-build-red-withholds-every-cell.md) —
  one row's link failure withholds every cell verdict on the platform, the
  mechanism that hid 1657.

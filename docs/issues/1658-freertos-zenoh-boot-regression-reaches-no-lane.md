---
id: 1658
title: "A boot regression in a cmake-built zenoh FreeRTOS image reaches no lane —
  issue 1657 sat on main for two days because the two lanes that hold the
  coordinate never got to their cells"
status: open
type: tech-debt
area: [ci, testing, freertos]
severity: medium
found: 2026-10-03
related: [1657, 1158, 1038, 968]
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

---
id: 1685
title: "The tier-1 lane claims tests that need Zephyr, riscv QEMU, PX4 and build-stage
  fixtures the `host-tests` runner never has — 81 undeclared capability skips fail
  `check-skip-budget` on their own"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-05
related: [1651, 1684, 1161, 0599]
---

## What was measured

Same run as issue 1684 (workflow_dispatch 37252649866, integration job
111583270143). Independently of the 214 real failures, `check-skip-budget`
refused the run:

    83 skipped for an unmet precondition — capability=82  lane=50  quarantine=1
    ERROR: 81 test(s) skipped for a capability this lane never declared it may lack

By reason:

| count | reason |
| --- | --- |
| 54 | `Zephyr not available` (no west workspace on the runner) |
| 6 | `zenoh-pico arm build not available` |
| 6 + 4 + 1 | `qemu-system-riscv64` not found / not available |
| 1 + 1 | `qemu-system-riscv32` (plain, and the Espressif fork) |
| 1 | `qemu-system-arm not on PATH` (orchestration_tiers_freertos) |
| 2 | `borrowed-e2e fixture not built (build/borrowed-e2e/.compile-ok)` |
| 1 | `no single-runtime link proof (build/link-determinism/lkproof)` |
| 2 | ROS 2 peer `examples_rclcpp_minimal_action_server` not installed |
| 1 | PX4 checkout absent |
| 1 | `config/posix/nros-platform.toml not present` (zpico_drift_gate) |

A local run on a host with ROS but no Zephyr workspace gave the same shape
(68 capability skips).

## Why it is one root cause

`board-support.toml` grants Zephyr and ThreadX tier 1, so the tier-1 coordinate
file puts their tests in lane — and the tier's own preflight says the same
thing from the other side ("the recorded fixture build did not cover every
module: zephyr"). The runner is provisioned by `just setup native`, which
installs none of the above. Two of the rows are build-stage artifacts a
`just check` gate produces (`borrowed-e2e`, link-determinism); in `just ci
tier1` (`all`) the gates ran first in the same tree, so splitting the tier
across two runners (issue 1651) means `test-all` can no longer inherit them by
accident — which was never a contract either, since no `test-all` ran.

## Options (not decided — each is a provisioning cost)

- Provision what tier 1 claims on this runner (`just setup zephyr` alone is a
  multi-GB west workspace).
- Narrow the lane this job runs to the host-provisionable cover, and give the
  Zephyr/riscv tier-1 rows an owner that has them (the self-hosted runner).
- Declare each in `.config/capability-skip-baseline.txt` with what retires it
  — honest only if some other lane runs them.

## Acceptance

`check-skip-budget` passes on the `host-tests` integration job without new
baseline lines that no other lane retires.

## 2026-10-06 — fixes landed; the lane now runs where its capabilities are

- #1699: a lane deselects (`[SKIPPED:lane]`) before a capability probe for a
  platform it selects no coordinate of (`require_platform_in_lane`), across
  the Zephyr/emulator/riscv/PX4/FreeRTOS/ThreadX-rv64/rtos_e2e/ROS-peer sites;
  the two host build-stage proofs are built by `build-test-fixtures`;
  `zpico_drift_gate` reads the real descriptor path and passes; ci-base
  installs the action-server peer.
- #1707: the Rust test resolver reads THE Zephyr workspace ladder (it had no
  store arm, so a host provisioned by `just zephyr setup` skipped 56 Zephyr
  tests), and the FVP tests deselect before their probes.
- This PR: tier 1's run leaves the hosted runner, which provisions none of
  Zephyr / ThreadX, for the self-hosted one that does.

Measured locally on a provisioned host, full `lane=tier1`: capability skips
**64 -> 3** (1 baselined, 2 FVP — fixed in #1707). STILL OPEN until the first
`run-matrix` `tier1` run's `check-skip-budget` passes.

## 2026-10-08 — the first self-hosted tier-1 run: the skip budget still fails, now on ROS 2

`run-matrix` run **37685900447** (workflow_dispatch, 2026-10-07 20:57Z), job
**113036322535** `tier 1 (cells)` on `nano-ros-runner`. This is the run the
"STILL OPEN until…" line above was waiting for:

```
check-skip-budget: 2563 ran, 115 deselected (out of lane), 61 skipped for an unmet precondition — capability=61  lane=115
ERROR: 61 test(s) skipped for a capability this lane never declared it may lack:
      14x ROS 2 / rmw_zenoh_cpp not available — install it from apt (`ros-$ROS_DISTRO-rmw-zenoh-cpp`, …)
      12x ROS 2 + rmw_cyclonedds_cpp not available
      10x ROS 2 not found
       9x ROS 2 + rmw_zenoh_cpp not available
       7x ROS 2 DDS not available
       …  qemu-system-arm not on PATH (1x), no AMENT layer ships std_msgs (1x)
```

The local measurement (64 -> 3) assumed a host with ROS 2 sourced. The
self-hosted tier-1 job runs without it: every ROS-peer cell skips, and those
skips are not declared. So the open question has moved. It is no longer
"wrong runner"; it is that this runner's tier-1 job does not source or provide
ROS 2 (and `qemu-system-arm`) for the rows the lane claims.

The same job also has a missing in-lane fixture with the cause this issue
names (PX4 checkout absent):

```
nros-tests::px4_bridge_compile px4_cpp_bridge_generated_messages_compile
Test fixture binary MISSING for an in-lane coordinate: …/build/compile-check-fixtures/px4_bridge_ffi/.compile-ok
```

Its compile-check build logged `px4: PX4-Autopilot submodule absent
(third-party/px4/PX4-Autopilot) — skipping`, so the lane claims a PX4 row that
its own build cannot produce.

The run's other failures are filed separately: `~/.cache` not writable after the
container restart (issue 1755), and the async action client's result stall
(issue 1756).

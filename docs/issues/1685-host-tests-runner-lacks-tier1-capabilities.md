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

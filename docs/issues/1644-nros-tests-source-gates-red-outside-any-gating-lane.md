---
id: 1644
title: "Three fixture-free `nros-tests` targets are red on `main`, and no merge-gating lane runs them"
status: open
type: bug
area: testing, ci
severity: medium
found: 2026-10-02
related: [1521, 1226, 1610]
---

## What this is

Issue 1521's class, measured once more while resolving it. To find which
`nros-tests` targets a merge-gating lane could afford, every target that calls no
runtime fixture resolver (per `scripts/check-lane-contracts.py`'s
`resolvers_used`) and needs no `required-features` — 71 of them — was run on a
worktree of `main` with NO fixture built. 52 passed outright; 17 of those (pure
source/in-process, no spawned process) joined `test-lane-contracts` in issue
1521's fix. Among the ones that did NOT pass, three are not fixture-shaped at
all — they are source invariants or skips that describe drift, red on `main`
today with nothing reporting them:

| target | measured |
| --- | --- |
| `no_local_axis_tables::no_matrix_axis_table_outside_matrix_and_interop` | FAIL: `tests/qos_event_interop.rs:72  const QOS_EVENT_CELLS` — a coordinate table outside `matrix.rs`/`interop.rs`, which the RFC-0051 single-matrix rule forbids |
| `multihost_partition_bake` (2 of 4) | FAIL: `nros codegen entry --model …/robot1_model.yaml` refuses `SystemModel … is stale: resolver pin changed (model 0.9.0 != ours bbf9c04496d0)`. Not diagnosed — may be environmental (which resolver the test's `resolve_ws_with_host` reached) rather than a tree defect |
| `zpico_drift_gate` (2) | `[SKIPPED] config/posix/nros-platform.toml not present — the phase-290 per-platform config layout drifted`. The posix config now lives at `packages/platform/nros-platform-posix/nros-platform.toml`; the gate's own skip text says it has lost its subject, so it currently guards nothing |

The other non-passing targets in the 71 failed for fixture/peer reasons
(`rtos_e2e`, `services`, `ros_editions_*`, …) and are not this issue.

## What to do

Fix each (or, for `multihost_partition_bake`, establish whether it is
environmental), then add the target to `just test-lane-contracts` under the rule
written there: it builds no fixture and spawns no process. `zpico_drift_gate`
and `multihost_partition_bake` spawn processes, so they need a lane decision
of their own, not just a fix.

## Acceptance

All three green on `main`; each either runs in a merge-gating lane or has a
written reason it cannot.

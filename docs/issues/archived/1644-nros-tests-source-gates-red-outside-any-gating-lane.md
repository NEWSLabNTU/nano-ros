---
id: 1644
title: "Three fixture-free `nros-tests` targets are red on `main`, and no merge-gating lane runs them"
status: resolved
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
| `zpico_drift_gate` (2) | **Fixed by PR #1699 (2026-10-06):** pointed at `packages/platform/nros-platform-posix/`, where the phase-290 per-platform config moved; it now runs and passes (16.6 s) instead of skipping on the retired `config/posix/nros-platform.toml`. Still spawns processes, so it still needs the lane decision below |

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

## 2026-10-07 — resolved: all three green on `main`, each placed or reasoned

Measured with the phase-475 census: the scheduled CI run of 2026-10-06, plus a
local gate-image run on `main` at `b92829321`.

| target | on `main` | lane |
| --- | --- | --- |
| `no_local_axis_tables` | PASS both runs (the inline coordinate landed in #1569) | admitted to `test-lane-contracts` |
| `multihost_partition_bake` | green where its fixture exists (issue 1692 asserts on the BUILT entries) | its fixture-free tests are admitted one by one; the bake test resolves a fixture, so it runs in tier 1 |
| `zpico_drift_gate` | PASS both runs in the CI census (#1699) | **not admitted, with a reason:** it runs `cargo` at test time, and in the local image census it failed while blocked on cargo's package-cache lock. A target that contends with a parallel run fails the census's every-run rule by design. Moving its compile to the build stage is issue 1656, item 2 |

The issue's rule, "builds no fixture and spawns no process", is now the
census: a target is admitted if it reaches a verdict in the gate image on every
run. The 2026-10-06 CI census reported `zpico_drift_gate` as NEWLY ADMISSIBLE,
and the local census FAILs it. That disagreement is the contention, and it is
why the target is not admitted.

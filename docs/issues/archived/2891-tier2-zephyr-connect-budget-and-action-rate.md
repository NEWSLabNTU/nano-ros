---
id: 2891
title: "Tier 2's real failures in run 44993007292: five tests across features, out-of-tree RMW, Zephyr C++, zero-copy and sched placement"
status: resolved
type: bug
area: [testing, zephyr, ci]
severity: medium
found: 2026-11-23
related: [phase-590]
resolved: 2026-11-25
resolved_in: "the change that archives this issue"
---

## What fails

`run-matrix.yml` job `tier 2 (1-wise matrix)`, run 44993007292 (2026-11-22, main `98b5d8e0c7`),
reports `Real failures: 6 / 6` in its `junit-real.xml`. Outside issue 2815's Renode twin, they are:

| test | owner |
| --- | --- |
| `image_features_shared_configure` | nros 139 |
| `out_of_tree_rmw_e2e` (the acme `pool_slots` knob) | nros wtf |
| `zephyr_cortex_m_qemu::zephyr_cortex_m_cpp_zenoh_pubsub_e2e` (timeout) | nros 139 |
| `zero_copy::test_zero_copy_message_info` | nros 139 |
| `sched_dims_applied_e2e::sched_dims_applied` (CorePinPlacement, a53) | nros 241, after #4679 |

This issue first named three `zephyr_connect_budget` cases and
`zephyr_rust_zenoh_action_goal_completion_rate`. Those were `[SKIPPED:lane]` in that run, and the
step's bare nextest summary counts a lane skip as a failure (CLAUDE.md, "Bare `cargo nextest`
counts `lane_skip!` panics as FAILURES"). The real list is `junit-real.xml`, not the
`FAIL` lines.

From 2026-11-23 every scheduled tier 2 run stops before building, on
`unknown lane 'lane=tier2'` (the fixture-lane regression #4685 fixes), so this list is from the last run
that built.

## Next

Once #4685 lands, dispatch `run-matrix.yml` on main and re-read `junit-real.xml`. Split this
issue per cause where the causes differ (CLAUDE.md, "Fix the CLASS").

## Acceptance

The tier 2 job is green in a scheduled run. A release blocker:
[phase-590](../../roadmap/phase-590-nros12-release-plan.md) W1.

## Resolution (2026-11-25)

All five tests pass in tier 2 at the nros12 release commit `730dfbebeb` (run
45422927491, the release dispatch, junit `post-submit-junit`):
`image_features_shared_configure`, `out_of_tree_rmw_e2e`,
`zephyr_cortex_m_cpp_zenoh_pubsub_e2e`, `zero_copy::test_zero_copy_message_info`
and `sched_dims_applied`. The last was still red at `0d9fa3e470` (run
45351389552) on its Zephyr CorePin and EDF markers.

That tier 2 run is not green: it has two other real failures,
`emulator::test_qemu_rtic_levels_preempt_and_wake_on_time` and
`isr_wake_rtos::a_timer_interrupt_wakes_the_parked_executor_on_baremetal_mps2`,
both QEMU wall-clock latency shares on baremetal mps2. Neither is one of this
issue's five, and both are tracked as release blocker B31 in
[phase-590](../../roadmap/phase-590-nros12-release-plan.md). This issue's
acceptance ("tier 2 green in a scheduled run") moves to B31 with them, by the
manager's ruling, so that one blocker owns the remaining red.

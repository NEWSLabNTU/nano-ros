---
id: 2891
title: "Tier 2's real failures in run 44993007292: five tests across features, out-of-tree RMW, Zephyr C++, zero-copy and sched placement"
status: open
type: bug
area: [testing, zephyr, ci]
severity: medium
found: 2026-11-23
related: [phase-590]
---

## What fails

`platform-matrix.yml` job `tier 2 (1-wise matrix)`, run 44993007292 (2026-11-22, main `98b5d8e0c7`),
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

Once #4685 lands, dispatch `platform-matrix.yml` on main and re-read `junit-real.xml`. Split this
issue per cause where the causes differ (CLAUDE.md, "Fix the CLASS").

## Acceptance

The tier 2 job is green in a scheduled run. A release blocker:
[phase-590](../roadmap/phase-590-nros12-release-plan.md) W1.

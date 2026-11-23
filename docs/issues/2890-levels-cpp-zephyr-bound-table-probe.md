---
id: 2890
title: "`levels-cpp`'s Zephyr image is refused at configure (the probe wrote no `builtin_interfaces` bound table), and the rt-sched census test fails"
status: open
type: bug
area: [zephyr, sizing, scheduling]
severity: medium
found: 2026-11-23
related: [phase-590]
---

## What fails

`host-tests.yml` job `real-time levels (rt-sched)` (run 45104661214, 2026-11-23, main
`90c2d0659f`):

```text
configure: demo_bringup:zephyr: not configured — `demo_bringup:zephyr`: message bounds not seeded,
because what it declares could not be derived: the metadata probe generated no bound table for
`builtin_interfaces` (examples/workspaces/levels-cpp/build/nros-metadata/metadata-probe-cmake/
build/nros-codegen/nano_ros_cpp/builtin_interfaces/nros_message_bounds.json) ... (NROS_PROBE_FAILURE=refuse)
FAIL [   0.844s] (3/5) nros-tests::rt_sched_levels_e2e linux_rust_thread_census_is_placed_by_the_priority_plan
```

`linux_levels_preempt_in_rank_order_on_one_cpu` passes in the same run.

## Next

Check whether the census failure follows from the refused configure or is separate. The missing
`builtin_interfaces` bound table is a probe-coverage question: which generator writes it for a
C++ image, and why not this one.

## Acceptance

The job is green in a scheduled `host-tests` run. A release blocker:
[phase-590](../roadmap/phase-590-nros12-release-plan.md) W1.

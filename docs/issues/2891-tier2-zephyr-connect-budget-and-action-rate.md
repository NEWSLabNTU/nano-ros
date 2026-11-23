---
id: 2891
title: "Tier 2: the Zephyr connect-budget cells and the Rust action goal-completion rate fail"
status: open
type: bug
area: [zephyr, testing, rmw-zenoh]
severity: medium
found: 2026-11-23
related: [phase-590]
---

## What fails

`run-matrix.yml` job `tier 2 (1-wise matrix)` (run 44993007292, 2026-11-22, main `98b5d8e0c7`):
`Real failures: 6 / 6`. Outside issue 2815's Renode twin, they are:

```text
nros-tests::zephyr_connect_budget zephyr_zenoh_open_against_no_listener_ends_at_its_budget
nros-tests::zephyr_connect_budget zephyr_zenoh_open_against_a_syn_dropping_peer_ends_at_its_budget
nros-tests::zephyr_connect_budget zephyr_zenoh_open_against_a_silent_listener_ends_at_its_budget
nros-tests::zephyr zephyr_rust_zenoh_action_goal_completion_rate
```

The slowest-test list in the same run also shows `sched_dims_applied_e2e::sched_dims_applied`
and `zephyr_cortex_m_qemu::zephyr_cortex_m_cpp_zenoh_pubsub_e2e` failing.

## Next

Read each test's output in the full job log. The three connect-budget cases fail together, which
suggests one cause: the budget fixture, or its build, not the three peers. Split this issue per
cause once known (CLAUDE.md, "Fix the CLASS").

## Acceptance

The tier 2 job is green in a scheduled run. A release blocker:
[phase-590](../roadmap/phase-590-nros12-release-plan.md) W1.

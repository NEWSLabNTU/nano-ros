---
id: 2563
title: "Two-tiers lab, Rust tab: the one-sched-task run prints `10 windows, 0 late`, not `10 late`"
status: open
type: bug
severity: medium
area: [realtime, book, probe]
related: [issue-2269, phase-537]
found: 2026-11-15
---

**Home: [phase-537](../roadmap/phase-537-book-user-guide-organization.md).**
The `lab-tiers` probe track (Rust, weekly) fails at its first run step.

## What

The lab ([two-tiers.md](../../book/src/tutorial/two-tiers.md)) puts a 100 Hz
control loop and a 60 ms telemetry job on one sched task, with
`group_tasks = { ctrl = "main" }` (and the telemetry group on `main` too), and
expects the loop to miss its deadlines: `10 windows, 10 late`. Then it moves
the loop to a higher-priority task and expects `10 windows, 0 late`.

In the Rust tab, on mps2-an385-freertos, the first run already prints
`10 windows, 0 late`, so the lab's lesson does not show and the track's
`probe-expect` for that step fails.

## Observed (2026-11-15)

- Reproduced on the book pass branch and on the tree before it, with the
  in-tree CLI on the host (not the docker probe).
- The build log of the one-task run shows a derived schedule with two tasks:
  `derived schedule: sched task derived-control_node priority 3 /
  derived-telemetry_node priority 2`. That reads as if the Rust build derives
  a task per node and does not apply `group_tasks = { … = "main" }`, which
  would put the loop on its own higher-priority task and explain `0 late`.
  This is a reading of the log, not a proven cause.
- The C and C++ tabs were not re-run.

## Plan

1. Decide which is wrong: the Rust road's handling of `group_tasks`, or the
   lab's assumption about how a Rust image is scheduled.
2. Fix that side; the lab's two runs must print `10 late` then `0 late` in
   every language.
3. Run the `lab-tiers`, `lab-tiers-c` and `lab-tiers-cpp` tracks.

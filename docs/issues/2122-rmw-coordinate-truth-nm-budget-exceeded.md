---
id: 2122
title: "`rmw_coordinate_truth` runs out of its 25 s `nm` budget on the tier-2 runner and reports 16 rows unread"
status: open
type: bug
area: [ci, testing]
severity: low
found: 2026-10-28
related: [2112]
---

## Measured

- Run: run-matrix 40643270547 (schedule, 06:20 UTC, main at `e0f81b4ac`).
- Job: 122666117078, `tier 2 (1-wise matrix)`.

```
rmw_coordinate_truth.rs:485:5:
16 row(s) got no `nm` verdict within budget (located 93 binaries in 2.6s, `nm` over them in 26.4s;
slowest: workspace-cpp-native-service-server 9.0s, workspace-cpp-native-service-client 8.9s, …):
  workspace-features-rust-before-boot: not read — the sweep had spent its 25s
```

The run's other real failure is issue 2112 (FreeRTOS `loan-copy-sub` MISSING).

## Reading

The test reads every linked artifact with `nm` inside a fixed 25 s budget. Single
binaries took 7–9 s here, so the budget measured the runner's load more than
the artifacts. The run-matrix tier-2 run of 2026-10-27 did not fail this test.

## What it is NOT

- **Not a wrong backend.** No row reported a mismatch. The 16 rows were not read
  at all.

## What would close it

Decide between two fixes:

- size the budget from the measured per-binary cost;
- or read the symbols in parallel.

Then confirm in a tier-2 run.

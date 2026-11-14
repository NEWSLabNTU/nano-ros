---
id: 2572
title: "Subscriber silence has no generated watchdog: a timer-backed one should come from `on_violation.mechanism: timer` and count in `ftti_budget`"
status: open
type: enhancement
area: [codegen, contract, zephyr]
severity: medium
found: 2026-11-13
related: [phase-560]
---

GitHub: NEWSLabNTU/nano-ros#3760

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md)** W7
(additions land when a contract needs one). L4 plan item NR-12 (gap G21).
BLOCKED on NEWSLabNTU/ros-launch-manifest#8 (`sub.<e>.on_violation.mechanism:
timer` in rlm).

## What is true now

The K344 safety island supervises the HPC heartbeat with a hand-written
hardware-timer watchdog, re-armed at each take (route 0). Phase-503's timer
supervision (`NROS_CPP_MECHANISM_TIMER`, route watch) covers `supervise:`
budgets. It does not cover subscriber silence, so the island's watchdog is
invisible to codegen and `ftti_budget` skips that route.

## Done when

Once rlm has `sub.<e>.on_violation.mechanism: timer`:

- the `k_timer` watchdog is generated from it;
- `ftti_budget` counts it.

The L4 RFC needs it for N2 at 30 ms.

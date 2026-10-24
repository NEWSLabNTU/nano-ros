---
id: 2018
title: "Kernel budget enforcement is not wired to levels (Linux SCHED_DEADLINE, NuttX SCHED_SPORADIC)"
status: open
type: enhancement
area: [executor, linux, nuttx]
severity: low
found: 2026-10-24
related: [phase-509, rfc-0108, 2016, 2017]
---

## Measured

phase-509 P10's last checklist item reads: "Kernel budget enforcement where
the kernel has it (Linux SCHED_DEADLINE, NuttX SCHED_SPORADIC), one PR per
platform, after its runner."

The runners have landed (#2548 native, #2553 FreeRTOS/NuttX, and the Rust
Zephyr/ThreadX PR). Budget enforcement has not:

- The NuttX C level runner calls `nros_nuttx_apply_current_sporadic` per level
  thread, as the per-tier runner did. The kernel therefore throttles the level
  thread. The executor's own budget accounting does not know about it.
- The native runner applies no `SCHED_DEADLINE`.
- On the C runners and on the Rust spawned levels,
  `apply_tier_sched_policy` (the executor's default scheduling context) is
  applied for the boot level only. A spawned level shares the core's executor,
  and that executor has one default context.

## Closes when

Each platform with kernel budgets ties a level's declared budget and period to
the level's thread, one PR per platform. The level's executor-side accounting
must agree with the kernel's. A test must show a level overrunning its budget
being throttled.

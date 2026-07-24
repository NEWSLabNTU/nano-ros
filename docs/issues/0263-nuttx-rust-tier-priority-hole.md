---
id: 263
title: "NuttX Rust arm: spawned tiers get their stack but not their priority off the sporadic path"
status: open
type: bug
severity: medium
area: boards
related: [issue-0246, phase-302]
---

## Finding (implementation-completeness audit, 2026-07-25)

`nros-board-nuttx/src/lib.rs` (~590-650): spawned Rust tier threads set
`Builder::stack_size` (the #246 fix) but never apply the tier's declared
SCHED_FIFO priority — std's Builder has no priority, and only the
sporadic path calls the kernel with sched params. A non-sporadic
`[tiers.*.nuttx] priority = N` on the Rust arm runs at the parent's
priority; the C/C++ arm (`nuttx_run_tiers.c`) applies it correctly at
`pthread_create` time.

The `realtime_tiers_e2e` nuttx rust cell still passes because the ratio
proof doesn't need preemption — the drop is invisible until contention.

## Fix

Call `pthread_setschedparam(pthread_self(), SCHED_FIFO, ..)` (via the
existing extern shims) at tier-thread entry before setup — same adopt
pattern the boot tier and the C arm use. Marker + e2e assert like the
W5.x consumers. Phase-302 W3.

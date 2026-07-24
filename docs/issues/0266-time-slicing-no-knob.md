---
id: 266
title: "Time-slicing exists in every kernel and has no nano-ros knob (ThreadX shim hardwires TX_NO_TIME_SLICE)"
status: open
type: enhancement
severity: low
area: orchestration
related: [rfc-0052]
---

## Finding (implementation-completeness audit, 2026-07-25)

Zephyr `CONFIG_TIMESLICING`, FreeRTOS `configUSE_TIME_SLICING`, ThreadX
`tx_thread_create` time-slice param (shim passes `TX_NO_TIME_SLICE`
unconditionally), NuttX RR interval — none has a tier-schema surface.
Same-priority tiers therefore run FIFO-until-block on every RTOS with no
way to request round-robin.

Demand-driven: add a `time_slice_us` tier field + per-RTOS lowering when
a consumer appears. Not scheduled; recorded so the absence is a decision.

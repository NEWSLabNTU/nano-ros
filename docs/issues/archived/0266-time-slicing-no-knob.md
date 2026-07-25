---
id: 266
title: "Time-slicing exists in every kernel and has no nano-ros knob (ThreadX shim hardwires TX_NO_TIME_SLICE)"
status: resolved
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

## Resolution (2026-07-25) — ThreadX consumer landed

The demand appeared (ThreadX, the named example). Added the `time_slice_us`
per-platform sub-table knob end-to-end + the ThreadX consumer:

- Schema: `TierPlatformSpec.time_slice_us` (rlm `db91f2b`) →
  `TierRtosSpec`/`ResolvedTier`/`TierSpec.time_slice_us` (orchestration-ir +
  nros-platform) → baked by the `nros::main!` macro. Same sub-table-scoped
  precedent as `preempt_threshold`.
- Bake-time fail-loud: `validate_tier_platform_applicability` rejects
  `time_slice_us` off ThreadX (the other RTOSes' time-slicing is a GLOBAL
  kernel config, not a per-tier knob — a per-tier value would be a silent
  drop). The C/C++ emit path (no time-slice field yet) errors loudly rather
  than dropping it.
- ThreadX consumer: `nros_threadx_create_task` gained a `time_slice_us` param
  (was hardwired `TX_NO_TIME_SLICE`); converts µs→ticks in the C shim and
  passes it to `tx_thread_create`. The boot tier self-applies via
  `tx_thread_time_slice_change`. Both print the `nros: time slice set tier=`
  marker (= `THREADX_TIME_SLICE_MARKER`), accept-only (ThreadX honors a
  per-thread slice unconditionally).
- ws-realtime-rust `low` tier declares `threadx.time_slice_us: 5000`; new
  `threadx_time_slice_applied` e2e (joined to the `threadx-realtime-rust-port`
  nextest group) boots the threadx-linux image and asserts the marker — PASS.

Other RTOSes remain a follow-up: FreeRTOS/NuttX/Zephyr time-slicing is global
(compile-time config), so it has no honest per-tier surface today — the
validation says so loudly rather than pretending.

---
id: 744
title: "freertos-posix: blocking waits in raw pthread primitives park the whole simulated kernel — RT tiers see ~80 ms stalls that priorities cannot fix"
status: open
type: bug
area: boards
related: [issue-0715, issue-0623]
---

# 0744 — freertos-posix: raw blocking waits starve the tiers

Measured on the ASI reference consumer (phase-4, controller workspace on
`nros-board-freertos-posix`, cyclonedds RMW, host closed loop against a
real Autoware planner):

- Control timer in an RFC-0047 group bound to `[tiers.control]`
  (`class = "real_time"`, `spin_period_us = 5000`, freertos priority 7 of
  `configMAX_PRIORITIES = 10`), period 30 ms.
- Callback compute is ~20 µs (the controller's own
  `processing_time_ms` debug topic: 0.015–0.021 ms lateral, 0.004 ms
  longitudinal — the emergency-stop path).
- Observed output: ~25 Hz instead of 33 Hz, `ros2 topic hz` min interval
  2 ms (catch-up bursts) and **max 80–85 ms stalls** — before tiers the
  same shape at ~12 Hz with 140 ms stalls, i.e. the tier bought priority
  but the stalls survive it.

A 20 µs callback in a priority-7 tier that still starves for 80 ms means
the STALL is not scheduling within FreeRTOS — nothing schedulable exists
for those 80 ms. That is the GCC/Posix port's known hazard: the port
simulates one core (one pthread holds the "running" token; the tick
handler switches tasks by signals AT FreeRTOS API boundaries). Any task
that blocks in a RAW host primitive — a `pthread_cond_timedwait` from
the platform shim (`nros-platform-freertos`'s zenoh-pico-layout
mutex/condvar arms are raw pthread objects), or a blocking socket
syscall reached from task context — parks the ENTIRE simulated kernel:
the tick cannot preempt a thread the port does not own the blocking of,
so the RT tier waits out the host-side timeout with it.

This is the same "RTOS threads + host Cyclone" seam phase-370's risk
note named (0715's class on threadx-linux, where it SEGVs; here it
degrades into latency).

## Repro shape

Any freertos-posix workspace with (a) a tier-bound timer of short period
and (b) a default-context subscription draining real traffic (ASI: five
subscriptions incl. 8.8 KiB trajectories at 10 Hz). Compare
`ros2 topic hz` of the timer-driven topic against 1/period; watch max
interval. ASI numbers above at nano-ros `9f0a387b9`, ASI branch
`nano-ros` @ `ed26a10` (+ the ctrl_period=0.03 launch param).

## Fix shape

On the freertos-posix board, executor/platform WAITS reached from task
context must be FreeRTOS-visible: route the executor's inter-poll wait
and the platform condvar arms through `vTaskDelay`/FreeRTOS sync (or the
port's `wait_for_event`) instead of raw pthread waits, and keep raw
blocking syscalls out of task context (Cyclone's own worker threads are
fine — they are not FreeRTOS tasks). An audit sweep of
`nros-platform-freertos/src/platform.c` wait sites × "is this ever
called from a FreeRTOS task on the posix board" would bound the class;
0623's tier-vs-transport priority table is the sibling concern once
waits are visible.

## Downstream state (until fixed)

ASI ships the tiers config anyway (12.4 → 19 Hz at the old implied
period; 25.3 Hz at the intended 30 ms period) and tracks the residual as
phase-4 profiling round 2.

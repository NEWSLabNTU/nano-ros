---
id: 2696
title: "No executor wake path is safe to call from an interrupt handler: GuardCondition::trigger reaches xSemaphoreGive on FreeRTOS, and the ISR-safe platform signal has no caller"
status: open
type: bug
severity: medium
area: [freertos, threadx, zephyr, rmw]
found: 2026-11-18
related: [phase-574]
---

## What

Nothing in the executor wakes a blocked spin safely from an interrupt handler.
A driver that completes work in an interrupt has no correct way to say so.
Examples are an NPU's job-done interrupt, a DMA transfer-complete callback, or
a camera frame signal.

The obvious call is `GuardCondition::trigger()`, documented as "safe to use
from any thread". It is not safe from an interrupt handler on FreeRTOS:

- `trigger()` (`packages/core/nros-node/src/executor/types.rs`) stores the
  flag, then calls `wake_cb`, which is `nros_rmw_runtime_wake_cb`.
- `nros_rmw_runtime_wake_cb` (`executor/spin.rs`) reaches
  `NodeWake::signal`, and through it `nros_platform_wake_signal`.
- On FreeRTOS that is `xSemaphoreGive`
  (`packages/platform/nros-platform-freertos/src/platform.c`). FreeRTOS
  requires the `FromISR` variant inside an interrupt handler. The non-ISR call
  can corrupt the scheduler's lists or assert, depending on the port and
  `configASSERT`.

The ISR-safe half exists and is never reached:

- `nros_platform_wake_signal_from_isr` is defined on every port. FreeRTOS's
  uses `xSemaphoreGiveFromISR` and `portYIELD_FROM_ISR`.
- `NodeWake` declares it in its extern block with
  `#[allow(dead_code)] // Wired by 124.B.7 ISR callers in a follow-up.`
- `nros_rmw_runtime_wake_cb_from_isr` (`executor/spin.rs`) is
  `#[allow(dead_code)]` and only forwards to the normal callback.
- The doc comment above `nros_rmw_runtime_wake_cb` says ISR callers "route
  through the platform's `signal_from_isr` API instead of this cb directly".
  No path does.

A second gap is that a trigger may not wake anything at all. `wake_cb` is
installed only on `alloc` + `rmw-cffi` builds. Elsewhere `trigger()` sets the
flag and returns, and a spin blocked in its wait sees it only at the next
timeout or transport event.

## Where each port stands

| Port | `wake_signal` | `wake_signal_from_isr` | `trigger()` from an ISR today |
| --- | --- | --- | --- |
| FreeRTOS | `xSemaphoreGive` | `xSemaphoreGiveFromISR` + `portYIELD_FROM_ISR` | unsafe |
| Zephyr | `k_sem_give` | `k_sem_give` | safe, because `k_sem_give` is ISR-safe |
| ThreadX | `tx_semaphore_ceiling_put` | forwards to `wake_signal` | safe, per the comment that the put is ISR-safe |
| POSIX | `sem_post` + pipe write | forwards to `wake_signal` | not applicable (no interrupt handlers) |
| mps2-an385 bare metal | flag | flag | safe, nothing is blocked |
| RTIC levels (bare metal) | not reached from an ISR | not reached from an ISR | not exposed: the ISRs only spawn; the waiter's wake runs in task context |

So two of the three RTOS ports are safe by luck rather than by design, and the
one with the strictest rule is the one that breaks.

## Why it matters now

Hardware that finishes work in an interrupt is how vision and neural
accelerators, 2D engines, codecs, cameras and DMA report completion. The
perception work under phase-574 needs a device job's completion to wake the
executor. Today the only sound pattern is a polling callback or an RTOS task
that waits on its own semaphore and then triggers. Each of those adds a stack,
latency or both.

## Fix direction

1. Add `NodeWake::signal_from_isr`, calling
   `nros_platform_wake_signal_from_isr`, beside `NodeWake::signal`.
2. Give `GuardCondition` an explicit interrupt-context entry point, for
   example `trigger_from_isr()`. It stores the flag and calls an ISR wake
   callback, installed beside `wake_cb`. Keep `trigger()` for thread context,
   and change its doc comment from "any thread" to "any thread, not an
   interrupt handler".
3. Give `nros_rmw_runtime_wake_cb_from_isr` a real body: the flag store, then
   `NodeWake::signal_from_isr`. Drop its `dead_code` allow.
4. Make a wake reachable without `alloc` + `rmw-cffi`, or state in the API
   that `trigger()` on such builds is observed at the next spin, not
   immediately.
5. Add a test that calls the ISR path on the FreeRTOS QEMU image from a real
   interrupt, for example a timer interrupt, and asserts that the blocked spin
   wakes within one tick.

Sweep for other callers that might signal from interrupt context:
`git grep -nE 'trigger\(|wake_signal\(' -- packages/boards packages/drivers packages/platform`.

## Acceptance

- A guard condition triggered from a FreeRTOS interrupt handler wakes a
  blocked `spin_once` within one tick, with no assertion. This is measured on
  the MPS2 FreeRTOS QEMU image.
- Every port's `wake_signal_from_isr` is reached by a test.
- The `dead_code` allows on the ISR wake path are gone.

---
id: 2696
title: "No executor wake path is safe to call from an interrupt handler: GuardCondition::trigger reaches xSemaphoreGive on FreeRTOS, and the ISR-safe platform signal has no caller"
status: resolved
type: bug
severity: medium
area: [freertos, threadx, zephyr, rmw]
found: 2026-11-18
resolved: 2026-11-18
resolved_in: "fix(executor, issue 2696): an interrupt-safe executor wake, measured on FreeRTOS and POSIX"
related: [phase-574, phase-579]
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

## Resolution (2026-11-18)

Fixed in [phase-579](../../roadmap/phase-579-device-ports.md) W1. The Rust
executor has an explicit interrupt path, the ports either take it under a test
or refuse it, and the two sites that already signalled from a handler use it.

- **API.** `GuardCondition::trigger_from_isr() -> bool` sets the flag and
  calls an interrupt wake callback installed beside the thread one. It returns
  whether the executor's wake object took the signal; `false` means the flag
  is seen at the next deadline. `trigger()` is documented as thread-only.
- **Plumbing.** `nros_rmw_runtime_wake_cb_from_isr` has a real body:
  `WakeCtx::signal_from_isr` then `NodeWake::signal_from_isr`, which calls the
  PX4 hook and the port's `nros_platform_wake_signal_from_isr`. Band workers
  are reached too: `BandWaiter::new` takes a `wake_from_isr`, so every waiter
  states one. The `dead_code` allows on this path are gone.
- **Ports.**
  - FreeRTOS: `xSemaphoreGiveFromISR` + `portYIELD_FROM_ISR`, tested.
  - POSIX: async-signal-safe (`sem_post` + pipe `write`, no `sem_getvalue`,
    `errno` restored; the waiter drains stacked posts), tested.
  - Zephyr, ThreadX, NuttX and mps2 bare metal REFUSE (-1) until a test
    reaches each: [issue 2706](../2706-isr-wake-ports-untested.md).
  - The trait default (`PlatformThreading::wake_signal_from_isr`) refuses
    instead of forwarding to the thread signal, and `platform.h` says a
    caller must not fall back to `signal()` from an interrupt.
- **A second site of the same class.** POSIX's termination signal handler
  posted every wake object through the thread signal, whose `sem_getvalue` is
  not async-signal-safe. It now posts through the interrupt one.
- **Board.** The FreeRTOS MPS2-AN385 vector table had no external interrupts.
  It has the 32 now, each a weak alias of `Default_Handler`.
- **Without `alloc` + `rmw-cffi`** there is no wake callback, and
  `trigger_from_isr` returns `false`: the flag is observed at the next spin.
  The doc comment states it (fix direction 4).

**Tests.**

- `tests/isr_wake_rtos.rs` (image `isr-wake-freertos-mps2`): CMSDK TIMER1
  fires every 20 ms, and its handler calls `trigger_from_isr` while the
  executor parks in `spin_once(1 s)`. Every interrupt is served, and the guard
  callback runs 83 to 102 us after it at `-icount shift=5`. Two mutations are
  red:
  - the thread `trigger()` in the handler: `FreeRTOS ASSERT FAILED ...
    port.c:438`, then a hard fault;
  - the interrupt path skipping the port signal: 50 callbacks for 2462
    interrupts, the worst 19.7 ms late.
- `c_port_posix_wake.rs`: a real `SIGUSR2` handler wakes a waiter on another
  thread, `errno` survives, and stacked posts are served by one wait.
- `band_wake_routing_tests`: an interrupt wake uses only each waiter's
  `wake_from_isr`, never its thread `wake`.

**Sweep.** `git grep -nE 'trigger\(|wake_signal\(' -- packages/boards
packages/drivers packages/platform` finds no caller that signals from an
interrupt through the thread path.

**Not in this fix.**

- **C and C++.** The ROS-mirrored types (`rcl_guard_condition_t`,
  `rclcpp::GuardCondition`) stay thread-only, as stock ROS has them (RFC-0139
  D1). C and C++ get an interrupt wake through phase-579 W3's device
  completion handle, on this plumbing; phase-579 W1b tracks it.
- **Considered, not taken: `trigger()` choosing its path by asking the port
  whether it runs in an interrupt** (IPSR, `xPortIsInsideInterrupt`,
  `k_is_in_isr`, ThreadX's system state). It would keep the ROS API unchanged
  and make an existing interrupt call correct. But a port whose `in_isr()`
  answered wrong would silently take the thread path and hard-fault, which is
  the failure the first mutation shows. It can be revisited if every port can
  answer `in_isr()` reliably and a test proves each answer.


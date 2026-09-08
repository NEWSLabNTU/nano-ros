---
id: 1237
title: "Cyclone's receive thread calls FreeRTOS scheduler primitives on the
  freertos-posix lane, and the port asserts: `vPortYield` from a
  non-FreeRTOS thread"
status: open
type: bug
area: platform, rmw
severity: high
found: 2026-09-07
related: [issue-0900, phase-124, phase-370]
---

## Symptom

The `freertos-posix` image runs normally, then aborts:

```
[INFO] nros: [ 2011.142000] Inputs not ready — commanding safe stop.
FreeRTOS ASSERT FAILED: .../portable/ThirdParty/GCC/Posix/port.c:389
Aborted (core dumped)          # exit 134
```

Boot is clean — `Actuation Safety Island is Live`, no arena error, no other
diagnostic. Observed once in roughly forty minutes of running an ASI
controller image on this lane.

## What line 389 asserts

```c
void vPortYield( void )
{
    /* This must never be called from outside of a FreeRTOS-owned thread, or
     * the thread could get stuck in a suspended state. */
    configASSERT( prvIsFreeRTOSThread() == pdTRUE );
```

So a thread the port does not own entered the scheduler. `prvIsFreeRTOSThread`
answers from a pthread TLS key the port sets when it creates a task's thread.

## The path, by inspection

1. `nros-rmw-cyclonedds/src/session.cpp:105`, `on_data_available`, whose own
   doc says: *"Cyclone calls this from its own receive/delivery thread"*.
2. It invokes the runtime wake callback. The comment states the contract:

   > Safe to call from a foreign thread by contract: the runtime callback does
   > a flag write plus a condvar signal and nothing else.

3. On this platform the wake is
   `nros-platform-freertos/src/platform.c::nros_platform_wake_signal`, whose
   `xSemaphoreGive` reaches `portYIELD_WITHIN_API` → `vPortYield`.

CONFIRMED from the core of the original abort (`coredumpctl`, systemd kept it):

```
#5  freertos_assert_failed ()
#6  vPortYield ()
#7  xQueueGenericSend ()
#8  nros_platform_wake_signal ()
#9  nros_rmw_cyclonedds::(anonymous namespace)::on_data_available(int, void*)
#10 libddsc.so.0                      <- Cyclone's receive thread
```

The class was predicted correctly and the function was not: this filing first
guessed `nros_platform_condvar_signal`. The wake primitive is the one on the
path.

The contract in step 2 holds on every OTHER FreeRTOS board, where Cyclone's
threads ARE FreeRTOS tasks. It does not hold here, and this board's own
descriptor says why:

> The FreeRTOS kernel's `ThirdParty/GCC/Posix` port runs tasks as host pthreads
> with signal-driven preemption

Cyclone is the HOST library on this lane, so its receive thread is an ordinary
pthread that the port never registered.

The ISR variant is not an escape: this port's `portYIELD_FROM_ISR` expands to
`portEND_SWITCHING_ISR`, which calls `vPortYield()` too.

## Reproduction (deterministic)

`kill -INT <pid>` on the running image aborts it every time; `SIGTERM` exits
cleanly (143). This port deliberately leaves SIGINT unblocked in EVERY thread —

```c
/* Don't block SIGINT so this can be used to break into GDB while
 * in a critical section. */
sigdelset( &xAllSignals, SIGINT );
```

— which is what makes a foreign-thread entry easy to provoke on demand. The
original occurrence needed no signal; it happened on its own after ~40 minutes.

## Why it is intermittent, and why CI has never seen it

The assert needs the signal to actually yield — a contended mutex, or a give
that unblocks a higher-priority task. Most signals do neither.

CI cannot see it for a separate reason: `run-freertos-posix-ci.sh` runs the
entry under `timeout`, and `timeout` reports 124 when it terminates the
command, whatever the command's own exit status would have been. A run that
aborts before the timeout would surface as 134 and fail
`is_success_or_timeout` — but the spin does not end on its own, so the timeout
always wins the race. The lane is green and the defect is real.

## Fix

The runtime already has the shape for the safe answer. `spin.rs` documents a
**poll-only** mode for backends that leave the `set_wake_callback` slot NULL
("Poll-only backends ... bare-metal"), and `condvar_wait_until` is bounded, so
a missed wake costs latency and not liveness.

Two candidates:

1. **Do not install the wake callback on a board whose RMW threads are
   foreign.** The freertos-posix board presents the poll-only slot; every other
   FreeRTOS board keeps today's behaviour. Small, and it uses a path that
   already exists — at the cost of the wake latency `on_data_available` was
   added to win (measured on an536: 5,069 takes per reader for 42 deliveries
   without it, though that measurement is from a board where the callback is
   safe).

2. **Make the wake foreign-thread-safe**, which is the plan the code already
   names: phase-124.B.7.c's `signalfd`/`eventfd` + runtime worker thread. The
   foreign thread touches no FreeRTOS API; a FreeRTOS-owned relay does the
   signal. Keeps the latency win and fixes the class rather than this
   instance.

A third option — swapping in the `FromISR` give — was measured and REJECTED.
On this port `xPortSetInterruptMask()` returns 0 and `vPortDisableInterrupts()`
no-ops unless the caller is a FreeRTOS thread, so the ISR variant drops the
assert and keeps the race: a quieter bug, not a fixed one. `portYIELD_FROM_ISR`
also expands to `vPortYield()` here, so it is not even quiet.

## Status

Fixed by declining the wake slot on this board (option 1). Verified:

* before — `kill -INT` gives `rc=134`, `FreeRTOS ASSERT FAILED`, core dumped;
* after — same signal, no assert, no core, process still running;
* delivery still works on the poll-only path: with a publisher on
  `/vehicle/status/steering_status`, the controller logs
  "Waiting for steering data" ONCE (before the publisher starts) while the
  three unpublished inputs keep reporting every cycle.

This filing originally guessed the wrong function and said what would refute
it. The core named `nros_platform_wake_signal`, so that guess was corrected
here rather than quietly dropped.

---
id: 1202
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

3. On this platform that condvar signal is
   `nros-platform-freertos/src/platform.c::nros_platform_condvar_signal`:

   ```c
   xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
   if (c->waiters > 0) { xSemaphoreGive((SemaphoreHandle_t) c->sem); ... }
   xSemaphoreGive((SemaphoreHandle_t) c->mutex);
   ```

   Both `xSemaphoreTake` (when contended) and `xSemaphoreGive` (when it
   unblocks a higher-priority task) reach `portYIELD_WITHIN_API` → `vPortYield`.

The contract in step 2 holds on every OTHER FreeRTOS board, where Cyclone's
threads ARE FreeRTOS tasks. It does not hold here, and this board's own
descriptor says why:

> The FreeRTOS kernel's `ThirdParty/GCC/Posix` port runs tasks as host pthreads
> with signal-driven preemption

Cyclone is the HOST library on this lane, so its receive thread is an ordinary
pthread that the port never registered.

The ISR variant is not an escape: this port's `portYIELD_FROM_ISR` expands to
`portEND_SWITCHING_ISR`, which calls `vPortYield()` too.

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

A third option — marking FreeRTOS-owned threads in `freertos_task_trampoline`
and having `condvar_signal` defer when unmarked — works but puts host-pthread
reasoning into a file that also compiles for three bare-metal targets.

## What this issue does not claim

The reproduction is one occurrence, caught without a debugger attached. The
mechanism above is established by reading the code, not by a captured stack: a
`gdb` soak was still running when this was filed. If someone reproduces it
under a debugger, the backtrace of the asserting thread should name a Cyclone
receive/delivery thread and `nros_platform_condvar_signal` beneath it — and if
it does not, this analysis is wrong and the issue should say so.

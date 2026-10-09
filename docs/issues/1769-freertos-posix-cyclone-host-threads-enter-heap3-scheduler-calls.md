---
id: 1769
title: "freertos-posix Cyclone images: Cyclone's host threads allocate through
  heap_3, whose vTaskSuspendAll/xTaskResumeAll are not callable off a FreeRTOS
  thread"
status: open
type: bug
area: [platform, rmw]
severity: medium
found: 2026-10-09
related: [1750, 1237, 0832]
---

## What is established (from code, NOT measured)

Issue 1750's class on the OTHER board whose kernel runs as host pthreads beside
a host Cyclone. On `freertos-posix`:

- Cyclone is the HOST library (host ddsrt), so its `gc`/`dq.*`/`tev`/`recv`
  threads are plain pthreads the FreeRTOS POSIX port never registered.
- ddsrt's heap is funnelled into `nros_platform_alloc` (issue 0832: the funnel
  is on whenever `NanoRos::Platform` exists, which it does on this board), and
  `nros_platform_alloc` is `pvPortMalloc` — heap_3 here
  (`NROS_PLATFORM_FREERTOS_HEAP_3`, `cmake/board/nano-ros-board-freertos-posix.cmake`).
- heap_3's `pvPortMalloc`/`vPortFree` bracket the C library call with
  `vTaskSuspendAll()` / `xTaskResumeAll()`, and
  `nros_platform_realloc` does the same by hand
  (`packages/platform/nros-platform-freertos/src/platform.c`).

From a foreign thread those two calls:

1. increment/decrement the kernel's `uxSchedulerSuspended` and (inside
   `xTaskResumeAll`'s `taskENTER_CRITICAL`) the port's GLOBAL
   `uxCriticalNesting` without any exclusion against the FreeRTOS task doing
   the same — `vPortDisableInterrupts` only masks signals when
   `prvIsFreeRTOSThread()`, so a foreign caller excludes nothing;
2. can reach `taskYIELD_IF_USING_PREEMPTION()` → `vPortYield()`, which
   `configASSERT(prvIsFreeRTOSThread() == pdTRUE)`s — the same assertion issue
   1237 captured on the wake path, here on the allocator.

Issue 1237 declined the foreign WAKE on this board; it did not touch the heap,
because heap_3 is "just malloc" — but the scheduler-suspension bracket around
it is exactly the part a foreign thread cannot call.

## Not established

- That any freertos-posix image actually corrupts or aborts. Not run:
  `freertos_posix_{c,cpp}_entry_delivers_over_cyclonedds` were not built or run
  for this filing. Unlike threadx-linux there is no recursion counter to read;
  the failure would be a lost/duplicated suspension count or an intermittent
  `configASSERT` abort, so a passing run does not clear it.

## Shape of a fix

The threadx-linux fix (issue 1750, `nros-platform-threadx`'s
`threadx_context.h`) is the template: ask "is the caller a kernel thread?" at
the platform seam and give a foreign caller the C library heap directly — on
heap_3 that is the SAME heap, so no address dispatch is needed, only skipping
the suspension bracket. The obstacle is the predicate: the POSIX port keeps
`prvIsFreeRTOSThread()` and its `xThreadKey` static, so either the FreeRTOS
kernel fork exports one (`xPortIsFreeRTOSThread()`, fork rules apply) or the
nros platform marks the tasks it creates itself (and the kernel's own idle /
timer tasks are accounted for).

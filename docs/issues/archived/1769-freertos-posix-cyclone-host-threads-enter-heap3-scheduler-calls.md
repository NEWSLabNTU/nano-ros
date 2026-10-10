---
id: 1769
title: "freertos-posix Cyclone images: Cyclone's host threads allocate through
  heap_3, whose vTaskSuspendAll/xTaskResumeAll are not callable off a FreeRTOS
  thread"
status: resolved
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

## Resolution

Fixed on `fix/1769-freertos-posix-foreign-threads`.

**Measured first (2026-10-11).** It depends on where Cyclone comes from:

- **Host Cyclone** (`find_package` finds ROS's `libddsc.so` — every host with
  ROS 2, and the default fixture build here): the issue-0832 funnel is OFF,
  so ddsrt's heap is the C library's. Under gdb (breakpoints on
  `nros_platform_{alloc,dealloc,realloc,wake_signal}` and `vTaskSuspendAll`,
  4 s run) only FreeRTOS tasks (`app`, `Tmr Svc`, the entry task) reached
  them. Not live.
- **Self-provisioned Cyclone** (a host with no ROS 2,
  `-DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON`): it did not even LINK — a
  second defect. The Phase 186 trims run only on the lwIP branch, so
  freertos-posix built a SHARED `libddsc.so` that, via the 0832 funnel,
  links the non-PIC FreeRTOS kernel archive: `relocation R_X86_64_PC32
  against symbol pxCurrentTCB can not be used when making a shared object`.
  With that fixed (below), the path is LIVE: Cyclone's `gc` (327), `tev` (42),
  `dq.builtins` (29), `dq.user` (2) and `recv` (2) threads called
  `vTaskSuspendAll` in a 4 s run, and over 10 runs the image exited 0 four
  times — five SIGABRT on `FreeRTOS ASSERT FAILED` (`queue.c:1678`,
  `tasks.c:4033`), one SIGSEGV — delivering in 6.

**Fix.**

- `nros-platform-freertos/src/platform.c`: on heap_3 the platform heap calls
  the C library directly (`malloc`/`free`/`realloc`), keeping
  `vApplicationMallocFailedHook` on NULL. heap_3 exists only on the host
  simulator board, whose glibc allocator is already thread-safe, so the
  scheduler bracket bought nothing and every caller can skip it — which also
  means no "is this a FreeRTOS thread?" predicate is needed (the POSIX port
  keeps it static in an unmodified upstream kernel, the obstacle the issue
  named).
- `cmake/board/nano-ros-board-freertos-posix.cmake`: a self-provisioned
  Cyclone is static with no DDS Security / SSL / SHM, as threadx-linux's is.
- The wake path was already declined on this board (issue 1237). No other
  FreeRTOS service is reached by a Cyclone thread: ddsrt's sync and threads
  are the host's.

**After.** Same self-provisioned build: 10/10 runs rc 0 and delivered; under
gdb no Cyclone thread reaches `vTaskSuspendAll` (only `app`, `heap_peak` — a
FreeRTOS task — `Tmr Svc` and the entry task).

**Regression test.** The `workspace-c-freertos-posix` row now
self-provisions Cyclone (the C++ row keeps the host's, so both provenances
are covered), and `freertos_posix_c_entry_delivers_over_cyclonedds` now reads
stderr too, refuses `nros_tests::output::FREERTOS_ASSERT_FAILED` and requires
a clean exit. Fixed: 3/3 PASS. Negative control (heap fix reverted, row
rebuilt): 3 of 5 FAIL on the assert.

Sweep: `git grep -n "pvPortMalloc\|vPortFree\|vTaskSuspendAll" -- packages/platform/nros-platform-freertos`.

## Not measured (at resolution)

- The C++ freertos-posix entry self-provisioned (it shares the platform TU).
- The measurement built against the main checkout's FreeRTOS kernel through
  an ambient `FREERTOS_DIR` (same commit, `0adc196d4`); the test runs used
  this worktree's.
- The `destroy_subscription failed with 1` line at teardown appears with the
  host Cyclone too, before and after; it is not this issue.

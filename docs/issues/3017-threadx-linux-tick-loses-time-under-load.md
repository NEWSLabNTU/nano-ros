---
id: 3017
title: "On threadx-linux the tick timer waits a relative period and reads CLOCK_REALTIME, so the tick loses time under load"
status: open
type: bug
severity: medium
area: [platform, threadx, timing]
related: [issue-3010]
found: 2026-11-28
---

## What happens

Same class as issue 3010, on the other host-simulated RTOS port. On
`threadx-linux`, nano-ros's monotonic clock is the ThreadX tick
(`tx_time_get`, `packages/platform/nros-platform-threadx/src/platform.c`).
The ThreadX Linux port's timer thread,
`_tx_linux_timer_interrupt` in
`third-party/threadx/kernel/ports/linux/gnu/src/tx_initialize_low_level.c`,
does three things wrong:

- each period it reads the clock and waits until **now + one period**
  (`sem_timedwait` on that deadline), then runs `_tx_timer_interrupt()`
  once. The wakeup latency is added to every period and never recovered,
  so the tick runs slow, as on FreeRTOS POSIX before 3010's fix;
- the deadline comes from `CLOCK_REALTIME`, so a wall-clock step (NTP, a
  manual set) shortens or stretches a tick;
- the thread calls `nice(10)`, which lowers its priority, so host load
  delays it more.

Not measured yet. The FreeRTOS POSIX port, whose structure is the same,
measured 9.3 % slow at load ~5 and 12.6 % slow under load (issue 3010).

## Fix direction

The one ruled for 3010: count the ticks due from `CLOCK_MONOTONIC` since
start, wait to absolute tick boundaries, and deliver every owed tick. The
ThreadX kernel is already a fork with a patch line (`nros-lp64-ulong` in
`.gitmodules`), so the patch lands there. Not done here: a fork patch is
its own change, and the ruling for 3010 named the FreeRTOS port.

## Acceptance

The same as 3010's: under deliberate host load, tick time tracks host
`CLOCK_MONOTONIC` within 0.1 % over a minute, with the unloaded case as
the control.

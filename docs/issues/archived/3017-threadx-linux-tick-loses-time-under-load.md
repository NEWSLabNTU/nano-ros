---
id: 3017
title: "On threadx-linux the tick timer waits a relative period and reads CLOCK_REALTIME, so the tick loses time under load"
status: resolved
resolved: 2026-11-28
resolved_in: "the change that adds this line"
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

Measured below: 0.51 % slow on a lightly loaded host and 0.74 % slow under
load. That is smaller than the FreeRTOS POSIX port's 9.3 % and 12.6 %
(issue 3010), because the ThreadX board ticks at 100 Hz: each period's
lost wakeup latency is a smaller share of a 10 ms period than of a 1 ms
one. It is still seven times the 0.1 % acceptance bound.

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

## Fix (2026-11-28)

The ThreadX fork's `nros-lp64-ulong` patch line gains one commit on
`_tx_linux_timer_interrupt`:

- the thread records a `CLOCK_MONOTONIC` start, then sleeps with
  `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, …)` to
  start + (delivered + 1) × period. It retries on `EINTR`, because the
  scheduler suspends and resumes the timer thread with signals;
- on each wakeup it runs one timer interrupt (`_tx_thread_context_save`,
  `_tx_timer_interrupt`, `_tx_thread_context_restore`) per tick due since
  start, so every owed tick is delivered. There is no burst cap, because a
  cap would bring back the loss (the same call as issue 3010);
- `nice(10)` is gone.

Under `TX_LINUX_NO_IDLE_ENABLE` the scheduler's idle loop posts the timer
semaphore. That used to end the timed wait early and fire a tick ahead of
time. The timer no longer waits on that semaphore after startup, so the post
is harmless. The board does not define that option.

## Measured (2026-11-28)

Harness, the same shape as issue 3010's: a standalone program built from the
pinned kernel sources with this board's `tx_user.h`
(`TX_TIMER_TICKS_PER_SECOND` = 100). One thread runs
`tx_thread_sleep(TX_TIMER_TICKS_PER_SECOND)` 60 times, and the tick count's
elapsed time is compared with host `CLOCK_MONOTONIC`. Host: 32 cores. The
unpatched and patched images ran side by side in each row.

| | load avg | tick time vs host | worst 1 s sleep |
|---|---|---|---|
| unpatched, ambient | ~2.8 | 0.509 % slow | 1005.3 ms |
| unpatched, 40 busy loops | 10 → 32 | 0.744 % slow | 1032.8 ms |
| patched, ambient | ~2.8 | 0.000 % | 1000.0 ms |
| patched, 40 busy loops | 10 → 32 | +0.0006 % | 1046.3 ms |

Patched, the tick is inside the 0.1 % bound with over a hundred times to
spare. The worst single sleep under load is wakeup latency of the sleeping
thread, not tick loss: the next sleep's deadline does not move. A tick at
100 Hz is 10 ms, so over 60 s the measurement resolves about 0.017 %.

No committed regression test, the same as issue 3010. A test that loads the
host would make the result depend on whatever else the runner is doing.

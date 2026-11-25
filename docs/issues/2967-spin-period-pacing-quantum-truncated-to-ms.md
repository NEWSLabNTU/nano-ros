---
id: 2967
title: "RTOS sched-task runners pass a derived spin period truncated to whole milliseconds, and Zephyr's handle park rounds to milliseconds below a finer tick"
status: open
type: bug
severity: medium
area: [executor, boards, zephyr, real-time]
related: [issue-2965, phase-474, phase-521]
found: 2026-11-25
---

Found by issue 2965's sweep. That issue fixed the parks that rounded a
microsecond deadline up to a whole millisecond and so cost a level its rate.
The sites below also convert a microsecond quantity to milliseconds. They are
a different defect and were left out of that fix on purpose.

## 1. The spin period is truncated to whole milliseconds

A sched task's `spin_period_us` is DERIVED from its members' periods
(`nros_orchestration_ir::spin_period`). So a 30 Hz member gives 33,333 us and
a 4 kHz one gives 250 us. Every RTOS runner hands it to the executor in whole
milliseconds, truncated and floored at 1:

- Rust runners:
  - `nros-board-freertos/src/entry.rs` (2 sites)
  - `nros-board-threadx/src/entry.rs` (2)
  - `nros-board-zephyr/src/entry_sched_tasks.rs` (2)
  - `nros-board-nuttx/src/lib.rs` (1)
- C runners, through the millisecond C ABI (`nros_cpp_spin_once`, the level
  spin):
  - `freertos_run_sched_tasks.c` (4)
  - `nuttx_run_sched_tasks.c` (4)
  - `zephyr_run_sched_tasks.c` (2)

This value is the pacing quantum, not a park bound. Every park is still bounded
by the next timer deadline in microseconds, so no timer loses its rate the way
issue 2965's did.

What it does change is the absolute release grid that `release-jitter-runtime`
judges against (phase-474 I11). That grid ticks every 33,000 us for a member
that releases every 33,333 us, and every 1,000 us for one that releases every
250 us. Not measured yet: whether this reports jitter that did not happen.

A fix needs a microsecond spin entry on the C ABI and rebuilt RTOS fixtures, so
it did not belong in a release-blocker PR.

## 2. Zephyr's handle park polls in milliseconds

`nros_platform_wake_park_handles_us` (`nros-platform-zephyr/src/platform.c`)
waits in `zsock_poll`, which takes milliseconds. The remainder is rounded up.
Meanwhile `nros_platform_wake_park_granularity_us` reports the kernel tick.
At `CONFIG_SYS_CLOCK_TICKS_PER_SEC=1000`, which every in-tree conf that sets
the rate uses (47 of them), the two agree.

On an image with a finer tick (Zephyr's tickless default is 10000), the
executor would size a 700 us park and the poll would wait 1 ms. That is issue
2965's mechanism, on a Zephyr owner whose session readiness is a socket (XRCE
over UDP). The fix is either:

- report `max(tick, 1000)` as the granularity while a handle park is possible, or
- wait out the sub-millisecond remainder on the semaphore.

## Sites that stay at milliseconds, and why

- `executor/spin.rs`, the transport drain's `timeout_ms` when nothing parked.
  It is reached only without a park primitive. There the granularity is
  1000 us, so the bound is already a whole millisecond. It feeds the
  transport's own millisecond receive.
- `nros-platform-{mps2-an385,stm32f4}` `sleep_us` via `sleep_ms`. Their clock
  is the millisecond SysTick counter (`clock::clock_ms`); no finer sleep exists.
- `nros-cpp` `run_levels` and `nros_cpp_spin_for` elapsed-time bookkeeping
  against `NROS_ENTRY_SPIN_MS`. It measures a millisecond bound and is not a
  wait.
- Monitor and profile conversions (`monitor.rs`, `profile.rs`,
  `sched_context.rs`). They report or compare, and do not wait.

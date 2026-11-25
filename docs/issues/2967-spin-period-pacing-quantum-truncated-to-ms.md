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

It needs rebuilt RTOS fixtures, so it did not belong in a release-blocker
PR.

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

## Fix (decided 2026-11-25 by the manager; supersedes "a microsecond spin entry")

The first draft of this issue proposed a microsecond spin entry on the C ABI.
Reading the two spin paths showed it is not needed. Nothing is added to the
ABI, and nothing is truncated.

**Where the period goes on each path:**
- **A spawned level** (`LevelCore::spin_level_once`, `nros_cpp_level_spin_once`)
  takes the band path (`band_prelude`), which keeps no release grid. There
  the period is only a park bound, and it wakes an idle level once a period.
  The Linux runner stopped doing that in phase-521 W6 (RFC-0113 D2): it passes
  `entry_spin_budget()`, and the band's deadlines bound the park.
- **The boot level** (`spin_boot_once`) and **a C runner's own executor**
  (`nros_cpp_spin_once`) take the owned path. There `record_release_jitter`
  makes the timeout the release grid's period when no cadence is declared.
  That is the truncated grid this issue describes.
- **An exact-microsecond cadence already exists:** `set_spin_nominal_wait_paced_us`
  (phase-436 A2b; C: `nros_cpp_executor_set_spin_nominal_wait_paced_us`). It
  sets the grid's period, keeps the timeout as a bound on each wait, and
  parks until the earlier of the two.

**So every sched-task runner, on every platform, Linux included:**
1. passes the entry spin budget (`entry_spin_budget()`, or C
   `nros_cpp_entry_spin_budget_ms`) where it passed `spin_period_us / 1000`;
2. declares `spin_period_us` once as the wait-paced cadence of every
   executor it spins on the owned path: the boot level's, and each C
   runner's own executor;
3. on Zephyr, `nros_platform_wake_park_granularity_us` reports
   `max(tick, 1000 us)` whenever a handle park is possible, so the executor
   never sizes a park the millisecond poll cannot keep.

**What changes for Linux's jitter record.** Linux's boot loop passes the
budget, up to a minute, and declares no cadence. So today its release grid's
period is the budget, and its jitter is judged against a grid that is not
its cadence. With step 2 the grid is the boot level's declared
`spin_period_us`, as on every RTOS.

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

## Progress (2026-11-25): the fix, as decided, on a draft PR

- `LevelCore::set_boot_cadence_us` declares the boot level's cadence
  (`set_spin_nominal_wait_paced_us`) on its executor.
- **Rust runners:**
  - FreeRTOS, ThreadX, Zephyr and NuttX pass `entry_spin_budget()` where they
    passed `spin_period_us / 1000`, on every spawned level and boot level;
  - each boot level declares its cadence first;
  - Linux's boot level declares it too, so its grid is the period rather
    than the budget;
  - the NuttX runner takes the budget as an argument, and ThreadX's setup
    banner prints the period in microseconds.
- **C runners** (FreeRTOS, NuttX, Zephyr):
  - every owned-path executor, each per-task executor and the boot one,
    declares `spin_period_us` through
    `nros_cpp_executor_set_spin_nominal_wait_paced_us` and spins with
    `nros_cpp_entry_spin_budget_ms`;
  - level passes spin with the budget of the scheduler's owner;
  - `SPIN_PERIOD_FLOOR_MS` is gone.
- **Zephyr:** `nros_platform_wake_park_granularity_us` reports
  `max(tick, 1000 us)` when the image can park on handles.

Not run here: the Zephyr and NuttX Rust boards and every C runner. They
build only for their targets, so the RTOS fixtures and their cells on 241
are the acceptance.

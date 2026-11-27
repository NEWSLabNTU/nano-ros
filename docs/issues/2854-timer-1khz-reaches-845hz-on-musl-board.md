---
id: 2854
title: "A 1 kHz timer reaches only 845 Hz on an embedded Linux board; the executor keeps phase, so the loss is below it"
status: open
type: bug
severity: low
area: [executor, posix, timing]
related: [phase-589]
found: 2026-11-23
---
Found by a downstream proof of concept on an embedded Linux camera SoC (HiSilicon Hi3519DV500: 2x Cortex-A55, Linux 5.10, musl 1.2.3, BusyBox), built as an out-of-tree hosted board; see phase-589.

## What
Measured on that board with a SCHED_FIFO executor thread: a 100 Hz timer has
a p99 jitter of about +/-80 us, but a 1 kHz timer fires at about 845 Hz.

## What is ruled out
The executor does not re-arm a repeating timer from its fire time. It
accumulates elapsed time and subtracts one period per fire
(`arena.rs`: `elapsed_us += delta_us`, then `elapsed_us -= period_us` under
CatchUp; `skip_overrun` keeps `elapsed_us % period_us`). Callback latency
therefore does not accumulate; the timer stays on its phase grid.

## What is left to check
845 Hz means about 1.18 ms per cycle, so each wait overshoots by about
180 us and fires are lost rather than delayed. Candidates:

- the platform sleep / wait granularity on that kernel: high-resolution timers
  absent or off, so a sub-ms wait rounds up to a jiffy fraction;
- a non-PREEMPT kernel, where wake-up latency at 1 kHz exceeds the margin;
- whether the spin's wait is computed from the next deadline or from a fixed
  poll interval.

Measure the platform's sleep overshoot on that board for 100 us to 1 ms
requests, and read the kernel config (`CONFIG_HIGH_RES_TIMERS`,
`CONFIG_PREEMPT*`, `CONFIG_HZ`).

## Acceptance
Cause measured. If it is the executor's wait computation, fixed; if it is the
kernel, documented with the kernel options a 1 kHz node needs.

## Findings (2026-11-24, nros 175, on an x86_64 glibc host)

**A real defect on musl, fixed:** `nros_platform_wake_park_handles_us`
(`nros-platform-posix/src/platform.c`) used `ppoll` only under glibc or NuttX.
Under musl it fell back to `poll`, whose timeout is whole milliseconds, while
`nros_platform_wake_park_granularity_us` (from `clock_getres`) still told the
executor 1 us. So every sub-millisecond park on a musl board was rounded up to
1 ms. Every Linux libc has `ppoll`, so it is now used on `__linux__`, with the
prototype stated because musl and glibc declare it only under `_GNU_SOURCE`.

**That defect does not explain 845 Hz on its own here.** A 1 kHz timer
(Linux talker leaf, zenoh client to a local `rmw_zenohd`, debug build, 26 s):

| Park path | Rate |
| --- | --- |
| `ppoll` (the fix) | 982.2 Hz |
| `poll` forced (what musl got) | 984.2 Hz |

The forced build imported `poll` alone (`nm -D`). A park at most 1 ms late
loses no period: the default timer policy is `Skip` (`arena.rs`), which drops
a period only when a spin comes a whole period or more late. Both runs lose
about 1.7 % to occasional late spins on this host.

**What is left, and needs the board:** with `Skip`, 845 Hz means about 15 %
of spins came at least a period late. On a kernel without high-resolution
timers (`CONFIG_HIGH_RES_TIMERS=n`), `poll` and `ppoll` round a sleep to a
jiffy (10 ms at `CONFIG_HZ=100`), which drops up to nine periods at a time.
So, on the board:

1. Read `CONFIG_HIGH_RES_TIMERS`, `CONFIG_HZ` and `CONFIG_PREEMPT*`, and
   `clock_getres(CLOCK_MONOTONIC)`. It reports 1 ns only with hrtimers on;
   with them off it reports the jiffy, and the executor then rounds parks to it.
2. Re-measure at the fixed commit, and read the timer's overrun count
   (`Executor::timer_overruns`; `Skip` counts dropped periods) to see whether the
   loss is in dropped periods.
3. Compare with `TimerOverrunPolicy::CatchUp` (`Executor::set_timer_overrun_policy`), which keeps the rate and
   moves the loss into jitter.

## Measurement checklist for the board (2026-11-27)

Studied with issues 2965 and 2967, the other parks that rounded a deadline
coarser than the executor assumed. Those two are fixed and resolved; this one
shares their family, but the cause left after the `ppoll` fix is on the board,
so it waits for whoever has the Hi3519. Everything below is one sitting on
the board, at a build that contains `6f92d262b0` (the `ppoll` fix) or later.

1. **The park the binary uses.** `nm -D <node binary> | grep -wE 'ppoll|poll'`
   must list `ppoll`. A binary that lists only `poll` predates the fix:
   rebuild it before measuring anything else.
2. **The kernel's timer configuration.** `zcat /proc/config.gz | grep -E
   'CONFIG_HIGH_RES_TIMERS|CONFIG_HZ=|CONFIG_HZ_|CONFIG_NO_HZ|CONFIG_PREEMPT'`
   (needs `CONFIG_IKCONFIG_PROC`; otherwise read the vendor kernel's
   `.config`).
3. **The clock the executor sizes parks from.** `grep -m1 resolution
   /proc/timer_list` gives the hrtimer resolution: `1 nsecs` with
   high-resolution timers, the jiffy (for example `10000000 nsecs` at
   `CONFIG_HZ=100`) without them. The executor reads the same answer through
   `clock_getres(CLOCK_MONOTONIC)` and rounds every park to it.
4. **The rate and where the loss goes.** Run the same 1 kHz timer node as
   before, `SCHED_FIFO`, for 30 s. Record fires per second, and at the end
   print `Executor::timer_overruns(<timer handle>)`, which counts the periods
   `Skip` (the default) dropped. 845 Hz with about 4,650 overruns in 30 s
   means late spins drop whole periods; 845 Hz with few overruns means the
   timer was never due that often, which points back at the executor.
5. **The same run under `CatchUp`.**
   `Executor::set_timer_overrun_policy(<timer handle>,
   TimerOverrunPolicy::CatchUp)` before spinning. If the rate comes back to
   about 1,000 Hz with bursty jitter, the loss in step 4 was dropped periods
   from late wake-ups, which is a kernel or scheduling cause, not a timer
   computation one.

For comparison, the x86_64 glibc host (Linux talker leaf, zenoh client to a
local `rmw_zenohd`, debug build, 26 s) measured 982.2 Hz with `ppoll` and
984.2 Hz with `poll` forced, so the musl park defect alone accounts for
neither the board's 845 Hz nor a loss of that size.

If steps 2-3 show no high-resolution timers, the cause is the kernel: a 1 kHz
node needs `CONFIG_HIGH_RES_TIMERS=y`. Write that up as the issue's answer
then, with the board's numbers, and not before.

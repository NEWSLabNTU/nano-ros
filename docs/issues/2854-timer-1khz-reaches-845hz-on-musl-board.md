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

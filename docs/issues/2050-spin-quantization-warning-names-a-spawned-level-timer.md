---
id: 2050
title: "The spin-quantization audit warns about a spawned level's timer against another level's spin period"
status: open
type: bug
area: [executor, logging]
severity: low
found: 2026-10-25
related: [phase-162, phase-516, phase-509]
---

## What

`Executor::audit_spin_quantization` (`packages/core/nros-node/src/executor/spin.rs`)
warns about each timer whose period is not a multiple of the spin period, and
skips a timer whose slot another level holds (`slot_held`). On a C++ Linux
levels image the warning still names a timer that runs on a different,
spawned level, against a spin period that is not that level's.

## Measured

`levels-cpp`'s `linux_s1` image (phase-162), 2026-10-25. Its tier table
gives the level holding `attitude`'s 1 ms timer `spin_period_us = 1000`. The
boot still prints:

```
[WARN] nros: timer period 1000 us is not a multiple of the 10000 us spin period: activations will alternate between 0 us and 10000 us (mean cadence preserved)
```

10000 us is neither level's spin period (1000 and 100000). The timer did not
fire on that schedule: over a pinned 4 s run, `attitude`'s longest gap
between releases was 4.5–5.0 ms (default policy, no `SCHED_FIFO`), not
10 ms. So the warning describes a cadence that did not happen.

Under `SCHED_FIFO` (phase-162 W2, 2026-10-25, levels on their real-time
priorities in a container granted `SYS_NICE`), the same boot still prints the
warning, and `attitude` kept 1 kHz: 59 windows of 1000 releases in 60 s,
worst gap 1.4-3.4 ms, 0-5 gaps over 2 ms in 59,000. A 10 ms burst would put
a gap over 2 ms on most releases. So the warning is wrong and the dispatch is
right.

Not measured: whether the audit runs before the spawned level has claimed the
slot (an ordering race), or reads the executor's default spin period rather
than the level's.

## Why it matters

It is the warning a reader looking at a real-time image would trust, and it
says a 1 kHz loop runs in 10 ms bursts, which it does not.

## Closes when

The audit judges each timer against the spin period of the level that holds
it, and a `linux_s1` boot prints no quantization warning for `attitude`.

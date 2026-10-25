---
id: 2050
title: "The spin-quantization audit warns about a spawned level's timer against another level's spin period"
status: resolved
type: bug
area: [executor, logging]
severity: low
found: 2026-10-25
resolved: 2026-10-25
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

## Resolution

Neither of the issue's two guesses. The audit runs once, in the prelude of
the executor's own worker, and walked every timer in the executor. In a level
runner that worker is the boot level, and `nros_board_native_run_levels_in`
spins it at its spin period clamped to 1–10 ms: `linux_s1`'s boot level is
the 100 ms one, so 10 ms. `attitude` sits on the spawned level, whose band
worker parks until its next timer deadline, so its timer was never paced by
that spin. `slot_held` skipped it only when its callback happened to be in
flight at that instant.

The audit now walks only the slots of its own worker's band
(`Executor::band_slots`, the mask that worker's pass dispatches from), through
`spin_quantized_timers`, which the log call wraps so the choice can be tested.
A spawned level is not audited: with a waiter its park is bounded by its own
next timer, so nothing quantizes it.

Test: `the_spin_quantization_audit_judges_only_its_own_levels_timers` (a
one-level executor still judges every timer; in a level runner the boot level
judges its own 15 ms timer and not the urgent level's 1 ms one). Mutated by
widening the mask to every slot, it fails with `[1000, 15000]`.

Measured after, `levels-cpp` `linux_s1` booted for 3 s against a local
router: no quantization warning.

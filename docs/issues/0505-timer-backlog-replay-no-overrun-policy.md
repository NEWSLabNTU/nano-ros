---
id: 505
title: "Periodic timers replay the whole backlog after a stall — 6 control callbacks 88 us apart — with no overrun policy and no overrun counter"
status: open
type: enhancement
area: embedded
related: [issue-0502]
---

## Problem

When an executor tier is preempted long enough to miss several periods
of a periodic callback, the timer fires every missed activation
back-to-back once the tier runs again. Observed on the FreeRTOS
mps2-an385 lane (QEMU, guest-clock timestamps): after a ~200 ms
preemption of the application tiers, a 10 ms control callback fired 6
times at ~88 us spacing (NEWSLabNTU/nano-ros-rt-eval, run
`results/20260810-044653Z`, guest stamps `t_us=6695984..6696429`).

For a control loop this is the wrong semantics on both ends:

- The replayed activations compute with stale inputs and publish a
  burst of 6 commands inside half a millisecond — a downstream actuator
  sees a command rate 100x the declared one, and `dt`-based math (which
  today assumes the nominal period, see issue 0504) integrates 6 steps
  of error.
- The application cannot even tell it happened: no overrun count, no
  "this activation is late by X" on the callback, nothing for a
  monitor to alert on. The stall is only visible to an external
  observer diffing timestamps.

## Fix direction

Two independent pieces:

1. **Overrun policy per timer** — at minimum `CatchUp` (today's
   behavior, correct for counters/accumulators) and `Skip` (coalesce
   the backlog into one activation aligned to the next period, correct
   for control). Default for real-time-class tiers should arguably be
   `Skip`; either way the choice must be declarable, not baked in.
2. **Overrun observability** — a per-timer missed-activation counter
   exposed where the executor's scheduling monitors can read it. The
   on-target monitors currently watch achieved rate at the publish
   site; a replayed burst *satisfies* a rate check while being exactly
   the failure the check exists for. An overrun counter is the cheap,
   unambiguous signal (and unlike period measurement it does not
   depend on clock resolution, issue 0502).

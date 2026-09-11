---
id: 1321
title: "A ROS-time timer caps the executor's park on WALL microseconds — its
  period is simulated time, and `next_timer_deadline_us` never asks which clock"
status: open
type: bug
area: core, api
related: [phase-425, phase-430, phase-436, issue-1192]
---

## Problem

`Executor::next_timer_deadline_us` (`packages/core/nros-node/src/executor/spin.rs:2838`)
walks every `EntryKind::Timer`, projects its `TimerHeader`, and offers
`period_us - elapsed_us` as a wake bound:

```rust
let remaining = header.period_us.saturating_sub(header.elapsed_us);
```

`TimerHeader` carries `clock_source`
(`packages/core/nros-node/src/executor/arena.rs:265`) and this walk never reads
it. The bound it returns is consumed as WALL MICROSECONDS —
`next_wake_bound_attributed_us` (`spin.rs:2711`) takes the minimum against the
caller's budget and the session's next event, and the winner becomes
`park_achieved_us`, the number handed to the port's park primitive
(`spin.rs:7093`–`7130`).

For a `TimerClockSource::Steady` timer that is correct and is exactly what
phase-436 W1 / issue 1192 landed. For `Ros` and `System` it is a units error.
A `Ros` timer's `elapsed_us` advances only by the `/clock` step
(`arena.rs:2209`–`2231`), so `period_us - elapsed_us` is a quantity of
SIMULATED microseconds with no wall meaning, and the two are related by the
bag's replay rate — which the executor does not know and by design cannot.

## What it costs

Bounded, and it is over-waking rather than a wrong activation: the bound can
only SHORTEN a park, and `timer_try_process` still gates the callback on the
clock, so nothing fires early. What is lost is the park itself.

* **Paused `/clock`.** `elapsed_us` stops advancing, so `remaining` freezes at
  a constant below `period_us` and every `spin_once` parks at most that long,
  forever. A 100 ms ROS timer on a paused bag wakes the image ten times a
  second to discover that simulated time did not move. Before phase-436 the
  park was the caller's budget and this cost nothing; the regression arrived
  with the feature that made parks tight.
* **A slow replay** (`rate < 1`) has the same shape scaled: the bound stays near
  `period_us` of wall while the timer is due far later.
* Both are a power cost on the targets `park_until` was added for — the
  bare-metal `wfi` path named in the comment at `spin.rs:7120`.

## The same omission, one function over

`audit_spin_quantization` (`spin.rs:2868`) also reads `header.period_us`
without `clock_source`, and warns when the period is not a multiple of the
declared spin period. For a ROS-time timer the two quantities are in different
clocks, so the warning's arithmetic ("activations will alternate between X and
Y us") is about nothing. Fix the class, not the site.

## Why the doc said otherwise

phase-430's delta table, row 1, verdicted property 1 (*"the wall timeout handed
to the platform must come from wall timers only"*) **NO LONGER APPLIES** on
2026-09-07, on the measurement that *"the executor derives NO wait from timers
at all"*. That was true of the tree it was measured in. phase-436 W1 made it
false nine hours later: `d1a1c7a32` (PR #691) merged at 2026-09-10T15:47 UTC,
the table's own commit `f447547d1` at 2026-09-10T06:23 UTC. One phase removed
the premise another phase's verdict rested on, and neither noticed. The row is re-verdicted
**NOT MET (property applies again)** in the 2026-09-11 column and cites this
issue.

## Shape of a fix (not done here)

`next_timer_deadline_us` skips a timer whose `clock_source` is not `Steady`, on
the ground that its next activation is a MESSAGE arrival and `/clock` already
wakes `drive_io` like any sample. That restores the pre-436 latency bound — the
spin cadence — for ROS-time timers only, which is what the e2e fixture's 5 ms
`sim-clock-listener` cadence already assumes. `System` wants the opposite
treatment (it IS a wall clock, so its remaining is meaningful, but it can jump),
so the three arms should be decided separately rather than collapsed into
`!= Steady`.

*Acceptance:* a `MockSession` unit test registers one ROS-time timer, holds
`/clock` still, and asserts `last_park()` is `(budget, WakeSourceId::CallerBudget)`
rather than the timer; a wall timer in the same executor still wins the park at
its own period.

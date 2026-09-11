---
id: 1321
title: "A ROS-time timer caps the executor's park on WALL microseconds — its
  period is simulated time, and `next_timer_deadline_us` never asks which clock"
status: resolved
resolved_in: "this branch — `Ros` contributes nothing to a wall park bound; `System` and `Steady` still do"
type: bug
area: core, api
related: [phase-425, phase-430, phase-436, issue-1192, issue-0736, issue-1334]
---

> **Resolved.** A timer may bound the executor's WALL park only if its own
> clock runs in wall microseconds — `TimerClockSource::remaining_is_wall_time`
> (`packages/core/nros-node/src/timer.rs`), one predicate, read by both sites
> named below. **`Steady` yes** (unchanged: phase-436 W1). **`System` yes, and
> decided separately rather than by `!= Steady`** — its clock IS wall time, so
> there is no rate indirection and nothing can hold it still; an NTP step makes
> the bound an estimate, but a bounded one, because the bound is one member of
> a `min` that can only SHORTEN a park and `timer_try_process` re-reads the
> clock before dispatching, so a step costs at most one extra wake and can
> never fire a callback early. **`Ros` no, unconditionally** — its remainder is
> simulated microseconds and its next activation is a `/clock` MESSAGE, which
> wakes `drive_io` like any sample; the park is never unbounded without it,
> because `next_wake_bound_attributed_us` seeds the `min` with the caller's
> budget, and that IS phase-430's property 1.
>
> Two answers this issue offered were declined, with reasons. A bound derived
> from the OBSERVED clock rate: the rate is the bag's, it changes and stops
> without notice, so a bound from past rate is wrong exactly when it matters
> (a pause) and buys nothing, because the wake already arrives as a message. A
> conservative CAP so a stalled `/clock` cannot park forever: unnecessary — the
> caller's budget is already the ceiling, so a cap would be a number nobody
> declared.
>
> The sweep (`grep -rn "period_us\|elapsed_us\|next_timer_deadline\|park_achieved"
> packages/core/nros-node/src`) found a THIRD site, and it is the one that was
> not bounded: issue 0736's budget-skip path in `spin_once` advanced EVERY
> timer's `elapsed_us` by the executor's wall `delta_us`, so a `Ros` timer bound
> to a starved sporadic context accumulated wall time while `/clock` was paused
> and would eventually fire an activation no simulated time paid for. Measured
> by the negative control: **202 359 µs** accumulated over five spins on a
> paused clock — two spurious activations of a 100 ms timer. It now advances on
> the timer's own clock through `arena::timer_advance_without_dispatch`, which
> shares `timer_clock_step` with the dispatcher, so "which clock moves this
> timer" has one answer and not two.
>
> One thing the fix had to measure rather than read, filed separately as
> **[issue 1334](../1334-ros-time-fallback-reads-an-unadvanced-steady-counter.md)**:
> the documented "a `Ros` timer with no `/clock` reads system time" fallback is
> not what `nros_core` does — it reads an in-image steady counter nothing
> advances. That is why the `Ros` answer is unconditional rather than "wall when
> no source is attached": the conditional rule would be wrong today, and a park
> bound that flipped with whether a publisher happened to be running is a
> behaviour no declaration named.

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

## What the fix measured

The acceptance above is met by `a_paused_ros_clock_does_not_bound_the_wall_park`
and `a_wall_timer_still_bounds_the_park_beside_a_paused_ros_timer`
(`packages/core/nros-node/src/executor/tests.rs`). Four of the six new tests are
negative controls — each FAILS on the pre-fix tree, with these numbers:

| Test | Pre-fix | Post-fix |
| --- | --- | --- |
| `a_paused_ros_clock_does_not_bound_the_wall_park` | winner `Timer` | `CallerBudget`, bound `>= 500 000` us |
| `a_paused_ros_clock_does_not_wake_the_image_once_per_timer_period` | parked **100 000** us per spin | 500 000 us, four spins, `last_park() == (500_000, CallerBudget)` |
| `a_budget_skipped_ros_timer_does_not_advance_on_wall_time` | `elapsed_us` = **202 359** | 0 |
| `the_spin_quantization_audit_speaks_only_of_wall_timers` | `Some((20 000, 30 000))` for a `Ros` timer | `None` |

The park test asserts the bound from BELOW (`>= budget`), because an
upper-bound-only assertion is satisfied by a park that never waited — and
shortening is precisely the failure here.

The two that pass BOTH before and after are the regression guards:
`a_system_time_timer_bounds_the_park` (the arm that keeps its bound) and
`a_wall_timer_still_bounds_the_park_beside_a_paused_ros_timer` (phase-436 W1's
own property, which the fix must not take away — it skips one clock source, not
the timer deadline source).

---
id: 2274
title: "tier_wake_fanout's threaded fan-out bound fails under the gate's parallel load and passes solo"
status: resolved
resolved: 2026-11-12
resolved_in: "sched_task_wake_fanout paces arrivals by the wakes being taken, and its probe budget is 1 s"
type: bug
severity: low
area: [testing, executor]
related: [phase-521, phase-543]
found: 2026-11-05
---

**Home: [phase-543](../../roadmap/phase-543-nextest-gates.md).**

## What

`nros-tests::tier_wake_fanout a_non_attributing_backend_wakes_every_high_tier_and_each_wake_is_bounded`
failed once inside `just ci gate` (`check::build` → `required-features-tests`,
the `trigger-test` feature lane) on a docs-only branch, in 0.97 s. The same
binary, run alone six times right after (`cargo nextest run --cargo-profile
nros-relwithdebinfo -p nros-tests --features trigger-test --test
tier_wake_fanout`), passed six times. Two earlier gates on the same host that
day passed it.

The failure text was not kept: the lane's log shows only the FAIL line, so it
is not known which of the test's two assertions fired.

## Why it is load-sensitive

The test measures a deployment-shaped cost on real threads: every tier parks
in its own thread, 2 ms-spaced arrivals land on the low tier, and it asserts

- at least `0.9 × (TIERS - 1)` high-tier wakes per arrival, and
- under 100 µs of thread CPU per spurious wake.

Both depend on the host scheduler. Under `check::build`'s parallel lanes a
parked thread may not run between two arrivals (fewer wakes than arrivals,
so the first assertion fails), and a wake that is switched out mid-pass is
charged more CPU time (the second). Neither is a property of the executor.

## What a fix looks like

Make the measured quantity independent of whether the host schedules the
thread in time, rather than retrying: for example count wakes from the
session's fan-out counter instead of from parks that ended, and measure the
per-wake cost from a pass the test drives itself. Or run this one test in a
nextest group with no parallel neighbours, if the bound is meant to be a
real-time property of an idle host. Say which in the RFC-0113 D2.5 text.

## Resolved (2026-11-12)

The test is `sched_task_wake_fanout` now (renamed by phase-536 W2), and both
of its scheduler-dependent measurements were changed so they measure the
fan-out:

- **The threaded count.** Arrivals were paced at a fixed 2 ms. On a loaded
  host a woken thread could stay off the CPU across two arrivals, so its two
  wakes ended ONE park and the count fell. Now each arrival waits, up to 2 s,
  until every high sched task has ended a park before the next arrival is
  fired. A fan-out that wakes every member counts exactly one wake per member
  per arrival, however slowly the host runs them. A member the fan-out misses
  is still a missing wake: the wait gives up on it, and the run prints how
  many arrivals went unanswered.
- **The single-thread probe** (`woken == SCHED_TASKS - 1`, the test's first
  assertion). "Woken" meant "returned within a quarter of a 200 ms spin", a
  50 ms margin that a preempted thread on a loaded host can overrun. The
  budget is now 1 s and the threshold half of it. An unwoken sched task still
  parks the whole budget, so the two cannot meet.

The failure text was never kept, so which assertion fired on 2026-11-05 is
not known. Both are covered. The CPU bound (100 µs per spurious wake) is
unchanged: thread CPU time only accrues while the thread runs.

**Measured** on newslab-118 (48 cores), each run the threaded test alone in
its own process, 400 runs at 48 in parallel beside 48 busy loops:

| | failures | wakes per arrival | CPU per wake |
| --- | --- | --- | --- |
| before (main, fixed 2 ms pacing) | 0 / 400 | 3.00 in 374, 2.96–2.99 in 26 | not compared |
| after | 0 / 400 | 3.00 in all 400 | 2.8–5.1 µs (median 3.2) |

This load did not take the old count below its 2.7 threshold. It did show
the merging the issue describes (26 runs below 3.00), which no longer
happens. RFC-0113 D2.5's measurement paragraph says how the count is taken.


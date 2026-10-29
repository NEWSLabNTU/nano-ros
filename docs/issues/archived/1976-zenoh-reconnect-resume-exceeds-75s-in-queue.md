---
id: 1976
title: "`declares_keep_running_across_a_reconnect` ran past its 75 s resume wait in a merge-queue `test-unit` and ejected a docs-only PR"
status: resolved
type: bug
area: [runtime, zenoh, testing]
severity: medium
found: 2026-10-18
resolved: 2026-10-29
resolved_in: "feat/508-close (phase-513 R5)"
related: [1802, 1751, 1749, phase-513]
---

**Home: [phase-513](../../roadmap/phase-513-nightly-reds-with-no-home.md)** — R5.

## Measured

- Run: merge_group gate 39171119513 (2026-10-18 22:11 UTC).
- Job: 117804199279, `check / test-unit (merge group + nightly/manual)`.
- Result: `2113 tests run: 2112 passed, 1 failed, 6 skipped`. The one failure:

```
SLOW [> 45.000s] (─────────) nros-rmw-zenoh::zenoh_integration declares_keep_running_across_a_reconnect
FAIL [  75.479s] (2113/2113) nros-rmw-zenoh::zenoh_integration declares_keep_running_across_a_reconnect
```

- Victim: #2398 was ejected with `failed_checks`. It adds a single issue
  document, so it cannot cause a Rust test failure. It was re-armed.

## Reading

75.48 s is the resume wait, which issue 1802 raised from 50 s to 75 s. That
fix also made a timeout end as a clean panic instead of a SIGSEGV through a
dead stack frame. This is the clean shape: the test thread panicked when the
wait ran out, and nothing crashed. The log shows no signal.

1802 measured a normal resume at 39–40 s and over 50 s in 1 run of 96 under
load. A merge-group `test-unit` has 2113 tests in flight, and here the
resume took longer than 75 s.

## What it is NOT

- Not 1802's crash. The run ended at the timeout with a test failure, not a
  signal.
- Not the PR. #2398 is a single new issue file.
- The cause is not yet known. The step log has no captured stderr, so it does
  not show whether the reconnect never completed or completed late.

## What would close it

Two outcomes are defensible.

- **Load:** the resume completes, only late. Then the wait or the nextest
  override (3 periods, 135 s) is sized for a quiet host, not for a queue
  batch. Measure the resume time under queue-like load before raising either
  value.
- **Hang:** some runs never resume. Then a longer timeout only hides the
  defect, and the reconnect path needs the fix.

To tell them apart, the test should log the time it actually took to resume
when it passes, so a passing run carries the number.

## Instrumentation (phase-513 R5)

The test now prints both times on a pass:

```
issue 1749: 607 declare rounds ok, 31441 failed across the outage; issue 1976: first declare 39.851851392s after the restart, resumed (50 rounds) 39.851851632s after it
```

That run was solo on an aarch64 bench, the first after the change. A timeout
now ends with `first at None (never resumed)` or `first at Some(..) (late)`. A
mutation run with the limit cut to 5 s printed the first form. The nextest
override shows a passing run's output and keeps it in junit. The shim's
per-declare `printf` lines, about 63,000 per run, go to a file during the
churn. Only their count and the last six reach the log.

Still open: the distribution from a merge-queue batch. Read it from a
`test-unit` job's log or junit, then pick the "load" or the "hang" fix above.

## The merge-queue distribution (2026-10-29)

Read from 100 merge-queue `test-unit` jobs, 2026-10-26 19:40 UTC to
2026-10-29 00:36 UTC: every `check / test-unit` job of the last 200
`merge_group` runs that ran that step. A passing run's own output reaches
junit only, not the step log, so the number taken is the test's nextest
duration. It tracks the resume time closely: the bench run above resumed at
39.85 s, and the shortest queue duration is 40.1 s.

| runs | passed | failed | min | p50 | p90 | p99 | max |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 100 | 100 | 0 | 40.1 s | 40.2 s | 40.3 s | 40.6 s | 40.8 s |

None took longer than 41 s, against a 75 s wait. Under a full queue batch
the resume is as fast as it was solo, so neither fix above is needed: the
wait is not undersized for load, and no run hung.

Every one of these runs postdates `48addebabc` (2026-10-26 13:44 UTC), which
moved zenoh-pico to 1.10.1 and put `zpico.c` on its executor (phase-508). That
replaced the lease task and the reconnect path the 2026-10-18 overrun ran
through, which was on 1.8.0. So the overrun is not explained; it is not
reproduced on the code that ships now. Resolved on that basis.

**Reopen if** a `test-unit` run fails this test again, or its duration
passes 50 s. The duration is the `PASS [ N s]` line for
`declares_keep_running_across_a_reconnect` in the job log.

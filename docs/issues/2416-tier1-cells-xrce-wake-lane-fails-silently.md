---
id: 2416
title: "Tier-2 run's `tier 1 (cells)` job: the four `xrce-wake-test` tests all fail at 14.5 s with no panic text in the log"
status: open
type: bug
severity: medium
area: [rmw-xrce, ci, tests]
related: [2142, 1734, 2108, 2109, 2144, 2420]
found: 2026-11-10
---

## Measured

`run-matrix` dispatch 42744194409 on `main` `5f6b2cd21a`, job 129622486593
"tier 1 (cells)". In the required-features lane
(`just _nextest-platform 'xrce_guard_wake xrce_concurrent_entities
xrce_entity_wake xrce_deadline' '' '--features xrce-wake-test'`), all four
tests fail together:

```
FAIL [  14.496s] nros-tests::xrce_deadline xrce_subscription_reports_requested_deadline_misses
FAIL [  14.496s] nros-tests::xrce_guard_wake guard_trigger_wakes_an_xrce_executor_parked_on_its_socket
FAIL [  14.496s] nros-tests::xrce_concurrent_entities concurrent_creates_on_one_xrce_session_succeed_on_distinct_slots
FAIL [  14.497s] nros-tests::xrce_entity_wake each_xrce_arrival_marks_its_own_subscription
Real failures: 4 / 4 total failures
```

Every other lane in the job counted 0 real failures. Its 271 other "FAIL"s
are 0.003 s `skip!` panics, which the junit rewrite drops.

## What is known

- The log has no panic text for these four, so the cause is not recorded.
  This is the `--failure-output` gap issue 2142 already noted.
- All four fail at the same 14.5 s and all start with `require_xrce_agent()`
  and `XrceAgent::start_unique()`. That points to a shared precondition (the
  Agent not starting, or `Executor::open` over XRCE timing out), not four
  separate defects. This is an inference, not a measurement.
- This is the first tier-2 run whose tier-1 cells job reached its tests. The
  earlier runs stopped at fixture builds: threadx_linux rc=2 on job
  129574166046, Zephyr rc=2 on job 129398652381. It may therefore be an old
  failure that was hidden, not a new regression.

## What it is NOT

- Not issue 2142 returning, as far as anyone can tell. 2142 failed in 0.17 s
  on a struct-mirror mismatch. These take 14.5 s and fail together.
- Not a skip miscounted as a failure: the tolerant run's own count says 4 are
  real.

## What would close it

First, record the panic: rerun the lane with `--failure-output final`, or
read the job's uploaded nextest junit if it holds the output. Then fix the
cause, or name it here. Separately, make the tier-1 cells lane print failure
output, so the next red carries its own diagnosis.

## Findings (2026-11-10)

**The silence, fixed for every lane.** Every lane that goes through
`_nextest-tolerant` passes `--failure-output never`, so that the hundreds of
`lane_skip!` panics a lane deselects stay out of its log. That also discarded
each real failure's panic text. The junit keeps it
(`store-failure-output = true`), but one job runs several lanes and each
resets the junit, so the uploaded `tier1-junit` holds only the last lane's.
For run 42744194409 that was not the xrce-wake lane, so its four panics are
gone for good.

`scripts/test/name-real-failures.py`, which both failure paths (`test-all` and
`_nextest-tolerant`) already call to name the real failures, now prints each
real failure's recorded output after the names: the `<failure>` message, then
the tail of stderr and stdout. It prints no output for a skip, so a lane's log
does not grow by its deselections. `--names-only` keeps the old behaviour. The
`skip-marker-matching` gate self-tests both directions on a junit holding one
real failure and one skip.

On run 42767563994's junit the change prints the `rmw_coordinate_truth` panic,
which its log did not carry. That failure is a separate intermittent defect,
filed as issue 2420.

**The four xrce-wake reds are not reproduced.**

- They passed in the next tier-2 run on main (42767563994, 0.2 to 2.8 s each)
  and locally (4/4, under 3 s).
- All four ended at the same 14.496 s, to the millisecond, in four separate
  test binaries. That points to one shared external event (the Agent, the
  host, a port), not four test defects.
- With the fix above, a recurrence prints its own cause.

**What would close it:** the next red of this lane is read for its printed
cause, or several more tier-2 runs show the lane green.

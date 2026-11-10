---
id: 2416
title: "Tier-2 run's `tier 1 (cells)` job: the four `xrce-wake-test` tests all fail at 14.5 s with no panic text in the log"
status: open
type: bug
severity: medium
area: [rmw-xrce, ci, tests]
related: [2142, 1734, 2108, 2109, 2144]
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

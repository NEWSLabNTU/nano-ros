---
id: 1997
title: "entity_wake's burst test fails its own 'the ring really filled' control under a loaded `check::build`, and passes alone"
status: resolved
type: bug
area: [testing, rmw]
severity: low
found: 2026-10-21
resolved: 2026-10-24
related: [1979, 1980]
---

## Measured

- `just ci gate`, step 7 `check::build` (26 gates in parallel), on branch
  `refactor/359-w10b-env-getenv` (phase-359 W10 stage B), 2026-10-21 03:41 UTC.
- The gate `required-features-tests` failed on one test:

  ```
  FAIL nros-tests::entity_wake
       a_burst_into_a_slow_subscription_ends_on_its_newest_sample::case_2_one_sample_per_dispatch
  panicked at packages/testing/nros-tests/tests/entity_wake.rs:631:5:
  OneSamplePerDispatch: the ring never filled, so the burst did not test eviction
  ```

- `just check required-features-tests` alone, three times right after on the
  same tree and build: 31/31, 31/31, 31/31.
- The diff under test touched nothing on this path (the process-environment
  reader and feature lists); the zenoh subscriber ring and the test are as on
  main.

## What it is

The test's first assertion (the newest sample was dispatched) passed. The
failure is the CONTROL added with issue 1979's fix:
`nros_rmw_zenoh::ring_full_drops_total()` must have moved, i.e. the publisher
outran the slow subscription long enough to fill the ring. Under a loaded host
the publisher thread is the one that gets descheduled, the subscriber keeps
up, nothing is evicted, and the control (correctly) refuses to call that a
test of eviction.

So this is a precondition the test cannot guarantee by timing, not a product
defect. It is the sibling of issue 1980 (the race test in the same file blames
the executor for its own failed publish), and the same file's two
load-sensitive reds have now each been mistaken for a regression in an
unrelated change.

## Fix direction

Make the burst fill the ring by construction rather than by speed: hold the
subscription's dispatch (a gate the callback waits on) until the publisher has
sent `ring depth + N` samples, then release it. The control then measures the
eviction path every time, on any load.

## Re-measured 2026-10-23: it fails ALONE too

The title's "passes alone" no longer holds on main. Run solo, with the lane's
own features (`--features trigger-test,component-runtime-test,phase216-substrate,signal-fd-wake-test`),
`-E 'test(a_burst_into_a_slow_subscription_ends_on_its_newest_sample)'`, 10
runs each, nothing else building:

| tree | failed runs |
| --- | --- |
| main at 686a23ddc6, plus PR #2536 (guard + Zephyr-setup fixes, no runtime change) | 4 / 10 |
| the same, plus the issue-1825 CLI change (rosidl-codegen only) | 5 / 10 |

Both cases fail, not only `case_2`:
`case_1_drains_to_empty` panicked at `entity_wake.rs:667` with "DrainsToEmpty:
the ring never filled, so the burst did not test eviction". In `just ci gate`
it stops `check::build` (feature lane `nros-tests … --test entity_wake`) on
roughly every other run, so it is no longer a load-only flake. Main gained
`40c179f39f` (issue 2001: a RELIABLE zenoh-pico publisher waits for the TX
mutex) the same day, which slows exactly the publisher this control depends
on outrunning the subscriber. That is a candidate, not a finding: the
fix direction above (fill the ring by construction) is unaffected either way.

## Fixed (2026-10-24)

The ring now fills by construction. After the first sample is dispatched
(discovery settled), samples 2..=25 are published while the executor is NOT
spinning; on POSIX zenoh-pico's background read task lands every arrival in
the subscription's ring (depth 4), which nothing drains. The test waits —
sleep-polling, no spin, bounded by `ARRIVAL_BUDGET` — until
`ring_full_drops_total()` has moved and then until it stops moving, so the
ring holds its final contents before the executor runs. Only then does it
spin and assert the newest sample was dispatched. "The ring filled" is now an
asserted precondition with its own message, not a race the publisher had to
win. The publisher thread, `CALLBACK_COST` and `PUBLISH_INTERVAL` are gone;
the read-task assumption was checked by measurement (the precondition passes
every run below). A callback gate, the alternative construction, was not
needed.

Measured on main at `a9f280e4fe`, solo, the lane's features, `-E
'test(a_burst_into_a_slow_subscription_ends_on_its_newest_sample)'`;
"under load" = one busy-loop per core (`nproc`) for the whole series:

| tree | solo | under load |
| --- | --- | --- |
| before (main) | 0 / 10 | 5 / 10 (both cases, `the ring never filled`) |
| after | 0 / 10 | 0 / 10, then 0 / 30 |

It still discriminates: with `_zpico_ring_reserve` mutated to drop the
incoming sample on a full ring (the pre-1979 keep-oldest behaviour), both
cases fail 3 / 3 on the newest-sample assertion (latest stops below 25),
while the fill precondition passes.

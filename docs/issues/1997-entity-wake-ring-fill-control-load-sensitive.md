---
id: 1997
title: "entity_wake's burst test fails its own 'the ring really filled' control under a loaded `check::build`, and passes alone"
status: open
type: bug
area: [testing, rmw]
severity: low
found: 2026-10-21
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

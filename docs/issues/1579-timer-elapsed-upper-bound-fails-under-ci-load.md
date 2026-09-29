---
id: 1579
title: "`timer_readiness_and_elapsed_agree_with_the_dispatcher` bounds a REAL
  sleep with a 10 ms allowance, so a loaded CI runner fails it — four sites,
  one helper"
status: open
type: bug
area: testing, nros-node, ci
severity: medium
found: 2026-09-29
related: [1353, 1139, 0355]
---

## What happened

`host-tests` (tier 1) run [36539890676], job 109312526836, step `just ci tier1`,
gate `node-std-tests`:

```
executor::tests::timer_readiness_and_elapsed_agree_with_the_dispatcher --- FAILED
thread '...' panicked at packages/core/nros-node/src/executor/tests.rs:4754:5:
one 40ms step: 84674us
```

529 of 530 tests passed; this is the one.

## Why

`elapse_then_spin_once` is not an injection — it sleeps for real:

```rust
fn elapse_then_spin_once(executor: &mut Executor, ms: u64) -> SpinOnceResult {
    super::spin::platform_sleep(core::time::Duration::from_millis(ms));
    executor.spin_once(core::time::Duration::from_millis(0))
}
```

So the elapsed reading is `requested sleep + whatever the OS actually slept +
the real cost of the spin`, and the assertion bounds all three at once:

```rust
assert!((40_000..50_000).contains(&elapsed), "one 40ms step: {elapsed}us");
```

10 ms of allowance over a 40 ms request. The measurement was 84,674 µs — the
step took 2.1× its nominal time on a runner building fixtures with
`NROS_CI_BUILD_JOBS: 4`. Nothing about the executor's accounting is wrong in
that number; the timer did advance, monotonically, by what the clock said.

The test's own comment already states which half carries the meaning:

> Bounds, not equalities, on the elapsed readings: `spin_once` credits the REAL
> wall-clock cost of the spin on top of the injected delta … **The lower bound
> is the assertion** — an upper-bound-only check passes a counter that never
> advanced.

The upper bound is the half that is not the assertion, and it is the half that
failed.

## The class, not the site

Four range assertions in that file bound a real sleep the same way, three of
them inside this one test:

| line | bound | allowance over the sleep |
| --- | --- | --- |
| 4755 | `(40_000..50_000)` | 10 ms |
| 4764 | `(80_000..95_000)` | 15 ms |
| 4776 | `(20_000..40_000)` | 20 ms |
| 4918 | `(40_000..50_000)` (`exchanging_a_timers_period_…`) | 10 ms |

Fixing 4755 alone leaves the other three armed at the same load.

## What this is NOT

- **Not issue 1353.** The same job carries `Free space left: 4 MB`, and 1353 is
  a live arm on this lane — but 1353's shape is a build truncated mid-compile
  or a runner that dies with an annotation and no failing step. Here the gate
  ran to completion, reported 529/530, and named an assertion. The sibling
  host-tests failure three hours earlier (job 109258463937) IS 1353:
  `workspace-features` cut off mid-`cargo clippy` with `Free space left: 0 MB`.
- **Not a defect in the executor's timer accounting.** The lower bound — the
  part the comment calls the assertion — held.
- **Not new code.** The test predates this run; what changed is how loaded the
  shared runner is.

## What would close it

The upper bound should express something the test can actually know. Either:

1. derive the allowance from a measured spin ceiling rather than a literal, so
   the bound scales with the host it runs on; or
2. drop the upper bound where it says nothing and keep the lower bound plus a
   separate, explicit assertion for whatever the upper bound was protecting
   (that the spin's own cost did not dominate the reading) — stated once, in a
   helper, for all four sites.

Not "widen the literal": 84,674 µs on one run is a sample of a distribution
nobody has measured, so any new literal is the same bet at a longer odds.

Acceptance: `node-std-tests` green across three consecutive scheduled
`host-tests` runs, with the chosen bound's basis written down beside it.

[36539890676]: https://github.com/NEWSLabNTU/nano-ros/actions/runs/36539890676

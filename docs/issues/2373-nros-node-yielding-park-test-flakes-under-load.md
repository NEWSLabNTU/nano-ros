---
id: 2373
title: "nros-node's a_yielding_park_with_a_pending_wake_keeps_the_pass fails under the build gates' load and passes solo"
status: open
type: test
severity: medium
area: [testing, executor, gates]
related: [phase-543, 2274, 2352]
found: 2026-11-09
---

**Home: [phase-543](../roadmap/phase-543-nextest-gates.md)**. It is a test whose verdict depends on how loaded the host is, the class that phase's resource groups bound. Issue [2274](2274-tier-wake-fanout-load-sensitive.md) is the same class in a sibling executor test.

## What

`just ci gate` step 7 (`check::build`) went red on the unit
`build::check::node-std-tests`. That unit runs `cargo test -p nros-node --lib
--features alloc,log,sim-time,param-services,lifecycle-services`, and one
test of the 746 failed:

```
thread 'executor::tests::a_yielding_park_with_a_pending_wake_keeps_the_pass'
  panicked at packages/core/nros-node/src/executor/tests.rs:3013:5:
assertion `left == right` failed
  left: 0
 right: 1
```

Line 3013 is the first assertion, `assert_eq!(CALLS.load(..), 1)`. The
installed park primitive was never called: `spin_once(50 ms)` returned
before it reached the park, so the test never got to the property it is
named for (a yield with a wake pending keeps the pass).

## Measured

- The red came on the `phase530-w3-pr` branch (PR #3346), on `arm-a100`
  (aarch64), at a host load average of about 100. The build units ran beside
  it in the same nextest run, and so did several other agents' builds.
- That branch changes nothing under `packages/core`: `git diff --stat
  origin/main HEAD -- packages/core` is empty.
- Solo, with the gate's own features, it passed 3 of 3:
  `cargo test -p nros-node --lib --features alloc,log,sim-time,param-services,lifecycle-services a_yielding_park`.

## What it is not

- It is not a regression in the change under test. The branch has no diff in
  `nros-node`, and the same test passes on the same tree when the host is
  quiet.
- It is not a shared-static race between tests: `CALLS` and `TICKS` are
  `static`s local to the test function.

## What would close it

One of these two:

- **A bound that holds under load.** The test asks for one park inside a
  50 ms `spin_once` budget, on `executor_with_clock`, whose `test_clock_us`
  is wall-clock (`Instant::elapsed` since a shared epoch). A descheduled
  thread spends the budget before the first park. Issue 1105 already gives
  a test its own controllable clock (`private_test_clock!` with
  `executor_with_clock_fn`): hold "now" still across the `spin_once`, so
  the budget cannot run out before the park is reached.
- **Serialise it.** Put the test in a nextest test-group, or run it in the
  serial set, so the build units cannot starve it. This is phase-543's
  mechanism, and it is the fix if the bound cannot be made load-independent.

Acceptance: the test passes in a full `just ci gate` run at a load average
of the order measured here, or it is in a group that keeps it off loaded
slots, with the group named in this issue.

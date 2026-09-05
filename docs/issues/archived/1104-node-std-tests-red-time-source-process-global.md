---
id: 1104
title: "`node-std-tests` is red on main: two tests share `time_source`'s process-global and neither can see the other coming"
status: resolved
type: bug
area: testing, core
severity: high
related: [phase-425, 1105, 1098]
found: 2026-09-05
---

# Two tests that pass alone cannot pass together

## Symptom

`just check node-std-tests` fails on main, deterministically — 3 runs of 3:

```
executor::tests::use_sim_time_attaches_and_detaches_the_clock_source --- FAILED
  panicked at src/executor/tests.rs:1860:
  installed and not armed would drop every sample
test result: FAILED. 344 passed; 1 failed
```

The test passes SOLO:

```
cargo test -p nros-node --lib --features std,sim-time,param-services -- --exact \
  executor::tests::use_sim_time_attaches_and_detaches_the_clock_source   # ok
```

So the assertion is not wrong about its own subject.

## Cause

`time_source`'s armed flag is a process-global `AtomicBool`
(`SIM_TIME_ACTIVE`, `time_source.rs:46`) while an `Executor` is per-test, and
`reconcile_ros_time_source` writes it from `spin_once`
(`spin.rs:6123` → `spin.rs:8799`).

The clobbering neighbour is **`ros_time_timer_follows_the_simulated_clock`**,
the one other test that drives these globals. It sets the ROS-time override,
spins executors, and clears the override on its way out; run concurrently with
`use_sim_time_…`, its spins move `SIM_TIME_ACTIVE` out from under the assertion
at line 1861.

That test's own doc comment already reasoned about this hazard and solved half
of it:

> One test rather than four, deliberately: the ROS-time override is
> process-global … so four tests would race each other inside the one test binary.

Merging four into one handles those four. It does nothing about a FIFTH test
landing beside it, which is what `use_sim_time_…` became.

**What is NOT the cause**, recorded because the first draft of this issue said it
was: it is not every spinning test. A default executor holds
`sim_time_requested == false`, and once the global has settled to `false` the
reconcile's first branch returns before writing. Only a test that moves the flag
off its resting value can disturb a neighbour, and only these two do. The
measurement is below.

## Fix

`SimTimeGuard` in `executor/tests.rs` — a mutex both tests take, restoring on
drop the value **observed on entry** rather than a guessed default:

```rust
struct SimTimeGuard { was_active: bool, _lock: std::sync::MutexGuard<'static, ()> }
```

It replaces the hand-written cleanup that ended `use_sim_time_…`:

```rust
// Leave the process-global as the rest of the suite expects it.
crate::time_source::set_active(true);
```

`true` was right only by coincidence — it is the static default, so the line said
nothing if that default ever moved, and it could not restore anything for the
test that runs *before* it.

A poisoned lock is taken anyway (`unwrap_or_else(|e| e.into_inner())`): poisoning
means a sibling panicked while holding it, which is a failure already being
reported somewhere else, and failing here too would bury it.

### Measured, both directions

* **Fix in place:** 345 passed, 0 failed — 5 runs of 5.
* **Mutation, guard removed from the NEIGHBOUR only:** the original failure
  returns, 3 of 3. So the fix is load-bearing, and it is the neighbour's
  participation that matters — not the subject test's.
* **A wider product guard was tried and REJECTED.** Making
  `reconcile_ros_time_source` skip executors that never declared `use_sim_time`
  also made the suite green, but with it mutated off the lock alone was still
  green 6 of 6 — so the extra change was not what fixed anything, and it is not
  in this fix. (It remains a real behaviour question — an executor that never
  heard of `use_sim_time` still disarms an explicitly installed source on its
  first spin, contradicting `SIM_TIME_ACTIVE`'s own doc. That deserves its own
  issue against phase-425, on its own evidence.)

## Residual, deliberately not closed here

The lock orders the two tests that MEAN to touch the flag. It does not make the
flag per-executor, so a future test that moves it off its resting value and does
not take the guard reopens this. The structural fix is phase-425's to make:
carry the armed state on the executor, so the subscription callback reads it
from the entity it belongs to. Nothing in `time_source`'s API warns a new caller
today.

## Why it landed and stayed

`node-std-tests` runs in `check-build`, which per CLAUDE.md is
`schedule` / `workflow_dispatch` only — **no pull-request or merge-queue event
runs it.** The lane exists because these features are non-default (`sim-time` is
off under `--workspace`, so its tests are "in no lane at all", as the recipe's
own comment says), and then it was put in a lane no merge gate reaches. Same
shape as issues 0319 and 1025, and as 1098 the same day.

It IS in `just ci gate` locally, which is how this was found — during a
`just ci matrix` run for unrelated work.

## Not covered

* `--test-threads=1` still fails, on a DIFFERENT test and for an unrelated
  reason: **issue 1105**, a window assertion measured off a free-running process
  clock. Not this defect; it merely surfaced when the threading mode changed.
* Whether other process-globals in `nros-node` couple tests the same way.
* Whether the nightly has been red on this lane and for how long.

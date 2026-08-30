---
id: 577
title: "Seven `std`-gated `nros-node` tests exist in no lane, and one of them had never passed"
status: resolved
type: bug
area: testing, core
related: [issue-0319, issue-0196, issue-0514, issue-0270]
---

## Symptom

`cargo test -p nros-node --lib` fails:

```
---- executor::tests::violations_beyond_the_ring_are_counted stdout ----
panicked at packages/core/nros-node/src/executor/tests.rs:3863:14:
called `Result::unwrap()` on an `Err` value: ExecutorFull
```

while a full `just ci` is green.

## Two defects, and the second is why the first survived

**1. The test asked for storage that cannot exist.** It registered
`MAX_VIOLATIONS + 4` = 12 timers to overflow the violation ring:

```rust
for _ in 0..(super::monitor::MAX_VIOLATIONS + 4) {
    executor.register_timer(TimerDuration::from_millis(10), || {}).unwrap();
}
```

`MAX_CBS` — the callback table — has defaulted to **4** since `96a48116e`
(2026-03-06). So `register_timer` returns `ExecutorFull` on the fifth call and
the `unwrap()` panics. The test landed 2026-08-11 in `95c795b94` (issue 0514)
and **has never passed on any tree**.

Overflowing the ring never needed one timer per violation: a single overrunning
timer produces a fresh violation on every stalled spin, and the test drains
nothing. One timer stalled `MAX_VIOLATIONS + 4` times overflows a ring of
`MAX_VIOLATIONS` — and, unlike the original, does not depend on `MAX_CBS`, which
is a build-time knob any consumer may set. Measured after the fix:
`dropped=4`, i.e. 12 violations into an 8-slot ring.

**2. No lane ran it.** `just test-all` runs `cargo nextest --workspace`, which
builds each crate with the features the WORKSPACE resolves — and `nros-node`'s
`std` is not among them, because a dependent takes it `default-features = false`
(the issue-0270 carve-out). So all seven `#[cfg(feature = "std")]` tests in
`executor/tests.rs` are compiled OUT of the sweep.

Established, not inferred — the same filter against both builds:

```console
$ cargo nextest list --workspace -E 'test(violations_beyond) or test(a_violation_is_logged)'
                                    # (nothing)
$ cargo nextest list -p nros-node -E 'test(violations_beyond) or test(a_violation_is_logged)'
nros-node executor::tests::a_violation_is_logged_and_still_drainable
nros-node executor::tests::violations_beyond_the_ring_are_counted
```

and corroborated against a real tier-1 sweep: 152 `nros-node` cases ran, none of
them these.

This is issue 0319's shape (a suite with no lane, red on main for two days) and
the issue-0196 rule generally — a gate covering less than the rule it enforces.

## Fix

* The test uses one timer and repeated stalls, so it is `MAX_CBS`-independent.
* New `just check node-std-tests` (`cargo test -p nros-node --lib --features
  std`), wired into `check-build` beside `check-cli-tests`, which it mirrors:
  both exist because a `--workspace` run does not reach that code.

262 tests pass under the new lane.

## Scope checked

`nros-node` is the outlier, not a general pattern. The other `std`-gated tests
in this tree live in `packages/api/nros`, and that crate gains tests under
`--workspace` rather than losing them (36 vs 34 for the same filter) — workspace
unification turns its `std` ON. Only `nros-node` has a dependent disabling its
defaults.

## Not done

No general gate proving "every test that exists in a per-package build also
exists in the workspace build". That would catch this class rather than this
instance, but it needs a listing diff across every package and is a lane of its
own. Recorded here as the obvious next step if this recurs.

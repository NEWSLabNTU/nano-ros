---
id: 761
title: "`qos_override_e2e::a_ros2_peer_sees_the_overridden_publisher_profile`
  flakes in a full sweep — ros2 daemon never discovers `/qos_chatter` under
  load; passes solo"
status: open
type: bug
area: testing, interop
related: [issue-0268, issue-0157]
---

## Problem

In the 2026-08-22 tier-2 sweep on newslab-241 (1658 tests, full matrix
lane running concurrently), the test failed with:

```
thread 'a_ros2_peer_sees_the_overridden_publisher_profile' panicked at
packages/testing/nros-tests/tests/qos_override_e2e.rs:166:9:
ros2 did not discover the nros publisher on /qos_chatter:
Unknown topic '/qos_chatter'
```

An immediate solo rerun on the same checkout and fixtures passed in
5.08 s (1/1). One of exactly two real failures in that sweep after the
skip rewrite (the other, the phase-372 matrix orphan, was
deterministic and is already fixed by `2cd7f3785`).

## Why this is the in-sweep-discovery class, not a code bug

The failing assertion is "the ROS 2 side's `ros2 topic info` sees the
nros publisher within the wait budget". Under sweep load the ros2
daemon + discovery exchange competes with ~30 concurrent e2e
processes; the same shape is on record for the QEMU lanes (287-W7: six
nuttx lanes failed 3/3 in-sweep, passed solo) and the repo rule is
"retest a QEMU/e2e red SOLO before filing it as a regression" — which
this red passes.

Filed anyway because the flake COSTS a verdict: every sweep that hits
it needs a human to re-run and re-judge, and the failure text
("Unknown topic") reads like a discovery regression, not like load.

## Possible angles

- Scale the discovery wait with observed load (the fixture harness
  already seeds domains per test; the wait budget is the fixed part).
- Or serialize the tests that talk to the ros2 daemon (a shared
  external singleton — unlike router-per-test zenoh lanes, daemon
  state is cross-test).
- Or have the failure text distinguish "peer never appeared" from
  "peer appeared after deadline" — the latter is diagnostic for load,
  and today both print the same `Unknown topic` line.

## Evidence

- Sweep log: `tmp/eb-t2d.log` (2026-08-22, newslab-241), test 1655/1658.
- Solo pass: `cargo nextest run -E 'test(a_ros2_peer_sees_the_overridden_publisher_profile)'` — PASS 5.083 s.

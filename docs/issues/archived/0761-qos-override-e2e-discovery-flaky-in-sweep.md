---
id: 761
title: "`qos_override_e2e::a_ros2_peer_sees_the_overridden_publisher_profile`
  flakes in a full sweep — ros2 daemon never discovers `/qos_chatter` under
  load; passes solo"
status: resolved
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

## Fixed 2026-08-23 — the remedy already existed one file over

**A single-shot query is a race by construction**, and this test was the site a
previous sweep of that same class missed. Issue 0705 had already replaced this
exact shape in `workspace_features_e2e` — fixed sleep, then one `ros2 topic
info` — with a poll loop. `qos_override_e2e` kept `sleep(3s)` + one shot, so it
kept the flake.

So the fix is not a longer sleep. The wait is now a DEADLINE with a poll
(`await_topic_endpoints`, 20 s, 500 ms interval), returning as soon as the
endpoints appear. Solo runtime went DOWN, 6.58 s to 3.25 s: the old code always
paid its 3 s whether or not discovery was done, and then asked once anyway.

### Two more defects at the same site, found by the sweep the fix required

Neither is what was reported, and both are the same class as the flake:

* **Issue 0690's selection bug was still here.** The test sliced the FIRST
  `Endpoint type: PUBLISHER` block in the report. The report is a flat list, and
  a foreign endpoint on `/qos_chatter` would be asserted against — which is
  precisely how `case_08_c_qos` once failed in-sweep and passed solo, printing a
  profile that was somebody else's. Now selected by NODE name
  (`reliable_talker` / `qos_listener`, from `rust_qos.launch.xml`), and more
  than one match is REPORTED rather than silently taking the first: a foreign
  process and a sibling cell need different remedies.
* **A local copy of `topic_endpoint_block`.** Identical logic to the library's,
  privately maintained, and therefore carrying 0690's bug after the library's
  copy was fixed. Deleted.

### One helper, not a third spelling

`await_topic_endpoints` lives in `nros_tests::ros2` and BOTH sites call it — the
0705 site's inline loop was replaced by the call. The reason this issue exists
is a remedy applied at one site and not its sibling, so leaving two loops that
agree today would rebuild the bug one level up.

It deliberately does not wait out two things: **more than one match** (the
sibling-cell case — waiting only makes the report bigger) and **a `ros2`
invocation that ERRORS** (a broken environment is not a slow one, and retrying
it for 20 s only delays the message saying so).

### The talker's lifetime had to move with it

`NROS_ENTRY_SPIN_MS` was 20000 — equal to the new discovery budget. Left alone,
a slow-discovery run would have raced the talker's own exit and reported "never
discovered" for a publisher that had merely stopped. Raised to 45 s; it costs
nothing on the happy path, since the test kills the talker as soon as the poll
returns.

### Verification

Both mutations, because a test that cannot fail proves nothing:

* Wanting a node that never appears (`no_such_talker`) fails after the full
  20 s — the deadline really is polled, and the message says
  "This is a DEADLINE, not a single shot", distinguishing exhausted-budget from
  the old one-shot miss (the issue's third angle).
* Flipping the expected reliability to `RELIABLE` fails against a report reading
  `BEST_EFFORT` — the QoS assertions still evaluate, and against the right
  endpoint's block.

Not reproduced under real sweep load; the fix is justified by the mechanism (an
unpolled 3 s window) rather than by a reproduction. Whether it holds is
answerable only by the next full sweep.

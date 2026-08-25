---
id: 770
title: "tier-2 runs the native interop cells against fixtures its own build
  lane did not refresh — the #482 exists-vs-fresh split, resurfaced for
  `interop::CELLS`"
status: open
type: bug
area: testing
related: [issue-0482, issue-0445, issue-0488, issue-0786]
---

## Independent corroboration (2026-08-25)

Hit again on a separate host in the same chain, and worth recording because the
second instance narrows the cause.

`examples/native/cpp/listener/build-cyclonedds/cpp_listener` and
`examples/native/c/service-client/build-cyclonedds/c_service_client` both went
STALE under a tier-2 run after a `lane=tier2` build reported all eight modules
OK. The newer input in each case was a cyclonedds source
(`third-party/dds/cyclonedds/src/ddsrt/src/atomics.c` after a submodule bump,
and `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/service.cpp` after an
edit). `build-test-fixtures lane=native` cleared both, exactly as reported here.

So this is not specific to `interop::CELLS` or to XRCE: it reaches any native
cell whose fixture the tier-2 build lane does not refresh. A cyclonedds source
edit is a good way to provoke it, because it stales native fixtures across
several languages at once while the 1-wise lane refreshes only some.

**And issue 0786 is the silent half of this same split.** There, two tests
hand-joined their fixture path instead of resolving it, so they never reached
`require_prebuilt_binary_fresh_cmake` at all — the out-of-lane stale artifact
did not fail, it RAN, and a five-day-old binary read as a runtime regression.
Fixing 0786 to resolve properly is what turned those two native fixtures into
the loud STALE verdicts above. That is the right direction (loud beats silent),
but it means 0786's fix INCREASES the surface this issue covers rather than
reducing it: cells that used to paper over the build/run mismatch now report it.

Useful framing for whoever takes this: 0786 was "the test never asked", this is
"the lane answered inconsistently". Only the second needs `nros_lane_build_lane`
and `CiLane::run_scope` to agree.

## Problem

In a clean `build-test-fixtures lane=tier2` → `ci-matrix` chain
(2026-08-24, newslab-241), the native XRCE interop tests
(`xrce_ros2_interop`, `c_xrce_api`, the zenoh↔xrce/cyclone bridge
tests) all failed with the issue-0445 absorbing verdict:

```
Failed to build xrce-talker: BuildFailed("Test fixture is STALE — a
source is newer than the built binary: … 5th consecutive stale verdict
for this fixture, first 1h ago.")
```

A subsequent `build-test-fixtures lane=native` cleared every one of
them (the xrce interop suite then ran 8/8). So:

* the tier-2 **run** treats these tests as in-scope (they executed and
  failed hard rather than skipping by coordinate), but
* the tier-2 **build** lane did not refresh the native fixtures they
  consume.

That is the #482 shape — "which fixtures must be FRESH vs which must
EXIST are different questions" — reappearing one layer up, for
`interop::CELLS`. Phase-340 W3 gave the *matrix* cells a single
coordinate predicate shared by build-set and run-set
(`row_coord()` / `row_artifact_root()`); interop cells have no
`fixtures.toml` row by design (RFC-0051 — ephemeral peer, native nano
side), so nothing narrows them by coordinate at run time, and the
tier-2 lane inherits them unconditionally while its build cover does
not include their fixture coordinates.

## Cost

Every tier-2 sweep on a tree whose native lane is stale reports these
cells as FAILED with a message that reads like a build-system defect,
and the junit real-failure count includes them — a red a human must
re-diagnose per run (this run: ~8 named failures, all one cause).

## Direction

Either narrow the interop cells out of the tier-2 run the same way the
matrix cells narrow (their nano-side fixture coordinate is knowable —
`NativeFixtures` in the cell row), or add their fixture coordinates to
`nros_lane_build_lane(tier2)`'s build cover so the run's requirement
and the build's product agree. One predicate, both sides — the #482
rule verbatim.

Note `matrix_fixture_coverage.rs` G1–G4 gate the CELLS↔fixtures
correspondence; whichever side moves, the gate should grow a case for
"every cell a lane RUNS has its fixture in that lane's BUILD cover".

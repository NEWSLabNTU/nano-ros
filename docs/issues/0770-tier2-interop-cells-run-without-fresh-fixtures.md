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

## A retracted "corroboration" (2026-08-25), and what it actually shows

An earlier revision of this issue claimed a second sighting on another host:
native cyclonedds fixtures (`native/cpp/listener`, `native/c/service-client`)
going STALE under tier 2. **That was wrong and is retracted.** The real
`ci-matrix` run had ZERO stale verdicts. Those STALEs came from bare
`cargo nextest` invocations, where `NROS_TEST_COORDS` is unset, so
`run_coords()` returns `None`, `require_in_lane` returns early, and every
resolved fixture must be fresh. Correct behaviour for a run outside any lane —
not this bug.

Worth keeping because it sharpens what this issue is NOT. Those rows attribute
fine: `native/cpp/listener` has three rows whose `artifact_root`s are distinct
(`build-zenoh` / `build-xrce` / `build-cyclonedds`), `linux,cpp,cyclonedds` is
not in tier 2's 14-coordinate cover, and in a real lane run it therefore SKIPS.
The machinery works wherever a row exists.

So this issue is specifically about cells with **no `fixtures.toml` row**:
`attribute_path` returns `None`, the caller's contract for `None` is "never
skip", and the lane's build cover never included them either. Row present ⇒
handled. Row absent ⇒ this bug. That is the whole boundary, and it means the fix
must come from giving these cells an attributable coordinate or from widening
the build cover — not from touching attribution, which is behaving as designed.

Issue 0786 is the sibling worth reading alongside: there the tests hand-joined a
fixture path, so they never reached `require_in_lane` at all and a stale
artifact RAN. Fixing them to resolve gives them attribution, so in a lane run
they now skip or fail honestly. That shrinks this problem rather than enlarging
it — an earlier note here claimed the opposite, also wrongly.

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

## Reproduction attempt, 2026-08-25 — does NOT reproduce on current main

Ran the exact chain on a fresh tree. After `just build-test-fixtures lane=tier2`,
with `NROS_TEST_COORDS` pointed at tier 2's own coords file (14 coordinates,
which is what `ci-matrix` does):

| test | result |
| --- | --- |
| `interop_e2e` | **10/10 pass, 0 stale** |
| `xrce_ros2_interop` | 7 `[SKIPPED:lane]`, 0 stale |
| `c_xrce_api` | 5 `[SKIPPED:lane]`, 0 stale |

The skips are correct and specific, e.g.

```
[SKIPPED:lane] out of lane: examples/native/rust/service-client is at coordinate
linux,rust,xrce, which this run's lane does not select, so
`just build-test-fixtures lane=<this lane>` deliberately did not build it.
```

**Why the machinery already handles this.** These fixtures resolve through
`build_example` / `build_example_rmw`, which call `select_row()` and then
`require_prebuilt_row_binary`, whose first act is
`require_coord_in_lane(&row.coord, &row.dir)` — keyed on the ROW's coordinate,
not on the artifact path. So it is immune to the path-attribution problem
entirely, and an interop cell narrows exactly like a matrix cell. Those
`require_coord_in_lane` call sites landed 2026-08-19/20 (`e986e07be`,
`9260d5bec`, `a12c6ebd7`) — **before** this report, so the report is not simply
predating the fix, and that is worth someone else's eyes.

**The confound to rule out first.** A stale tree presents IDENTICALLY to this
bug: pull anything, and every fixture built before the pull reads STALE, in-lane
or not. I reproduced ~9 such verdicts on this very tree and briefly mistook them
for this issue — twice, once via a bogus "ambiguous artifact root" theory and
once via a "rows=0" attribution check that was matching a POST-redirect group
path against pre-redirect leaf roots. Both were my analysis error, not the code.
Distinguishing signature:

* **stale tree** — the "newer" file is a repo SOURCE (`vtable.cpp`, a submodule
  bump). Rebuilding the SAME lane clears it.
* **this bug** — the fixture's coordinate is genuinely outside the lane's build
  cover, so rebuilding the same lane does NOT clear it, and only a wider lane
  (`native`, `all`) does. The reporter's note that `lane=native` cleared it is
  consistent with EITHER, because `native` is also simply a later build.

So the decisive datum, if this recurs: after `lane=tier2` build → `lane=tier2`
run, is the fixture still stale? If yes, this is real. If a second `lane=tier2`
build clears it, it was the treadmill.

**Not closed.** One host's non-reproduction does not disprove the report, and
the reporter's `newslab-241` run is real evidence I cannot see. Left open with
the above so the next sighting can be classified in one step instead of
re-derived.

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

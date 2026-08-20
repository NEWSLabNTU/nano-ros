---
id: 713
title: "Tier 2's zephyr BUILD set is narrower than its RUN set — 7 coordinates built, more demanded as in-lane"
status: open
type: bug
area: build/zephyr
related: [issue-0702, issue-0482, issue-0677]
---

## Symptom

Tier 2, with the zephyr fixture lane reporting success:

```
== zephyr == OK
```

then, in the test run:

```
Test fixture binary MISSING for an in-lane coordinate:
  .../zephyr-workspace/build-cortex-m-rust-talker-zenoh/zephyr/zephyr.elf
```

Seven of tier 2's thirteen failures are this, across
`zephyr_cortex_m_qemu` (cpp + rust), `qos_zephyr_ros2_interop_e2e`,
`logging_smoke`, `entry_matrix`, `multihost`, `realtime_tiers` and
`sched_dims_applied`.

## What is actually happening

The build dirs ARE created and configured — `CMakeCache.txt`, `build.ninja`,
`zephyr/arch`, `zephyr/drivers` all present. What is missing is the LINK:

```
$ ls zephyr-workspace/ | grep -c '^build-'          # 70
$ ls zephyr-workspace/*/zephyr/zephyr.elf | wc -l   # 9
```

and those 9 are leftovers from earlier one-off manual builds, not from any lane.

## CORRECTION — the first version of this issue was wrong

It claimed `just zephyr build-fixtures` "exits 0" after failing, and built the
whole argument on that. It does not. Measured unpiped:

```
$ just zephyr build-fixtures > log 2>&1; echo $?
1
```

The original reading came from `just zephyr build-fixtures 2>&1 | tail -8`,
whose exit status is `tail`'s, not `just`'s. A measurement error, not a defect —
and the code agrees with the correction: the recipe runs under `set -e`, the
driver call sits in an `if` BODY (where `set -e` applies), the driver itself has
`set -euo pipefail`, and issue 0700 deliberately REMOVED a `|| true` from the
neighbouring `west-fixtures.sh` call for exactly this reason.

A second thing that re-measurement surfaced: a direct run currently aborts on the
STALE in-tree CLI precondition (`nros-cli-core/src/lib.rs:77`) AFTER the
configure pass — which is why a hand-run leaves 70 configured dirs and no ELFs.
That is the documented CLI-then-fixtures ordering, not a bug.

## Diagnosed (2026-08-20)

The stage did exactly what it was asked. From its own log
(`tmp/build-test-fixtures-latest/zephyr.log`, and the joblog row
`zephyr … 18 0`):

```
zephyr-fixture-make-driver: targets=
    zephyr-fixture-1-build-cpp-talker-xrce
    zephyr-fixture-2-build-cpp-listener-xrce
    zephyr-fixture-3-build-cpp-service-server-xrce
    zephyr-fixture-4-build-cpp-service-client-xrce
    zephyr-fixture-5-build-cpp-action-server-xrce
    zephyr-fixture-6-build-cpp-action-client-xrce
    zephyr-fixture-7-build-cortex-m-c-talker-zenoh
```

SEVEN targets — tier 2's 1-wise zephyr cover. All were already present, so they
reused, the stage took 18 seconds and exited 0. Nothing is broken about that.

The console silence that started this investigation is also normal: the make
path redirects each stage to a per-stage log (`>$log 2>&1`), so a successful
stage prints nothing between `== zephyr ==` and `== zephyr == OK`.

What the failing tests demand is a DIFFERENT set:

```
build-cortex-m-cpp-talker-zenoh      build-cortex-m-rust-talker-zenoh
build-ws-rs-qos-entry-zenoh          (+ logging-smoke, entry_matrix cells, …)
```

none of which the build lane was asked to produce — and the resolver treats them
as IN-LANE coordinates, so it fails hard rather than skipping.

That is issue 0482's subject exactly: **a lane answers TWO questions and they
have different answers** — which fixtures must be FRESH (the build's cell cover)
versus which must EXIST (a property of the RUN). `CiLane::run_scope` and
`nros_lane_build_lane` are supposed to keep those in step; for zephyr they do
not. Either the run is admitting coordinates the tier-2 cover excludes, or the
cover is too narrow for the cells bound to it.

## Superseded notes

## What remains established

* Tier 2's fixture lane printed `== zephyr == OK`, with no build output between
  `== zephyr ==` and the OK.
* Seven of tier 2's failures are then `Test fixture binary MISSING for an
  in-lane coordinate`, naming ELFs that do not exist.

So the lane's stage said OK while the images its own in-lane tests require were
absent. Whether the stage ran and did nothing, was skipped, or ran against a
different coordinate set is NOT diagnosed — and the earlier guess (a stale-stamp
skip) is already refuted: building the dirs by hand cleared exactly ONE of the
seven, because they were configured, not built.

## Why it matters more than seven test failures

**A build lane that reports success having built nothing is the same defect
class as issue 0702**, one level up. 0702 was about tests that cannot fail;
this is a BUILD that cannot fail. Everything downstream inherits the lie: the
lane prints `== zephyr == OK`, `build-test-fixtures` exits 0, and the first
thing to notice is a test looking for an ELF twenty minutes later — where it
reads as a fixture-freshness problem rather than a build failure.

It also means **no Zephyr coordinate has been built by tier 2 on this host**,
and tier 2 is the only tier that builds Zephyr at all (tier 1 is native-only).

## Note on a wrong first reading

The first hypothesis here was that the lane SKIPPED zephyr on a stale stamp:
the fixture log shows `== zephyr ==` followed immediately by `== zephyr == OK`
with no build output between them, and only 8 build dirs existed at the time.
Running `just zephyr build-fixtures` directly then ran for >10 minutes and
produced 70 dirs, which looked like confirmation.

It was not. Re-running tier 2 afterwards cleared exactly ONE of the seven
failures. The dirs had been configured, not built — so the lane is not skipping,
it is failing silently, and the extra dirs changed nothing.

## Direction

1. Decide which side is wrong: does tier 2's zephyr cover need those
   coordinates, or should the tests bound to them be out-of-lane for tier 2?
   `matrix::CELLS` / `interop::CELLS` declare the run set; `fixtures-manifest.py
   coords` computes the build set. They must agree per issue 0482, and the
   `matrix_fixture_coverage.rs` gates exist to enforce exactly that — so the
   first question is why those gates pass while this diverges.
2. Separately, the `make: *** wait: No child processes` seen in a hand-run is
   real and worth understanding, but it is NOT this issue's mechanism. The `NROS_JOBSERVER=1` path omits
   `ninja -j` / `CMAKE_BUILD_PARALLEL_LEVEL` deliberately, so a token-pool fault
   here starves the leaves rather than slowing them.
3. A post-condition worth having regardless: the lane knows which coordinates it
   was asked to build, and could assert their outputs exist before reporting OK
   — the same "assert the artifact" rule the fixture manifest applies elsewhere.

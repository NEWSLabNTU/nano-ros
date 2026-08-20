---
id: 713
title: "Tier 2's zephyr BUILD set is narrower than its RUN set — 7 coordinates built, more demanded as in-lane"
status: resolved
type: bug
area: build/zephyr
related: [issue-0702, issue-0482, issue-0677]
resolved: 2026-08-20
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

## Root cause (2026-08-20) — an assumption in `lane.rs` that tier 2 breaks

`nros_tests::fixtures::lane`'s module doc states the skip rule and its
justification:

> A path that attributes to NO row never skips (fail closed). Families built
> module-level rather than by coordinate — the Zephyr west leaves, the
> compile-check lane — have no manifest row and keep today's hard failure,
> **which is correct: their build is not narrowed either, so nothing is
> missing.**

The last clause is the bug. For tier 2 the zephyr build IS narrowed. Measured:

```
$ cargo run -q -p nros-tests --bin lane-coords -- tier2 | grep zephyr
zephyr,cpp,xrce
zephyr-cortex-m,c,zenoh
```

and the stage's driver was handed exactly the 7 targets those two coordinates
cover. Every other zephyr west leaf — `zephyr,rust,zenoh` among them — is
therefore NOT built by the lane, has no manifest row to attribute to, and so can
never be lane-skipped either. Guaranteed hard failure, for as many tests as are
bound to those leaves: seven, here.

The rule is right for the `native` lane, where the west families genuinely are
not narrowed. It is wrong for any lane that narrows the zephyr build, which is
what tier 2 does.

## Direction

1. The two coherent fixes, and it is a maintainer's call which:
   * **Attribute the west leaves.** Give them manifest rows (or a
     coordinate-bearing artifact root) so the resolver can skip them by
     coordinate like everything else. Then a narrowed lane narrows both sides
     and the 0482 invariant holds for zephyr too.
   * **Widen tier 2's zephyr cover** to every west leaf a bound test needs.
     Honest, but it makes the zephyr half of tier 2 close to `ci-full`, which
     is the cost the 1-wise cover exists to avoid.
   Note the `matrix_fixture_coverage.rs` G1-G4 gates do NOT cover this: they
   check interop cells, not the west-leaf/lane-cover relationship.
2. Separately, the `make: *** wait: No child processes` seen in a hand-run is
   real and worth understanding, but it is NOT this issue's mechanism. The `NROS_JOBSERVER=1` path omits
   `ninja -j` / `CMAKE_BUILD_PARALLEL_LEVEL` deliberately, so a token-pool fault
   here starves the leaves rather than slowing them.
3. A post-condition worth having regardless: the lane knows which coordinates it
   was asked to build, and could assert their outputs exist before reporting OK
   — the same "assert the artifact" rule the fixture manifest applies elsewhere.


## Resolved (2026-08-20) — the lane decides once, in the shared helper

West leaves DO have manifest rows; they are only unattributable **by path**,
because west writes into the Zephyr build root rather than under
`row_artifact_root`. `fixtures::lane`'s fail-closed arm therefore called every
one in-lane, on a premise its own docs stated:

> their build is not narrowed either, so nothing is missing.

That was true when written and stopped being true when phase-350 W1.b narrowed
the zephyr BUILD by coordinate. Since then a tier-2 run resolved leaves the
build had deliberately skipped, and reported a broken promise indistinguishable
from a regression — seven of tier 2's twelve failures.

The call went into `require_prebuilt_binary_fresh_zephyr`, the ONE helper every
zephyr west leaf funnels through, keyed on the build-directory name that
`fixtures-manifest.py west-leaves` already carries a coordinate for
(`get_prebuilt_zephyr_example` had taken that route since issue 0517). Putting
it there rather than at the ~14 resolvers is the difference between fixing the
class and fixing the two sites whose failures happened to be read — the shape
CLAUDE.md names for #282's second idiom and #328's unswept resolvers. A new
zephyr resolver now gets the narrowing without knowing it exists, the same
argument issue 0466 settled one block down for the source-candidate check.

`build_logging_smoke_zephyr_native_sim` resolves through the generic freshness
helper instead, so the lane call could not ride along and is named there.

Swept: all 16 zephyr image resolvers across `binaries/mod.rs` and `zephyr.rs`
route through the helper or the named call.

    grep -n 'zephyr_build_root()\.join\|build_root\.join' \
        packages/testing/nros-tests/src/fixtures/binaries/mod.rs \
        packages/testing/nros-tests/src/zephyr.rs

The stale premise in `lane.rs`'s module docs was corrected in the same commit —
leaving it would have re-taught the next reader the thing that caused this.

### Verified

Tier 2's coordinate file (`zephyr,cpp,xrce` + `zephyr-cortex-m,c,zenoh`) against
`entry_e2e` + `multihost_e2e`: every zephyr cell now reports

    zephyr/rust/qos: [SKIPPED:lane] out of lane: .../zephyr_rust_qos_entry is at
    coordinate zephyr,rust,zenoh, which this run's lane does not select, so
    `just build-test-fixtures lane=<this lane>` deliberately did not build it.

instead of failing MISSING. `entry_matrix` then reports `[SKIPPED]` for all 15
cells; under bare `cargo nextest` that prints FAIL because `skip!` is a panic,
and `just test-all`'s junit rewrite is what records it as a skip (CLAUDE.md).

An in-lane coordinate still fails exactly as hard when its image is missing or
stale — the skip is keyed on the coordinate, never on absence (issue 0445).

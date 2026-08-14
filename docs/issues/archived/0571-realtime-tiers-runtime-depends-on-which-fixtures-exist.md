---
id: 571
title: "`realtime_tiers` passes tier 1 in 12 s by SKIPPING 15 of its 16 cells —
  build the images and it exceeds nextest's 60 s"
status: resolved
type: bug
area: testing
related: [issue-0572, issue-0564, issue-0565, issue-0460, issue-0357, issue-0466, issue-0445, rfc-0051, phase-329]
---

## Symptom

Two `just ci` runs, same tree, hours apart:

```
PASS    [  12.310s] nros-tests::realtime_tiers_e2e realtime_tiers
TIMEOUT [  60.013s] nros-tests::realtime_tiers_e2e realtime_tiers
```

Nothing about the test changed between them. What changed is the *tree around
it*: a `lane=tier2` fixture build ran in between, so the embedded QEMU images
exist now. Run directly, with no nextest timeout:

```
test result: FAILED. finished in 66.42s
realtime_tiers: 1 of 16 row(s) FAILED:
  nuttx-arm/rust: high-tier /ctrl counter 0 is not ≥3× the low-tier /telem counter 4
```

## What is actually wrong

`realtime_tiers` is ONE test iterating every `Workload::RealtimeTiers` runtime
cell serially (`tests/realtime_tiers_e2e.rs:461`), booting a QEMU image per
embedded cell. Its wall-clock is therefore a function of **how many fixtures
happen to exist on the machine**, and its verdict is a function of the same:

| images present | cells actually run | time | verdict |
| --- | --- | --- | --- |
| native only | 1 | 12 s | PASS |
| + embedded | 16 | ~70 s | TIMEOUT (and one red cell inside) |

The 12-second PASS is the dangerous half. Tier 1 does not build embedded
fixtures, so on the lane where this test runs it reports green **because the
images are missing** — the absent-fixture case is indistinguishable from the
working case, which is issue 0445's shape at the suite level. A cell can rot
for months and tier 1 will keep saying PASS.

Then, the moment a developer builds any embedded lane locally, tier 1 starts
failing on a 60-second timeout that has nothing to do with their change — which
is how this was found (during phase-351 W3, whose diff is unrelated).

## Why the timeout is the wrong signal

nextest kills at 60 s and prints TIMEOUT. The test swallows per-cell panics
(`catch_unwind` + a silenced hook) and only reports at the END, so a timed-out
run prints **nothing at all** — not the cells that passed, not the one that
failed. The red cell (issue 0572) was invisible until the binary was run
outside nextest.

## Fixed (2026-08-14)

**The timeout half was fixed in parallel by issue 0564** (another session, same
day): a binary-level `slow-timeout = { period = "180s", terminate-after = 3 }`.
That is why this issue's two observations came apart — 0564 measured 127 s and
204 s and budgeted for them; this issue is about the OTHER half, which a budget
cannot address.

**The lane half.** `scripts/test/lane-filter.sh native` narrows tier 1 by
excluding platform tokens in binary and test NAMES. Issue 0357 already recorded
that binary exclusion alone was insufficient and added the test-name exclusion.
Consolidation defeats both: FOUR consumers are one generically-named test each
over every platform's cells —

    entry_e2e entry_matrix
    multihost_e2e multihost
    realtime_tiers_e2e realtime_tiers
    roundtrip_xprocess_e2e roundtrip_xprocess

so no name filter can reach their cells. They now narrow their own cell list
through `nros_tests::lane_scope::admits` — the run-scope twin of
`fixtures::lane::require_coord_in_lane`, which solves the same problem for tier
2's coordinate scoping (issue 0482: a lane that cannot be a name filter must be
applied where the test binds to a platform). Gate:
`check-lane-scope-consumers` (mutation-verified).

**The visibility half.** Every consumer now PRINTS what it did not run.
`realtime_tiers` under `NROS_TEST_SCOPE=native`:

    realtime_tiers: 4 row(s) ran, 0 skipped, 12 out of lane
      - nuttx/rust: out of lane (NROS_TEST_SCOPE=native admits the host board only)
      …

and a run where NO row ran is a `skip!`, not a pass. `entry_matrix` already had
this (issue 0460, the same finding one binary over).

**A third defect, found on the way.** Five `[test-groups.*-realtime-*-port]`
groups existed to serialize a realtime_tiers CASE against the partner e2e
sharing its baked image and slirp port. Every filter naming them selected a
test name phase-329 deleted, so all five had NO live members — the port
serialization had been silently off, and both surviving binaries
(`realtime_tiers_e2e`, `sched_dims_applied_e2e`) could boot the same port
concurrently. `sched_dims_applied_e2e`'s own comment had predicted exactly this
residual. Retired into one `matrix-consumers-serial` group holding all five
consolidated consumers. Verified with `cargo nextest show-config test-groups`:
zero overrides now match no test.

*Verified:* tier 1 green — `1403 tests run: 1337 passed, 72 skipped`,
`realtime_tiers` PASS in 22 s having reported its 12 out-of-lane rows.

## Original plan (superseded by the above)

1. **Per-cell time budget + report as you go.** A cell that overruns should fail
   as that cell, not take the suite's clock with it, and results should print
   incrementally so a kill still leaves evidence.
2. **A cell whose image is absent must SKIP LOUDLY** (`nros_tests::skip!`),
   counted in the summary — never silently drop out of a green run. Whether
   tier 1 should then skip 15 of 16 is a lane question (RFC-0061), but it must
   be *visible* that it did.
3. Possibly split the embedded cells into their own binary so the toolchain-gated
   filters in `test-all` can exclude them by name, which they cannot today: the
   binary is `realtime_tiers_e2e` and the test is `realtime_tiers`, so no
   `~nuttx` / `~qemu` pattern matches, and the embedded cells ride into every
   tier-1 run.

## Not this

Not a flake, and not a stale fixture: the nuttx images were rebuilt from
scratch (`just nuttx build-fixtures-arm`, clean) and the timing and the failing
cell both reproduce exactly.

---
id: 1983
title: "`a_rows_rmw_is_the_backend_its_artifact_linked` ran past nextest's 60 s terminate in tier 2, after issue 1690 put its `nm` sweep on a pool"
status: resolved  # duplicate of 1984
type: bug
area: [testing]
severity: medium
found: 2026-10-19
related: [1690, 1984]
resolved_in: 2026-10-20
---

## Measured

- Run: tier 2 run-matrix 39210994277 (schedule, 2026-10-19 06:37 UTC, head
  `c75d1265e`), on runner newslab-server-241.
- Job: 117924100355, `tier 2 (1-wise matrix)`, step `just ci matrix`.
- `Real failures: 1 / 1 total failures` after the junit rewrite. The other
  303 nextest reds in the summary are `skip!` panics, which the rewrite reclassifies as skips.
- The one real failure:

```
 TERMINATING [> 60.000s] (─────────) nros-tests::rmw_coordinate_truth a_rows_rmw_is_the_backend_its_artifact_linked
     TIMEOUT [  60.004s] (3044/3084) nros-tests::rmw_coordinate_truth a_rows_rmw_is_the_backend_its_artifact_linked
```

This is the first tier-2 run in days to reach its cells. Earlier runs stopped
at issue 1952's pin-baseline refusal. So there is no recent tier-2 measurement
of this test to compare against.

## Reading

Issue 1690 measured this test's serial `nm` sweep at 54.8 s of the 60 s budget
and moved it onto a pool: `par_map`, width = available parallelism capped at
`POOL_CAP` (8). The pool divides the work, but the total still grows with the
number of located workspace binaries, and tier 2 runs this test while about
40 other tests are in flight on the same host. `available_parallelism()`
reports the host's cores, not the share this test gets, so 8 workers can
still queue behind the rest of the sweep.

The test has no nextest override of its own (`.config/nextest.toml` has one
for its sibling `a_rows_entry_registers_its_nodes_at_runtime`, also from
1690), so it gets the default 60 s terminate.

## What it is NOT

- Not a wrong-backend verdict: the test was killed before it asserted
  anything, so whether every row linked the backend it declares is unknown
  for this run.
- Not 1690's sibling test, `a_rows_entry_registers_its_nodes_at_runtime`;
  that one has its own override and did not time out here.
- Not fixture staleness: the lane built and tested in one job.

## What would close it

Measure first, then pick one:

1. **Its own budget, sized from a number.** Time the sweep in a tier-2 job
   (located binaries, wall time per `nm`, pool width actually reached), and
   give the test a nextest override sized from that measurement, the way 1690
   did for its sibling.
2. **Less work per row.** `nm` reads the whole symbol table to count a few
   backend namespaces. A cheaper probe, or a cache of per-artifact answers
   keyed on the artifact's mtime, would take the cost off the test's
   critical path.

Acceptance: the test reaches a verdict in a tier-2 run.

## Resolution

**Duplicate of issue [1984](1984-run-matrix-coordinate-truth-nm-sweep-timeout.md)**,
which was filed from the same run (39210994277) in another session and fixed
by PR #2432 (merged 2026-10-19). This issue's option 2 is what landed, plus
bounds, not option 1:

- The manifest is read once per process instead of one `python3` per row.
- Every `nm` pass is bounded (20 s), and so is the sweep (25 s). Either case
  fails the test, naming the row and binary, instead of a suite-level TIMEOUT.
- Every run prints its locate time, its sweep time and its five slowest rows.
- No nextest override was added.

**Acceptance met.** The next scheduled run-matrix, 39380448771 (2026-10-20,
head `22a482fae4`), reached a verdict in both jobs:
`PASS [ 0.583s] … a_rows_rmw_is_the_backend_its_artifact_linked` in tier 2
(job 118488124531) and `PASS [ 0.589s]` in tier 1 (job 118507538649). Both
jobs ran on runner 118. The test has not yet run on runner 241, the slow host
1984 was measured on.

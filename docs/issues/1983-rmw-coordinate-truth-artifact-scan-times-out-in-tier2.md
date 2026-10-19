---
id: 1983
title: "`a_rows_rmw_is_the_backend_its_artifact_linked` ran past nextest's 60 s terminate in tier 2, after issue 1690 put its `nm` sweep on a pool"
status: open
type: bug
area: [testing]
severity: medium
found: 2026-10-19
related: [1690]
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

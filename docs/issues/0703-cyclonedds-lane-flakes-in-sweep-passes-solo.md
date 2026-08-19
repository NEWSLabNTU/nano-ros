---
id: 703
title: "`check-rmw-cyclonedds` fails intermittently INSIDE `just check` and
  passes solo — two different tests, same session"
status: open
type: bug
area: testing, rmw
related: [phase-359, issue-0319]
---

## Observed

Two `just check` runs on 2026-08-19, same working tree, ~40 minutes apart,
each red at `check-rmw-cyclonedds` on a DIFFERENT test:

```
The following tests FAILED:
	  4 - nros_rmw_cyclonedds_pubsub_smoke (Failed)
...
The following tests FAILED:
	  5 - nros_rmw_cyclonedds_data_roundtrip (Failed)
```

Solo, immediately after each, the same lane passes:

```
just check-rmw-cyclonedds
100% tests passed, 0 tests failed out of 17
```

Tally for the session: **2 red in ~5 in-sweep runs, 0 red in 4 solo runs**
(one after the first failure, three consecutive after the second). Both reds
re-ran green in the following full `just check` with no code change between.

## Why this is filed rather than dropped

Two different tests failing means it is not one test's bug, and passing solo
means it is not the code the sweep just built. What is left is the environment
the sweep creates — CPU load, and DDS discovery on a machine already running
other lanes. That is the shape CLAUDE.md records for the QEMU lanes ("six nuttx
lanes failed 3/3 in-sweep, passed solo — retest a QEMU red SOLO before filing"),
and this is the first time it is written down for the Cyclone lane.

Recording it so the next person who sees a red here does not spend the
afternoon bisecting a change that is not responsible. It cost part of one
already.

## What is NOT yet known

- Whether the two tests share a mechanism (both open a participant and expect
  discovery within a fixed window) or fail for unrelated reasons. Neither
  failure's ctest output was captured beyond the summary line — `just check`
  prints the tail, and the per-test log was not kept.
- Whether the domain ids collide. The Cyclone fixture pairs bake distinct
  domains (50–58) precisely so parallel SPDP does not interfere; the ctest suite
  is a separate set and its domain allocation has not been audited against the
  lanes running beside it.
- Whether it reproduces under deliberate load, which is the cheap next step:
  run the lane with a parallel `just check-workspace` and see if the rate rises.

## Next step, if it recurs

Capture the failing test's own output rather than the ctest summary:

```
cd packages/rmw/cyclonedds/nros-rmw-cyclonedds/build
ctest --output-on-failure -R nros_rmw_cyclonedds_data_roundtrip
```

If it is discovery timing, the fix is a bounded wait on a condition rather than
a fixed window (the repo's `condition-based-waiting` rule); if it is a domain
collision, it is the 0161 class one lane over.

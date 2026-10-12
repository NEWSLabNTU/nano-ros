---
id: 1846
title: "live-peer's host job fills a 110 GB runner disk building the fixtures its cells resolve, so the host rows produce no verdict"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-13
related: [1353, 1811]
---

## Measured

The `live-peer regression` job `rows whose board IS this runner`
(`ubuntu-22.04`, container `nano-ros-ci:humble`) died in "Build the fixtures
those rows resolve" with `No space left on device`. It did so on all three
attempts of run 38251106285 on `main` (`199f34afbd`): jobs 114817335407,
114858465004 and 114887201126.

`scripts/ci/reclaim-disk.sh` reports `freed 31083 MB; 110080296 KB free`
before the build. The disk then fills in the step's last build, and did so on
every attempt:

```
16:10:46  workspace-fixtures-build.sh linux --id <8 ids>   (5 min total)
16:15:49  fixtures-build.sh linux c cyclonedds             ( 6 min)
16:22:16  fixtures-build.sh linux c zenoh                  (17 min)
16:38:55  fixtures-build.sh linux cpp zenoh                (16 min)
16:54:25  fixtures-build.sh linux rust cyclonedds          ( 4 min)
16:58:23  fixtures-build.sh linux rust xrce                ( 2 min)
17:00:30  fixtures-build.sh linux rust zenoh               (11 min)
17:11:25  compile-check-fixtures.sh cpp_robot_entry
17:13:08  compile-check-fixtures.sh shadowing
17:14:36  error: failed to build archive at `.../build/cmake-fixtures/shadowing/
          build/consumer/cmake/cargo/.../libnros_c.a`: No space left on device
```

The job has been red on every run since at least 2026-10-08, and the
reasons vary:

- a fixture-build failure on 10-08;
- a cells-step failure on 10-10;
- a `generate-bindings` failure on 10-11;
- runs ending with no failed step at all, on 10-09 (37883358718) and twice
  on 10-12 (38203690335, 38209324700). That is the shape a runner lost
  mid-step leaves. Disk is the reason in the three runs measured here.
Whether it caused the ones with no failed step is **unconfirmed**.

## What is known about the cause

- The workflow already narrows the workspace builds to the rows the cells
  reach (8 of 121 ids, run 37883358718's fix). The plain-row builds are not
  narrowed: each `fixtures-build.sh linux <lang> <rmw>` builds every native
  row for that pair, not the rows the cells resolve. The two 16-17 minute
  C and C++ zenoh builds are the largest steps, which is consistent with that,
  but no per-step disk figure exists. The job uploads no disk transcript,
  unlike the board jobs (`disk-transcript-live-peer-board*`).
- Issue 1353 is the same exhaustion on the scheduled `gate` lane.

## Next step

Measure before narrowing. Either upload the host job's
`NROS_DISK_TRANSCRIPT` as the board jobs do, or run the same step sequence
locally in a fresh `build/` and record `du -sh` after each build. Then narrow
the largest consumer to the rows the cells resolve, the way the workspace
half already is. Closed by this job reaching "Run the cells with a recorded
PASS" on a hosted runner.

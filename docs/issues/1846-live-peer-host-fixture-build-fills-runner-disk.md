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

## Cause (2026-10-13)

The plain-row C/C++ builds did not share their cargo directory. `just native
build-fixture-extras` exports `NROS_CMAKE_EXTRA_DEFS` (with
`-DNROS_SHARED_CARGO_ROOT`, `CMAKE_BUILD_TYPE=Release`, the codegen tool, …)
before calling `fixtures-build.sh`. The live-peer host job calls
`fixtures-build.sh linux <lang> <rmw>` DIRECTLY and passed none. Its log shows
42 C/C++ leaf configures with no extra defs, so each leaf compiled the whole
Rust stack (`nros-c`/`nros-cpp` plus the RMW) into its own
`build-<rmw>/cargo/`. One local unshared leaf (`examples/native/c/talker/build-zenoh`)
holds 2.9 GB, nearly all of it that cargo directory. The same omission meant
the lane built these fixtures in a different shape from `just native` (no
Release build type).

Measured with the defs, from fresh build dirs: `c/talker` and `c/listener`
(zenoh) both link `build-*/cargo` to ONE keyed directory, 1.3 GB, and each leaf
dir is 48 MB. Thirteen C zenoh leaves therefore need about 2 GB instead of
13 × 1.3–2.9 GB.

The earlier lead in this issue, that the unnarrowed rows were the cost, was
half right. The rows are still built per (lang, rmw) and not per cell, but
sharing removes most of what each extra row cost.

## Fix

- `scripts/build/native-cmake-defs.sh`: `nros_native_cmake_extra_defs`, the
  one spelling of the native C/C++ defs. `native.just` wrote it twice by hand.
- `fixtures-build.sh` applies it itself for `linux` when the caller states no
  `NROS_CMAKE_EXTRA_DEFS` (a caller's own value still wins), so a direct call
  builds what `just native` builds.
- The live-peer host job prints free disk after every build, so the next
  exhaustion names the build that caused it.

Still open until a live-peer host job reaches its cells with that headroom
printed.

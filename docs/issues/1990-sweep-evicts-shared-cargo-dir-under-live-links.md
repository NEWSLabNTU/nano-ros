---
id: 1990
title: "runner-sweep evicts `build/corrosion-cargo/<platform>` while example build dirs still link into it, so the next build without a reconfigure fails with `File exists (os error 17)`"
status: open
type: bug
area: [ci, build]
severity: medium
found: 2026-10-20
related: [0805, 1094]
---

## Measured

- Run: nightly 39385346818 (schedule, 2026-10-20, main at `237ca8f39`), on
  runner `nano-ros-runner-newslab-118-server`.
- Job: 118503733224, `tier 2 nightly (pairwise cover)`, step
  `just build tier2-nightly`. The `linux` and `freertos` fixture families
  failed; `zephyr`, `threadx_linux`, `nuttx` and `qemu` built.
- The same error for all six FreeRTOS C leaves (`talker`, `listener`,
  `service-server`, `service-client`, `action-server`, `action-client`, each
  `examples/mps2-an385-freertos/c/<leaf>/build-zenoh`) and for
  `examples/linux/cpp/talker/build-xrce`:

```
[0/16] cd .../examples/linux/cpp/talker/build-xrce/nano_ros/packages/api/nros-c/nros-c-image && ... cargo rustc ... --target-dir .../examples/linux/cpp/talker/build-xrce/cargo/nros-c-image_2c1c6 ...
error: failed to create directory `/home/runner/_work/nano-ros/nros-fixture-tree/examples/linux/cpp/talker/build-xrce/cargo`

Caused by:
  File exists (os error 17)
```

`[0/16]` is the first edge ninja ran. The build did not re-run cmake before
it reached cargo.

The same job on the earlier nightly, run 39373548195 (job 118466509430, main
at `d2adb51f7`), built both families (`== freertos == OK`, `== linux == OK`)
and has no `os error 17` anywhere in its log. That run used a different runner
(`nano-ros-runner-newslab-server-241`), so it says nothing about the state of
the 118 runner's tree.

### Sighting, 2026-10-22: two more lanes, same runner

The same `File exists (os error 17)` shows up in both jobs of the scheduled
tier-2 run 39734065923 (2026-10-22 06:27 UTC, main at `f83b2a4e1`). Both jobs
ran on `nano-ros-runner-newslab-118-server`.

- Job 119675552774, `tier 2 (1-wise matrix)`: three hits, all on the
  `examples/rv-virt-threadx/rust/*/build-{zenoh,cyclonedds}` leaves.
- Job 119678607819, `tier 1 (cells)`: one hit, on
  `examples/linux/cpp/talker/build-xrce`, which is the same leaf as the first
  sighting. It fails `build-fixture-extras`, so the job never reaches the
  cells.

```
error: failed to create directory `/home/runner/_work/nano-ros/nros-fixture-tree/examples/linux/cpp/talker/build-xrce/cargo`
FAILED: [code=101] nano_ros/packages/api/nros-c/CMakeFiles/_cargo-build_nros_c ...
```

So the defect is not limited to the nightly. Every lane that shares the 118
runner's fixture tree hits it, including ThreadX RISC-V Rust leaves.

## Cause

Three facts from the code at `237ca8f39`, and one from the logs.

1. **The shared dir is created only at configure time.**
   `nros_shared_cargo_dir()` (`cmake/NanoRosSharedCargoDir.cmake`) runs
   `file(MAKE_DIRECTORY "${_dir}")` under `NROS_SHARED_CARGO_ROOT`, and
   `nros_share_corrosion_cargo_dir()` (`cmake/NanoRosCorrosion.cmake`) then
   makes `<build>/cargo` a symlink to it with `file(CREATE_LINK ...)`. Nothing
   at build time checks the target or creates it again. The FreeRTOS lane
   passes `-DNROS_SHARED_CARGO_ROOT=$(nros_build_dir corrosion-cargo freertos)`
   (`just/freertos.just`); the native lane passes the same kind through
   `scripts/build/native-cmake-defs.sh`.
2. **The sweep deletes that dir as one unit.** `scripts/ci/runner-sweep.sh`
   (`_sweep_disk`) lists `corrosion-cargo` among the fixture cache kinds and
   evicts each child of it, oldest first, until the area is under its 60 GiB
   budget. Each child is a whole platform: `build/corrosion-cargo/freertos`,
   `.../native`, and so on.
3. **The sweep does not touch the links.** The example leaf build dirs
   (`examples/**/build-*`) are not under the build root and are not eviction
   units. A swept `corrosion-cargo/<platform>` leaves every
   `examples/**/build-*/cargo` link that pointed into it dangling.
4. **The sweep does evict these units on these runners.** The post-job sweep
   of run 39373548195 (runner 241) printed, among 134 evictions:

   ```
   [did]   rm -rf /home/runner/_work/nano-ros/nros-fixture-tree/build/corrosion-cargo/native  (4.4 GiB)
   [did]   rm -rf /home/runner/_work/nano-ros/nros-fixture-tree/build/corrosion-cargo/freertos  (3.8 GiB)
   ```

   Run 39385346818's own sweep on the 118 runner reported
   `fixtures: 108.6 GiB used / 60.0 GiB budget (high-water 180.8 GiB)` and
   evicted `build/corrosion-cargo/metadata-probe`. So that runner had been
   over budget and sweeping before this run.

`mkdir` on a dangling symlink fails with EEXIST. Archived issue 0805 item 4
recorded the same message for its re-point branch. When the next build of a
leaf runs without a configure, cargo's `--target-dir` is
`<build>/cargo/<crate>_<hash>`, cargo tries to create `<build>/cargo`, and it
fails this way. Here no configure ran: the commits between `d2adb51f7` and
`237ca8f39` touch no cmake input, and they do touch `nros-node`, so the
staleness probe rebuilt the leaves with `cmake --build` only.

## What could not be determined

This log does not show which earlier sweep on the 118 runner deleted
`corrosion-cargo/freertos` and `corrosion-cargo/native`. That runner's
earlier job logs were not read. The mechanism above needs only that some
sweep on that runner evicted them after these leaves were last configured.

`threadx_linux` and `nuttx` also pass a `corrosion-cargo` root and built
fine. NuttX's FFI driver sets `CARGO_TARGET_DIR` directly, with no symlink,
and cargo creates a missing directory itself. Why `threadx_linux` was not hit
(its unit not evicted on this runner, or its leaves reconfigured) is not
measured here.

## What it is NOT

- Not a Corrosion change. The `--target-dir` in the failing command is the
  link path, as it is on every working build.
- Not a key change. That path goes through the `IS_SYMLINK` re-point branch
  at configure time, and no configure ran here.
- Not a disk-full failure. Run 39385346818's own sweep reported
  `168G free on the checkout's volume` afterwards.
- Not the shared root moving between checkouts (issues 1904/1921). Both runs
  use the same `nros-fixture-tree` path.

## What would close it

Either side can own the fix. Choose one and gate it:

1. **The build re-creates what the configure made.** Re-create the shared dir
   (or re-point the link) at build time, for example as a cheap
   always-run step in front of `_cargo-build_<target>`. Then a swept cache
   costs a rebuild and not a failure. This is the sweep's own stated contract:
   "Evicting a fixture makes the next job's staleness probe rebuild it."
2. **The sweep removes links with their target.** When it evicts a
   `corrosion-cargo/<platform>` unit, it also removes (or marks for
   reconfigure) the `examples/**/build-*` dirs whose `cargo` link points
   into it.
3. **Do not evict the unit.** Take `corrosion-cargo` out of the sweep's kinds
   and size it on its own.

Acceptance: on a tree whose leaves are configured, `rm -rf
build/corrosion-cargo/freertos` followed by `just build tier2-nightly` (no
reconfigure) builds the FreeRTOS C leaves.

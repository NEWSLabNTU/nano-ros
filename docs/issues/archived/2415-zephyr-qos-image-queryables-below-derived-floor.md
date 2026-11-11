---
id: 2415
title: "Zephyr `zephyr_rust_qos` image states `max_queryables = 16`, and the zpico build now refuses it against a derived floor of 18, which turns the live-peer board job red"
status: resolved
type: bug
severity: medium
area: [zephyr, rmw-zenoh, examples, ci]
related: [0460, 2241, phase-392, phase-481]
found: 2026-11-10
resolved_in: "PR #3464: the qos image's `max_queryables = 16` row is gone, so the table is derived (18), and the image gets its own pthread-cond pool (27)"
---

## Measured

Scheduled `live-peer regression` run 42754129258 on `main` `20d55a6101`,
job 129625080890 "rows whose board is NOT this runner", Zephyr fixture
build. The build of `build-ws-rs-qos-entry-zenoh`
(row `workspace-zephyr-rs-qos`, image `zephyr_rust_qos` in
`examples/workspaces/features/src/demo_bringup/system.toml`) stops in
`zpico-sys`'s build script:

```
panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:448:9:
NROS_ZENOH_MAX_QUERYABLES=16 is below this image's DERIVED floor of 18 (phase-392 W5.f).
```

The job then ends with `✗ Zephyr fixture build FAILED (exit 2)`.

The same job passed on every scheduled run from 2026-11-05 to 2026-11-09. The
last green run was 2026-11-09 04:12 on `6e35469bde`, so the regression lies in
`6e35469bde..20d55a6101` (239 commits). None of them touches
`nros-zpico-build` or `zpico-sys/build.rs`. One, `a9061b0f1c` (issue 2365),
touches the bringup, but only its per-board stack settings, not the queryable
row.

## Cause (2026-11-10)

`1f493fd18f` (phase-530 W3, "resolve the Rust west entry"). Before it, the
generated Rust west entry had no sizing descriptor, so `zpico-sys` saw no
declaration and `check_queryable_override` returned early. W3 writes the
image's descriptor on this road. For this image it states `model_nodes = 2`
and `infra_families = "param+lifecycle"`, so the check ran, and the floor it
computes is the image's real need: 2 nodes x 6 parameter services + 5 lifecycle
services + 1 transient-local publisher = 18. That is the same per-node rule
`nros-rmw-zenoh` sizes from at boot. W3 did not double-count anything; the
stated 16 had always been under the image's need, and nothing checked it.
Reproduced locally from a clean build of `build-ws-rs-qos-entry-zenoh` on
`main`.

## Fix

The issue's first option, with the row deleted rather than restated:

- `[image.zephyr_rust_qos.knobs.zenoh] max_queryables = 16` is gone, so the
  table is DERIVED (18): a pool the declarations answer is not stated.
- 18 queryables need a pthread-cond pool of 1 subscriber + 18 + 4 zenoh-pico
  fixed + 4 headroom = 27 (`nros_cargo_build.cmake`'s floor). The image gets
  its own fragment, `boards/native_sim_native_64/prj-qos.conf`
  (`CONFIG_MAX_PTHREAD_COND_COUNT=27`), merged after the shared
  `prj-zenoh.conf`, which the other three images keep at 16. Mutexes need 47 of
  the shared 64, so nothing changes there.

Measured: the fixture builds (`CONFIG_MAX_PTHREAD_COND_COUNT=27`,
`CONFIG_NROS_MAX_QUERYABLES=-1`, i.e. derived), and `entry_e2e`'s
`zephyr/rust/qos` cell boots it and passes against a live router.

## Confirmed (2026-11-11)

Locally, on `main` `684011c5be` (which has #3464):

- `NROS_ZEPHYR_FIXTURE_FILTER=build-ws-rs-qos-entry-zenoh just zephyr build-fixtures`
  builds the image (`build/zephyr-workspace-builds/3.7/build-ws-rs-qos-entry-zenoh/zephyr/zephyr.exe`),
  and its `.config` has `CONFIG_NROS_MAX_QUERYABLES=-1` (derived) and
  `CONFIG_MAX_PTHREAD_COND_COUNT=27`.
- With the `int32-sink-zenoh` observer built, `entry_e2e`'s `entry_matrix`
  runs the `zephyr/rust/qos` cell and it passes. The other 16 cells stop on
  `[UNMET PRECONDITION]` because their fixtures were not built here, so none
  of them is a verdict.

The `live-peer regression` board job has NOT confirmed it yet. In both
dispatches on `main`, the job that builds this fixture ("rows whose board is
NOT this runner") died on a hosted runner with `No space left on device` in
the runner's own `_diag` worker log, before it reported: run 42911987237
(`cb799bc93b`) and run 42946634453 (`ae3172d80b`). That is runner disk, an
operator matter (see the maintainer's wontfix on 1353), not this image.

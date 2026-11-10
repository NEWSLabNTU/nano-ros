---
id: 2415
title: "Zephyr `zephyr_rust_qos` image states `max_queryables = 16`, and the zpico build now refuses it against a derived floor of 18, which turns the live-peer board job red"
status: open
type: bug
severity: medium
area: [zephyr, rmw-zenoh, examples, ci]
related: [0460, 2241, phase-392, phase-481]
found: 2026-11-10
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

## What it is

`check_queryable_override` refuses only when `sizing.declared` is true. The
image's own comment already records a derived 18 (phase-481 W3) and states 16
on purpose. So the likely change is that the declared sizing now reaches the
zpico build on this road, where before it fell back to "not declared" and the
check returned early. Not bisected. Phase-530's configure changes are in the
range (W6 adds the census step, W7 deletes the fixed point) and are the first
place to look.

## Why it is not a one-line fix

The image's comment explains why the row stays at 16: raising it to 18 makes
the configure refuse on Zephyr's static pthread-cond pool
(`CONFIG_MAX_PTHREAD_COND_COUNT=16`, while 1 subscriber plus 18 queryables
need 27). Raising that pool means editing the shared `prj-zenoh.conf` that
three other images merge. There are at least two defensible fixes:

1. Raise `max_queryables` to 18 and give this image its own pthread-cond pool
   fragment, so the shared conf is left alone.
2. Show that the floor of 18 over-counts for this image, for example because
   infrastructure services are counted twice once the declared inventory
   arrives, and fix the derivation.

## What it is NOT

- Not issue 2241 (live-peer jobserver/make). That issue names the same build
  dir for a different native_sim link failure.
- Not the floor being wrong in general: issue 0460's rule (a service server
  is a queryable) still holds.

## What would close it

The `live-peer regression` board job builds `build-ws-rs-qos-entry-zenoh`
again, with whichever fix is chosen, and the image's `system.toml` comment
says why that number is right.

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

## Sweep

Only four images in the tree state `max_queryables`, all in this bringup:
`zephyr_rust_qos`, which is fixed here, and the params, lifecycle and C++
params images, each at 16. Run 42754129258 built the other three past the same
check, and the qos image was its only refusal, so no other image on this road
states a number below its floor.

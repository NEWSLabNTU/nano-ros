---
id: 2705
title: "The NuttX guest-make road lets Kconfig select Cyclone DDS, then builds an image with no Cyclone backend in it"
status: resolved
type: bug
area: [nuttx, build, rmw]
severity: medium
found: 2026-11-18
resolved_in: "fix(issue 2705): the NuttX make road refuses an RMW it cannot build"
related: [2704, phase-550]
---

**Resolved:** the make road refuses, at configure time, every RMW it cannot
build, with or without a project. See [Fix](#fix).

## Found

While sweeping every build road that can produce a Cyclone DDS image for
issue 2704 (the runtime's own interface types). Read from the code, not
measured on a build.

## What the road does

`integrations/nuttx/Kconfig` offers `CONFIG_NROS_RMW_CYCLONEDDS` ("Eclipse
Cyclone DDS via the rmw_cyclonedds_cpp bridge") beside zenoh and XRCE.
`integrations/nuttx/Makefile` turns the choice into `NROS_RMW=cyclonedds` and
runs `cmake/NanoRosImageCrate.cmake` in script mode. For a backend the image
does not bundle as an rlib (Cyclone is CMake-linked, `[rmw.link]
strategy = "cmake"`), that generator "leaves everything as it was": the API
crate with the features it was given, no backend. `integrations/nuttx/Make.defs`
then puts only `libnros_c.a` or `libnros_cpp.a` on `EXTRA_LIBS`. Nothing on
this road builds or links `nros_rmw_cyclonedds`, `ddsc`, or any Cyclone
descriptor archive, and the Cyclone CMake has no NuttX platform arm.

So a NuttX apps-tree build that selects Cyclone either fails to link on the
backend's register symbol or links an image with no backend registered. In
neither case does it get a Cyclone image, and no step says that Cyclone is
not available on this road. (Which of the two happens is not measured.)

## Direction

Refuse the selection until the road can build one: `depends on` on
`NROS_RMW_CYCLONEDDS` (or drop it from the choice) in
`integrations/nuttx/Kconfig`, or a configure-time error in the Makefile
naming the road and the RMWs it builds (zenoh, XRCE). When the road gains a
Cyclone build, it must also call `nros_runtime_interface_types()` (issue 2704,
`check-runtime-interface-types-roads`).

## Fix

The class, not Cyclone: any RMW the image crate does not bundle is refused on
this road, and the set it builds is read from the RMW registry, never listed.

- `RmwDescriptor::image_crate_bundles()` (`cargo-nano-ros/src/rmw_descriptor.rs`)
  states the generator's own rule: `[rmw.link]` strategy `umbrella` and a
  Rust register crate, the two facts `nros_image_backend()` reads off the
  dispatch query. In tree that is zenoh and XRCE; Cyclone and uORB are
  `strategy = "cmake"`.
- `nros locate nuttx-cargo-config` now requires `--rmw` and checks it first
  (`check_nuttx_make_rmw`, `cmd/locate.rs`), before the board, the project
  and anything the build generates. The search is the project's RMW index
  when the app names one, else the SDK root's.
- `integrations/nuttx/Makefile` passes `--rmw` and `--kconfig` on every
  build, and runs that step (`NROS_CARGO_CONFIG_STEP`) first in
  `NROS_ARCHIVE_STEPS`, ahead of the image crate generator and cargo.
- The Kconfig choice is hand-written, so a generated `depends on` (option b)
  was not available; its help now says the build refuses what it cannot
  build, naming no backend.

The refusal, for Cyclone:

```
the NuttX make road (integrations/nuttx/Makefile) cannot build RMW `cyclonedds`,
which <kernel>/.config selects (`CONFIG_NROS_RMW_CYCLONEDDS=y`). This road links
the image staticlib alone, and `cyclonedds` is not bundled into it ([rmw.link]
strategy = "cmake"): its CMake target `nros_rmw_cyclonedds` is linked by a CMake
build, which this road does not run. RMWs this road builds: xrce, zenoh. Select
one of them (`make menuconfig`, nano-ros -> RMW backend), or build a
`cyclonedds` image on a road that links CMake backends (`nros build` on the
CMake road, or west on Zephyr) where the backend supports the platform.
```

Tests (`nuttx_make_rmw_tests` in `cmd/locate.rs`): Cyclone refused with and
without a project (a project whose `system.toml` also states `cyclonedds`,
the case the phase-550 W6a agreement check let through); zenoh and XRCE pass
with and without one; an unknown name is refused naming the buildable set;
and every RMW `integrations/nuttx/Kconfig` offers resolves to a provider and
is either built or refused, as the registry says (no NuttX build needed).

**When this road gains a Cyclone build** (or any CMake-linked backend), it
must also call `nros_runtime_interface_types()` (issue 2704,
`check-runtime-interface-types-roads`), and the refusal moves with it: the
road's buildable set is whatever it can link, not `image_crate_bundles()`.

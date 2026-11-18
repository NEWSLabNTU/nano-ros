---
id: 2705
title: "The NuttX guest-make road lets Kconfig select Cyclone DDS, then builds an image with no Cyclone backend in it"
status: open
type: bug
area: [nuttx, build, rmw]
severity: medium
found: 2026-11-18
related: [2704, phase-550]
---

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

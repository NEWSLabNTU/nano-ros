---
id: 2646
title: "No SDK Eigen for cross builds: Eigen 3.4's psincos does not compile on arm-none-eabi, where newlib's int32_t is long"
status: wontfix
type: enhancement
area: [sdk, cmake, cpp]
related: [phase-478, rfc-0014]
found: 2026-11-16
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md) I27.**
GitHub [#4013](https://github.com/NEWSLabNTU/nano-ros/issues/4013).

## Resolution (2026-11-17): wontfix, user ruling

nano-ros does not ship patched general-purpose C++ libraries. A user picks
their own library set and patches it for their board; a library from outside
nano-ros is supported by design: a nano-ros package's `CMakeLists.txt` finds
and links it as it would without nano-ros, and nano-ros neither provides nor
blocks it. So there is no `[source.eigen]`, no fork and no
`cmake/find/FindEigen3.cmake`, and the fork step the design below started
with is dropped. GitHub #4013 is closed with this reason.

**How a user brings their own library.** The book has no page for it today:
[Build inside an existing CMake or vendor tree](../../../book/src/guide/build/existing-cmake-or-vendor-tree.md)
covers adding nano-ros to a tree, not adding a library to a nano-ros package
for a cross target. That is a documentation gap, open: a page would say to
point `find_package(<Lib>)` at the user's own copy (`<Lib>_DIR` or
`CMAKE_PREFIX_PATH`, or `add_subdirectory` of a vendored tree), and that the
board toolchain file, not the host, decides what builds. The island already
does this (it copies and patches an Eigen of its own).

The analysis below stays as a note for anyone patching their own Eigen 3.4
for `arm-none-eabi`.

## What happens

The Autoware Safety Island's vendored Autoware MPC and PID controllers use
Eigen 3.4. On `arm-none-eabi`, newlib defines `int32_t` as `long` (ILP32), and
Eigen 3.4's `psincos` path assumes it is `int`, so it does not compile. The
island copies the HOST's Eigen at configure time and applies its own
`actuation_module/src/asi_common/patches/eigen-psincos-int32.patch`
(`asi_common/CMakeLists.txt:158-192`). nano-ros's `cmake/find/` has no Eigen,
so every C++ robotics package ported to Cortex-R/M through nano-ros meets this
and solves it alone.

## Design (withdrawn by the resolution above)

**The patch** (the island's
`eigen-psincos-int32.patch`, read 2026-11-16): in
`Eigen/src/Core/arch/Default/GenericPacketMathFunctions.h`, `psincos_float`
declares its huge-argument scratch buffer `y_int2` as plain `int` and then
calls `pstoreu`/`ploadu<PacketI>` on it. The NEON integer packets
(`Packet2i`/`Packet4i`) and their load/store specializations are keyed on
`int32_t`, which newlib types as `long` on ILP32 `arm-none-eabi`, so no
specialization matches and the generic fallback fails to compile. Every other
vectorised transcendental (`plog`, `pexp`, `pldexp`, `pfrexp`) already uses
the packet's `ScalarI`. The patch types `y_int2` as
`unpacket_traits<PacketI>::type` (`ScalarI`), and because `trig_reduce_huge()`
still takes `int*`, writes its result through an `int` temporary and assigns
it back, rather than aliasing a `long` through `int*`. One commit, with that
reason, so the branch stays a readable patchset.

- A `[source.eigen]` row in `nros-sdk-index.toml` pinning the fork's
  `nano-ros` branch commit (upstream 3.4.0 plus the patch above) (the CLAUDE.md submodule rule: a stable version on a fork's
  patch-line branch, never patched in place). `nros setup --source eigen`
  provisions it into the store like the other sources.
- `cmake/find/FindEigen3.cmake` defines the upstream target name
  `Eigen3::Eigen` (header-only, INTERFACE), resolved through `nros locate
  source eigen` (RFC-0103 D3's one ladder), so a ported package keeps writing
  `find_package(Eigen3)` as upstream does.
- A host build keeps the host's Eigen: the SDK copy is for cross targets,
  where there is no system Eigen and the patch is needed.
- The patch is offered upstream too; ours carries it until upstream's
  released line has it.

---
id: 2646
title: "No SDK Eigen for cross builds: Eigen 3.4's psincos does not compile on arm-none-eabi, where newlib's int32_t is long"
status: open
type: enhancement
area: [sdk, cmake, cpp]
related: [phase-478, rfc-0014]
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md) I27.**
GitHub [#4013](https://github.com/NEWSLabNTU/nano-ros/issues/4013).

## What happens

The Autoware Safety Island's vendored Autoware MPC and PID controllers use
Eigen 3.4. On `arm-none-eabi`, newlib defines `int32_t` as `long` (ILP32), and
Eigen 3.4's `psincos` path assumes it is `int`, so it does not compile. The
island copies the HOST's Eigen at configure time and applies its own
`actuation_module/src/asi_common/patches/eigen-psincos-int32.patch`
(`asi_common/CMakeLists.txt:158-192`). nano-ros's `cmake/find/` has no Eigen,
so every C++ robotics package ported to Cortex-R/M through nano-ros meets this
and solves it alone.

## Design (proposed)

**Step 1, the maintainer's:** create the fork (`NEWSLabNTU/eigen`) and its
patch-line branch (`nano-ros`, off upstream's `3.4.0` tag). Agents do not
create or push forks; nothing below is pushed until that exists.

**The patch** (proposed for that branch; it is the island's
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

## Acceptance

- A C++ package with `find_package(Eigen3 REQUIRED)` and an `Eigen::Matrix`
  `sin`/`cos` use builds for an `arm-none-eabi` board with no host Eigen and
  no consumer-side patch.
- ASI deletes its copy step and patch.

---
id: 881
title: "Cyclone's platform-alloc funnel has no fast-line lane — the only build that compiles it has no platform to funnel into"
status: resolved
area: ci
severity: medium
found: 2026-08-29
related: [0832, phase-391, RFC-0075]
---

# The funnel-ON path is built by no lane `just check` runs

## What was measured

Issue 0832 routes Cyclone's `ddsrt` heap through `nros_platform_alloc` behind
`NROS_DDSRT_PLATFORM_FUNNEL`. The switch is set in the **self-provision** branch
of `nros_provide_cyclonedds()`, which is the only place it can be set — a
`find_package` build links a Cyclone whose `heap.c` was already compiled.

That leaves three build shapes, and the funnel is exercised by none of the ones
on the fast line:

| build | Cyclone provenance | `NanoRos::Platform` | funnel |
| --- | --- | --- | --- |
| `just check rmw-cyclonedds` (standalone backend project) | source | **absent** | OFF |
| native nano-ros on this host | **find_package** (SDK store `0.10.5-nros1`) | present | impossible |
| embedded / cross | source (find_package skipped when `CMAKE_CROSSCOMPILING`) | present | **ON** |

Verified by configure output. The standalone project prints

```
-- nano-ros: CycloneDDS ddsrt heap -> libc (no NanoRos::Platform target to funnel into)
```

and the native root configure resolves

```
-- nano-ros: CycloneDDS via find_package (/home/aeon/.nros/sdk/cyclonedds/0.10.5-nros1/...)
```

so it never reaches the block at all. Only a cross build lands on the source
branch with a platform present, and those are the embedded cyclone fixtures in
tier 2.

## Why this matters more than a coverage gap

The funnel is a LINK-shape change: compiling it in turns three platform-ABI
symbols into undefined references in every archive that absorbs `heap.c`. When
`9f945dd93` set the define without the corresponding dependency, nothing failed
at the switch — it failed at whichever executable linked `ddsc` first, three
targets deep in the backend's own test suite. That was caught only because the
standalone project happened to build those executables.

After the dependency fix (`ad0e0e0ed`), the standalone project no longer
compiles the funnel at all. So the lane that caught the breakage is now the lane
that cannot see it, and the next regression in this shape surfaces in tier 2 —
a day of latency, on a build that also costs a cross toolchain.

## Verification that stands in for a lane today

Reproducible by hand, ~4 min warm, and worth keeping in whatever fix lands:

```
cmake -S . -B <dir> -DNANO_ROS_PLATFORM=posix -DNANO_ROS_RMW=cyclonedds \
      -DCMAKE_BUILD_TYPE=Release -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON
cmake --build <dir> --target ddsc
nm <dir>/.../heap.c.o | grep nros_platform      # U alloc/realloc/dealloc, no U malloc/free
```

Forcing `-DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON` is what makes a NATIVE
build take the source branch, which is the cheapest way to reach funnel-ON
without a cross toolchain.

## Candidate fixes

1. **A native funnel-ON configure+link in `check-rmw-cyclonedds`.** Costs a
   second Cyclone build from source (~4 min warm), which is most of why the
   recipe is currently 22 s. Probably too expensive for the fast line as-is.
2. **Give the standalone backend project a platform target.** It is the nano-ros
   Cyclone backend, and the funnel is now part of its contract; `nros_platform_posix`
   is a standalone cmake project. Would need the path passed in as a cache var,
   the way `CYCLONEDDS_SOURCE_DIR` already is — the module never walks the tree.
   Turns the existing 22 s lane into a funnel-ON lane and keeps its three
   executables as the link witness.
3. **A configure-only assertion**: reach the source branch, assert the define
   landed and the dependency is on the target, without building. Cheap, but it
   checks the cmake and not the link, which is the half that broke.

(2) looks best: it restores exactly the coverage that caught the original
breakage, at no new build.

## Note on the tier model

Funnel-off on native is not a defect. `unified` is an embedded promise, native
hosts have a real libc heap, and a `find_package` Cyclone cannot be funnelled at
all. The gap is in TESTING the embedded shape cheaply, not in the shape itself.

## Resolved (2026-08-29)

Candidate 2, as expected — and the XRCE half of issue 0832 forced the same
change one project over, which is what settled it. `nros-rmw-xrce`'s shim
started allocating through the funnel, so its standalone project stopped
linking until it gained a platform implementation; the pattern that fixed it
transfers verbatim.

The cyclone project now resolves `NROS_PLATFORM_IMPL_DIR` (a cache PATH
defaulted to the sibling `nros-platform-posix`, the same convention
`CYCLONEDDS_SOURCE_DIR` and the header vars already use) and aliases it as
`NanoRos::Platform`, which is what the funnel gate asks for. Both guarded on
the targets not already existing, so a parent nano-ros build still wins.

Measured on `just check rmw-cyclonedds`:

```
-- nano-ros: CycloneDDS ddsrt heap -> nros_platform_alloc (issue 0832)
libddsc.a  heap.c.o -> U nros_platform_{alloc,realloc,dealloc}
           one object left on a raw libc allocator (sysdeps.c.o's free)
nros_rmw_cyclonedds_ros2_{sub,srv_client,srv_server}: 7 funnel call sites each
100% tests passed, 0 tests failed out of 22
```

Those three executables are the ones that caught the original missing link
dependency, so the lane is back to covering the shape it covered before the
dependency fix — this time with the funnel actually compiled, and end to end
rather than at link only.

Cost: the lane builds a platform archive it did not build before. Cyclone
itself was already built from source here, so there is no second Cyclone build
and the ~22 s figure in the recipe's own comment is unaffected in kind.

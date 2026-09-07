---
id: 1215
title: "The root CMakeLists is a FIFTH closed rmw list: `uorb` is offered by the cache UI and FATAL_ERRORs at configure"
status: open
area: cmake, build, rmw
severity: medium
found: 2026-09-08
related: [1214, 1216, 1218, RFC-0071]
---

# `NANO_ROS_RMW=uorb` is advertised and fatal

RFC-0071 Q3 named three disagreeing closed lists of RMW names and used the
disagreement as its strongest argument ("A closed enum has already stopped
covering the tree it governs"). phase-347 W3 reconciled those three onto
`NROS_RMW_KNOWN`. It missed the root, and the root is the one that decides.

## The two lines, 370 apart in one file

`CMakeLists.txt:27-29` — the advertised set, derived from the descriptors:

```cmake
set(NANO_ROS_RMW "zenoh" CACHE STRING
    "RMW backend — one of: ${NROS_RMW_KNOWN}")
set_property(CACHE NANO_ROS_RMW PROPERTY STRINGS ${NROS_RMW_KNOWN})
```

`CMakeLists.txt:398-402` — the accepted set, hand-written:

```cmake
else()
    message(FATAL_ERROR
        "Unknown NANO_ROS_RMW: '${NANO_ROS_RMW}' "
        "(expected: zenoh, xrce, or cyclonedds)")
endif()
```

The chain above it is `if(NANO_ROS_RMW STREQUAL "zenoh" OR ... "xrce")` at `:267`
and `elseif(... "cyclonedds")` at `:275`. There is no `uorb` arm.

## Measured

```
$ cmake -P probe.cmake     # include(cmake/NanoRosRmwDispatch.cmake)
-- PROBE nros_rmw_is_known(uorb) = TRUE
-- uorb target=nros_rmw_uorb extra=nros_rmw_uorb
```

`NROS_RMW_KNOWN` is `cyclonedds;uorb;xrce;zenoh`
(`cmake/NanoRosRmwDispatch.cmake:58`) and `nros_rmw_dispatch("uorb")` returns a
full row (`:24-32`). So `uorb` is:

* offered in the cmake-gui / `ccmake` drop-down for `NANO_ROS_RMW` (`:29`),
* named in the cache variable's own help string (`:28`),
* accepted by `cmake/NanoRosFeatureSet.cmake:122` (`nros_rmw_is_known`),
* handled by `packages/api/nros-cpp/CMakeLists.txt:270-286`,
* and a configure-time `FATAL_ERROR` at the repo root.

## Consequences beyond the error message

Because no arm ever runs `add_subdirectory(packages/rmw/uorb/nros-rmw-uorb)`, the
`nros_rmw_uorb` cmake target is never created, so the one consumer of
`NROS_RMW_CMAKE_TARGET` is permanently inert:

`packages/api/nros-cpp/CMakeLists.txt:283-284`

```cmake
if(NROS_RMW_CMAKE_TARGET AND TARGET ${NROS_RMW_CMAKE_TARGET})
    target_link_libraries(nros-cpp-headers INTERFACE ${NROS_RMW_CMAKE_TARGET})
endif()
```

`uorb` is the only backend that sets `NROS_RMW_CMAKE_TARGET`
(`NanoRosRmwDispatch.cmake:30`), and the `AND TARGET` guard is never satisfied.
This is the *only* route by which a cmake-provided backend reaches a link line
through the normal entry path, and it has never fired.

uorb reaches a binary only through the PX4 shell, which does not use this chain
at all: `integrations/px4/NanoRosPx4Module.cmake:275-293` compiles
`${NANO_ROS_ROOT}/packages/rmw/uorb/nros-rmw-uorb/src/*.cpp` directly into the
PX4 module via `SRCS`, with no `add_subdirectory` and no cmake target. (RFC-0071
Q3's description of this — "`add_subdirectory`s the backend and links the
`nros_rmw_uorb` target directly" — is stale; neither happens.)

`examples/fixtures.toml` carries 0 uorb rows against 242 zenoh / 75 cyclonedds /
43 xrce, so nothing in the matrix exercises the path that would have caught this.

## Why this is the load-bearing one

This is the line a pure-CMake/C++ provider hits. The provider mechanism itself is
sound — `packages/rmw/uorb/nros-rmw-uorb/` is a working zero-Rust C++ backend
(no `Cargo.toml`, no `.rs`, `add_library(... STATIC)` at `CMakeLists.txt:113`) —
but selecting a new one through `NANO_ROS_RMW` requires editing this `if/elseif`
chain, which is exactly the core edit RFC-0071 D5 undertook to make unnecessary.

## Fix direction (not applied)

The chain's three arms are three link strategies (bundled-into-umbrella /
add_subdirectory+whole-archive / cmake-target), and the descriptor already
declares which one a backend wants: `[rmw.provides.cargo]` vs
`[rmw.provides.cmake]` in `nros-rmw.toml`. Those tables are parsed and then
discarded — `packages/cli/cargo-nano-ros/build.rs:154` says
`[rmw.provides.*]` and `[rmw.codegen]` "are for later waves and are **ignored**".
Making the root chain read them is the shape of the fix; until then, note that
the drop-down advertises a value the configure rejects.

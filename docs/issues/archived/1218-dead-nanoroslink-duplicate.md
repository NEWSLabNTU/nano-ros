---
id: 1218
title: "The dead `packages/api/nros-c/cmake/NanoRosLink.cmake` misled two of four independent readers in one session — it holds a fourth closed rmw list and a force-link that does not happen"
status: resolved
area: cmake, docs
severity: medium
found: 2026-09-08
related: [1215, 1216, 0475, RFC-0071]
---

# A dead duplicate that reads as authoritative, with measured cost

Two files define the public verb `nano_ros_link_rmw`:

* `cmake/NanoRosLink.cmake` — **live**. Included by
  `cmake/platform/nano-ros-posix.cmake:24`, `nano-ros-threadx.cmake:61`,
  `nano-ros-nuttx.cmake:149`, `nano-ros-esp_idf.cmake:32`,
  `nano-ros-freertos.cmake:61` (all as `../NanoRosLink.cmake`, i.e. the root
  `cmake/` copy).
* `packages/api/nros-c/cmake/NanoRosLink.cmake` — **dead**. `grep -rn
  'nros-c/cmake/NanoRosLink'` over `*.cmake` / `CMakeLists.txt`, excluding
  `third-party/` and build dirs, returns nothing. Nothing includes it; it is not
  installed.

This is already recorded as audit finding **A1** in
`docs/development/audit-findings-2026-07-28.md` ("A dead 261-line duplicate of
`cmake/NanoRosLink.cmake` defining the same public verbs from a retired design
era, sitting beside four live modules so it reads as authoritative"). It is filed
here because the predicted harm has now been observed, and because the audit doc
is a snapshot rather than a tracked item.

## Measured harm

In one analysis session (2026-09-08), four independent readers were asked to
trace the RMW link path. **Two of the four** reported the dead file's contents as
the live mechanism, specifically:

* that `nano_ros_link_rmw` emits `-Wl,-u,nros_rmw_<name>_register` to force-link
  the backend archive — it is at
  `packages/api/nros-c/cmake/NanoRosLink.cmake:259` and **absent from the live
  file**;
* that adding a backend requires "a branch in `_nano_ros_rmw_targets`" — that
  function is at the dead file's `:148-162` and **exists nowhere else**.

Both statements are false of every build this repo runs, and both are the kind of
claim that ends up in a fix plan.

## What the dead copy uniquely contains

`packages/api/nros-c/cmake/NanoRosLink.cmake:148-162` — a **fourth** closed
rmw list, never counted by RFC-0071 Q3 and never reconciled by phase-347 W3:

```cmake
function(_nano_ros_rmw_targets OUT_PKG OUT_NAME RMW)
    if(RMW STREQUAL "zenoh")        ... NrosRmwZenoh
    elseif(RMW STREQUAL "xrce")     ... NrosRmwXrce
    elseif(RMW STREQUAL "cyclonedds") ... NrosRmwCyclonedds
    else()                          ... "" (silent)
```

`uorb` is absent and falls to the silent empty branch — the exact failure mode
RFC-0071 used as its motivating evidence, preserved in amber.

It also carries, at `:232-238`, a hardcoded pair of name special-cases:

```cmake
if((NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"
        OR NANO_ROS_PLATFORM STREQUAL "threadx_linux"
        OR NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
        AND _chosen STREQUAL "zenoh")
    return()
```

and the `find_dependency(${_pkg} CONFIG)` install-layout consumer path that
phase-140 deleted (the live file's header, `cmake/NanoRosLink.cmake:5-9`, says so).

## What the live copy has that the dead one does not

The 0475 fix and its 0837 generalisation:

* `cmake/NanoRosLink.cmake:101-104` — `LINK_DEPENDS
  "$<TARGET_FILE:nros_rmw_${_chosen}>"`, the file edge a raw `-Wl,` flag cannot
  carry.
* `cmake/NanoRosLink.cmake:125-134` — the `NANO_ROS_LINK_DEPEND_FILES` list
  walk, so any other file inside a raw flag gets an edge.

The dead file has **no `LINK_DEPENDS` at all**. Anyone repairing a stale-artifact
bug from it would be reading the pre-0475 design.

## Fix direction (not applied)

Delete `packages/api/nros-c/cmake/NanoRosLink.cmake`. If any of its content is
still wanted (the `-u` force-link is arguably the general mechanism the live file
lacks — see issue 1216), move that content into the live file in the same commit
rather than leaving the duplicate as its record.

## Resolution — deleted (2026-09-11, phase-444 W4.b)

`packages/api/nros-c/cmake/NanoRosLink.cmake` is gone. Re-verified before
deleting: **no `include()` anywhere reaches it** — every platform file includes
the LIVE `cmake/NanoRosLink.cmake` as
`${CMAKE_CURRENT_LIST_DIR}/../NanoRosLink.cmake` (esp_idf, freertos, nuttx,
posix, threadx), the only other mentions are prose in `CMakeLists.txt`,
`NanoRosRmwDispatch.cmake`, `NanoRosPx4Module.cmake` and the book, all naming
the live path, and `packages/api/nros-c/CMakeLists.txt` has carried no
`install()` rules since phase 140. Nothing of the dead copy's content was
moved, because none of it was reachable; issue 1216 (resolved) is where the
`-u` force-link question was settled.

It was also the fourth closed backend list issue 1219 counted, and the new
`check-rmw-agnostic` gate reported it — 4 code lines, `if(RMW STREQUAL
"zenoh")` / `"xrce"` / `"cyclonedds"` — on its first run. Deleting it is why
that gate starts with 13 baselined files rather than 14.

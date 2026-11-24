---
id: 2931
title: "A build dir configured against one nano-ros root kept that root's cached paths after a move to another, so an upgraded project compiled new sources against old headers"
status: resolved
type: bug
area: [cmake, build, sdk, release]
severity: high
found: 2026-11-24
resolved: 2026-11-24
related: [1280, 1593, 1840, 2912]
---

# A build dir kept the previous SDK's cached paths

## What happened

The installed upgrade trial (issue 2912's harness) failed for every C and C++
project:
1. Create a project under nros11.
2. `nros pin` it to nros12.
3. `nros configure --migrate`.
4. `nros build` fails in the metadata probe:

```text
sync: source metadata — probe FAILED for up_c_node::up_c_node:
/root/.nros/sdk/nros/0.0.0-probe/share/nano-ros/packages/platform/nros-platform-posix/src/platform.c:216:1:
error: unknown type name 'nros_wall_clock_state_t'
```

The checkout CLI did not show it.

## Cause

Platform crates' CMake projects default `NROS_PLATFORM_CFFI_INCLUDE` from
their own tree, written once:

```cmake
if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    set(NROS_PLATFORM_CFFI_INCLUDE "${_nros_platform_api_include}" CACHE PATH ...)
endif()
```

The probe build dir under `build/nros-metadata/` survives the pin. Its cache
still named nros11's `packages/platform/nros-platform-api/include`, and the
SDK store keeps nros11 after the move. So the only self-heal (taken when the
cached dir has no `nros/platform.h`) never fired. nros12's `platform.c` was
then compiled against nros11's `nros/platform.h`, which predates the
wall-clock state type.

Issue 1593 already re-pins `nano_ros_DIR` and `CMAKE_PREFIX_PATH` on every
probe configure, but those are two of the cache's tree paths. 70
`CACHE PATH`/`FILEPATH` definitions exist across `cmake/` and the
platform/RMW/board projects, most of them tree-derived defaults with the same
write-once shape. The same move breaks an image's own build dir by the same
route.

## Fix

Issue 1280's rule, applied to the CMake cache, with one helper for all 70:
`nros_reroot_cached_checkout_paths()` in `cmake/NanoRosCheckoutRoot.cmake`.
It runs at the top of every configure that loads nano-ros
(`nano_rosConfig.cmake`, and the root `CMakeLists.txt` for a bare
`add_subdirectory`). For each cached `PATH`/`FILEPATH` value:

| Where the value points | What happens |
| --- | --- |
| inside a DIFFERENT nano-ros root (deepest marker walk; an installed SDK ships the marker) | re-rooted onto this root, same sub-path, and named in one status line |
| outside every root (toolchains, SDKs in the store, a user's own tree) | kept |
| inside this root | kept |
| inside the consuming project's own source or build tree | kept |

So a release move, a checkout move and a worktree reusing another checkout's
build dir all converge, without wiping anything.

## Checked

- `check-inherited-checkout-paths` gained `probe_cmake_cache_reroot`, which
  runs in both checkout shapes (side by side, and nested as agent worktrees
  are). It configures a project against one root and then another, and
  asserts:
  - the tree default follows to the new root;
  - an outside path is kept;
  - a path in the project's own build tree is kept.
- Negative control: with the helper turned into a no-op, the gate fails on
  the tree default in both shapes, which is the state of `main` before this
  change.

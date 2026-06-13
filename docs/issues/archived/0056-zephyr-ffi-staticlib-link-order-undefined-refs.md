---
id: 56
title: Zephyr FFI message staticlibs link before their callers → undefined reference at final link
status: resolved
type: bug
area: zephyr
related: [issue-0052, phase-242, rfc-0044]
resolved_in: ASI FVP workspace-mode bring-up (zephyr/cmake/nros_generate_interfaces.cmake — whole-archive FFI .a)
---

## Symptom

The downstream Zephyr+C++ consumer (ASI FVP, Zephyr v3.7.0) compiled fully but
failed at the **final image link**:

```
…/nano_ros_cpp/nav_msgs/msg/nav_msgs_msg_odometry.hpp:52:
  undefined reference to `nros_cpp_deserialize_nav_msgs_msg_odometry'
…undefined reference to `nros_cpp_publish_*` / `nros_cpp_deserialize_*`
  for every message package.
```

The symbols exist (`nm libnano_ros_cpp_ffi_nav_msgs.a` → `T
nros_cpp_deserialize_nav_msgs_msg_odometry`) and all 10 FFI `.a`s are on the
link line — yet ld reports them undefined.

## Root cause

The generated message C++ headers call the
`nros_cpp_{serialize,deserialize,publish}_*` FFI symbols from **inline
functions**, compiled into the app objects and into the component library
(`nano_ros_node_register`). The Zephyr generator linked each FFI staticlib to
`app` as a plain library (`target_link_libraries(app PRIVATE ${ffi})`). On the
final link line the FFI `.a`s sit **before** the objects/lib that reference
them. GNU ld processes left→right and pulls only `.a` members that resolve a
currently-undefined symbol; references that appear later don't pull anything, so
the members are dropped → undefined at the end.

## Fix

Whole-archive the FFI staticlibs (order-independent — all members retained
regardless of link position). The FFI glue is small (per-message ser/de/publish),
so the size cost is acceptable. CMake 3.24's `$<LINK_LIBRARY:WHOLE_ARCHIVE,…>`
is unavailable on the Zephyr-pinned CMake (3.22), so raw flags are used, with an
explicit `add_dependencies(app <ffi>_build)` to keep the `.a` build edge (the
imported target is no longer link-listed):

```cmake
target_link_libraries(app PRIVATE
  "-Wl,--whole-archive" "${_ffi_lib}" "-Wl,--no-whole-archive")
add_dependencies(app ${_ffi_target_name}_build)
```

Verified: ASI FVP image links — `zephyr.elf` produced.

## Follow-up

Same drift root as [[issue-0052]]: the Zephyr FFI generator and the canonical
`cmake/NanoRosGenerateInterfaces.cmake` (which already whole-archives /
order-fixes its ffi+cyclonedds-ts libs) duplicate FFI-link logic. Funnel them.

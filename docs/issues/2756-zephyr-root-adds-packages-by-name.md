---
id: 2756
title: "The generated Zephyr application adds an image's packages sorted by name, not in <depend> order"
status: open
type: bug
severity: medium
area: [cli, zephyr, build]
related: []
found: 2026-11-19
---

## Measured

nano-ros 93368ca929, the downstream island workspace (branch
`nano-ros-next`). The island split its vendored Autoware controller library
into one package per upstream package, seven in all. Some of them depend on
each other through `<depend>`, for example `autoware_interpolation` on
`autoware_universe_utils`. A package configured on its own (the metadata
probe) adds a missing dependency itself:

```cmake
if(NOT TARGET autoware_universe_utils)
  add_subdirectory(../autoware_universe_utils autoware_universe_utils)
endif()
```

The FreeRTOS images configure. The Zephyr FVP image's configure fails with
CMake's "cannot create target ... because another target with the same name
already exists". The generated application adds `autoware_interpolation`
first, its fallback adds `autoware_universe_utils`, and then the
application adds `autoware_universe_utils` a second time.

The island works around it with an add-once guard at the top of each of
these packages (`if(TARGET <self>) return()`).

## Cause

`packages/cli/nros-cli-core/src/builder/west_app.rs`, in the function that
collects the application's subdirectories (`subdirs.sort()`, line 672 on
main). The `add_subdirectory` list is sorted by path so the file is stable,
and this discards the dependency order `package_closure` computed. The
CMake road's generated root states `ORDER_FROM_DEPENDS` and so adds a
package after the packages it depends on.

## Direction

Order the Zephyr application's subdirectories by `<depend>` (topological
order, with names as the tie-break so the file stays stable), as the CMake
road does.

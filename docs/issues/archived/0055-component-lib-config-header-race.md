---
id: 55
title: nano_ros_node_register component lib races the per-build config-header generation → stub #error on clean build
status: resolved
type: bug
area: cmake
related: [phase-242, rfc-0044, issue-0051]
resolved_in: ASI FVP workspace-mode bring-up (NanoRosNodeRegister.cmake — add_dependencies on nros_{cpp,c}_cargo_build)
---

## Symptom

On a **clean** Zephyr build, the component library produced by
`nano_ros_node_register` (e.g. ASI's `controller_pkg_controller_component`,
which compiles vendored Autoware C++) failed:

```
nros/nros_cpp_config_generated.h:32: error: #error "nros_cpp_config_generated.h
  must be supplied per-build by the build system…"
nros/guard_condition.hpp:119: error: 'NROS_GUARD_CONDITION_SIZE' not declared
```

Intermittent: it passed on an incremental build (the generated header already
existed) and failed on a fresh wipe — the classic ordering race.

## Root cause

`<nros/nros_cpp_config_generated.h>` / `<nros/nros_config_generated.h>` (storage
sizes etc.) are emitted as **byproducts of the nros-cpp / nros-c cargo builds**
into `${CMAKE_BINARY_DIR}/nros-rust/nros-{cpp,c}-generated`, which
`zephyr/CMakeLists.txt` prepends to the include path. The `app` target
`add_dependencies(app nros_cpp_cargo_build)` so it waits for them. But the
component library from `nano_ros_node_register` is a **separate**
`add_library(STATIC …)` with no such dependency — so on a clean build ninja can
schedule its TUs before the generators run, the include falls through to the
in-tree **stub** config header, and the stub `#error`s.

(This is why it looked like ccache: an incremental rebuild had the header from
a prior run, masking the missing edge.)

## Fix

Add the dependency in `cmake/NanoRosNodeRegister.cmake` when building the
component lib, matching what `app` already does:

```cmake
foreach(_nrc_gen_dep nros_cpp_cargo_build nros_c_cargo_build)
    if(TARGET ${_nrc_gen_dep})
        add_dependencies(${_lib} ${_nrc_gen_dep})
    endif()
endforeach()
```

Verified: clean wipe build of the ASI component lib compiles deterministically
(reached link stage, no stub `#error`).

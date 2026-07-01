---
id: 123
title: "`workspace-shadowing` template-smoke fails a sizes-header mirror race in its C++ FFI std_msgs shadow path (0088/0090/0114/0122 class)"
status: open
type: bug
area: cmake
related: [0088, 0090, 0114, 0122]
---

## Summary

The `shadowing` cell of the template compile-check smoke
(`scripts/build/compile-check-fixtures.sh`, template
`examples/templates/workspace-shadowing`) fails to compile with the per-build sizes-header stub
race — the same family as issues 0088 / 0090 / 0114 / 0122, but on the **C++ FFI std_msgs shadow**
path rather than the plain message library:

```
nros-c/include/nros/nros_generated.h:1779: error: 'PUBLISHER_OPAQUE_U64S' was not declared in this scope
  … SERVICE_SERVER_OPAQUE_U64S / SERVICE_CLIENT_OPAQUE_U64S / GUARD_HANDLE_OPAQUE_U64S /
    NROS_LIFECYCLE_CTX_OPAQUE_U64S …
nros-cpp/include/nros/publisher.hpp:285: error: 'class nros::Publisher<std_msgs::msg::Marker>'
    has no member named 'storage_'
```

i.e. a translation unit that instantiates `nros::Publisher<std_msgs::msg::Marker>` (and reads
`nros_generated.h`) compiled BEFORE Corrosion's `nros_c_config_header` / `nros_cpp_config_header`
mirror custom commands populated the per-build headers, so it read the in-tree `#error` stub.

This template builds its `std_msgs` (carrying `Marker.msg`) through `nros_workspace_interfaces()`
(the workspace-over-AMENT shadow path — log line `nros_workspace_interfaces: building std_msgs from
…/workspace-shadowing/src/std_msgs`) plus a `nano-ros-cpp-ffi-std_msgs` Rust FFI crate. The mirror
`OBJECT_DEPENDS` wiring that fixes the race for the plain message library
(`NanoRosGenerateInterfaces.cmake`, issues 0114/0122) and for entry sources
(`NanoRosEntry.cmake`) does not cover the TU in this shadow C++ FFI path.

## Environment / trigger

- Surfaces only when an **AMENT `std_msgs`** is present in the build env (this box has `/opt/ros`),
  so the shadow cell actually builds instead of skipping.
- Also required the in-tree `nros` CLI to be rebuilt to current source (`just setup-cli`) — the same
  rebuild that exposed the sibling pre-existing template drift (pure-c-workspace `launch`→
  `default_launch` + missing `rmw`/`domain_id`, since fixed).

## Direction

Extend the mirror-dependency wiring (an `add_dependencies` on `nros_{c,cpp}_config_header` + a hard
file-level `OBJECT_DEPENDS` on the mirrored headers) to the TU(s) produced by the
`nros_workspace_interfaces()` / `nano-ros-cpp-ffi-<pkg>` shadow path — mirroring the
`NanoRosGenerateInterfaces.cmake` fix (0122, which gates on the mirror target existing rather than a
platform name). Verify with `bash scripts/build/compile-check-fixtures.sh` on a box with an AMENT
`std_msgs` available.

## Context

Not a platform-fixture defect: all eight `build-test-fixtures` platform lanes
(native/nuttx/freertos/zephyr/qemu/stm32f4/threadx-linux/threadx-riscv64) build green. This is the
last residual in the template compile-check smoke after the pure-c-workspace schema fixes.

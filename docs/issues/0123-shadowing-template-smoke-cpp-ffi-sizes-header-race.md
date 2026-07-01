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

## Root cause — corrected: it is an INCLUDE-PATH ordering bug, NOT a build-order race

Initial triage read this as the 0088/0090/0114/0122 build-order race and four ordering fixes were
tried — ALL INEFFECTIVE (see below). Direct inspection of a failing
`build/cmake-fixtures/shadowing` shows why:

- The per-build sizes header **already exists** when `consumer.cpp` compiles
  (`build/cmake-fixtures/shadowing/cargo/nano-ros_1147c/nros-c-generated/nros/nros_config_generated.h`
  is present). So there is nothing to wait for — not a timing race.
- `nros_c-static`'s INTERFACE include dirs (`packages/core/nros-c/CMakeLists.txt:180`) are ordered
  correctly: the **mirror** dir `${CMAKE_CURRENT_BINARY_DIR}/include` (into which the
  `nros_c_config_header` custom target copies the real `nros/nros_config_generated.h`) is listed
  BEFORE the source `include` dir (which holds the `#error` stub).
- BUT for the workspace-shadowing **consumer** (a verbatim rclcpp exe that pulls nros-cpp only
  transitively through the `std_msgs__nano_ros_cpp` binding + `NanoRos::NanoRosCpp`), the
  `nros_c_config_header` / `nros_cpp_config_header` mirror custom targets **had not run** when
  `consumer.cpp` compiled — the mirror dir was still empty, so `#include
  "nros/nros_config_generated.h"` fell through to the next `-I` entry, the source `#error` stub →
  `*_OPAQUE_U64S undeclared` / `Publisher<M> no member storage_`.

So it IS the 0088-family race after all — but the standard remedy
(`add_dependencies(<consumer> nros_{c,cpp}_config_header)`) never took effect on this consumer.

### Fixes that DO NOT work (confirmed)

`add_dependencies(<consumer> nros_{c,cpp}_config_header / cargo-build_nros_{c,cpp})` via any hook —
the C++ FFI INTERFACE binding, its `_gen` codegen target, a `_nros_find_ros_msg_package` directory
DEFER, and `nano_ros_link_rmw(TARGET)` — all leave the error in place, because the problem is header
RESOLUTION, not build ORDER.

## Direction

Make `NanoRos::NanoRosCpp` (and the `<pkg>__nano_ros_cpp` binding) propagate the **per-build
generated-header include dir BEFORE the in-tree `packages/core/nros-{c,cpp}/include` dir** in their
`INTERFACE_INCLUDE_DIRECTORIES`, so any consumer — including a plain rclcpp exe — resolves the real
`nros/nros_config_generated.h` ahead of the `#error` stub. (Equivalently: stop shipping the stub on
a dir that can precede the real header, or make the mirror overwrite the in-tree copy.) Verify with
`bash scripts/build/compile-check-fixtures.sh` on a box with an AMENT `std_msgs` available.

## Context

Not a platform-fixture defect: all eight `build-test-fixtures` platform lanes
(native/nuttx/freertos/zephyr/qemu/stm32f4/threadx-linux/threadx-riscv64) build green. This is the
last residual in the template compile-check smoke after the pure-c-workspace schema fixes.

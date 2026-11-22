---
id: 2817
title: "The native carrier of an EXISTING_TARGET / nano_ros_auto_add_library registration ignores the library's usage requirements"
status: resolved
type: bug
area: [cmake, build]
severity: medium
found: 2026-11-22
resolved: 2026-11-22
related: [phase-583, rfc-0057, 2744, 0088]
---

**Home: [phase-583](../../roadmap/archived/phase-583-owned-entry-executables.md)**
(one carrier per EXECUTABLE).

## What fails

A C++ component registered on an existing library
(`nano_ros_auto_add_library(<lib>)` + `nros_components_register_node(<lib> ...
TYPED)`, i.e. `nano_ros_node_register(EXISTING_TARGET <lib>)`) whose library
has a PUBLIC include directory outside `<pkg>/include`, or a PUBLIC link, fails
to build on the `linux` board: the native carrier `<pkg>__<exe>` does not
compile.

Measured in a downstream safety-island workspace (nano-ros pin ee02c58931):
its `controller_pkg` registers `controller_lib` with `EXECUTABLE controller
TYPED`, and states, after the registration,

```cmake
target_include_directories(controller_lib PUBLIC
  "${CMAKE_CURRENT_SOURCE_DIR}/autoware_trajectory_follower_node/include")
target_link_libraries(controller_lib PUBLIC ${_AUTOWARE_DEPS} asi_common)
```

Its bringup declares `[image.linux] board = "linux"` (a host image for the
contract census). `nros build controller_bringup:freertos-posix` then stops in
the census's host build with

```
controller.hpp:35: fatal error: autoware/trajectory_follower_node/controller_node.hpp: No such file or directory
```

The generated `linux_entry` itself builds: it links the component library and
inherits its usage requirements. Only the carrier `controller_pkg__controller`
fails, and it fails the whole build.

## Why

`_nros_node_register_carrier_image` (`cmake/NanoRosNodeRegister.cmake`) builds
the carrier by `add_executable(<target> <entry TU> ${_NRC_SOURCES})`. In
EXISTING_TARGET mode `_NRC_SOURCES` is read off the existing target's SOURCES,
so the carrier compiles the library's sources a second time, with only

- `<pkg>/include` and `<pkg>/src` as include directories,
- `NROS_PKG_NAME`,
- `NanoRos::NanoRosCpp` (or `NanoRos::NanoRos`) and the directory's generated
  interface libs.

It never links the existing target, so it sees none of the library's
INCLUDE_DIRECTORIES, COMPILE_DEFINITIONS, compile features or links. The
library's own private definitions (`NROS_NODE_CLASS_NAME`,
`NROS_DECLARED_CALLBACK_GROUPS`) are missing from the carrier's copy of its
TUs too.

All four carrier call sites pass `${_NRC_SOURCES}` and share the bug: NuttX,
ThreadX, FreeRTOS and native (posix). The native one is reached by any C/C++
TYPED registration configured for posix, with or without DEPLOY, which is how a
host image of an RTOS system meets it.

## Reproduction

`packages/testing/nros-tests/fixtures/carrier_usage_reqs_ws` (row
`workspace-carrier-usage-reqs`): one C++
component, `cur_pkg::Probe`, whose library has a PUBLIC include directory
(`node/include`) and a PUBLIC link (`cur_vendor`), both stated after the
registration. Building its `linux` image fails on main:

```
cur_pkg/include/cur_pkg/Probe.hpp: fatal error: cur_pkg_node/probe_period.hpp: No such file or directory
```

## Expected

In EXISTING_TARGET mode the carrier compiles only its own TUs (the generated
entry, plus the app-config TU on FreeRTOS) and links the existing target, so it
inherits every usage requirement the library has, whenever it is stated. No
include directories are mirrored. Compiling the library's sources as well would
define every symbol twice. The Zephyr carrier already works this way: it links
the component library into `app`.

## Consumer workaround

State the usage requirements on the carrier when it exists:

```cmake
if(TARGET controller_pkg__controller)
  target_include_directories(controller_pkg__controller PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/autoware_trajectory_follower_node/include")
  target_link_libraries(controller_pkg__controller PRIVATE ${_AUTOWARE_DEPS} asi_common)
  target_compile_features(controller_pkg__controller PRIVATE cxx_std_17)
endif()
```

## Resolution

In EXISTING_TARGET mode `nano_ros_node_register` hands the carrier arms no
library sources (`_nrc_carrier_srcs` is empty), and
`_nros_node_register_carrier_image` links the existing target PRIVATE. The
carrier compiles only its entry TU (and the app-config TU on FreeRTOS) and
inherits the library's usage requirements, however late they are stated. A
fused registration keeps compiling its own SOURCES, as before. The gating is
unchanged: the native carrier is still built for any C/C++ TYPED registration
on posix.

On NuttX the board link already finds a component library on the carrier's
LINK_LIBRARIES (by its `NROS_COMPONENT_PKG_SYM`) and recompiles its SOURCES for
the target, so the carrier's component code arrives by that path now.

Measured: the fixture's linux image builds, and its carrier links
`libcur_lib.a` and prints `CARRIER_USAGE_REQS answer=42`
(`tests/carrier_usage_reqs.rs`). The ThreadX and NuttX typed-carrier rows
build, the ThreadX carrier compiles no `Talker.c` of its own and links
`libc_typed_carrier_lib.a`, and `typed_carrier_e2e` passes 4/4 (both boot and
publish under QEMU). The FreeRTOS arm, reached with the
mps2-an385 C talker plus `DEPLOY freertos` (no in-tree row registers one),
builds `freertos_c_talker__talker` linking `libtalker_lib.a`, and it boots to
its session open under QEMU.

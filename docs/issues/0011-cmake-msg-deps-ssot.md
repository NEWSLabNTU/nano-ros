---
id: 11
title: C/C++ examples do not use package.xml as single source of truth for message deps
status: open
type: tech-debt
area: cmake
related: []
---

Many C/C++ examples manually call `nros_generate_interfaces()` in
CMakeLists.txt with hardcoded package names and DEPENDENCIES. The intended
pattern is for `package.xml` to be the single source of truth, with
`nros_find_interfaces()` resolving deps via the AMENT index.

**Current state** (after the Wave A migration, commit `3fc6d0bca`):

Migrated to `package.xml` + `nros_find_interfaces()` and build-verified:
native C/C++ + workspace/template packages (pre-existing), **plus** FreeRTOS
C **and C++** (12, fixing the C++ regression), NuttX C (6), ThreadX-linux C
(6, + new `package.xml`), ThreadX-riscv64 C (6), and 5 native CMake
templates. `nros_find_interfaces()` was confirmed to work cross-compiled
(host `nros codegen resolve-deps` runs at configure time).

**Remaining**:

- **Zephyr C (blocked)** — the Zephyr module ships its own
  `nros_generate_interfaces` and never defines `nros_find_interfaces`.
  Closing this needs a `nros_find_interfaces` wrapper added to the **Zephyr
  module** (`zephyr/cmake/`), out of the examples scope. ← the main open item.
- **`examples/native/c/custom-msg`** — intentionally kept on
  `nros_generate_interfaces`: it is its own interface package, so
  `resolve-deps` against its `package.xml` emits nothing.
- **NuttX C++** is `package.xml`-driven via the `nano_ros_node_register` /
  `nano_ros_deploy` deploy pipeline (functional, not via an explicit
  `nros_find_interfaces()`); **ThreadX-linux C++** uses `find_package(std_msgs)`
  via `cmake/compat/NrosRclcppCompat.cmake`. Converging these onto
  `nros_find_interfaces()` is optional cleanup.

**To close**: add the Zephyr-module `nros_find_interfaces` wrapper and
migrate the Zephyr C examples; optionally converge the NuttX C++ /
ThreadX-linux C++ paths.

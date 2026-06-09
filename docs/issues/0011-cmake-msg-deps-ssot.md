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

**Remaining** (all optional / edge cases — the core C migration is done):

- ~~**Zephyr C**~~ **(done, `0a3b867a5`)** — added
  `zephyr/cmake/nros_find_interfaces.cmake` (a Zephyr-module wrapper that
  mirrors the native resolve-deps and delegates to the Zephyr
  emit-into-`app` generator), and migrated all 6 Zephyr C examples to the
  package.xml SSoT pattern (build-verified via `just zephyr build-fixtures`).
- **`examples/native/c/custom-msg`** — intentionally kept on
  `nros_generate_interfaces`: it is its own interface package, so
  `resolve-deps` against its `package.xml` emits nothing.
- **NuttX C++** is `package.xml`-driven via the `nano_ros_node_register` /
  `nano_ros_deploy` deploy pipeline (functional, not via an explicit
  `nros_find_interfaces()`); **ThreadX-linux C++** uses `find_package(std_msgs)`
  via `cmake/compat/NrosRclcppCompat.cmake`. Converging these onto
  `nros_find_interfaces()` is optional cleanup.

**RMW-variant verification — DONE.** The migrated C examples were
build-verified on the **non-zenoh** RMWs too: native C × {XRCE, CycloneDDS}
(12), ThreadX-linux/riscv64 C × CycloneDDS (8), Zephyr C × {XRCE, CycloneDDS}
(12) — **32/32 build clean**. Confirmed `nros_find_interfaces` (message
bindings from `package.xml`) coexists with the per-example CycloneDDS
IDL/descriptor path on every platform. (Intermittent failures during the
sweep were a parallel-build fs race, not a regression — see issue #19.)

**To close** (all optional): converge the NuttX C++ /
ThreadX-linux C++ paths onto `nros_find_interfaces()`; migrate the
`examples/templates/zephyr-byo/app` copy-out starter; build-verify the
`talker-aemv8r` C++ cyclonedds carve-out. The substantive migration (all C
examples across native/FreeRTOS/NuttX/ThreadX/Zephyr + native C/C++ + FreeRTOS
C++ + templates, on all three RMWs) is **done and build-verified** — what
remains is convergence of the two C++ holdouts (which already resolve deps
from `package.xml`, just via different mechanisms).

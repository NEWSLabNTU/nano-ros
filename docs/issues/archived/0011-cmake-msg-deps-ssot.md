---
id: 11
title: C/C++ examples do not use package.xml as single source of truth for message deps
status: resolved
type: tech-debt
area: cmake
related: [issue-0020]
resolved_in: "341c722f3 (+ the C-example + Zephyr-module-wrapper migration)"
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

**Resolved.** Every example now resolves message deps from its `package.xml`
— there is no longer a meaningful hardcoded holdout:

- All C examples (native/FreeRTOS/NuttX/ThreadX-linux/ThreadX-riscv64/Zephyr)
  + native C/C++ + FreeRTOS C++ + templates use `nros_find_interfaces()`,
  build-verified on all three RMWs (32/32 C variants — zenoh/XRCE/CycloneDDS).
- **ThreadX-linux C++** converged (`341c722f3`) — the `find_package` via the
  rclcpp-compat shim was scaffolding; swapped to `nros_find_interfaces()`,
  build-verified 6/6 zenoh.
- **NuttX C++** intentionally kept on the deploy pipeline — it uses the pure
  string-based API (no typed `*.hpp` include) and its real ELF is the
  cross-compile cargo FFI crate, so `nros_find_interfaces()` would only add an
  unused cross-FFI staticlib. Its deps are already `package.xml`-driven via
  system-codegen (the SSoT intent is met by a different, correct mechanism).

Documented exceptions (not holdouts): `examples/templates/zephyr-byo/app` (a
copy-out starter whose README intentionally shows the explicit form);
`native/c/custom-msg` (its own interface package — `resolve-deps` emits
nothing). Separate pre-existing bug surfaced during the C++ work:
ThreadX-linux C++ **CycloneDDS** fails to link (`undefined reference to
nros_rmw_cffi_register_named`) — see issue #20, reproduced with the pristine
form so not caused by this work.

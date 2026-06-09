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

**Current state**:

- **25** CMakeLists now use `package.xml` + `nros_find_interfaces()` — all
  native C/C++ examples, plus the workspace/template packages.
- **~45** still hardcode `nros_generate_interfaces(<pkg> DEPENDENCIES …)`:
  FreeRTOS C **and C++**, NuttX C (which carry an ignored `package.xml`),
  ThreadX-linux C, ThreadX-riscv64 C, Zephyr C, plus several templates.

Notable per-platform detail:

- **FreeRTOS C++** regressed back to the hardcoded
  `nros_generate_interfaces(... DEPENDENCIES ...)` form.
- **NuttX C++** is `package.xml`-driven, but via the
  `nano_ros_node_register` / `nano_ros_deploy` deploy pipeline rather than an
  explicit `nros_find_interfaces()` call.
- **ThreadX-linux C++** uses `find_package(std_msgs)` via
  `cmake/compat/NrosRclcppCompat.cmake`.

**To migrate**: Add `package.xml` to each remaining example declaring
`<depend>` on its message packages, and replace the manual
`nros_generate_interfaces()` calls with `nros_find_interfaces()`. Bring the
regressed FreeRTOS C++ example back onto the `package.xml` path, and
converge the NuttX C++ / ThreadX-linux C++ approaches onto the same
`nros_find_interfaces()` resolution where practical.

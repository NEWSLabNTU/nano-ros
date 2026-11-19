---
id: 2754
title: "A workspace CMake root adds every package, so an owned-entry image also builds another bringup's node packages and their component executables"
status: open
type: limitation
severity: medium
area: [cli, cmake, build]
related: [2433, 2750, 2751, 2752]
found: 2026-11-19
---

Part of the owned-entry run road theme (see 2750): an image whose
`[image.<id>] entry` names a hand-written C/C++ package. The other parts are
2750, 2751 and 2752.

## Measured

nano-ros 93368ca929, the downstream island workspace (branch
`nano-ros-next`): a controller bringup (one C++ component, `controller_pkg`,
plus its support packages) and a second bringup per test program, each with
owned-entry images on `freertos-posix` and `mps3-an536-freertos`.

`nros build unit_test_bringup:freertos-posix` generated a root with every
workspace package, the controller's included, and failed in it:

```
[ 99%] Building CXX object pkg/controller_pkg/CMakeFiles/controller_pkg.dir/nros-entry/main.cpp.o
.../src/controller_pkg/include/controller_pkg/controller.hpp:35:10: fatal error: autoware/trajectory_follower_node/controller_node.hpp: No such file or directory
gmake[2]: *** [pkg/controller_pkg/CMakeFiles/controller_pkg.dir/build.make:99: pkg/controller_pkg/CMakeFiles/controller_pkg.dir/nros-entry/main.cpp.o] Error 1
```

In that root the component's `nros_components_register_node(... EXECUTABLE
controller ...)` builds a standalone executable for it, which the
controller's own images never build. The reverse holds too: the
controller's roots add the test packages, whose executables are images of
other bringups.

- `nros build <image> --packages-up-to asi_unit_test` builds, but `nros run`
  takes no package selection and its implicit build reconfigures with every
  package.
- `nros run --base-paths ...` would scope it, but its forwarding is broken
  (2753).

The island works around it with a guard at the top of each package's
CMakeLists (`return()` when `NROS_BRINGUP_DIR` is another bringup's).

## Cause

`cmake_root_excluded` (`packages/cli/nros-cli-core/src/cmd/build.rs:6426`)
leaves out only framework entries and entries for other boards. Node
packages of another bringup, and owned entries of another bringup on the
same board, stay in.

## Direction

A root built for one bringup's images adds that bringup's closure: the
packages its components and owned entries depend on, transitively (the
closure the west road already uses, `package_closure`), not the whole
workspace.

---
id: 864
title: "The derived cmake root adds INTERFACE packages as subdirs, so a workspace
  that defines its own messages needs a ROS install to configure — tier 1 cannot
  build fixtures on a host without ROS"
status: resolved
type: bug
area: cli
related: [phase-383]
---

## Problem

`just build-test-fixtures lane=native` fails on a host with no ROS:

```
CMake Error at examples/workspaces/features/src/custom_msgs/CMakeLists.txt:11 (find_package):
  Could not find a package configuration file provided by "ament_cmake"
Error: configure failed: cmake -S build/posix-zenoh-native ... -DNROS_RMW=zenoh
```

`custom_msgs` is a **verbatim upstream ROS msg package** — `find_package(ament_cmake
REQUIRED)` on its first line — and deliberately so: its own header says it
"builds unchanged under `colcon build`", while the nano-ros build routes
`rosidl_generate_interfaces` through the nano-ros codegen pipeline instead. It is
a schema declaration, not something we configure.

The hand-written root knew that and left it out of `_ws_subdirs`, with the reason
written down:

```cmake
# `src/custom_msgs` is deliberately NOT a `_ws_subdirs` entry — it declares the
# schema only; the components carry the type name as a string and hand-encode CDR
```

phase-383 W10.a (`0e943eacf`) replaced that root with a derived one, and the
derivation's filter was only *"has a `CMakeLists.txt` and is not excluded"*. An
interface package has a `CMakeLists.txt`, so it came back as a subdir.

## Why it landed

**On a host WITH ROS this configures and looks correct.** `find_package(ament_cmake)`
succeeds, the package builds the ROS way, and nothing complains. The failure needs
a host with no ROS — which is exactly where tier 1 is contracted to run
("minutes, host only"), and where this repo's ROS deliberately lives in a
distrobox instead.

So the commit that introduced it could be verified green by its author and still
break the tier that matters. Same shape as the capability-skip class: an absent
dependency that silently changes what a check means.

## Fix

`builder::cmake_root` now skips interface packages, using the canonical ROS
marker (`<member_of_group>rosidl_interface_packages</member_of_group>`, plus the
`msg/` `srv/` `action/` directory probes for packages that carry schemas without
declaring it).

That predicate already existed **twice**, both spellings identical, in
`cmd::ws`. Rather than write a third — which is the thing CLAUDE.md's class rule
exists to prevent — it is now one function,
`nros_cli_core::interface_package::is_interface_package`, and both `cmd::ws`
sites call it. Consolidated as part of this fix rather than filed separately —
it has no life independent of the caller that needed a third spelling.

### Verified

* `nros build demo_bringup:native_c_custom_msg_talker` configures and links on a
  host with no ROS, all 21 packages.
* The custom-msg node packages still link, so the generated crate reaches its
  consumers — only the schema package's own configure is skipped, which is the
  intent.
* `cargo test -p nros-cli-core --lib interface_package` — 4 cases.

### Worth checking separately

The other in-tree interface packages (`examples/templates/local-msg-package`,
`workspace-shadowing`, `examples/native/c/custom-msg`) are not in a migrated
workspace root today. When their workspaces migrate, this filter is what keeps
them out; a test that asserts it for a migrated root would be better than the
per-build discovery that found this one.

# local-msg-package — Phase 210.A.4 fixture

Demonstrates the **ROS-convention codegen** Phase 210 ships:

* `src/local_msgs/` — a **verbatim** ROS 2 msg package (stock `package.xml` +
  stock `CMakeLists.txt` calling `rosidl_generate_interfaces(...)`). Zero
  nano-ros-specific lines in the package's files. The same directory builds
  unchanged under `colcon build`.

* `src/extra_msgs/` — second workspace msg pkg with `<depend>local_msgs
  </depend>`; proves topo-sort + cross-workspace deps.

* `src/consumer/` — a **verbatim** ROS 2 C++ consumer node, **pulling msgs
  from BOTH the workspace AND the AMENT_PREFIX_PATH** (the stock ROS
  install). Includes:
    * `local_msgs::msg::Greeting`  — workspace
    * `extra_msgs::msg::Echo`      — workspace (depends on local_msgs)
    * `geometry_msgs::msg::Point`  — AMENT (`/opt/ros/.../share/geometry_msgs`)
    * `sensor_msgs::msg::Imu`      — AMENT (transitively pulls geometry_msgs
                                      + std_msgs)

  All four pkgs resolve through the same `find_package(<pkg>)` call shape —
  the smart Find-stub walks the layered search path
  (`NROS_INTERFACE_SEARCH_PATH > AMENT_PREFIX_PATH > bundled`) and routes
  each pkg's codegen identically regardless of which layer it lived in.

* `CMakeLists.txt` (this dir) — the **only** nano-ros-specific file. Pulls
  nano-ros, points `NROS_INTERFACE_SEARCH_PATH` at `./src/`, includes
  `NrosRclcppCompat.cmake`, calls `nros_workspace_interfaces()` to bulk-
  build the workspace msg pkgs (one line instead of N
  `add_subdirectory(src/<pkg>)`), then `add_subdirectory(src/consumer)`.

## Build

```sh
cmake -B build -S .
cmake --build build -j
./build/src/consumer/consumer        # publishes on /greetings via zenoh
```

## What's exercised

| Phase 210 piece | Where |
|---|---|
| `rosidl_generate_interfaces(...)` wrapper (210.A.1) | `src/local_msgs/CMakeLists.txt` |
| Smart Find-stub (`_NrosFindRosMsgPackage`, 210.A.2) | `find_package(local_msgs)` in `src/consumer/CMakeLists.txt` |
| Per-pkg Find delegators (210.A.3) | `find_package(std_msgs)` |
| Workspace Find-stub auto-emit (210.A.4) | `NROS_INTERFACE_SEARCH_PATH=./src` → auto-emits `Findlocal_msgs.cmake` so the consumer resolves it |
| `nros_workspace_interfaces()` bulk + topo-sort (210.B.2) | `local_msgs` built before `extra_msgs` automatically |
| Mixed workspace + AMENT msg sources | `find_package(local_msgs)` + `find_package(geometry_msgs)` resolve identically; codegen wires both |
| Multi-level dep closure cache (`_NROS_PKG_<pkg>_GENERATED_RS_FILES`) | `sensor_msgs` FFI sees `std_msgs` types even though `std_msgs` was generated indirectly via `local_msgs` earlier in the configure pass |
| `${pkg}::${pkg}` upstream-shape link target | `target_link_libraries(consumer local_msgs::local_msgs)` |

## Cross-build parity

The `src/` tree is what you'd drop into a colcon workspace. To prove parity:

```sh
cd src
colcon build               # upstream ROS 2 build of the SAME source.
```

Both build systems compile the same `consumer.cpp` against the same
`local_msgs/msg/Greeting.msg`; the difference is just which RCL implementation
links in.

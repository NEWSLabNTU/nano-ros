# ament-workspace-port

A ported ROS 2 workspace, built for posix, FreeRTOS and Zephyr with the
package's own `CMakeLists.txt` unchanged (phase-518 W4).

`src/` holds two stock ament_cmake packages, written the way ROS 2 packages
are. Neither has a nano-ros line in it:

- `workspace_port_core`, a LIBRARY package (the shape of Nav2's `nav2_util`):
  `add_library(sequence SHARED ...)` linked with
  `target_link_libraries(sequence PUBLIC ${std_msgs_TARGETS})`, exported with
  `ament_export_targets` / `ament_package()`. Its header includes
  `std_msgs/msg/header.hpp`, which includes `builtin_interfaces`, and holds a
  `std::mutex`.
- `workspace_port_nodes`, which finds `workspace_port_core` with
  `find_package`, and has:
  - `include_directories(include)`, which the library's and the program's
    sources both need;
  - `add_library(counter_component SHARED ...)`, with a
    `target_compile_definitions` of its own and
    `ament_target_dependencies(counter_component rclcpp std_msgs workspace_port_core)`;
  - `add_executable(workspace_port src/main.cpp)`, linked to the library with
    the plain `target_link_libraries` signature;
  - a `main` that calls `rclcpp::init`, `rclcpp::spin` and `rclcpp::shutdown`.

Both packages add `-Wall -Wextra -Wpedantic -Werror -fPIC` to their whole
directory, as Autoware's and Nav2's build helpers do. On a board that
directory also compiles the image's own entry code.

Each of these shapes is one that ros2/demos `composition`, Nav2 or Autoware
uses, and each failed the FreeRTOS or the Zephyr build until phase-518. On a
board, nano-ros builds `add_executable` as an image with
`nano_ros_add_executable(... ROS2_MAIN)`. It builds the `SHARED` libraries
`STATIC`, and makes the packages' C++ hosted.

Each root sets `NROS_INTERFACE_SEARCH_PATH` to `../src` and calls
`nros_workspace_interfaces()` before adding the packages, which is what lets
`find_package(workspace_port_core)` resolve to the sibling package rather
than to an installed copy.

The node publishes `std_msgs/Int32` on `/workspace_port_topic` every 200 ms and
logs `workspace_port up` from its constructor.

## The three roots

Each directory beside `src/` is a root that configures the same two packages.

| root | how it brings nano-ros in | build |
| --- | --- | --- |
| `posix/` | `add_subdirectory(<nano-ros>)` plus the ament surface include | `cmake -S posix -B build -Dnano_ros_ROOT=<nano-ros> && cmake --build build` |
| `mps2-an385-freertos/` | `find_package(nano_ros)`; the board comes from `system.toml` | `nros build` in the directory |
| `zephyr/` | `find_package(Zephyr)` then `find_package(nano_ros)`; the board comes from `system.toml` | `nros sync && nros build` in the directory |

On Zephyr the target is `mps2/an385` under QEMU; `native_sim` cannot run a
ported node. `system.toml` names the board package `mps2-an385-zephyr`, which
lowers to `west build -b mps2_an385` and carries the QEMU network Kconfig, so a
copy of this template names nothing outside itself (issue 1782).

## Run

The router is ROS 2's `rmw_zenohd`, listening where the image's locator points
(`tcp/127.0.0.1:7447` for posix, Slirp's `10.0.2.2:7447` on FreeRTOS). From a
ROS 2 install:

```bash
ros2 topic echo /workspace_port_topic std_msgs/msg/Int32
```

`port_templates_e2e` runs all three builds, which are the `Workload::PortWorkspace` cells
of `matrix::CELLS`. A host `int32-sink` must receive the topic.

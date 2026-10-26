# ament-workspace-port

A ported ROS 2 workspace, built for posix, FreeRTOS and Zephyr with the
package's own `CMakeLists.txt` unchanged (phase-518 W4).

`src/workspace_port_nodes/` is written the way a stock ament_cmake package is.
It has no nano-ros line in it:

- `include_directories(include)`, which the library's and the program's
  sources both need;
- `add_library(counter_component SHARED ...)`, with a
  `target_compile_definitions` of its own and
  `ament_target_dependencies(counter_component rclcpp std_msgs)`;
- `add_executable(workspace_port src/main.cpp)`, linked to the library with
  the plain `target_link_libraries` signature;
- a `main` that calls `rclcpp::init`, `rclcpp::spin` and `rclcpp::shutdown`.

Each of these shapes is one that ros2/demos `composition` uses, and each failed
the FreeRTOS or the Zephyr build until phase-518. On a board, nano-ros builds
`add_executable` as an image with `nano_ros_add_executable(... ROS2_MAIN)`.
It builds the `SHARED` library `STATIC`, and makes the package's C++ hosted.

The node publishes `std_msgs/Int32` on `/workspace_port_topic` every 200 ms and
logs `workspace_port up` from its constructor.

## The three roots

Each directory beside `src/` is a root that configures the same package.

| root | how it brings nano-ros in | build |
| --- | --- | --- |
| `posix/` | `add_subdirectory(<nano-ros>)` plus the ament surface include | `cmake -S posix -B build -Dnano_ros_ROOT=<nano-ros> && cmake --build build` |
| `mps2-an385-freertos/` | `find_package(nano_ros)`; the board comes from `system.toml` | `nros build` in the directory |
| `zephyr/` | `find_package(Zephyr)` then `find_package(nano_ros)`; the board comes from `system.toml` | `nros sync && nros build` in the directory |

On Zephyr the board is `mps2-an385-zephyr`, run under QEMU. `native_sim`
cannot run a ported node. The board's QEMU network Kconfig is
`<nano-ros>/cmake/zephyr/mps2-an385.conf`. The fixture build adds it, and a
copy of this template must name it in `os_config` (issue 1782).

## Run

The router is ROS 2's `rmw_zenohd`, listening where the image's locator points
(`tcp/127.0.0.1:7447` for posix, Slirp's `10.0.2.2:7447` on FreeRTOS). From a
ROS 2 install:

```bash
ros2 topic echo /workspace_port_topic std_msgs/msg/Int32
```

`port_templates_e2e` runs all three builds, which are the `Workload::PortWorkspace` cells
of `matrix::CELLS`. A host `int32-sink` must receive the topic.

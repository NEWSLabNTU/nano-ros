# rclcpp-compat smoke

Integration test for the Phase 209 MVP-quartet (A `rclcpp_compat.hpp` + B
`NrosRclcppCompat.cmake` + C `rclcpp_components_compat.hpp` + D
`nros-diagnostic-updater`). Builds a ROS-2-idiom node — `rclcpp::Node` subclass
with `create_publisher`, `diagnostic_updater::Updater`, `rclcpp::spin_some` —
against nano-ros with **only one nano-ros-specific glue line per layer**:

| Layer | Stock-ROS-2 form | Nano-ros change |
|---|---|---|
| Source `.cpp` | `#include <rclcpp/rclcpp.hpp>` etc. | none — force-included by the compat module |
| Source `.cpp` | `#include "std_msgs/msg/int32.hpp"` | use `"std_msgs/std_msgs.hpp"` umbrella (closed by 209.E) |
| `CMakeLists.txt` | n/a | `set(NANO_ROS_PLATFORM …) + add_subdirectory(<nano-ros>)` |
| `CMakeLists.txt` | n/a | `include(<nano-ros>/cmake/compat/NrosRclcppCompat.cmake)` |
| `CMakeLists.txt` | `find_package(std_msgs)` resolves the msg | `nros_generate_interfaces(std_msgs LANGUAGE CPP)` (closed by 209.E) |
| `CMakeLists.txt` | `ament_auto_add_executable` etc. | unchanged |

Everything below the `include(NrosRclcppCompat.cmake)` line is an unmodified
stock ROS 2 `CMakeLists.txt`.

## Build + run (native posix, RMW=zenoh)

```bash
cd examples/templates/rclcpp-compat-smoke
cmake -B build -S . -DNROS_RMW=zenoh
cmake --build build -j

# In terminal 1 (host daemon — required for zenoh):
zenohd -l tcp/127.0.0.1:7447 &

# In terminal 2:
./build/rclcpp_compat_smoke
# Expect: "rclcpp_compat_smoke up; publishing std_msgs/Int32 on smoke_topic"
# Expect: published Int32s at ~10 Hz on `smoke_topic`.
# Expect: a `diagnostic_msgs/DiagnosticArray` on `/diagnostics` once per second.
```

Subscribe-side verify (any ROS 2 host or `nros listener` example):

```bash
# from a ROS 2 install:
ros2 topic echo /smoke_topic
ros2 topic echo /diagnostics
```

## What this proves

- A ported source compiles **with the rclcpp call shape preserved**.
- The cmake module wires `find_package(ament_cmake_auto)` / `find_package(rclcpp)`
  / `find_package(diagnostic_updater)` / `ament_auto_add_executable` /
  `ament_target_dependencies` / `ament_auto_package` through to nano-ros.
- `diagnostic_updater::Updater` publishes correctly through `rclcpp::Node`'s
  shim Publisher.
- The force-include carries `nros/rclcpp_compat.hpp` +
  `nros/rclcpp_components_compat.hpp` onto the compile so `#include
  <rclcpp/rclcpp.hpp>` resolves without a source edit.

## Known gaps (tracked by remaining 209 items)

- **209.E** (bulk codegen) — replaces the per-package
  `nros_generate_interfaces(...)` lines with the bulk
  `nros generate cpp --workspace <ws>` form and emits the upstream
  `<pkg>/msg/<name>.hpp` per-message header layout (so `#include
  <std_msgs/msg/int32.hpp>` resolves directly, matching what a ported ROS 2
  source already writes).
- **209.F** (yaml params bake) — for nodes that `declare_parameter<T>("name",
  default)`. This smoke doesn't exercise parameters yet.
- **209.G** (real port) — drops a real ROS 2 node (`topic_state_monitor`)
  through the same path and produces the book "porting a ROS 2 node" page.

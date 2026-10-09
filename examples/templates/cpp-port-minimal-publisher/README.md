# cpp-port-minimal-publisher — Phase 209.G iter 2

The canonical ROS 2 "minimal publisher" tutorial node
([source pattern](https://docs.ros.org/en/humble/Tutorials/Beginner-Client-Libraries/Writing-A-Simple-Cpp-Publisher-And-Subscriber.html)
from the upstream ROS 2 docs), **vendored unmodified**, building against
nano-ros through its own rclcpp / ament CMake surface.

The acceptance for 209 lands here: a normal ROS 2 C++ node compiles + links +
runs against nano-ros by **swapping the build glue + zero `#include` edits** —
not by rewriting the source.

## What changed vs an upstream ROS 2 package

The C++ source (`src/minimal_publisher.cpp`) is the upstream tutorial's
`minimal_publisher.cpp` **verbatim** — true as of 2026-09-04 (phase-417 stage 1).
It was NOT true when first written: three lines carried adaptations, and the
README claimed verbatim anyway. What made the claim true is that
`rclcpp::Publisher<T>::SharedPtr` / `rclcpp::TimerBase::SharedPtr` now resolve
(the nested aliases exist on the entity types) and `FixedString<N>` accepts a
`std::string`, so the two member declarations and the
`message.data = "Hello, world! " + std::to_string(count_++);` assignment are
upstream's own lines again.

**One of those two lines is now on a deprecation clock** (phase-430 W7,
2026-09-08). `rclcpp::TimerBase` is RETIRED: nano-ros has no timer hierarchy,
because the executor dispatches through a raw function pointer and a
polymorphic base would be a vtable nothing calls. The name survives one release
as a deprecated alias for `rclcpp::Timer` — which is precisely why this file is
still verbatim, and why `rclcpp::TimerBase::SharedPtr timer_;` now compiles with
a warning carrying the migration. When the alias goes, THIS FILE STOPS BEING
VERBATIM: the line becomes `rclcpp::Timer::SharedPtr timer_;`.

That is worth stating rather than quietly fixing, because it is a real datum
about the campaign: RFC-0089's rule is that a name we cannot honour must fail to
compile, and here it collides with this example's whole purpose. The deprecation
is the interval in which both can be true. The CMakeLists.txt's stock-ROS-2 shape
(`find_package(ament_cmake_auto)` / `ament_auto_add_executable` /
`ament_target_dependencies` / `ament_auto_package`) is **untouched**.

The only delta — three lines prepended:

```cmake
# 1) Pull nano-ros (NanoRos::NanoRos / NanoRosCpp / nros_generate_interfaces).
set(NANO_ROS_PLATFORM posix)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../.." nano_ros)

# 2) The ament / rclcpp surface: rclcpp / ament_cmake_auto / rclcpp_components.
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosAmentSurface.cmake")

# 3) Generate the message bindings the source includes (folded by 209.E).
nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
```

## Build + run

```bash
cd examples/templates/cpp-port-minimal-publisher
cmake -B build -S . -DNROS_RMW=zenoh
cmake --build build -j

ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd &
./build/minimal_publisher
# Publishing: 'Hello, world! 0'
# Publishing: 'Hello, world! 1'
# …
```

## Running the same source on an RTOS

The two directories beside `src/` build **the same `src/minimal_publisher.cpp`**
for a microcontroller (phase-482 W3). Each one is the whole port for its
platform: a `CMakeLists.txt`, the package's `package.xml`, a `system.toml`
naming the board, and on Zephyr the Kconfig fragments.

| directory | target | run with |
| --- | --- | --- |
| `mps2-an385-freertos/` | FreeRTOS on MPS2-AN385 (Cortex-M3), QEMU | `qemu-system-arm -M mps2-an385 -kernel build/minimal_publisher -nic user,model=lan9118,net=192.0.3.0/24,host=192.0.3.1` |
| `zephyr/` | Zephyr on `mps2/an385` (Cortex-M3), QEMU | `west build -b mps2/an385 zephyr -- -DCONF_FILE="prj.conf;prj-zenoh.conf;<nano-ros>/cmake/zephyr/mps2-an385.conf"` |

The line that makes it work is

```cmake
nano_ros_add_executable(minimal_publisher ../src/minimal_publisher.cpp ROS2_MAIN)
```

`ROS2_MAIN` says the sources are a ROS 2 program with its own
`int main(int argc, char** argv)`. A microcontroller has no C runtime that
calls `main` that way. The board's startup calls `nros_app_main` instead, so
the build renames the program's `main` in those sources only and generates the
one-line `nros_app_main` that calls it. It also gives the ported code the C++
standard library it was written against. On FreeRTOS it removes the toolchain's
`-ffreestanding` for this package, and on Zephyr `prj.conf` selects the full
libstdc++.

The router address is baked into the image, because a microcontroller has no
environment variables to read it from. On FreeRTOS that is the
`NROS_ENTRY_LOCATOR` cache variable; on Zephyr it is `CONFIG_NROS_ZENOH_LOCATOR`.
Under QEMU's user networking the host is `192.0.3.1` on FreeRTOS and `10.0.2.2`
on Zephyr.

Zephyr's `native_sim` is **not** a target for this. A ported program needs the
full C++ standard library. On `native_sim` Zephyr links its own C library, which
the host's libstdc++ cannot run on (the blocker phase-209 G.2 measured), so the
port uses a Cortex-M board instead.

`port_templates_e2e` runs all three builds: posix, FreeRTOS and Zephyr. They
are the `Workload::Port` cells of `matrix::CELLS`.

## Caveats found during this port (all in `book/.../porting-a-cpp-node.md`)

- nano-ros codegen emits message string fields as `rclcpp::FixedString<N>` (not
  `std::string`). Assignments stay one-liners (`message.data = s.c_str()`)
  but cross-package code may need a small adapter. **Tracked: 209.E should
  emit a `std::string`-compatible field type alongside FixedString.**
- The generated message umbrella is `"std_msgs/std_msgs.hpp"` (the nano-ros
  layout). Upstream uses `<std_msgs/msg/string.hpp>` (per-message header).
  **Tracked: 209.E (codegen emits the upstream layout too).**

Both are codegen-side, not surface-side. The rclcpp surface itself (Node,
Publisher, Subscription, Timer, init/shutdown/spin/ok, log macros, QoS,
diagnostic_updater) lands the source unchanged.

# Your own message package

You write a ROS 2 msg package **once**. The same `src/my_msgs/` directory
builds under both:

* a nano-ros build — `rosidl_generate_interfaces(...)` is intercepted by
  nano-ros's wrapper and routed through nano-ros codegen. **No ROS 2
  install needed** — even the `rclcpp`-shaped consumer below compiles
  against nano-ros's source-compat layer.
* `colcon build` — upstream `rosidl_default_generators` produces the
  upstream-ROS bindings (this half, and only this half, needs a ROS 2
  install; it is the optional cross-build proof at the end).

Different build systems, identical source tree. No nano-ros-specific
files in the msg package.

> **Where this fits.** This chapter uses ROS 2 vocabulary
> (`ament_cmake`, `rosidl`, `rclcpp`) because its point is that stock
> ROS shapes work unchanged. If you came straight from
> [First Project](first-project.md) and none of that means anything
> yet, you can skim to "The Rust side" below or skip ahead to
> [First Node — Rust](first-node-rust.md) and return later. The
> standalone tree here is for clarity — in your scaffolded `my_robot`
> workspace the same drop-in applies: put `src/my_msgs/` next to your
> node packages.

## The msg package — stock ROS shape

Drop a verbatim ROS msg pkg under `src/` of your workspace:

```
src/my_msgs/
├── package.xml
├── CMakeLists.txt
└── msg/
    └── MyMsg.msg
```

`src/my_msgs/package.xml`:

```xml
<?xml version="1.0"?>
<package format="3">
  <name>my_msgs</name>
  <version>0.1.0</version>
  <description>My ROS 2 msg package</description>
  <maintainer email="you@example.org">you</maintainer>
  <license>Apache-2.0</license>

  <buildtool_depend>ament_cmake</buildtool_depend>
  <depend>std_msgs</depend>
  <build_depend>rosidl_default_generators</build_depend>
  <exec_depend>rosidl_default_runtime</exec_depend>

  <member_of_group>rosidl_interface_packages</member_of_group>

  <export>
    <build_type>ament_cmake</build_type>
  </export>
</package>
```

`src/my_msgs/CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.20)
project(my_msgs)

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
find_package(std_msgs REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
    msg/MyMsg.msg
    DEPENDENCIES std_msgs
)

ament_export_dependencies(rosidl_default_runtime)
ament_package()
```

`src/my_msgs/msg/MyMsg.msg`:

```
std_msgs/Header header
string payload
int32 sequence
```

Zero nano-ros-specific lines. Run `colcon build` from this dir — upstream
ROS produces a working msg package. Drop the same dir into a nano-ros
build (below) — nano-ros codegen produces the equivalent bindings.

## The consumer — stock ROS shape

```cpp
// src/my_app/src/my_app.cpp
#include <chrono>
#include <memory>

#include <rclcpp/rclcpp.hpp>
#include "my_msgs/msg/my_msg.hpp"

using namespace std::chrono_literals;

class MyNode : public rclcpp::Node {
public:
    MyNode() : rclcpp::Node("my_node") {
        publisher_ = this->create_publisher<my_msgs::msg::MyMsg>("topic", 10);
        timer_ = this->create_wall_timer(500ms, [this]() {
            my_msgs::msg::MyMsg m;
            m.payload = "hello";
            publisher_->publish(m);
        });
    }
private:
    std::shared_ptr<rclcpp::TimerBase> timer_;
    std::shared_ptr<rclcpp::Publisher<my_msgs::msg::MyMsg>> publisher_;
};

int main(int argc, char* argv[]) {
    rclcpp::init(argc, argv);
    rclcpp::spin(std::make_shared<MyNode>());
    rclcpp::shutdown();
    return 0;
}
```

`src/my_app/CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.20)
project(my_app LANGUAGES CXX)

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)
find_package(my_msgs REQUIRED)
find_package(std_msgs REQUIRED)

add_executable(my_app src/my_app.cpp)
target_link_libraries(my_app
    rclcpp::rclcpp
    my_msgs::my_msgs
    std_msgs::std_msgs
)

ament_package()
```

## The nano-ros umbrella — the only nano-ros-specific file

The two `src/` packages are stock ROS. One umbrella CMakeLists.txt at the
workspace root pulls nano-ros in:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_workspace LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 14)

# Pull nano-ros.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory(/path/to/nano-ros nano_ros)

# Point the smart Find-stub at this workspace (must precede the compat
# include so the workspace Find<pkg>.cmake auto-emit picks it up).
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_SOURCE_DIR}/src")

# Pull the rclcpp source-compat layer (find_package(rclcpp) etc.).
include(/path/to/nano-ros/cmake/compat/NrosRclcppCompat.cmake)

# Bulk-build every workspace msg pkg in topo order. One line, no
# add_subdirectory(src/<pkg>) per pkg.
nros_workspace_interfaces()

# Build the consumer app.
add_subdirectory(src/my_app)
```

Build:

```sh
cmake -B build -S .
cmake --build build -j
./build/src/my_app/my_app
```

## The Rust side

The same msg package feeds a Rust node with no CMake involved: declare
it in your node package's `package.xml` (`<depend>my_msgs</depend>`)
and manifest (`my_msgs = { path = "../../generated/my_msgs", version =
"0.0.0", default-features = false }` — workspace-level `generated/`,
constant `0.0.0` version), then run `nros sync` from the workspace
root — the package index finds `src/my_msgs/` by its `package.xml`,
generates the `generated/my_msgs` crate, and wires the cargo patch
table. The shipped precedent is
`examples/workspaces/features/src/custom_msgs/` with its
`rust_reading_listener_pkg` consumer. Full
detail: [Message Generation](../user-guide/message-generation.md).

```rust,ignore
use my_msgs::msg::MyMsg;   // generated crate, same field names
```

## Cross-build proof — same source under colcon (optional; needs ROS 2)

```sh
cd src && colcon build
```

`src/my_msgs/` produces the upstream `my_msgs` bindings; `src/my_app/`
links against the upstream `rclcpp` + `my_msgs::my_msgs`. The
nano-ros build above produces the same source linked against
`NanoRos::NanoRosCpp` through the nano-ros codegen, with the smart
Find-stub forwarding `find_package(my_msgs)` → `my_msgs::my_msgs`.

## The interface-package search path

`find_package(<pkg>)` walks three layers, highest priority first:

| Layer | Source | Notes |
|---|---|---|
| 1 | `NROS_INTERFACE_SEARCH_PATH` | Colon/semicolon-separated colcon-`src/`-style roots; immediate subdirs with `package.xml` are candidates. |
| 2 | `AMENT_PREFIX_PATH` | The standard ROS install-prefix layout (`<prefix>/share/<pkg>/{msg,srv,action}/`). |
| 3 | Bundled | `<nano-ros>/packages/cli/interfaces/<pkg>/` (in-tree bundled definitions). |

Shadowing — a workspace `my_msgs` and an AMENT `my_msgs` resolve to the
workspace one, with a `message(STATUS ...)` line noting the shadow.

## Shadowing contract

When two layers carry the same package name (e.g. a workspace
`std_msgs` and `/opt/ros/<distro>/share/std_msgs/`), the **higher
layer wins** — silently and deterministically. Concretely:

1. **`NROS_INTERFACE_SEARCH_PATH` > `AMENT_PREFIX_PATH` > bundled.**
   The smart Find-stub
   (`cmake/compat/stubs/_NrosFindRosMsgPackage.cmake`) walks the
   three layers in order; the first hit wins. Lower layers are
   skipped entirely.

2. **The configure pass emits a `message(STATUS ...)` line** noting
   the resolved path, e.g.

   ```
   -- nros: find_package(std_msgs) -> /path/to/workspace/src/std_msgs
   ```

   Grep your configure log for `nros: find_package(<pkg>)` to confirm
   which layer supplied each pkg.

3. **Intra-workspace shadowing** — when two roots under
   `NROS_INTERFACE_SEARCH_PATH` both ship the same pkg, the
   `nros_workspace_interfaces()` bulk orchestrator keeps the
   earlier-listed one and warns about the shadowed copy.

4. **No partial overrides.** Shadowing is whole-package: a workspace
   pkg replaces ALL of an AMENT pkg's interfaces, not a subset. If
   you only want to add `MyExtraMsg.msg` to `std_msgs`, ship a
   separate pkg (e.g. `my_std_extra_msgs`) — don't shadow.

### Compile-time fail-safe

Shadowing is observed at compile time: if your workspace `std_msgs`
declares a `Marker.msg` and your consumer includes
`"std_msgs/msg/marker.hpp"`, the build only succeeds when the
workspace copy is the one linked. AMENT's `std_msgs` ships no
`Marker.msg`, so a fall-through resolves to a missing header. The
build outcome is the strongest evidence — `nm` on the linked binary
is the symbol-level corroborator.

### Reference fixture for shadowing

The canonical smoke proof for the Layer 1 > Layer 2 case ships at
[`examples/templates/workspace-shadowing/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/workspace-shadowing) —
a workspace `std_msgs` carrying a `Marker.msg` shadows the
AMENT-installed `std_msgs`. The fixture's `README.md` walks through
the `cmake` + `nm` verification. A regression test
(`packages/testing/nros-tests/tests/workspace_shadowing.rs`)
re-runs the same proof in CI when AMENT is sourced.

## Reference fixture

[`examples/templates/local-msg-package/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/local-msg-package)
ships the exact pattern above end-to-end (two workspace msg pkgs with a
dep between them, plus a consumer node). Use it as a copy-out template.

## Next

- Something broke on the way here? [Troubleshooting — First 10
  Minutes](./troubleshooting-first-10-min.md).
- Single-node deep dive: [First Node — Rust](./first-node-rust.md).
- Grow the workspace: [Node packages](workspace-node-pkgs.md).

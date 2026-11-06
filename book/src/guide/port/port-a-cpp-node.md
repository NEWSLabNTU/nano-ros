# Port a C++ node

<!-- type: how-to · level: 3 -->

**Needs first:** [Move a ROS 2 project over](move-a-ros2-project.md)

Port a C++ node when you have one that builds with `colcon build` against ROS 2
Humble and you want it on nano-ros, with as few source edits as possible.

nano-ros declares the `rclcpp::` names in its own C++ headers, and its SDK
root carries the CMake side: the `ament_*` verbs, `Find<pkg>.cmake` modules
for the most-cited ROS 2 packages, and `diagnostic_updater`. So a port is
mostly build glue. What the source still needs is what the compiler names:
nano-ros has no exceptions, and a few handles observe rather than own.

## Before you start

- nano-ros installed, the host provisioned with `nros setup linux`, and ROS 2
  sourced for the first configure.
- The package's message dependencies reachable through `AMENT_PREFIX_PATH`.
  For a project with its own message packages, source the project's install
  too (`source <project>/install/setup.bash`); nano-ros generates its own
  bindings from those `.msg` files.

## Steps

1. Keep the package's `CMakeLists.txt` as it is. It needs no nano-ros line:

   ```cmake
   cmake_minimum_required(VERSION 3.20)
   project(my_node LANGUAGES CXX)

   find_package(ament_cmake REQUIRED)
   find_package(rclcpp REQUIRED)
   find_package(std_msgs REQUIRED)

   add_executable(my_node src/my_node.cpp)
   ament_target_dependencies(my_node rclcpp std_msgs)

   ament_package()
   ```

   `find_package(rclcpp)` resolves to nano-ros's find module, which wins over
   an installed ROS 2 even when ROS 2 is sourced, because CMake tries modules
   first. Every target an ament verb creates, or that links `rclcpp::rclcpp`,
   is built with `NROS_CPP_STD`, the `std::shared_ptr`-shaped API ported code
   expects.

2. Add one root `CMakeLists.txt` beside `src/`. It is the only file that
   names nano-ros:

   ```cmake
   cmake_minimum_required(VERSION 3.22)
   project(my_workspace LANGUAGES CXX)
   set(CMAKE_CXX_STANDARD 14)

   set(NROS_PLATFORM posix)
   set(NROS_RMW zenoh)
   add_subdirectory("${NANO_ROS_SDK}" nano_ros)

   set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_SOURCE_DIR}/src")
   include("${NANO_ROS_SDK}/cmake/NanoRosAmentSurface.cmake")
   nros_workspace_interfaces()     # every message package in src/, in order

   add_subdirectory(src/my_node)
   ```

   Add each package before the packages that `find_package` it; the
   configure refuses an out-of-order find by name.

3. Configure and build:

   ```bash
   source /opt/ros/humble/setup.bash
   cmake -B build -DNANO_ROS_SDK="$(nros locate nano-ros)"
   cmake --build build
   ```

4. Fix what the compiler names. Most stock nodes need no edit. These are the
   two most common, from the stock ros2/examples ports:

   A global `Node::SharedPtr` assigned from a temporary does not compile,
   because nano-ros's `SharedPtr` observes the node rather than owning it.
   Keep the node in a local first:

   ```cpp
   // Before
   g_node = rclcpp::Node::make_shared("minimal_service");
   // After
   auto node = rclcpp::Node::make_shared("minimal_service");
   g_node = node;
   ```

   A `try`/`catch` does not compile, because nano-ros is built
   `-fno-exceptions`. A failed call reports a result instead:

   ```cpp
   // Before
   try {
     publisher->publish(message);
     rclcpp::spin_some(node);
   } catch (const rclcpp::exceptions::RCLError & e) {
     RCLCPP_ERROR(node->get_logger(), "unexpectedly failed with %s", e.what());
   }
   // After
   publisher->publish(message);
   rclcpp::spin_some(node);
   ```

   A node you construct by hand, outside a generated entry, cannot throw
   either: check `node.ok()` after constructing it. The other spellings and
   what each maps to are in
   [rclcpp compatibility](../../reference/rclcpp-compatibility.md).

5. Run it against the router your ROS 2 peers use:

   ```bash
   NROS_LOCATOR=tcp/127.0.0.1:7447 ./build/src/my_node/my_node
   ```

6. Size the pools for a large node. A package built this way declares no
   entity counts, so each pool starts at its embedded default and start-up
   stops at the first one that fills, naming its knob. In a nano-ros
   workspace, let the census count them: state `entities = "census"` on the
   component row, and the configure runs the program once on the host and
   sizes the pools from what it created:

   ```toml
   [[component]]
   pkg = "my_pkg"
   class = "my_pkg::MyNode"
   name = "my_node"
   entities = "census"
   ```

   Otherwise set the knobs in the configure's environment. Autoware's
   `autoware_external_cmd_selector` (12 subscriptions, 7 publishers, a
   service) needed:

   ```bash
   export NROS_EXECUTOR_MAX_CBS=40 NROS_ZENOH_MAX_LIVELINESS=128 \
          NROS_ZENOH_MAX_PUBLISHERS=32 NROS_ZENOH_MAX_SUBSCRIBERS=32 \
          NROS_ZENOH_MAX_QUERYABLES=32
   ```

7. To run it on a board, add a per-platform directory beside the source with
   a `system.toml` naming the board, and build it with `nros build`:

   ```cmake
   find_package(nano_ros REQUIRED)
   nano_ros_add_executable(minimal_publisher ../src/minimal_publisher.cpp ROS2_MAIN)
   ```

   No C runtime calls `main` on a board, so the build renames your `main` and
   calls it from the platform's entry, with `argc == 0`. `ROS2_MAIN` gives
   the package the C++ standard library (on Zephyr, also set
   `CONFIG_REQUIRES_FULL_LIBCPP=y` and `CONFIG_GLIBCXX_LIBCPP=y` in
   `prj.conf`) and the `NROS_CPP_STD` API. A board has no command line, so
   state the launch file's parameters on the image:

   ```toml
   [image.<id>]
   params_files = ["config/x.param.yaml"]
   ros_args = ["--ros-args", "-p", "update_rate:=10.0"]
   ```

   Zephyr's `native_sim` cannot run a ported node; use a Cortex-M board such
   as mps2-an385 in QEMU.

## Example

[`examples/templates/cpp-port-minimal-publisher/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/cpp-port-minimal-publisher)
vendors the ROS 2 tutorial's `minimal_publisher.cpp` unmodified and builds it
through the steps above, on Linux and, from its `mps2-an385-freertos/` and
`zephyr/` directories, on two boards. Its package `CMakeLists.txt` is the
stock `ament_cmake_auto` shape:

```cmake
find_package(ament_cmake_auto REQUIRED)
find_package(rclcpp REQUIRED)
ament_auto_find_build_dependencies()

ament_auto_add_executable(minimal_publisher src/minimal_publisher.cpp)
ament_target_dependencies(minimal_publisher rclcpp)

ament_auto_package()
```

## What still needs an edit

- **Exceptions.** Guard a `throw` with `#if defined(__cpp_exceptions)` and
  abort in the other arm.
- **`MultiThreadedExecutor`.** A compile error naming the alternative:
  parallelism is a sched task
  ([Pick a scheduling model](../realtime/pick-a-scheduling-model.md)), not a
  callback group. Linux scheduling calls such as `sched_setscheduler` become
  no-ops on a board, where the sched task's priority applies.
- **A thread per action goal.** Run the goal from the accepted callback; a
  goal handle used from a thread you start races the executor.
- **Unbounded strings and sequences** in a message you subscribe to stop the
  compile. Bound them in a `nros-codegen.toml` at the workspace root:

  ```toml
  [fields]
  "std_msgs/String.data" = 256
  ```

- **Message strings** are `rclcpp::FixedString<N>`, not `std::string`: assign
  with `message.data = s.c_str()`.
- **Out of scope:** `tf2`, `image_transport`, `pluginlib`, `class_loader` and
  `dlopen` composition, and generic publishers by type name alone (pass the
  type hash: `create_generic_publisher(topic, type, type_hash, qos)`).
  Parameters are not persisted across boots.

## If it doesn't work

- **`find_package(<pkg>)` is refused, naming an installed file.** nano-ros
  does not ship that upstream library (`bondcpp`, `tf2_ros`). Remove the
  dependency, or port the library into your workspace as a package.
- **A node's publishers silently do nothing.** It failed to construct, and
  without exceptions nothing said so. Check `node.ok()` after a hand-written
  construction.
- **A stored timer or subscription keeps running after you drop it.** Its
  `SharedPtr` is a handle, not an owner. Call `timer_.reset()` to release it;
  `timer_->reset()` restarts it, as upstream.
- **`${CMAKE_SOURCE_DIR}` points at the wrong directory.** Every package
  configures in one CMake run, so it is the workspace root. Use
  `${PROJECT_SOURCE_DIR}`.

## See also

- [rclcpp compatibility](../../reference/rclcpp-compatibility.md):
  every upstream spelling and what it maps to, and the measured ports.
- [How nano-ros differs from ROS 2](../../background/differences-from-ros2.md).
- [Message generation](../../reference/message-generation.md#which-cmake-spelling-one-table):
  the CMake spellings for message bindings.

## Design background

- [RFC-0089: ROS 2 API adoption, and the compile-or-conform rule](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)
- [RFC-0018: C++ API design](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0018-cpp-api-design.md)
- [RFC-0033: message field capacity](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0033-message-field-capacity-config.md)

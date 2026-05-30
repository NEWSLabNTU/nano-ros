# Porting a ROS 2 C++ node to nano-ros

Goal: take a normal ROS 2 C++ node (one that compiles + runs under
`colcon build` against `ros-humble-*`) and run it under nano-ros — without
rewriting the source. The Phase 209 compat layer (`nros/rclcpp_compat.hpp` +
`cmake/compat/NrosRclcppCompat.cmake` + `nros-diagnostic-updater`) is built for
this; the only delta is **build-script glue**.

The canonical proof lives at
[`examples/templates/cpp-port-minimal-publisher/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/cpp-port-minimal-publisher) —
the ROS 2 tutorial's `minimal_publisher.cpp` vendored unmodified, building
against nano-ros via the three glue lines below.

## The three-line glue

In your ported package's `CMakeLists.txt`, **before** the stock
`find_package(ament_cmake_auto REQUIRED) / ament_auto_*` block:

```cmake
# 1) Pull nano-ros into the build (NanoRos::NanoRos / NanoRosCpp + the codegen).
set(NANO_ROS_PLATFORM posix)         # or zephyr / freertos / nuttx / threadx
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("/path/to/nano-ros" nano_ros)

# 2) Drop-in source-compat for rclcpp / ament_cmake_auto / rclcpp_components /
#    diagnostic_updater. Everything below this line is unmodified stock ROS 2.
include("/path/to/nano-ros/cmake/compat/NrosRclcppCompat.cmake")

# 3) Generate the message bindings the source includes. (Phase 209.E folds
#    this into one bulk `nros generate cpp --workspace <ws>` call; today it's
#    one line per package.)
nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
# … plus any other msg packages the source `#include`s.
```

Below that block, the original `find_package(rclcpp) / find_package(<msg-pkg>)
/ ament_auto_add_executable / ament_target_dependencies / ament_auto_package`
stays unchanged.

## What "just works" without source edits

The compat surface covers the patterns a typical ROS 2 C++ node uses:

| rclcpp surface | nano-ros mapping | Notes |
|---|---|---|
| `class MyNode : public rclcpp::Node` | `rclcpp::Node` shim → `nros::Executor` + `nros::Node` | Ctor takes `(name)`. |
| `std::make_shared<MyNode>()` | inherits `enable_shared_from_this` | `shared_from_this()` works. |
| `create_publisher<M>(topic, qos)` | shared_ptr-returning wrapper | `qos` can be `rclcpp::QoS(10)` or an int. |
| `create_subscription<M>(topic, qos, callback)` | polling pump dispatched from `spin*` | **Capturing lambdas + `std::function` all work** (Phase 209.A.follow-up). |
| `create_wall_timer(period, callback)` | wall-timer dispatched from `spin*` | `std::chrono::duration` arg, capturing-lambda callback. |
| `rclcpp::init(argc, argv) / shutdown() / ok() / spin(n) / spin_some(n)` | wraps `nros::init/shutdown/ok/spin_once` | argc/argv ignored. |
| `RCLCPP_INFO / WARN / ERROR / DEBUG / FATAL` | dispatched through `NROS_*` macros | `_THROTTLE` variants degrade to plain log. |
| `rclcpp::QoS / KeepLast(n) / SystemDefaultsQoS()` | subclass of `nros::QoS` with the `(depth)` ctor | Chainable setters inherited. |
| `diagnostic_updater::Updater` + `DiagnosticStatusWrapper` | `nros-diagnostic-updater` shim (Phase 209.D) | Publishes `/diagnostics`. |
| `rclcpp_action::Server<A> / Client<A>` | aliases for `nros::ActionServer/Client<A>` | The action call shapes (send_goal_async etc.) match. |
| `RCLCPP_COMPONENTS_REGISTER_NODE(class)` | no-op macro + cmake-side `rclcpp_components_register_node()` emits a thin `int main()` per registration | Single-binary embedded. |
| `find_package(ament_cmake_auto / rclcpp / rclcpp_components / diagnostic_updater / std_msgs / …)` | Find-stubs at `cmake/compat/stubs/` | ~24 of the most-cited ROS 2 packages stubbed; add your own under `cmake/compat/stubs/Find<pkg>.cmake` for more. |

## What's documented as "needs adapt" (codegen-side, not surface-side)

These are not source-compat regressions — they're cosmetic codegen
differences nano-ros's per-package codegen and the upstream
`rosidl_default_runtime` codegen don't yet share. Both are tracked under
Phase 209.E (bulk codegen with the upstream layout).

- **Message string fields.** nano-ros codegen emits `nros::FixedString<N>`,
  upstream emits `std::string`. Assigning a `std::string` needs a one-token
  adapter: `message.data = s.c_str()`. The reverse `(std::string{}.c_str())`
  is what RCLCPP_INFO already takes.
- **Generated message header path.** nano-ros codegen emits a per-package
  umbrella at `<pkg>/<pkg>.hpp` (e.g. `<std_msgs/std_msgs.hpp>`); upstream
  emits the per-message form `<std_msgs/msg/string.hpp>`. Use the nano-ros
  umbrella include for now; 209.E will emit both.

## What's out of scope (will need code adapt or a follow-up phase)

- **`rclcpp_lifecycle::LifecycleNode`** — Phase 209.H (deferred). Until that
  lands, replace `LifecycleNode` with `Node` + manual configure/activate
  bookkeeping.
- **Yaml-loaded parameters.** `declare_parameter<T>("name", default)` reads
  from a launch yaml in stock ROS 2. nano-ros embedded has no yaml loader;
  Phase 209.F bakes the original yaml + the source into a constexpr
  parameter table (`nros bake-params`). Until that ships, expose parameters
  as compile-time constants (or via `nano_ros_read_config(... "nros.toml")`
  + `nano_ros_generate_config_header(...)`).
- **`tf2`, `image_transport`, `pluginlib`** — out of nano-ros scope. Project-
  specific helpers (autoware `universe_utils`, PX4 uORB shims) are not
  nano-ros's to ship; the porting user vendors them or replaces the call
  sites with raw `nros-cpp` ones.

## When the port hits a gap

The 209 phase doc tracks open follow-ups (E for bulk codegen, F for yaml
params bake, H for LifecycleNode). If your port surfaces a *new* gap not
covered by the compat header, file it under Phase 209 (Track-A = tree-side
fix that lands in `cmake/compat/` or `packages/core/nros-cpp/`; Track-B = a
codegen change). The two in-tree templates
([`rclcpp-compat-smoke`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/rclcpp-compat-smoke)
and
[`topic-state-monitor-port`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/topic-state-monitor-port))
exist as regression fixtures — drop your reduced-case node under
`examples/templates/<your-port>/` and add it to the CI build matrix once the
gap closes.

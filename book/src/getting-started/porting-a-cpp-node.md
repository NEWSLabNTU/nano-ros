# Porting a ROS 2 C++ node to nano-ros

Goal: take a normal ROS 2 C++ node (one that compiles + runs under
`colcon build` against `ros-humble-*`) and run it under nano-ros — without
rewriting the source. The `rclcpp::` names are declared by nano-ros's own C++
headers — `<rclcpp/rclcpp.hpp>` is nros-cpp's own umbrella, not a layer over
another API (RFC-0096 D4). The
CMake side is part of nano-ros's own package: `cmake/NanoRosAmentSurface.cmake`
defines the `ament_*` verbs, `cmake/find/` holds the `Find<pkg>.cmake` modules,
and `diagnostic_updater` is `packages/api/nros-diagnostic-updater`. So the only
delta is **build-script glue**.

The canonical proof lives at
[`examples/templates/cpp-port-minimal-publisher/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/cpp-port-minimal-publisher) —
the ROS 2 tutorial's `minimal_publisher.cpp` vendored unmodified, building
against nano-ros via the three glue lines below.

## The one compile definition, and why you do not type it

A ported file names `rclcpp::Node`, `std::make_shared<MyNode>()` and
`create_publisher<M>(...)` returning a `SharedPtr`. Those signatures are spelled
in `std::shared_ptr` and `std::string`, and nano-ros compiles that surface only
when a build **asks** for it by defining `NROS_CPP_STD`. It is never inferred
from what your compiler happens to have on its include path.

**nano-ros's ament surface sets it for you**: every target created through
`ament_auto_add_*` / `rclcpp_components_register_node`, and every target linking
`rclcpp::rclcpp`, gets it. A project following the two layers of glue below
needs nothing extra. You type it yourself only when you build a ported
translation unit some other way:

```cmake
target_compile_definitions(my_ported_node PRIVATE NROS_CPP_STD=1)
```

Two things worth knowing before you reach for it:

* **This is not a host-versus-embedded switch.** Code written against
  `rclcpp::Node` does not want the flag on a native Linux build either; ported
  rclcpp code wants it on a Cortex-M3 build just as much. The split is *whose
  code it is*, not *what it runs on*. See
  [C++ API — two surfaces](../reference/cpp-api.md#two-surfaces-freestanding-and-nros_cpp_std).
* **Define it per target, not per file.** It changes the layout of
  `rclcpp::Node`, so two translation units of one image disagreeing about it is
  an ODR break rather than a missing function.

If you forget it the failure is loud and at compile time — an unknown
`rclcpp::Node`, or a `create_publisher` overload that does not match — never a
silent behaviour change.

## Two layers of glue

### Per-package CMakeLists.txt — **zero nano-ros lines**

The ported pkg's `CMakeLists.txt` carries **only stock ROS 2 syntax**.
Same file builds under both `colcon build` AND a nano-ros build:

```cmake
cmake_minimum_required(VERSION 3.20)
project(my_node LANGUAGES CXX)

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)
find_package(std_msgs REQUIRED)
# … any other msg packages the source #include's.

add_executable(my_node src/my_node.cpp)
ament_target_dependencies(my_node rclcpp std_msgs)

ament_package()
```

`find_package(rclcpp)` resolves through nano-ros's `Findrclcpp.cmake`, which
defines `rclcpp::rclcpp` over the nros-cpp runtime — and with it the include
path on which `<rclcpp/rclcpp.hpp>` and
`<rclcpp_components/register_node_macro.hpp>` are nano-ros's own headers;
`find_package(std_msgs)` resolves through the message-package resolver (it
walks `NROS_INTERFACE_SEARCH_PATH > AMENT_PREFIX_PATH > bundled`); nano-ros's
`ament_target_dependencies` wires both link targets.

These are find MODULES rather than `<pkg>Config.cmake` files on purpose:
`find_package()` tries module mode first, so nano-ros's `rclcpp` wins over an
installed ROS 2 even when `/opt/ros/<distro>/setup.bash` has been sourced.

### Workspace umbrella CMakeLists.txt — **one nano-ros include**

The umbrella `CMakeLists.txt` (sits at the workspace root, next to
`src/`) is the **only** nano-ros-aware file:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_workspace LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 14)

# 1) Pull nano-ros in.
set(NROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NROS_RMW "${NROS_RMW}")
add_subdirectory("/path/to/nano-ros" nano_ros)

# 2) Point the message-package resolver at this workspace's src/ (must
#    precede the ament-surface include so workspace-pkg Find<pkg>.cmake
#    auto-emit picks it up).
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_SOURCE_DIR}/src")

# 3) nano-ros's ament / rclcpp surface.
include("/path/to/nano-ros/cmake/NanoRosAmentSurface.cmake")

# 4) Bulk-build every workspace msg pkg in topo order (one line instead of
#    N add_subdirectory(src/<pkg>) lines).
nros_workspace_interfaces()

# 5) Build consumer apps.
add_subdirectory(src/my_node)
```

No `nros_generate_interfaces(<pkg>)` calls per consumer — the smart
Find-stub does the codegen at `find_package(<pkg>)` time.

### Reference fixture

[`examples/templates/local-msg-package/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/local-msg-package)
ships the pattern end-to-end: two workspace msg pkgs (`local_msgs`,
`extra_msgs`) with intra-workspace dep + a C++ consumer pulling msgs
from BOTH the workspace AND AMENT (`std_msgs`, `geometry_msgs`,
`sensor_msgs`) via one `find_package` shape. Cross-build proof: the
same `src/` builds under `colcon build` (CI-gated).

### Legacy `nros_generate_interfaces(<pkg>)` shape

(The full current-vs-legacy spelling table lives in
[Message Generation](../user-guide/message-generation.md#which-cmake-spelling-one-table).)

Per-package `nros_generate_interfaces(std_msgs LANGUAGE CPP SKIP_INSTALL)`
calls still work (back-compat preserved) but are **deprecated for new
code** — they bypass the ROS-convention smart Find-stub + workspace
discovery. Existing in-tree examples will migrate as part of Phase
210.E.3.

## Running the ported node on a microcontroller

The same source runs on an RTOS, `main` included. Build glue for each platform
sits beside the source, and the source itself does not change.
`examples/templates/cpp-port-minimal-publisher/` carries two such directories,
`mps2-an385-freertos/` and `zephyr/`, each with a `CMakeLists.txt` built
around one line:

```cmake
find_package(nano_ros REQUIRED)
nano_ros_add_executable(minimal_publisher ../src/minimal_publisher.cpp ROS2_MAIN)
```

The board comes from the directory's `system.toml`, as for any nano-ros image.
Every `nano_ros_add_executable` reaches your `main` (phase-499): no C runtime
calls `int main(int argc, char** argv)` on a board, so the build renames
`main` in your sources only (never in the board's own startup) and calls it
from the platform's entry, with `argc == 0`. `ROS2_MAIN` adds what a ported
program needs on top of that:

- **It gives the ported code the C++ standard library.** A ported node uses
  `std::string`, `std::to_string` and `<chrono>` literals.
  - On FreeRTOS the toolchain compiles C++ `-ffreestanding`, which makes
    `<string>` an `#error`, so the flag is removed for this package's
    directory. nano-ros's own code stays freestanding.
  - On Zephyr, select the full library in `prj.conf`:
    `CONFIG_REQUIRES_FULL_LIBCPP=y` and `CONFIG_GLIBCXX_LIBCPP=y`.
- **It asks for the std-shaped API** (`NROS_CPP_STD`), the same one the ament
  verbs give a ported package on a host.

Where the node connects is decided when the image is built, because a board has
no environment to read it from: the `NROS_ENTRY_LOCATOR` cache variable on
FreeRTOS, the image's `locator` in `system.toml` on Zephyr (rendered into
`CONFIG_NROS_ZENOH_LOCATOR`). `rclcpp::init(argc, argv)`
reads it in every source file that includes `<rclcpp/rclcpp.hpp>`.

A node with a hand-written spin loop also works. The other two templates,
`rclcpp-compat-smoke/` and `topic-state-monitor-port/`, have the same two
directories, and their `main` is the common shape:

```cpp
while (rclcpp::ok()) {
    node->tick();
    rclcpp::spin_some(node);
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
}
```

On FreeRTOS the board provides what the C++ library needs for this:
`std::this_thread::sleep_for` sleeps the calling task, and
`std::chrono::steady_clock` counts from boot, because the board has no real-time
clock. A package that uses another library, such as `diagnostic_updater`, names
it the usual way, `ament_target_dependencies(<name> diagnostic_updater)`. On
Zephyr the dependency goes to `app`, where the sources are compiled.

Zephyr's `native_sim` cannot run a ported node. There Zephyr links its own C
library, and the host's libstdc++ cannot run on top of it. Use a Cortex-M
board such as `mps2/an385` in QEMU, which is what the template targets.

## What "just works" without source edits

nano-ros covers the patterns a typical ROS 2 C++ node uses:

| rclcpp surface | nano-ros mapping | Notes |
|---|---|---|
| `class MyNode : public rclcpp::Node` | `rclcpp::Node` — ours, and the same name. | Ctor takes `(name)`, `(name, options)` or `(name, ns, options)`. |
| `std::make_shared<MyNode>()` | works | `auto node = std::make_shared<MyNode>()` and `rclcpp::spin(std::make_shared<MyNode>())` are unchanged. `rclcpp::Node::SharedPtr node = std::make_shared<…>(…)` does not compile — write `auto`; see "`Node::SharedPtr` observes, it does not own" below. `shared_from_this()` works too, with the same caveat. |
| `rclcpp::Node::make_shared("name")` | `std::make_shared<rclcpp::Node>(…)`, the owning `std::shared_ptr` | Hosted builds. `auto node = rclcpp::Node::make_shared("x");` is unchanged; assigning the temporary straight to a global `rclcpp::Node::SharedPtr` is not — keep it in a local first (`auto node = …; g_node = node;`). |
| `create_publisher<M>(topic, qos)` | shared_ptr-returning wrapper | `qos` can be `rclcpp::QoS(10)` or an int. |
| `create_subscription<M>(topic, qos, callback)` | registered on the executor arena; dispatched by **any** spin verb | **Capturing lambdas + `std::function` + `std::bind` all work**. The callback may take `const M&` (no allocation, every target) or, on hosted builds, upstream's `M::UniquePtr` / `M::SharedPtr` / `M::ConstSharedPtr` / `std::shared_ptr<const M>`: each delivery is then copied into a fresh heap object the callback owns and may keep. A freestanding build refuses the pointer shapes at compile time, naming `const M&`. |
| `create_wall_timer(period, callback)` | registered on the executor arena; dispatched by **any** spin verb | `std::chrono::duration` arg, capturing-lambda callback. Returns `rclcpp::TimerBase::SharedPtr`, which is `rclcpp::Timer::SharedPtr` — one flat type, two names. See below. |
| `rclcpp::create_timer(node, clock, period, cb)` | the clock-taking verb; a `RCL_ROS_TIME` clock follows `/clock` | Humble's only form. `create_wall_timer` stays on the steady clock. |
| `create_service<S>(name, handler)` with upstream's `std::shared_ptr<Request>` / `std::shared_ptr<Response>` handler (optionally with `std::shared_ptr<rmw_request_id_t>` first) | lent from the dispatch: the handler gets pointers to the request and response the dispatch decoded | Hosted builds; a plain function, a lambda with or without captures, or `std::bind(&Node::on_request, this, _1, _2)` (a handler's state is copied into the service's executor entry, and whatever it returns, such as Autoware's `bool`, is ignored). The handler must not KEEP a pointer past its call: that aborts, naming why. The request header is zero-filled. |
| `rclcpp::executors::SingleThreadedExecutor` + `add_node` + `spin` | the process executor every node already lives on | `add_node` checks rather than attaches. `MultiThreadedExecutor` is a compile error naming the alternative: an executor dispatches on one task, and parallelism is a tier, not a callback group. |
| `executor.spin_some()` / `rclcpp::spin_some(node)` | drain what is ready, without waiting | The bound is milliseconds (`spin_some(5)`), not a `std::chrono` duration. |
| `client->wait_for_service(1s)` / `wait_for_action_server(10s)` | the millisecond wait, converted | Hosted builds. The budget is still required; a negative duration is refused at the call. |
| `rclcpp::spin_until_future_complete(node, client->send_request(req))` | spins until the reply arrives | The timeout is milliseconds. |
| `auto fut = client->async_send_request(std::make_shared<S::Request>(…))` / `fut.get()` / `fut.wait_for(1s)` / `client->remove_pending_request(fut)` | the poll client's `rclcpp::Future`, under upstream's verbs | Hosted builds, except `remove_pending_request` (issue 1996). `get()` returns the reply as an owning `std::shared_ptr<Response>` but never blocks: call it after `spin_until_future_complete` returned `SUCCESS` (before the reply it logs a refusal and returns nullptr). `wait_for` returns `std::future_status` and spins the executor while it waits. Each future is keyed by its request's sequence number (issue 2009), as upstream's is: several may be in flight on one client, each gets its own reply, and a reply that arrives after `remove_pending_request` (or `prune_pending_requests`) is dropped. How many is capped by the transport's own outstanding-request limit (`NROS_ZENOH_MAX_PENDING_GETS` on zenoh); a send past it returns an empty future (`valid()` false). What it returns is upstream's `FutureAndRequestId` (issue 2037): the future as `.future`, the request's sequence id as `.request_id` (`auto [future, id] = …` works), and `share()` hands back a `SharedFutureAndRequestId` whose `future` is a `std::shared_future<SharedResponse>` every copy of which gets the same response; `remove_pending_request` takes the id too. `.future` is an `rclcpp::Future`, not a `std::future`. A freestanding build has `send_request(req)` + `fut.try_take(resp)`; `get()` and `share()` there are compile errors naming them. |
| `#include "rclcpp_action/rclcpp_action.hpp"` | an umbrella over `Server<A>` / `Client<A>` and the two free factories | See [Actions](#actions-goal-handles-on-a-hosted-build) below. |
| `rclcpp::init(argc, argv) / shutdown() / ok() / spin(n) / spin_some(n)` | wraps `rclcpp::init_in/shutdown_in/ok/spin_once` | `--ros-args -r from:=to` is honoured, beneath any remap the launch file sets for the same name. `-p [node:]name:=value` and `--params-file <file>` are honoured in an image with a parameter store: the value replaces the default when the node declares the parameter (scalars only from a file; a value of the wrong type is logged and the default kept). Any other ROS argument (`__node`, log flags) aborts, naming it. |
| `RCLCPP_INFO / WARN / ERROR / DEBUG / FATAL` | dispatched through the `NROS_LOG_*` family into `nros_log` | Carries the logger, so `get_logger("x")` selects a real per-logger level, and `FATAL` is a distinct severity. Reaches `LOG_ERR`/`printk` on embedded, where the legacy `NROS_*` sink is a no-op (issue 1019). `_THROTTLE` variants are REFUSE-LOUD. |
| `get_logger().get_child("sub")` | a real child logger, `<parent>.sub` (RFC-0102) | Its level, when unset, follows the nearest ancestor's, so `set_level` on the node logger reaches it. A full name over 48 bytes or a full runtime-logger arena emits through the PARENT and `set_level` on it is refused — never a silent move of the parent's threshold. Iron/Jazzy images publish children on `/rosout`; Humble ones, as upstream, do not. |
| `executor.spin_once() / client->wait_for_service() / action_client->wait_for_action_server()` | **the budget is required** | Upstream defaults all three to "block forever", which nano-ros has no form of; the no-argument call is a compile error naming the alternative rather than a silently substituted 10 ms / 5 s budget. Write `spin_once(10)`, `wait_for_service(10000)`. |
| `rclcpp::QoS / KeepLast(n) / SystemDefaultsQoS()` | one class with upstream's implicit `(depth)` ctor | Chainable setters inherited. |
| `diagnostic_updater::Updater` + `DiagnosticStatusWrapper` | `packages/api/nros-diagnostic-updater` | Publishes `/diagnostics`. |
| `rclcpp_action::Server<A> / Client<A>` | ours, under upstream's names | Hosted builds take upstream's goal-handle shapes as written; freestanding builds key goals by id. See [Actions](#actions-goal-handles-on-a-hosted-build) below. |
| `class MyNode : public rclcpp_lifecycle::LifecycleNode` | ours, under upstream's name (`<rclcpp_lifecycle/lifecycle_node.hpp>`) | It IS an `rclcpp::Node`. The `on_configure(const State&)` … `on_shutdown` overrides, `CallbackReturn::SUCCESS`, `configure()` / `activate()` / … and `get_current_state()` keep upstream's signatures; the REP-2002 services are registered by the constructor. `create_publisher<M>(topic, qos)` returns a managed `LifecyclePublisher<M>::SharedPtr` that sends only while Active. One managed node per executor. |
| `RCLCPP_COMPONENTS_REGISTER_NODE(class)` + `rclcpp_components_register_node(lib PLUGIN "ns::Class" EXECUTABLE exe)` | the macro defines the component's factory in its own source file; the cmake verb emits a thin `int main()` per registration that declares the class and calls that factory | Single binary, no class_loader. The PLUGIN's qualifiers must be namespaces (a class nested in a class cannot be forward-declared). A component library built `SHARED` is built static. |
| plain CMake components: `add_library(<c> SHARED …)`, `ament_target_dependencies(<c> rclcpp …)`, `rclcpp_components_register_nodes(<c> "ns::Class")`, a hand-written main (`add_executable` + `target_link_libraries`) | each `SHARED` or `MODULE` library builds static, and the configure prints a line saying so; the plural verb and `ament_environment_hooks` are accepted and do nothing | One image is one binary with one runtime. `ament_target_dependencies` works beside a `target_link_libraries` in either signature (plain or `PRIVATE`). A hand-written main gets the link pass the ament verbs give theirs. `IMPORTED` libraries keep their type. A project that overrides `add_library` itself replaces this. |
| `find_package(ament_cmake_auto / rclcpp / rclcpp_components / diagnostic_updater / std_msgs / …)` | find modules at `cmake/find/` | ~28 of the most-cited ROS 2 packages; a message package not among them is found by the resolver from `NROS_INTERFACE_SEARCH_PATH` / `AMENT_PREFIX_PATH`. |
| `ament_auto_find_build_dependencies()` + `ament_auto_add_library` / `ament_auto_add_executable` | reads `package.xml`; each target gets the package's `include/` and every dependency nano-ros has a module for | A dependency nano-ros does not provide (a project helper such as `autoware_utils`) is listed at configure and not looked up; a source that includes it fails on the include. |
| a project's own message packages (`autoware_vehicle_msgs`, `tier4_control_msgs`, …) installed under `AMENT_PREFIX_PATH` | generated by nano-ros codegen from the installed `.msg` / `.srv` | Never the installed package's upstream C++. Source the project's install first (below). |
| `create_callback_group(rclcpp::CallbackGroupType::MutuallyExclusive)`, `SubscriptionOptions::callback_group`, `create_subscription<M>(topic, qos, cb, options)` | accepted; an unnamed group runs on the node's default context | One executor task already keeps every callback mutually exclusive, so `Reentrant` runs them one at a time too. A NAMED group (`create_callback_group("name")`) is the tier-mapped one. |
| `create_service<S>(name, cb, rmw_qos_profile_services_default, group)` | upstream's trailing QoS and group | `rmw_qos_profile_t` has upstream's fields and enum values. A profile converts to `rclcpp::QoS`, `QoS(QoSInitialization::from_rmw(p), p)` builds one, and `qos.get_rmw_qos_profile()` reads it back. `rmw_qos_profile_system_default` is absent, because every field of it is a placeholder for the RMW to fill in, and the RMWs fill it in differently. A file that also includes `<nros/rmw_entity.h>` is refused, because that header defines the RMW backend's own struct of the same name. |
| `pub_->publish(msg);` as a statement | allowed on a ported (ament-built) target, even under `-Wall -Werror`; a failure is logged | Code written for nano-ros keeps the `[[nodiscard]] Result`. |
| several packages in one workspace (a library package such as Nav2's `nav2_util`, a cmake-only one such as `nav2_common`, and the node that finds them) | `find_package(<sibling>)` resolves to the sibling configured earlier in the same run: `<pkg>::<pkg>` over what it exported with `ament_export_libraries` / `_targets` / `_dependencies`, its `include/`, and its `ament_package(CONFIG_EXTRAS …)` files run | Add each package before the packages that find it; the configure refuses an out-of-order find by name. Every package configures in ONE CMake run, so `${CMAKE_SOURCE_DIR}` is the workspace root, not the package: write `${PROJECT_SOURCE_DIR}` (rare: 0 of 417 Autoware CMakeLists name it). |
| `find_package(<pkg>)` for an upstream C++ library nano-ros does not ship (`bondcpp`, `tf2_ros`, …) | refused at configure, naming the package and the installed file CMake fell back to | Remove the dependency, or port the library into your workspace as a package of its own. |
| `rclcpp::Time(msg->header.stamp)` | implicit, from any `{sec, nanosec}` message, on the ROS clock | |
| `msg.header.stamp = now();`, `stamp + rclcpp::Duration::from_seconds(0.1)` | a `{sec, nanosec}` message converts from `rclcpp::Time`, and `+` / `-` a `Duration` | |
| `last_cmd_.twist == geometry_msgs::msg::Twist()` | a generated message has field-wise `operator==` / `operator!=` | Messages only; a service's or action's own `Request` / `Goal` structs have none. |
| `rclcpp::Node::WeakPtr parent` + `parent.lock()` | the same non-owning handle; `lock()` hands it back | `lock()` cannot tell a destroyed node from a live one, because the handle owns nothing; it answers null only for an unset handle. |
| `rclcpp::Clock::SharedPtr clock_ = get_clock();` | a pointer to the node's clock | `std::make_shared<rclcpp::Clock>(...)` does not convert; hold a `rclcpp::Clock` by value. |
| `timer_->cancel();` / `timer_->reset();` as a statement | allowed on a ported target under `-Werror` | As `publish()`: the result is still readable. |
| `descriptor.floating_point_range.resize(1)` | a bounded sequence resizes within its bound | Past the bound it is refused and returns `false`, where rosidl throws. |
| `rclcpp::spin(node->get_node_base_interface())` | spins the node | |
| `get_node_base_interface()->get_context()->add_pre_shutdown_callback(f)` | `rclcpp::Context` views the executor's shutdown hooks; `f` runs on `rclcpp::shutdown()` | The hooks share the executor's table (`NROS_EXECUTOR_MAX_SHUTDOWN_CBS`, default 2). A full table returns an invalid handle. |
| a package that depends on `rclcpp_lifecycle` | its runtime carries `lifecycle` with `param_services`, and `ros2 lifecycle get /<node>` finds it | |
| `std::static_pointer_cast<Derived>(shared_from_this())` (Nav2's `nav2_util::LifecycleNode`) | **does not compile** | The handle owns nothing, so there is no `std::shared_ptr` to cast. Drop the override; `rclcpp_lifecycle::LifecycleNode::shared_from_this()` returns its own type. |
| `pub_->get_subscription_count()` | takes the executor and an out-parameter | See `Publisher::get_subscription_count`. Nav2 calls it only in its tests. |
| `declare_parameter(name, rclcpp::ParameterValue(...)[, descriptor])`, `get_parameter(name)` as an `rclcpp::Parameter`, `set_parameter(rclcpp::Parameter(...))` | the same store the typed `declare_parameter<T>` uses; the descriptor may be an `rcl_interfaces::msg::ParameterDescriptor` | Hosted builds. A byte or string array can be built but not declared. A wrong-type `as_double()` answers `0.0`, not an exception. |
| `add_on_set_parameters_callback(std::bind(&C::cb, this, _1))` with `cb(std::vector<rclcpp::Parameter>)` returning `SetParametersResult` | runs on every write, local or `ros2 param set`; `successful = false` refuses it; the returned handle unregisters on destruction | One parameter per call, because writes are validated one at a time. The `reason` does not reach the remote caller. Keep the handle as a member of your node class, so it goes before the node does. |
| `node->get_node_parameters_interface()` | `has_parameter`, `declare_parameter`, `get_parameter` through `->` | `get_parameter_overrides()` is absent (issue 2057). |
| `pub_->publish(std::move(msg))` with a `std::unique_ptr<M>` | publishes `*msg`; the message is freed when the call returns | Hosted builds. Every publish serializes, so there is no intra-process hand-over to save a copy. A null pointer is refused and logged. |
| `rclcpp::Client<S>::SharedPtr client_ = create_client<S>(name);` + `client_->async_send_request(req, callback)` | **not supported** (issue 2055) | `create_client<S>(name)` returns the poll client by value: store it as `rclcpp::Owned<rclcpp::PollClient<S>>` and keep the `FutureAndRequestId` that `async_send_request(req)` returns as a member. Read it on a later callback with `future.is_ready()` and `get()`. Inside a callback, ask `service_is_ready()` instead of calling `wait_for_service`, which drives the executor. |
| `diagnostic_updater::Updater updater_{this};` | publishes `/diagnostics` on its own timer, every `period` (`setPeriod`), as upstream's does | `update()` still works and is no longer required. |
| `DiagnosticTask`, `FrequencyStatus`, `TimeStampStatus`, `Heartbeat`, `HeaderlessTopicDiagnostic`, `TopicDiagnostic` | upstream's classes, levels and messages; `updater.add(task)` | A clock argument is a `const rclcpp::Clock*`, so `node->get_clock()` still passes. The tasks take no lock, so a node that calls `tick()` from its own thread must keep that call from overlapping an update. `DiagnosedPublisher` is absent, because a nano-ros publisher handle can be moved but not copied. Keep the publisher and a `TopicDiagnostic`, and call `tick(stamp)` next to each `publish`. |
| the six parameter services (`ros2 param get/set/list`) | on for an ament package built on a host: every node serves them, as upstream's does | Comes from the package's `package.xml` naming `ament_cmake`; set `NROS_FEATURES` to declare the capabilities yourself. Values come from `--ros-args --params-file <yaml>` or `-p`. |

## Measured: the stock rclcpp examples

`packages/api/nros-cpp/tests/compile/rclcpp-examples/` holds 13 files from
[ros2/examples](https://github.com/ros2/examples) (humble and jazzy) verbatim,
each beside its port. The repository's C++ check compiles and links every port
against nros-cpp, boots it on a stub middleware, and fails if the number of edited
lines changes in either direction (`edits.json`). Today:

| example | edited lines | what the edits are |
|---|---|---|
| `minimal_publisher` lambda / member_function | 0 | — |
| `minimal_subscriber` member_function | 0 | — |
| `minimal_subscriber` lambda (jazzy) | 0 | — |
| `minimal_timer` lambda / member_function | 0 | — |
| `minimal_client` | 0 | — |
| `minimal_service` | 3 | global `Node::SharedPtr` from a temporary |
| `minimal_subscriber` not_composable | 3 | global `Node::SharedPtr` from a temporary |
| `minimal_publisher` not_composable | 11 | the `try`/`catch` goes: no exceptions |
| `multithreaded_executor` | 18 | `MultiThreadedExecutor`, callback groups |
| `minimal_action_client` member_functions | 2 | `seq.back()` on a generated sequence |
| `minimal_action_server` member_functions | 6 | the goal runs in the accepted callback, not on a thread of its own |

The remaining causes, largest first:

- **Callback groups and the multi-threaded executor** (18 lines).
- **Exceptions** (11 lines). There is nothing to catch.
- **The action server's thread per goal** (6 lines). The stock
  `handle_accepted` detaches a `std::thread` that drives the goal handle; a
  goal handle used from a thread you started races the executor (nano-ros
  starts no threads behind it and a one-level image has no executor lock). The
  port calls `execute(goal_handle)` in the accepted callback and drops the
  `rclcpp::Rate::sleep`, which would re-enter the executor from that callback.
- **A global `Node::SharedPtr` assigned from `make_shared`** (2 lines each).
- **`seq.back()` on a generated sequence** (2 lines). `FixedSequence` /
  `HeapSequence` are not `std::vector`; write `seq[seq.length() - 1]`.

The two action examples took 164 edited lines until issue 2015 made upstream's
goal-handle shapes drop-ins (77 → 6 and 87 → 2); the total went from 199 to 43.

## Actions: goal handles on a hosted build

The action types keep upstream's names and concepts, and `GoalResponse` /
`CancelResponse` / `ResultCode` keep upstream's enumerators and values.

**On a hosted build** (`-DNROS_CPP_STD`, what the porting flow sets) the stock
spellings are drop-ins (issue 2015):

- `rclcpp_action::create_server<A>(node->get_node_base_interface(), …clock,
  …logging, …waitables, name, handle_goal, handle_cancel, handle_accepted)` and
  `rclcpp_action::create_client<A>(…base, …graph, …logging, …waitables, name)`,
  or the `(node, name, …)` shape. Each returns `X<A>::SharedPtr`, which is
  `rclcpp::Owned<X<A>>`: the entity by value, so a member
  `Server<A>::SharedPtr action_server_;` works and nothing is shared. The
  `get_node_*_interface()` getters return a handle naming the node, whose only
  use is these two calls.
- The callbacks take upstream's shapes, `std::bind`-ed members included:
  `handle_goal(const GoalUUID&, std::shared_ptr<const Goal>)`,
  `handle_cancel(std::shared_ptr<ServerGoalHandle<A>>)`,
  `handle_accepted(std::shared_ptr<ServerGoalHandle<A>>)`.
- `ServerGoalHandle<A>` answers `get_goal_id`, `get_goal`, `is_active`,
  `is_executing`, `is_canceling`, and `publish_feedback` / `succeed` / `abort` /
  `canceled` take upstream's `std::shared_ptr`s. Each call forwards to the
  server, keyed by the goal id; the goal itself lives in the server's static
  arena, and the handle is a token over it. `execute()` moves a goal accepted
  with `ACCEPT_AND_DEFER` to EXECUTING and publishes the status (issue 2027);
  for `ACCEPT_AND_EXECUTE` the server does that itself before
  `handle_accepted`, as upstream's does.
- `Client<A>::SendGoalOptions` carries upstream's `goal_response_callback`
  (a `ClientGoalHandle<A>::SharedPtr`, null when rejected), `feedback_callback`
  (`std::shared_ptr<const Feedback>`) and `result_callback`
  (`WrappedResult{goal_id, code, result}`), and `async_send_goal(goal, options)`
  binds them to that goal. Giving a result callback requests the result once the
  goal is accepted, as upstream does.
- `async_get_result(goal_handle, result_callback)` and
  `async_cancel_goal(goal_handle, cancel_callback)`, plus
  `async_cancel_all_goals` / `async_cancel_goals_before(stamp)`, return
  upstream's `std::shared_future` (of a `WrappedResult`, or a
  `CancelResponse::SharedPtr` with `return_code` and `goals_canceling`), so
  `rclcpp::spin_until_future_complete(node, f)` then `f.get()` work unchanged
  (issue 2027). The future drives the client while it is polled or waited on.

What stays different, each a compile error or written below:

- **`async_send_goal` returns `rclcpp::Future<GoalAccept>`**, the goal-keyed
  future `send_goal_future` returns, not a `std::shared_future` of a goal
  handle. `auto f = client->async_send_goal(goal, opts);` compiles; a caller
  that waits on `f` gets the acceptance (RFC-0021: the future never blocks
  without spinning).
- **A cancel reply is taken by the executor's spin** (issue 2034), as upstream's
  is: the spin settles the `async_cancel_*` future and runs its
  `cancel_callback`, so code that keeps only the callback ports as written.
  A nano-ros server acts on the zero-id cancel-all / cancel-before forms
  as an rclcpp server does (issue 2029).
- **A goal handle is driven from the executor's task.** Do not hand it to a
  thread you start (see the 6 lines above). It names its server by address, so
  the server must not move while a handle is in use; a `SharedPtr` does not
  move it.
- **No `get_goal_stamp`**: the arena keeps no acceptance time.
- It is not a `Waitable` (no `execute` / `is_ready` / `set_on_ready_callback`):
  the executor drives the server directly.
- **`wait_for_action_server` returns `Result`** and the budget is required
  (see the table above).

**On a freestanding build** there is no allocator for a goal handle, so
`ClientGoalHandle::SharedPtr`, `ServerGoalHandle::SharedPtr`,
`rclcpp_action::create_server`, `async_send_goal` and an assignment to
`SendGoalOptions`' upstream fields are each a compile error naming the
goal-id-keyed form:

- **Server.** Declare the `Server<A>`, initialise it with
  `node.create_action_server(srv, name)`, and install
  `set_goal_callback_with_ctx` / `set_cancel_callback_with_ctx` /
  `set_accepted_callback_with_ctx` (function pointers with a `void*` context).
  The callbacks receive the 16-byte goal id; feedback and termination are calls
  on the server keyed by it — `srv.publish_feedback(id, fb)`,
  `srv.succeed(id, result)`, `abort`, `canceled` — taking the message by const
  reference.
- **Client.** `client.set_callbacks(SendGoalOptions{goal_response, feedback,
  result, context})` installs raw callbacks once for every goal the client
  sends; `send_goal_async(goal, goal_id)` sends. `send_goal_future` /
  `get_result_future` are the future-shaped spellings, each keyed by its goal.
  `rclcpp_action::create_client` works here too (no allocator involved).

## Two things the compiler will not tell you

Everything else on this page either works or fails to compile. These two
compile and behave slightly differently, so they are written down rather than
left for a debugging session.

### `Node::ok()` — nano-ros cannot throw, so YOU have to ask

Upstream's `rclcpp::Node` constructor THROWS when the node cannot be created.
nano-ros is built `-fno-exceptions` (RFC-0018), so the constructor cannot: it
records the failure and the node answers `false` to `ok()`.

```cpp
rclcpp::Node node("talker");
if (!node.ok()) return 1;        // <- this line replaces upstream's try/catch
```

A **generated entry already does this** and halts naming the node, so a
workspace component needs nothing. A **hand-written `main` does not**, and a
node that failed to create will otherwise go on to create publishers that
quietly do nothing. There is no diagnostic for it — the object exists either
way, which is the whole point of not throwing — so checking `ok()` after
constructing a node by hand is on you.

The same shape applies to `init()`, the two-phase form:

```cpp
rclcpp::Node node;
if (!node.init("talker").ok()) return 1;
```

`Result` carries `[[nodiscard]]` on C++17 and later, so *that* one the compiler
does warn about.

### `Node::SharedPtr` observes, it does not own

`rclcpp::Node::SharedPtr` is `rclcpp::Handle<rclcpp::Node>` on every target,
embedded included: a copyable reference that does not keep the node alive. A
node is constructed by your `main` (or the generated entry) and outlives
everything it is handed to, so storing and copying it works as upstream's does.
Two consequences:

- **`shared_from_this()`** returns that handle. It is safe to pass to anything
  that holds it for less than the node's lifetime
  (`diagnostic_updater::Updater(shared_from_this(), 1.0)` is the common case),
  and NOT safe to store somewhere that outlives the node. It never throws where
  upstream would raise `bad_weak_ptr`. nano-ros cannot derive from
  `std::enable_shared_from_this<Node>`: its `std::weak_ptr` member exists only
  where `<memory>` does, so two translation units of one firmware image could
  disagree about the object's layout.
- **`rclcpp::Node::SharedPtr node = std::make_shared<MyNode>();` is a compile
  error.** The `shared_ptr` there is a temporary, and a handle to it would
  dangle at the end of the line; the compiler names the deleted conversion.
  Write `auto node = std::make_shared<MyNode>();` instead. A `shared_ptr` you
  keep converts to `Node::SharedPtr` freely, and `rclcpp::spin(…)` accepts the
  temporary directly.

### `rclcpp::TimerBase` is a NAME here, not a hierarchy

`rclcpp::TimerBase::SharedPtr timer_;` compiles unchanged, and you should keep
writing it — it is the only spelling that works against real ROS 2 as well as
against nano-ros, since upstream has no `rclcpp::Timer`.

What differs is behind the name. nano-ros has no timer hierarchy: `TimerBase`
is an ALIAS for the one flat `rclcpp::Timer`, so `is_same<TimerBase,
Timer>::value` is true and neither spelling carries a vtable. The executor
dispatches through a raw function pointer, so a polymorphic base would be cost
with no caller.

Consequences worth knowing:

* `Timer::SharedPtr` is a small handle, not a `std::shared_ptr`. Its
  callback lives in the executor, so the handle does not own the timer.
  Dropping or overwriting `timer_` leaves the timer firing; destroying the node
  stops it. `timer_.reset()` releases the timer for good. `timer_->reset()`
  restarts it, as upstream. A member declared as
  `std::shared_ptr<rclcpp::TimerBase>` does not compile: spell it
  `rclcpp::TimerBase::SharedPtr`.
* The same holds for `Subscription<M>::SharedPtr`, `Service<S>::SharedPtr` and
  `Client<S>::SharedPtr`: dropping the handle leaves the entity running, and
  `sub_.reset()` / `srv_.reset()` / `cli_.reset()` releases it for every copy
  at once. A one-shot timer or subscription that resets ITSELF from its own
  callback works: the release takes effect when the callback returns.
* `rclcpp::WallTimer<...>` and `rclcpp::GenericTimer<...>` do not exist. The
  clock axis is the second verb `rclcpp::create_timer(node, clock, period, cb)`
  plus a runtime field, not a type parameter.
* DERIVING from `rclcpp::TimerBase` gets you a concrete handle with a
  non-virtual destructor. Upstream code almost never does this; if yours does,
  it needs rework. There is no timer hierarchy here: the executor dispatches through a
raw function pointer, so a polymorphic base would be a vtable nothing calls, and
`TimerBase` is a name that promises `WallTimer` and `GenericTimer` siblings we
deliberately do not have. The clock axis is a runtime field plus the second verb
`rclcpp::create_timer`, not a type parameter.

## A real package: Autoware's `autoware_external_cmd_selector`

Phase-488 ported this node (Autoware 0.45.0; 12 subscriptions, 7 publishers,
a service, two parameters, `diagnostic_updater`, a component registered with
`rclcpp_components_register_node(... EXECUTABLE ...)`) through a workspace
umbrella like the one above. Its package needed **no source edit and no
CMakeLists edit**, and it runs against a stock ROS 2 Humble `rmw_zenoh_cpp`
peer: topics, the service, `ros2 param`, `/diagnostics`. Three things it
needed from the person porting it:

1. **Source the project's install before configuring.** Its CMakeLists calls
   `find_package(autoware_cmake)` and its message packages come from
   Autoware's own install; both reach a machine through
   `source <autoware>/install/setup.bash`. nano-ros reads what that puts on
   `AMENT_PREFIX_PATH` and `CMAKE_PREFIX_PATH`: `autoware_cmake`'s own macros
   run over nano-ros's ament verbs, and each message package is generated by
   nano-ros codegen, never taken from the installed upstream C++.
2. **Raise the pool sizes.** A package built this way declares no entity
   counts, so every pool sits at its embedded default, and start-up stops at
   the first one that fills, naming its knob. Until issue 2043 derives them
   from the program, set them in the environment of the configure and build:

   ```bash
   export NROS_EXECUTOR_MAX_CBS=40 NROS_ZENOH_MAX_LIVELINESS=128 \
          NROS_ZENOH_MAX_PUBLISHERS=32 NROS_ZENOH_MAX_SUBSCRIBERS=32 \
          NROS_ZENOH_MAX_QUERYABLES=32
   ```

   Those are what this node needed with headroom; a smaller node needs less.
3. **Name the router.** `NROS_LOCATOR=tcp/<host>:7447` at run time, the same
   router the ROS 2 side's `rmw_zenoh_cpp` uses.

## What's documented as "needs adapt" (codegen-side, not surface-side)

These are cosmetic codegen differences nano-ros's per-package codegen
and the upstream `rosidl_default_runtime` codegen don't share; both are
tracked as ROS-convention codegen work.

- **Message string fields.** nano-ros codegen emits `rclcpp::FixedString<N>`,
  upstream emits `std::string`. Assigning a `std::string` needs a one-token
  adapter: `message.data = s.c_str()`. The reverse `(std::string{}.c_str())`
  is what `RCLCPP_INFO` already takes.
- **Generated message header path.** **CLOSED** (alias
  headers): nano-ros codegen emits BOTH the per-message form
  `<std_msgs/msg/string.hpp>` (upstream-shape) AND the umbrella
  `<std_msgs/std_msgs.hpp>`. Use whichever the original source picks.

## What's out of scope (will need code adapt or a follow-up phase)

- **Persisting parameters.** A `--params-file <yaml>` on the command line is
  read (scalars), as is `-p`, and `declare_parameter<T>(name)` with no default
  takes its value from there (the Autoware port below declares both of its
  parameters that way). Parameters then live in a volatile store the standard
  parameter services can update until the next boot; nothing writes them back.
- **A message with an unbounded string or sequence** (`std_msgs/String`'s
  `data`) — a subscription to it stops the compile at
  `NROS_UNBOUNDED__<pkg>_msg_<type>__field_<field>`. The receive buffer is
  sized from a bound, and an unbounded field has none. Give it one in a
  `nros-codegen.toml` at the workspace root or beside the package's
  `CMakeLists.txt`:

  ```toml
  [fields]
  "std_msgs/String.data" = 256
  ```

  The wire format does not change. The other levels (`[defaults]`,
  `[packages.*]`, `[types.*]`) and the storage modes are in
  [RFC-0033](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0033-message-field-capacity-config.md).
- **`class_loader`, `dlopen` composition, a component that is not an
  `rclcpp::Node`** (`get_node_base_interface()`) — nano-ros has no runtime
  loader and no node interfaces. Build the components into one image with a
  hand-written main, as ros2/demos `manual_composition` does.
- **`tf2`, `image_transport`, `pluginlib`** — out of nano-ros scope. Project-
  specific helpers (autoware `universe_utils`, PX4 uORB shims) are not
  nano-ros's to ship; the porting user vendors them or replaces the call
  sites with raw `nros-cpp` ones.

## When the port hits a gap

Known open follow-ups: pool sizing on the ament road (issue 2043, below),
the per-request client callback (issue 2055), listing parameter overrides
(issue 2057), and the in-tree migration of legacy
`nros_generate_interfaces(<pkg>)` call sites. If your port surfaces a *new*
gap, file an issue with a reduced case — a fix lands either tree-side (in
`packages/api/nros-cpp/`, `cmake/NanoRosAmentSurface.cmake` or `cmake/find/`)
or as a codegen change, each with its own in-tree test.

In-tree regression fixtures:

* [`local-msg-package`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/local-msg-package)
  — mixed workspace (workspace + AMENT msg sources) C++ + Rust consumers.
* [`cpp-port-minimal-publisher`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/cpp-port-minimal-publisher)
  — ROS 2 tutorial `minimal_publisher.cpp` verbatim.
* [`rclcpp-compat-smoke`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/rclcpp-compat-smoke)
  — minimal source-compat regression test.
* [`topic-state-monitor-port`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/topic-state-monitor-port)
  — multi-sub / wall-timer / diagnostic_updater exercise.

Your ported code stays yours: nano-ros does not take a copy of it as a
fixture. What lands is the fix, tested by code written for the test: the
Autoware port above left behind `tests/cmake-ament-surface-tests.sh` and
`packages/api/nros-cpp/tests/compile/ament_port_surface_runtime.cpp`, neither
of which contains a line of Autoware's.

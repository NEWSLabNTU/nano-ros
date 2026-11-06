# rclcpp compatibility

<!-- type: reference · level: 3 -->

What a ported rclcpp node gets, by the upstream spelling it uses. "Hosted
builds" means a target built with `NROS_CPP_STD`, which the ament verbs set
for a ported package. The porting steps are in
[Port a C++ node](../guide/port/port-a-cpp-node.md).

| rclcpp surface | nano-ros | Notes |
| --- | --- | --- |
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
| `auto fut = client->async_send_request(std::make_shared<S::Request>(…))` / `fut.get()` / `fut.wait_for(1s)` / `client->remove_pending_request(fut)` | the poll client's `rclcpp::Future`, under upstream's verbs | Hosted builds, except `remove_pending_request`. `get()` returns the reply as an owning `std::shared_ptr<Response>` but never blocks: call it after `spin_until_future_complete` returned `SUCCESS` (before the reply it logs a refusal and returns nullptr). `wait_for` returns `std::future_status` and spins the executor while it waits. Each future is keyed by its request's sequence number, as upstream's is: several may be in flight on one client, each gets its own reply, and a reply that arrives after `remove_pending_request` (or `prune_pending_requests`) is dropped. How many is capped by the transport's own outstanding-request limit (`NROS_ZENOH_MAX_PENDING_GETS` on zenoh); a send past it returns an empty future (`valid()` false). What it returns is upstream's `FutureAndRequestId`: the future as `.future`, the request's sequence id as `.request_id` (`auto [future, id] = …` works), and `share()` hands back a `SharedFutureAndRequestId` whose `future` is a `std::shared_future<SharedResponse>` every copy of which gets the same response; `remove_pending_request` takes the id too. `.future` is an `rclcpp::Future`, not a `std::future`. A freestanding build has `send_request(req)` + `fut.try_take(resp)`; `get()` and `share()` there are compile errors naming them. |
| `#include "rclcpp_action/rclcpp_action.hpp"` | an umbrella over `Server<A>` / `Client<A>` and the two free factories | See [Actions with upstream's goal handles](#actions-with-upstreams-goal-handles-hosted). |
| `rclcpp::init(argc, argv) / shutdown() / ok() / spin(n) / spin_some(n)` | wraps `rclcpp::init_in/shutdown_in/ok/spin_once` | `--ros-args -r from:=to` is honoured, beneath any remap the launch file sets for the same name. `-p [node:]name:=value` and `--params-file <file>` are honoured in an image with a parameter store: the value replaces the default when the node declares the parameter (scalars only from a file; a value of the wrong type is logged and the default kept). Any other ROS argument (`__node`, log flags) aborts, naming it. |
| `RCLCPP_INFO / WARN / ERROR / DEBUG / FATAL` | dispatched through the `NROS_LOG_*` family into `nros_log` | Carries the logger, so `get_logger("x")` selects a real per-logger level, and `FATAL` is a distinct severity. Reaches `LOG_ERR`/`printk` on embedded, where the legacy `NROS_*` sink is a no-op. `_THROTTLE` variants are REFUSE-LOUD. |
| `get_logger().get_child("sub")` | a real child logger, `<parent>.sub` | Its level, when unset, follows the nearest ancestor's, so `set_level` on the node logger reaches it. A full name over 48 bytes or a full runtime-logger arena emits through the PARENT and `set_level` on it is refused — never a silent move of the parent's threshold. Iron/Jazzy images publish children on `/rosout`; Humble ones, as upstream, do not. |
| `executor.spin_once() / client->wait_for_service() / action_client->wait_for_action_server()` | **the budget is required** | Upstream defaults all three to "block forever", which nano-ros has no form of; the no-argument call is a compile error naming the alternative rather than a silently substituted 10 ms / 5 s budget. Write `spin_once(10)`, `wait_for_service(10000)`. |
| `rclcpp::QoS / KeepLast(n) / SystemDefaultsQoS()` | one class with upstream's implicit `(depth)` ctor | Chainable setters inherited. |
| `diagnostic_updater::Updater` + `DiagnosticStatusWrapper` | nano-ros's own `diagnostic_updater` | Publishes `/diagnostics`. |
| `rclcpp_action::Server<A> / Client<A>` | ours, under upstream's names | Hosted builds take upstream's goal-handle shapes as written; freestanding builds key goals by id. See [Actions with upstream's goal handles](#actions-with-upstreams-goal-handles-hosted). |
| `class MyNode : public rclcpp_lifecycle::LifecycleNode` | ours, under upstream's name (`<rclcpp_lifecycle/lifecycle_node.hpp>`) | It IS an `rclcpp::Node`. The `on_configure(const State&)` … `on_shutdown` overrides, `CallbackReturn::SUCCESS`, `configure()` / `activate()` / … and `get_current_state()` keep upstream's signatures; the REP-2002 services are registered by the constructor. `create_publisher<M>(topic, qos)` returns a managed `LifecyclePublisher<M>::SharedPtr` that sends only while Active. One managed node per executor. |
| `RCLCPP_COMPONENTS_REGISTER_NODE(class)` + `rclcpp_components_register_node(lib PLUGIN "ns::Class" EXECUTABLE exe)` | the macro defines the component's factory in its own source file; the cmake verb emits a thin `int main()` per registration that declares the class and calls that factory | Single binary, no class_loader. The PLUGIN's qualifiers must be namespaces (a class nested in a class cannot be forward-declared). A component library built `SHARED` is built static. |
| plain CMake components: `add_library(<c> SHARED …)`, `ament_target_dependencies(<c> rclcpp …)`, `rclcpp_components_register_nodes(<c> "ns::Class")`, a hand-written main (`add_executable` + `target_link_libraries`) | each `SHARED` or `MODULE` library builds static, and the configure prints a line saying so; the plural verb and `ament_environment_hooks` are accepted and do nothing | One image is one binary with one runtime. `ament_target_dependencies` works beside a `target_link_libraries` in either signature (plain or `PRIVATE`). A hand-written main gets the link pass the ament verbs give theirs. `IMPORTED` libraries keep their type. A project that overrides `add_library` itself replaces this. |
| `find_package(ament_cmake_auto / rclcpp / rclcpp_components / diagnostic_updater / std_msgs / …)` | find modules in the SDK root's `cmake/find/` | ~28 of the most-cited ROS 2 packages; a message package not among them is found by the resolver from `NROS_INTERFACE_SEARCH_PATH` / `AMENT_PREFIX_PATH`. |
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
| a package that depends on `rclcpp_lifecycle` | its runtime carries `lifecycle` (with `param_services` on a host), and `ros2 lifecycle get /<node>` finds it | On a board the cmake road gives it `lifecycle` alone. A Zephyr application states it in `system.toml`: `[system] features = ["lifecycle"]`. |
| `std::static_pointer_cast<Derived>(shared_from_this())` (Nav2's `nav2_util::LifecycleNode`) | **does not compile** | The handle owns nothing, so there is no `std::shared_ptr` to cast. Drop the override; `rclcpp_lifecycle::LifecycleNode::shared_from_this()` returns its own type. |
| `pub_->get_subscription_count()` | takes the executor and an out-parameter | See `Publisher::get_subscription_count`. Nav2 calls it only in its tests. |
| `declare_parameter(name, rclcpp::ParameterValue(...)[, descriptor])`, `get_parameter(name)` as an `rclcpp::Parameter`, `set_parameter(rclcpp::Parameter(...))` | the same store the typed `declare_parameter<T>` uses; the descriptor may be an `rcl_interfaces::msg::ParameterDescriptor` | Hosted builds. A byte or string array can be built but not declared. A wrong-type `as_double()` answers `0.0`, not an exception. |
| `add_on_set_parameters_callback(std::bind(&C::cb, this, _1))` with `cb(std::vector<rclcpp::Parameter>)` returning `SetParametersResult` | runs on every write, local or `ros2 param set`; `successful = false` refuses it; the returned handle unregisters on destruction | One parameter per call, because writes are validated one at a time. The `reason` does not reach the remote caller. Keep the handle as a member of your node class, so it goes before the node does. |
| `node->get_node_parameters_interface()` | `has_parameter`, `declare_parameter`, `get_parameter` and `get_parameter_overrides()` through `->` | `get_parameter_overrides()` lists this node's `-p` / `--params-file` values as a `std::map<std::string, rclcpp::ParameterValue>`, one copy per node, so Nav2's `declare_or_get_parameter` compiles. |
| `pub_->publish(std::move(msg))` with a `std::unique_ptr<M>` | publishes `*msg`; the message is freed when the call returns | Hosted builds. Every publish serializes, so there is no intra-process hand-over to save a copy. A null pointer is refused and logged. |
| `rclcpp::Client<S>::SharedPtr client_ = create_client<S>(name);` + `client_->async_send_request(req, callback)` with a `void(Client<S>::SharedFuture)` callback | the handle holds the poll client; the callback runs from the executor when the reply arrives, with a `SharedFuture` whose `get()` returns it | Hosted builds with `std::promise`. The client registers one 1 ms timer for its callbacks, cancelled while none is waiting. A removed request's callback never runs. `wait_for_service(1s)` inside a callback waits without spinning, so on a polled backend it runs out its budget. On an RTOS toolchain (no `std::promise`) store the client as `rclcpp::Owned<rclcpp::PollClient<S>>`, keep the `FutureAndRequestId` that `async_send_request(req)` returns, and read it on a later callback with `future.is_ready()` and `get()`. |
| `diagnostic_updater::Updater updater_{this};` | publishes `/diagnostics` on its own timer, every `period` (`setPeriod`), as upstream's does | `update()` still works and is no longer required. |
| `DiagnosticTask`, `FrequencyStatus`, `TimeStampStatus`, `Heartbeat`, `HeaderlessTopicDiagnostic`, `TopicDiagnostic` | upstream's classes, levels and messages; `updater.add(task)` | A clock argument is a `const rclcpp::Clock*`, so `node->get_clock()` still passes. The tasks take no lock, so a node that calls `tick()` from its own thread must keep that call from overlapping an update. `DiagnosedPublisher` is absent, because a nano-ros publisher handle can be moved but not copied. Keep the publisher and a `TopicDiagnostic`, and call `tick(stamp)` next to each `publish`. |
| the six parameter services (`ros2 param get/set/list`) | on for an ament package built on a host: every node serves them, as upstream's does | Comes from the package's `package.xml` naming `ament_cmake`; set `NROS_FEATURES` to declare the capabilities yourself. Values come from `--ros-args --params-file <yaml>` or `-p`. On a board they are off unless declared (`param_services` in `NROS_FEATURES`, or `[system] features` on Zephyr): they cost six queryables and their RAM. |

## Measured: the stock rclcpp examples

Each file below, from [ros2/examples](https://github.com/ros2/examples)
(Humble and Jazzy), is compiled, linked and booted beside its port. The table
is rendered from the record the test holds the counts to, so it cannot drift:

<!-- rclcpp-examples:begin (generated by scripts/gen-rclcpp-examples-table.py) -->
| Example | Edited lines | What the edits are |
| --- | --- | --- |
| `rclcpp/topics/minimal_publisher/lambda.cpp` | 0 | — |
| `rclcpp/topics/minimal_publisher/member_function.cpp` | 0 | — |
| `rclcpp/topics/minimal_publisher/not_composable.cpp` | 11 | exceptions: the try/catch around publish + spin_some is removed (-fno-exceptions; there is no rclcpp::exceptions::RCLError) |
| `rclcpp/topics/minimal_subscriber/lambda.cpp` | 0 | — |
| `rclcpp/topics/minimal_subscriber/member_function.cpp` | 0 | — |
| `rclcpp/topics/minimal_subscriber/not_composable.cpp` | 3 | global Node::SharedPtr from a temporary: `g_node = rclcpp::Node::make_shared(...)` is refused because Node::SharedPtr does not own; keep the shared_ptr in a local (`auto node = ...; g_node = node;`) |
| `rclcpp/timers/minimal_timer/lambda.cpp` | 0 | — |
| `rclcpp/timers/minimal_timer/member_function.cpp` | 0 | — |
| `rclcpp/services/minimal_service/main.cpp` | 3 | global Node::SharedPtr from a temporary (as minimal_subscriber_not_composable) |
| `rclcpp/services/minimal_client/main.cpp` | 0 | — |
| `rclcpp/actions/minimal_action_server/member_functions.cpp` | 6 | no hidden threads: the stock handle_accepted detaches a std::thread per goal, and a goal handle used from a thread the application started races the executor (no executor lock on a one-level image). The port runs execute(goal_handle) in the accepted callback and drops rclcpp::Rate::sleep, which would re-enter the executor from that callback |
| `rclcpp/actions/minimal_action_client/member_functions.cpp` | 2 | generated sequences are FixedSequence/HeapSequence, not std::vector: `.back()` is spelled `[length() - 1]` (adding `back()` would move the codegen version surface, for one line) |
| `rclcpp/executors/multithreaded_executor/multithreaded_executor.cpp` | 18 | MultiThreadedExecutor is refused (one task per executor, no hidden threads) — renamed to SingleThreadedExecutor; callback groups: create_callback_group(CallbackGroupType) / SubscriptionOptions::callback_group / CallbackGroup::SharedPtr have no counterpart (every group is mutually exclusive on one task); the group plumbing is deleted |
<!-- rclcpp-examples:end -->

## Two surfaces: freestanding, and `NROS_CPP_STD`

One set of headers ships two API shapes, and a build picks one.

**The freestanding surface is the default and it is not a fallback.** It is
plain C++14 with no standard library, no exceptions and no RTTI:
`rclcpp::Node`, `create_*` writing through an out-reference and returning an
`rclcpp::Result`, `const char*` for names, `uint64_t period_ms` for durations.
It is the surface for every node written for nano-ros, native host builds
included.

**The std surface is an opt-in and it is reached only by defining
`NROS_CPP_STD`.** Defining it turns on the six `NROS_CPP_HAS_*` capability
macros in `rclcpp/detail/std_detect.hpp`, and with them: the `std::string` /
`std::function` / `std::chrono` / `std::vector` convenience overloads in
`nros/std_compat.hpp`, the `RCLCPP_*_STREAM` logging macros, and — the piece
that actually motivates the flag — `rclcpp::Node`'s upstream-shaped
factories, which return `std::shared_ptr` and take `const std::string&`
names.

### Do I need this flag?

**The split is *ported rclcpp code* versus *code written for nano-ros*. It is
not host versus embedded.** A native Linux build does not want the flag any
more than a Cortex-M3 build does; a ported node wants it on both.

| You are… | Flag | Why |
| --- | --- | --- |
| writing a new node against `rclcpp::Node` / `rclcpp::Publisher<M>` / `rclcpp::Executor` | **no** | The freestanding surface is the whole API you are using. This is the common case on every platform, native included. |
| compiling an upstream ROS 2 `.cpp` unmodified — it says `class MyNode : public rclcpp::Node`, `std::make_shared<MyNode>()`, `create_publisher<M>(...)` returning a `SharedPtr` | **yes** | Those signatures are *spelled in* `std::shared_ptr` and `std::string`. Without the flag they do not exist and you get a compile error naming a missing overload. |
| passing a `std::string` topic name or a `std::chrono` period into an otherwise nano-ros-native file | **yes** | Those are the `std_compat.hpp` overloads. |
| building for Zephyr, FreeRTOS, NuttX, ThreadX, or any `-ffreestanding` toolchain | **no** | And here it is not merely unnecessary: asking for the std surface asks the headers to `#include <string>`, which on those toolchains is either absent or a hard `#error`. |

If you are unsure, build without it. The failure direction is loud: a missing
overload at compile time, never a silent behaviour change.

### Turning it on

Define it on the **target**, not per file:

```cmake
target_compile_definitions(my_ported_node PRIVATE NROS_CPP_STD=1)
```

Per target rather than per `#define`-before-include, because the macro changes
the *layout* of `rclcpp::Node` (it carries hosted-only members and an
`enable_shared_from_this` base), so two translation units of one image that
disagree about it are an ODR/layout break rather than a missing function. An
image may set the flag on one module and not another, so keep the definition
at target granularity and give every translation unit of a target the same
answer.

**A ported package usually needs none of this.** nano-ros's ament surface sets
`NROS_CPP_STD=1` on every target its `ament_*` verbs create, and
`find_package(rclcpp)` sets it on whatever links `rclcpp::rclcpp`, so a ported
package already has the surface it needs; see
[Porting a ROS 2 C++ node](../guide/port/port-a-cpp-node.md).

### The ported shape

With the flag set, an upstream node's own spelling compiles: `std::make_shared`,
`SharedPtr` returns, a `std::string` topic, a QoS depth, a capturing lambda and
a `std::chrono` period.

```cpp
#include <rclcpp/rclcpp.hpp>
#include "std_msgs/std_msgs.hpp"

using namespace std::chrono_literals;

int main(int argc, char** argv) {
    rclcpp::init(argc, argv);
    auto node = std::make_shared<rclcpp::Node>("talker");
    auto pub = node->create_publisher<std_msgs::msg::Int32>("chatter", 10);
    int count = 0;
    auto timer = node->create_wall_timer(1s, [&]() {
        std_msgs::msg::Int32 msg;
        msg.data = ++count;
        pub->publish(msg);
    });
    rclcpp::spin(node);
    rclcpp::shutdown();
}
```

A failed `create_*` here ABORTS, naming the call, because upstream throws and
nano-ros has no exceptions. To handle the failure instead, use the
`Result`-returning forms above.

### Why it is not detected for you

Earlier releases *discovered* the capability macros from the include path with
`__has_include`, so a hosted compiler handed you the std-flavoured API whether
you asked for it or not. That was removed because no probe answers the
question correctly on both embedded toolchains:

| | `__STDC_HOSTED__` | `__has_include(<string>)` | `#include <string>` |
| --- | --- | --- | --- |
| arm-none-eabi 13.2, `-ffreestanding` (FreeRTOS) | 0 | TRUE | hard `#error` |
| Zephyr `-nostdinc++`, minimal libcpp | 1 | FALSE | absent |
| hosted g++ 12.3 | 1 | TRUE | works |

Each probe is right on exactly one embedded toolchain and wrong on the other:
libstdc++ 13 added `bits/requires_hosted.h`, so header *presence* stopped
implying *usability*. Only the request is right on both, because it is not a
probe — it is you saying which surface you want. The full measurement is in
the comments of `rclcpp/detail/std_detect.hpp`.

## Hosted-only shapes

### Subscriptions with `MessageInfo` (hosted)

On a hosted build (`NROS_CPP_STD`) a dispatch subscription's callback may take
upstream's `const rclcpp::MessageInfo&` as a second argument, after the message
in any of its four forms (`const M&`, `std::shared_ptr<const M>`,
`std::shared_ptr<M>`, `std::unique_ptr<M>`):

```cpp
auto sub = node->create_subscription<std_msgs::msg::Int32>(
    "/chatter", 10, [](const std_msgs::msg::Int32& msg, const rclcpp::MessageInfo& info) {
        const auto& r = info.get_rmw_message_info();
        printf("%d from seq %llu at %lld ns\n", msg.data,
               (unsigned long long)r.publication_sequence_number, (long long)r.source_timestamp);
    });
```

`get_rmw_message_info()` carries `source_timestamp`, `received_timestamp`,
`publication_sequence_number`, `reception_sequence_number` and the 24-byte
`publisher_gid`, as the backend reported them. A backend that reports none
(XRCE, uORB) leaves every field zero. `from_intra_process` is always `false`.
A freestanding build refuses the shape at compile time: the metadata path keeps
the callable on the heap. `SubscriptionOptions::message_info = true` with a
callback that takes no `MessageInfo` aborts the create, because it would
otherwise configure nothing.

### Actions with upstream's goal handles (hosted)

On a hosted build (`NROS_CPP_STD`) the stock `rclcpp_action` spellings are
drop-ins over the two classes above:

```cpp
using Fibonacci = example_interfaces::action::Fibonacci;
using GoalHandle = rclcpp_action::ServerGoalHandle<Fibonacci>;
using namespace std::placeholders;

// Server: free factory over the node interfaces, std::bind-ed members.
rclcpp_action::Server<Fibonacci>::SharedPtr server = rclcpp_action::create_server<Fibonacci>(
    get_node_base_interface(), get_node_clock_interface(), get_node_logging_interface(),
    get_node_waitables_interface(), "fibonacci",
    std::bind(&MyNode::handle_goal, this, _1, _2),    // (const GoalUUID&, shared_ptr<const Goal>)
    std::bind(&MyNode::handle_cancel, this, _1),      // (shared_ptr<GoalHandle>)
    std::bind(&MyNode::handle_accepted, this, _1));   // (shared_ptr<GoalHandle>)
// in handle_accepted: goal_handle->get_goal(), ->publish_feedback(fb),
// ->is_canceling(), ->succeed(result) / ->abort(result) / ->canceled(result)

// Client: SendGoalOptions' upstream fields, bound per goal by async_send_goal.
auto client = rclcpp_action::create_client<Fibonacci>(this, "fibonacci");
auto opts = rclcpp_action::Client<Fibonacci>::SendGoalOptions();
opts.goal_response_callback = [](rclcpp_action::ClientGoalHandle<Fibonacci>::SharedPtr h) {};
opts.feedback_callback = [](auto h, const std::shared_ptr<const Fibonacci::Feedback> fb) {};
opts.result_callback = [](const rclcpp_action::Client<Fibonacci>::WrappedResult& r) {
    // r.code: rclcpp_action::ResultCode::SUCCEEDED / ABORTED / CANCELED; r.result
};
auto fut = client->async_send_goal(goal, opts);   // rclcpp::Future<GoalAccept>

// The handle-taking calls return upstream's std::shared_future.
auto result = client->async_get_result(goal_handle);   // shared_future<WrappedResult>
auto cancel = client->async_cancel_goal(goal_handle);  // shared_future<CancelResponse::SharedPtr>
if (rclcpp::spin_until_future_complete(node, result) == rclcpp::FutureReturnCode::SUCCESS) {
    const auto& wrapped = result.get();
}
// server, after handle_goal returned ACCEPT_AND_DEFER:
goal_handle->execute();   // ACCEPTED -> EXECUTING, status array published
```

`X<A>::SharedPtr` is `rclcpp::Owned<X<A>>` (the entity by value), and the
`get_node_*_interface()` handles name the node and do nothing else. A goal
handle is a heap token over the goal id: every call forwards to the server or
client keyed by it, and the goal's state stays in the arena. Drive it from the
executor's task only — not from a thread you started — and do not move the
server while a handle is in use. The callbacks run from the executor's
dispatch, beside the client-wide `set_callbacks` ones. `async_send_goal`
returns the goal-keyed `rclcpp::Future<GoalAccept>`, not a
`std::shared_future`; `get_goal_stamp` is absent.

`async_get_result` / `async_cancel_goal` / `async_cancel_all_goals` /
`async_cancel_goals_before` return a
`rclcpp_action::detail::DrivenSharedFuture<T>`, which IS-A
`std::shared_future<T>`: its `is_ready()` (what `spin_until_future_complete`
polls), `wait_for` and `get` drive the client's dispatch and executor while
they wait, since no other thread fulfils it. Converted to a bare
`std::shared_future` it still completes, but only when something else drives
the client. Every `async_get_result` for one goal returns the handle's own
future; a result-aware goal's handle is kept by the client until its result
arrives, as upstream's pending request keeps it. Cancel replies are taken by
the executor's spin, as upstream's executor delivers them: the spin settles
the waiting `async_cancel_*` future and runs its `cancel_callback`, so a
caller that keeps only the callback needs no `poll()`. A nano-ros server acts on the zero-id cancel-all / cancel-before forms
as an rclcpp server does. A reply no future
claims is kept for `try_recv_cancel_response`, oldest first, up to
`rclcpp_action::CANCEL_STASH_CAP` (4) per client. These need `std::promise`,
so they are hosted-only twice over: a libstdc++ built without threads
(arm-none-eabi) refuses them as a freestanding build does.

`ServerGoalHandle::execute()` and `Server<A>::execute(goal_id)` move an
ACCEPTED goal to EXECUTING and publish the status array; the latter reports
`NotFound` / `NotAllowed` where upstream raises. `create_server` executes an
`ACCEPT_AND_EXECUTE` goal before `handle_accepted`; the goal-id API leaves it
ACCEPTED.

**Goal slots.** Each action sub-client holds at most `MAX_CONCURRENT_GOALS` (4)
requests in flight. A future dropped (or `cancel()`ed) before its reply frees
its request's slot, unless something else still waits on that reply — a
client-wide `set_callbacks` hook, or an upstream goal (`async_send_goal` with
callbacks, `async_get_result`). A late reply for a freed slot is dropped. On a
freestanding build each of these shapes is a compile error
(`NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE`) naming the goal-id-keyed form above;
`create_client` alone works there.

## Action client futures

**Future-based async.** `send_goal_future()` / `get_result_future()` return a
`Future<T>` keyed by its goal: each future receives only its own goal's
acceptance or result, whether the executor dispatched it during `spin_once` or
the future's own poll took it, and a result for a goal nobody waits on is
dropped. At most `MAX_CONCURRENT_GOALS` of each may be pending per client;
past that the send is refused and the future is empty (`valid()` false):

```cpp
auto fut = client.send_goal_future(goal);
typename decltype(client)::GoalAccept accept;
RCLCPP_TRY(fut.wait(executor.handle(), 5000, accept));
if (accept.accepted) {
    auto rfut = client.get_result_future(accept.goal_id);
    Fibonacci::Result result;
    RCLCPP_TRY(rfut.wait(executor.handle(), 10000, result));
}
```

`GoalAccept` is a nested type on `rclcpp_action::Client<A>` (16-byte UUID +
`bool accepted`).

## Parameters and `/parameter_events`

Parameters are methods on the node (`declare_parameter<T>`, `get_parameter<T>`,
`set_parameter<T>`, `undeclare_parameter`), over the executor's one store.
`node.start_parameter_services()` serves them on the six `rcl_interfaces`
services, so `ros2 param` reaches them, and publishes `/parameter_events`
(`rcl_interfaces/msg/ParameterEvent`, `rmw_qos_profile_parameter_events`) as
rclcpp's node does:

```cpp
rclcpp::Node node;
RCLCPP_TRY(executor.create_node(node, "ctrl"));
node.declare_parameter<int64_t>("gain", 3);   // before or after: both reported
RCLCPP_TRY(node.start_parameter_services());  // start-up event: new_parameters = [gain]
node.set_parameter<int64_t>("gain", 4);       // changed_parameters = [gain]
node.undeclare_parameter("gain");             // deleted_parameters = [gain]
```

Each event names the node's fully qualified name in `node`; the events are
published on the next spin. The image needs the `param_services` capability.
One publisher per executor serves every node. It is on for every served node
unless the node was built with
`rclcpp::NodeOptions().start_parameter_event_publisher(false)`, which opts
that one node out; the other nodes keep publishing.

To react to OTHER nodes' changes, a hosted build has
`rclcpp::ParameterEventHandler`. It subscribes to `/parameter_events` once, on
the node you give it:

```cpp
auto handler = std::make_shared<rclcpp::ParameterEventHandler>(node);
auto h = handler->add_parameter_callback(
    "gain", [](const rclcpp::Parameter& p) { use(p.as_int()); }, "/controller");
auto e = handler->add_parameter_event_callback(
    [](const rcl_interfaces::msg::ParameterEvent& ev) { /* every event */ });
```

Keep the handles: dropping one unregisters it, as upstream's do. An empty node
name means this node, and a relative one resolves against this node's
namespace. A parameter callback sees new and changed values, never deletions.
An event callback receives the type it takes, so `rcl_interfaces` must be
generated for the image. The per-parameter handle is
`rclcpp::ParameterCallbackHandle`, as upstream (the handler's nested
`ParameterEventHandler::ParameterCallbackHandle` names the same type). A
freestanding build refuses the handler at compile time; there, subscribe to
`/parameter_events` with `create_subscription` yourself.

## Design background

- [RFC-0089: ROS 2 API adoption, and the compile-or-conform rule](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md) — upstream's namespaces and names, the `_in` rule, the hosted surface
- [RFC-0018: C++ API design](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0018-cpp-api-design.md) — freestanding C++14, no exceptions, the `Result` channel

# C++ API Guide

The nros C++ API (`nros-cpp`) provides a freestanding C++14 interface for embedded ROS 2 development. It mirrors rclcpp naming conventions while requiring no standard library, exceptions, or RTTI — making it suitable for Zephyr, FreeRTOS, NuttX, ThreadX, and bare-metal targets.

## Overview

- **Freestanding C++14** — no STL dependency in default mode
- **Direct Rust FFI** — wraps `nros-node` directly via typed `extern "C"` FFI (not the C API), preserving type safety per message type
- **rclcpp naming, in upstream's namespaces** — `rclcpp::Node`, `Publisher<M>`, `Subscription<M>`, `Service<S>`, `Client<S>`, `Timer`, `GuardCondition`, `Executor`, and `rclcpp_action::Server<A>` / `Client<A>`. There is no `nros::` namespace: every user-facing name is in `rclcpp::`, `rclcpp_action::` or `rclcpp_lifecycle::` (RFC-0089, phase-483). The RTOS extensions with no upstream counterpart (`Poll*`, the `*Storage` types, `FixedString`, …) are in `rclcpp::` too
- **Result-based error handling** — `rclcpp::Result` + `NROS_TRY` macro (no exceptions)
- **Generated message types** — `std_msgs::msg::Int32`, `example_interfaces::srv::AddTwoInts`, etc.
- **Opt-in std surface** — `NROS_CPP_STD` enables `std::string`, `std::function`, `std::chrono` conveniences and `rclcpp::Node`. It is a PORTING surface, requested by the build; it is never detected from the toolchain

## Building with CMake

### Native Linux

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_app LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory(<path-to-nano-ros> nano_ros)

nano_ros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    LANGUAGE CPP
    SKIP_INSTALL
)

add_executable(my_app src/main.cpp)
target_link_libraries(my_app
    PRIVATE
        std_msgs__nano_ros_cpp
        NanoRos::NanoRosCpp
)
nros_platform_link_app(my_app)
```

That is the out-of-tree spelling, where the consumer owns the whole
`CMakeLists.txt` and pulls nano-ros in by path. The in-tree examples use the
ament shape instead — `find_package(nano_ros REQUIRED)` +
`nano_ros_add_executable()` — and take the board, RMW, domain and network
identity from a `system.toml` beside the `CMakeLists.txt` rather than from
`set(NANO_ROS_RMW …)` or a `-D` flag. See the FreeRTOS section below for what
that file holds.

The `nano_ros_generate_interfaces()` function:
1. Resolves `.msg`/`.srv`/`.action` files (local directory, ament index, or bundled)
2. Generates C++ headers (`.hpp`) and Rust FFI glue (`.rs`)
3. Compiles the FFI glue into a static library via Corrosion
4. Creates a `<pkg>__nano_ros_cpp` CMake target with include paths and FFI library linkage

### FreeRTOS (ARM Cortex-M3)

Same shape as the native (host) build, with one difference: **the board and the
RMW are not on the command line.** They are in the example's `system.toml`,
beside its `CMakeLists.txt` (RFC-0098 D3/D5) — verbatim from
`examples/mps2-an385-freertos/cpp/talker/system.toml`:

```toml
[system]
name      = "freertos_cpp_talker"
rmw       = "zenoh"
domain_id = 0
locator   = "tcp/192.0.3.1:7447"

[image.mps2-an385-freertos]
board = "mps2-an385-freertos"
```

`find_package(nano_ros)` reads that through `nros ws leaf-system` and derives
the platform (`freertos`) from the board, so `-DNANO_ROS_PLATFORM`,
`-DNANO_ROS_BOARD` and `-DNROS_RMW` are retired as the way a user chooses.
Switching board is editing `[image.*] board`. Phase 138's
`cmake/platform/nano-ros-freertos.cmake` +
`cmake/board/nano-ros-board-mps2-an385-freertos.cmake` still compose the kernel
+ lwIP + LAN9118 driver in-tree; no install step needed.

The one thing a cross build still needs on the command line is the toolchain
file, because CMake pins the compiler at the first configure:

```bash
cmake -S examples/mps2-an385-freertos/cpp/talker -B build/talker \
    -DCMAKE_TOOLCHAIN_FILE=$(pwd)/cmake/toolchain/arm-freertos-armcm3.cmake
cmake --build build/talker
```

`nros build` is what maps a board to its toolchain file, so this hand-passed
flag goes away once a single-package C/C++ leaf can use it —
[issue 1296](../issues/1296-nros-build-c-leaf-bringup-name-mismatch.md) is why
it cannot today. `nros init` + `cmake --preset <board>` is the other way to
avoid spelling the path (the presets `nros setup <board>` wrote).

A single-package C/C++ leaf needs **no `nros sync`**: its message bindings are
a CMake-time output, so a clean copy of the directory configures and builds with
no sync at all. Workspaces and Rust leaves do need it.

### Zephyr

```cmake
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app LANGUAGES CXX)

nros_generate_interfaces(std_msgs LANGUAGE CPP)
target_sources(app PRIVATE src/main.cpp)
```

Requires `CONFIG_NROS=y` and `CONFIG_NROS_CPP_API=y` in `prj.conf`.

## Core API

### Initialization

```cpp
#include <nros/nros.hpp>

// Global init (simple applications). `init_in` is the Result-returning form:
// upstream's `rclcpp::init(argc, argv)` returns `void`, so the name differs
// where the contract does (RFC-0089's `_in` rule).
rclcpp::Result ret = rclcpp::init_in("tcp/127.0.0.1:7447", 0);

// Or explicit executor (multi-executor patterns)
rclcpp::Executor executor;
NROS_TRY(rclcpp::Executor::create(executor, "tcp/127.0.0.1:7447"));
```

### Node

```cpp
rclcpp::Node node;
NROS_TRY(rclcpp::create_node(node, "my_node"));

// Or with explicit executor:
NROS_TRY(executor.create_node(node, "my_node", "/namespace"));
```

### Publisher

```cpp
#include "std_msgs.hpp"

rclcpp::Publisher<std_msgs::msg::Int32> pub;
NROS_TRY(node.create_publisher(pub, "/chatter"));

std_msgs::msg::Int32 msg;
msg.data = 42;
NROS_TRY(pub.publish(msg));
```

### Subscription

Poll-style subscriptions — call `spin_once()` to drive I/O, then `take()` to check
for messages. The type is `rclcpp::PollSubscription<M>` (phase-456 W2b): the
caller owns the subscriber, so the caller is the one that can take from it.
`rclcpp::Subscription<M>` is the DISPATCH subscription, whose samples the
executor arena delivers to a callback.

```cpp
rclcpp::PollSubscription<std_msgs::msg::Int32> sub;
NROS_TRY(node.create_subscription(sub, "/chatter"));

rclcpp::spin_once(100);

std_msgs::msg::Int32 msg;
if (sub.take(msg).ok()) {
    printf("Received: %d\n", msg.data);
}
```

### Service Server

```cpp
#include "example_interfaces.hpp"

using AddTwoInts = example_interfaces::srv::AddTwoInts;

// `rclcpp::PollService<S>` (phase-456 W5) — the caller owns the server and
// drains it. `rclcpp::Service<S>` is the DISPATCH server, which takes a handler.
rclcpp::PollService<AddTwoInts> srv;
NROS_TRY(node.create_service(srv, "/add_two_ints"));

// In your main loop:
rclcpp::spin_once(10);
AddTwoInts::Request req;
int64_t seq;
if (srv.take_request(req, seq).ok()) {
    AddTwoInts::Response resp;
    resp.sum = req.a + req.b;
    srv.send_response(seq, resp);
}
```

### Service Client

```cpp
// `rclcpp::PollClient<S>` (phase-456 W9) — the caller owns the client and drains
// the reply. `rclcpp::Client<S>` is the DISPATCH client, whose one verb is
// `async_send_request` and whose reply reaches a handler during `spin_once`.
rclcpp::PollClient<AddTwoInts> client;
NROS_TRY(node.create_client(client, "/add_two_ints"));

AddTwoInts::Request req;
req.a = 1; req.b = 2;
AddTwoInts::Response resp;
NROS_TRY(client.call(req, resp));
// resp.sum == 3
```

### Action Server

`rclcpp_action::Server<A>` is a DISPATCH server: the executor answers goal and
cancel requests through callbacks you install, and the goal itself is driven
through the server by its 16-byte id. Upstream hands `handle_accepted` an owning
`std::shared_ptr<ServerGoalHandle>`; here the goals live in the server's static
arena, so the callback receives the id instead. A polled form, where the caller
takes each goal request itself, is `rclcpp::PollingActionServer<A>`.

```cpp
#include "example_interfaces.hpp"

using Fibonacci = example_interfaces::action::Fibonacci;

rclcpp_action::Server<Fibonacci> srv;
NROS_TRY(node.create_action_server(srv, "/fibonacci"));

// Stateless callables: an empty-capture lambda or a plain function pointer.
// The `*_with_ctx` forms add a `void*` for state.
NROS_TRY(srv.set_goal_callback([](const uint8_t id[16], const Fibonacci::Goal& goal) {
    return goal.order > 0 ? rclcpp_action::GoalResponse::ACCEPT_AND_EXECUTE
                          : rclcpp_action::GoalResponse::REJECT;
}));
NROS_TRY(srv.set_accepted_callback([](const uint8_t id[16]) {
    // Record `id`; the work happens outside the callback, which must return
    // promptly.
}));

// Later, from the main loop, for an accepted goal `goal_id`:
Fibonacci::Feedback fb;
// ... fill feedback ...
srv.publish_feedback(goal_id, fb);

Fibonacci::Result result;
// ... fill result ...
srv.complete_goal(goal_id, result);  // GoalStatus::Succeeded; another overload takes a status
```

### Action Client

`rclcpp_action::Client<A>` is an arena-storage handle: the goal, feedback, and result buffers (plus the four underlying transport channels) live in fixed-size storage inside the `Client<A>` instance itself. Nothing is heap-allocated per `send_goal` call, and the type is move-only — moves go through `nros_cpp_action_client_relocate` so the trampoline `context` pointer follows the new `this`.

**Blocking convenience.** `send_goal()` and `get_result()` spin the executor internally until the server replies or the per-call timeout expires:

```cpp
rclcpp_action::Client<Fibonacci> client;
NROS_TRY(node.create_action_client(client, "/fibonacci"));

Fibonacci::Goal goal;
goal.order = 10;
uint8_t goal_id[16];
NROS_TRY(client.send_goal(goal, goal_id));

Fibonacci::Result result;
NROS_TRY(client.get_result(goal_id, result));
```

These helpers are syntactic sugar over `send_goal_async()` + `spin_once()`; they cannot be called from inside a dispatch callback (returns `NROS_RET_REENTRANT`).

**Future-based async.** `send_goal_future()` / `get_result_future()` return a `Future<T>` that polls the same arena slot:

```cpp
auto fut = client.send_goal_future(goal);
typename decltype(client)::GoalAccept accept;
NROS_TRY(fut.wait(executor.handle(), 5000, accept));
if (accept.accepted) {
    auto rfut = client.get_result_future(accept.goal_id);
    Fibonacci::Result result;
    NROS_TRY(rfut.wait(executor.handle(), 10000, result));
}
```

`GoalAccept` is a nested type on `rclcpp_action::Client<A>` (16-byte UUID + `bool accepted`).

**Feedback.** Feedback is not goal-scoped at the stream layer — `feedback_stream()` yields `FeedbackType` across every active goal for this client:

```cpp
auto& fb_stream = client.feedback_stream();
Fibonacci::Feedback fb;
while (fb_stream.try_next(fb).ok()) { /* ... */ }
// Or blocking:
NROS_TRY(fb_stream.wait_next(executor.handle(), 500, fb));
```

For per-goal feedback dispatch, use the callback API.

**Callback API: `SendGoalOptions` + `set_callbacks()`.** This is the rclcpp-style entry point. `SendGoalOptions` is a nested POD on `rclcpp_action::Client<A>`; populate the three function-pointer fields (`goal_response`, `feedback`, `result`) plus an optional `context` pointer, then install once. Callbacks fire from `spin_once()`:

```cpp
typename decltype(client)::SendGoalOptions opts;
opts.goal_response = [](bool accepted, const uint8_t id[16], void* ctx) {
    auto* state = static_cast<MyState*>(ctx);
    state->accepted = accepted;
    std::memcpy(state->goal_id, id, 16);
};
opts.feedback = [](const uint8_t id[16], const uint8_t* data, size_t len, void* ctx) {
    Fibonacci::Feedback fb;
    if (Fibonacci::Feedback::ffi_deserialize(data, len, &fb) == 0) {
        // dispatch on `id`
    }
};
opts.result = [](const uint8_t id[16], int32_t status, const uint8_t* data, size_t len, void* ctx) {
    // status: 4=Succeeded, 5=Canceled, 6=Aborted
};
opts.context = &my_state;
NROS_TRY(client.set_callbacks(opts));

NROS_TRY(client.send_goal_async(goal, goal_id));  // fire-and-forget
while (!my_state.done) { rclcpp::spin_once(10); }
```

Because callback storage lives in the arena, `set_callbacks()` may be called before or after `send_goal_async()` — the executor's trampoline reads the latest pointers on each dispatch. The C++-side trampoline always stashes the most recent feedback / result bytes too, so the same `Client` can drive `feedback_stream().try_next()` and `get_result_future().wait()` even with callbacks installed.

### Timer

Timers fire during `spin_once()`:

```cpp
void on_timer(void* ctx) {
    // periodic work
}

rclcpp::Timer timer;
NROS_TRY(node.create_wall_timer(timer, 1000, on_timer));      // 1000ms period
NROS_TRY(node.create_timer_oneshot(timer, 5000, on_timer)); // one-shot after 5s

timer.cancel();
timer.reset();  // restart from zero
```

### GuardCondition

Guard conditions allow cross-thread signaling:

```cpp
void on_signal(void* ctx) {
    // handle event
}

rclcpp::GuardCondition guard;
NROS_TRY(node.create_guard_condition(guard, on_signal));

// From another thread:
guard.trigger();
// Callback fires on next spin_once()
```

### Executor

```cpp
rclcpp::Executor executor;
NROS_TRY(rclcpp::Executor::create(executor));

rclcpp::Node node;
NROS_TRY(executor.create_node(node, "my_node"));

// Create publishers, subscriptions, etc. on node...

while (executor.ok()) {
    executor.spin_once(10);
}
executor.shutdown();
```

### Spinning

```cpp
// Global spin (after rclcpp::init_in())
rclcpp::spin_once(10);           // single poll, 10ms timeout
rclcpp::spin_in(5000);           // spin for 5 seconds
rclcpp::spin_in(5000, 50);       // spin for 5s, 50ms poll interval
rclcpp::spin_in();               // until rclcpp::shutdown() flips rclcpp::ok()

// Explicit executor
executor.spin_once(10);
executor.spin_for(5000, 50);    // bounded; executor.spin() blocks until shutdown
```

## Error Handling

All fallible operations return `rclcpp::Result`. Use `NROS_TRY` for early return:

```cpp
rclcpp::Result setup() {
    NROS_TRY(rclcpp::init_in());
    NROS_TRY(rclcpp::create_node(node, "my_node"));
    NROS_TRY(node.create_publisher(pub, "/topic"));
    return rclcpp::Result::success();
}
```

Check results manually when needed:

```cpp
rclcpp::Result ret = pub.publish(msg);
if (!ret.ok()) {
    printf("Error: %d\n", ret.raw());
}
```

Error codes (`rclcpp::ErrorCode`):
- `Ok` (0) — success
- `Error` (-1) — generic error
- `Timeout` (-2) — operation timed out
- `InvalidArgument` (-3) — bad parameter
- `NotInitialized` (-4) — entity not initialized
- `Full` (-5) — buffer full
- `TransportError` (-100) — middleware transport failure

## Opt-in `std` Surface (`NROS_CPP_STD`)

`NROS_CPP_STD` selects the std-flavoured API: the STL convenience overloads
below, plus `rclcpp::Node` and its `std::shared_ptr`-returning factories.

**Ask for it when you are compiling ported rclcpp code; do not ask for it
otherwise.** The split is *whose code it is*, not *what it runs on* — a native
Linux build of a node written against `rclcpp::Node` stays on the freestanding
surface, and a ported upstream node wants this flag on a Cortex-M3 build just
as much as on a host. Nothing detects it for you: the headers do not probe the
include path, because no probe answers correctly on both embedded lanes (the
measurement is in `packages/api/nros-cpp/include/nros/std_detect.hpp`, and
before phase-438 the probe that was there broke every embedded C++ FreeRTOS
build — issue 1187).

Set it on the target, so every translation unit of an image agrees:

```cmake
target_compile_definitions(my_ported_node PRIVATE NROS_CPP_STD=1)
```

A per-file `#define NROS_CPP_STD` ahead of `<nros/nros.hpp>` compiles, but it
changes the layout of `rclcpp::Node`, so mixing it within one image is an ODR
break rather than a missing function.

If you build a ported package through nano-ros's ament surface
(`cmake/NanoRosAmentSurface.cmake`, `find_package(rclcpp)`), the flag is
already set on every target it creates or links and you need nothing here.

### The ported shape

With the flag set, an upstream node's own spelling compiles: `std::make_shared`,
`SharedPtr` returns, a `std::string` topic, a QoS depth, a capturing lambda and a
`std::chrono` period.

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
nano-ros has no exceptions (RFC-0018). To handle the failure instead, use the
`Result`-returning forms above. `examples/templates/cpp-port-minimal-publisher`
is the ROS 2 tutorial publisher in this shape, unmodified, with runtime tests on
posix, FreeRTOS and Zephyr.

## Zephyr Integration

### `prj.conf`

```ini
CONFIG_CPP=y
CONFIG_STD_CPP14=y

CONFIG_NROS=y
CONFIG_NROS_CPP_API=y
CONFIG_NROS_ZENOH_LOCATOR="tcp/192.0.2.2:7456"
CONFIG_NROS_DOMAIN_ID=0

CONFIG_POSIX_API=y
CONFIG_MAX_PTHREAD_MUTEX_COUNT=32
CONFIG_MAX_PTHREAD_COND_COUNT=16
```

### `CMakeLists.txt`

```cmake
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app LANGUAGES CXX)

nros_generate_interfaces(std_msgs LANGUAGE CPP)
target_sources(app PRIVATE src/main.cpp)
```

### `src/main.cpp`

```cpp
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

extern "C" {
#include <zpico_zephyr.h>
}

#include <nros/nros.hpp>
#include "std_msgs.hpp"

LOG_MODULE_REGISTER(my_app, LOG_LEVEL_INF);

int main(void)
{
    zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS);

    rclcpp::Result ret = rclcpp::init_in(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID);
    if (!ret.ok()) return 1;

    rclcpp::Node node;
    if (!rclcpp::create_node(node, "my_node").ok()) return 1;

    // ... create publishers, subscriptions, etc.

    while (true) {
        rclcpp::spin_once(100);
    }
}
```

## Examples

| Directory | Description |
|-----------|-------------|
| `examples/native/cpp/talker/` | Publish Int32 on `/chatter` (native Linux) |
| `examples/native/cpp/listener/` | Subscribe to `/chatter` (native Linux) |
| `examples/native/cpp/service-server/` | AddTwoInts server (native Linux) |
| `examples/native/cpp/service-client/` | AddTwoInts client (native Linux) |
| `examples/zephyr/cpp/talker/` | Publish Int32 on `/chatter` (Zephyr) |
| `examples/zephyr/cpp/listener/` | Subscribe to `/chatter` (Zephyr) |

## See Also

- [Creating Examples](../../book/src/internals/creating-examples.md) — How to create new examples
- [Message Binding Generation](../../book/src/user-guide/message-generation.md) — Message generation details
- [RFC-0098](../design/0098-generated-leaf-build-config.md) — a leaf states its
  board once, in `system.toml`; every build setting is generated from it
- [docs/roadmap/phase-66-cpp-api.md](../roadmap/archived/phase-66-cpp-api.md) — Phase 66 roadmap (design decisions)
- [docs/design/0018-cpp-api-design.md](../design/0018-cpp-api-design.md) — Full design rationale

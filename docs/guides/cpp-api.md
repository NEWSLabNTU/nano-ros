# C++ API Guide

The nros C++ API (`nros-cpp`) provides a freestanding C++14 interface for embedded ROS 2 development. It mirrors rclcpp naming conventions while requiring no standard library, exceptions, or RTTI — making it suitable for Zephyr, FreeRTOS, NuttX, ThreadX, and bare-metal targets.

## Overview

- **Freestanding C++14** — no STL dependency in default mode
- **Direct Rust FFI** — wraps `nros-node` through `nros-cpp-ffi` (not the C API), preserving type safety per message type
- **rclcpp naming** — `Node`, `Publisher<M>`, `Subscription<M>`, `Service<S>`, `Client<S>`, `ActionServer<A>`, `ActionClient<A>`, `Timer`, `GuardCondition`, `Executor`
- **Result-based error handling** — `nros::Result` + `NROS_TRY` macro (no exceptions)
- **Generated message types** — `std_msgs::msg::Int32`, `example_interfaces::srv::AddTwoInts`, etc.
- **Optional std mode** — `NROS_CPP_STD` enables `std::string`, `std::function`, `std::chrono` conveniences

## Building with CMake

### Native Linux

```cmake
cmake_minimum_required(VERSION 3.16)
project(my_app LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(NanoRos REQUIRED CONFIG)

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
```

The `nano_ros_generate_interfaces()` function:
1. Resolves `.msg`/`.srv`/`.action` files (local directory, ament index, or bundled)
2. Generates C++ headers (`.hpp`) and Rust FFI glue (`.rs`)
3. Compiles the FFI glue into a static library via Corrosion
4. Creates a `<pkg>__nano_ros_cpp` CMake target with include paths and FFI library linkage

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

// Global init (simple applications)
nros::Result ret = nros::init("tcp/127.0.0.1:7447", 0);

// Or explicit executor (multi-executor patterns)
nros::Executor executor;
NROS_TRY(nros::Executor::create(executor, "tcp/127.0.0.1:7447"));
```

### Node

```cpp
nros::Node node;
NROS_TRY(nros::create_node(node, "my_node"));

// Or with explicit executor:
NROS_TRY(executor.create_node(node, "my_node", "/namespace"));
```

### Publisher

```cpp
#include "std_msgs.hpp"

nros::Publisher<std_msgs::msg::Int32> pub;
NROS_TRY(node.create_publisher(pub, "/chatter"));

std_msgs::msg::Int32 msg;
msg.data = 42;
NROS_TRY(pub.publish(msg));
```

### Subscription

Subscriptions use manual polling — call `spin_once()` to drive I/O, then `try_recv()` to check for messages:

```cpp
nros::Subscription<std_msgs::msg::Int32> sub;
NROS_TRY(node.create_subscription(sub, "/chatter"));

nros::spin_once(100);

std_msgs::msg::Int32 msg;
if (sub.try_recv(msg)) {
    printf("Received: %d\n", msg.data);
}
```

### Service Server

```cpp
#include "example_interfaces.hpp"

using AddTwoInts = example_interfaces::srv::AddTwoInts;

nros::Service<AddTwoInts> srv;
NROS_TRY(node.create_service(srv, "/add_two_ints"));

// In your main loop:
nros::spin_once(10);
AddTwoInts::Request req;
int64_t seq;
if (srv.try_recv_request(req, seq)) {
    AddTwoInts::Response resp;
    resp.sum = req.a + req.b;
    srv.send_reply(seq, resp);
}
```

### Service Client

```cpp
nros::Client<AddTwoInts> client;
NROS_TRY(node.create_client(client, "/add_two_ints"));

AddTwoInts::Request req;
req.a = 1; req.b = 2;
AddTwoInts::Response resp;
NROS_TRY(client.call(req, resp));
// resp.sum == 3
```

### Action Server

```cpp
#include "example_interfaces.hpp"

using Fibonacci = example_interfaces::action::Fibonacci;

nros::ActionServer<Fibonacci> srv;
NROS_TRY(node.create_action_server(srv, "/fibonacci"));

// Goals are auto-accepted during spin_once()
nros::spin_once(10);
Fibonacci::Goal goal;
uint8_t goal_id[16];
if (srv.try_recv_goal(goal, goal_id)) {
    // Publish feedback
    Fibonacci::Feedback fb;
    // ... fill feedback ...
    srv.publish_feedback(goal_id, fb);

    // Complete goal
    Fibonacci::Result result;
    // ... fill result ...
    srv.complete_goal(goal_id, result);
}
```

### Action Client

```cpp
nros::ActionClient<Fibonacci> client;
NROS_TRY(node.create_action_client(client, "/fibonacci"));

Fibonacci::Goal goal;
goal.order = 10;
uint8_t goal_id[16];
NROS_TRY(client.send_goal(goal, goal_id));

Fibonacci::Result result;
NROS_TRY(client.get_result(goal_id, result));
```

### Timer

Timers fire during `spin_once()`:

```cpp
void on_timer(void* ctx) {
    // periodic work
}

nros::Timer timer;
NROS_TRY(node.create_timer(timer, 1000, on_timer));      // 1000ms period
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

nros::GuardCondition guard;
NROS_TRY(node.create_guard_condition(guard, on_signal));

// From another thread:
guard.trigger();
// Callback fires on next spin_once()
```

### Executor

```cpp
nros::Executor executor;
NROS_TRY(nros::Executor::create(executor));

nros::Node node;
NROS_TRY(executor.create_node(node, "my_node"));

// Create publishers, subscriptions, etc. on node...

while (executor.ok()) {
    executor.spin_once(10);
}
executor.shutdown();
```

### Spinning

```cpp
// Global spin (after nros::init())
nros::spin_once(10);             // single poll, 10ms timeout
nros::spin(5000);                // spin for 5 seconds
nros::spin(5000, 50);            // spin for 5s, 50ms poll interval

// Explicit executor
executor.spin_once(10);
executor.spin(5000, 50);
```

## Error Handling

All fallible operations return `nros::Result`. Use `NROS_TRY` for early return:

```cpp
nros::Result setup() {
    NROS_TRY(nros::init());
    NROS_TRY(nros::create_node(node, "my_node"));
    NROS_TRY(node.create_publisher(pub, "/topic"));
    return nros::Result::success();
}
```

Check results manually when needed:

```cpp
nros::Result ret = pub.publish(msg);
if (!ret.ok()) {
    printf("Error: %d\n", ret.raw());
}
```

Error codes (`nros::ErrorCode`):
- `Ok` (0) — success
- `Error` (-1) — generic error
- `Timeout` (-2) — operation timed out
- `InvalidArgument` (-3) — bad parameter
- `NotInitialized` (-4) — entity not initialized
- `Full` (-5) — buffer full
- `TransportError` (-100) — middleware transport failure

## Optional `std` Mode (`NROS_CPP_STD`)

For hosted environments (Linux, POSIX), define `NROS_CPP_STD` to enable STL convenience overloads. This is automatically available when including `<nros/nros.hpp>` with the macro defined.

```cpp
#define NROS_CPP_STD
#include <nros/nros.hpp>
```

### `std::string` overloads

```cpp
std::string locator = "tcp/127.0.0.1:7447";
nros::init(locator);

std::string topic = "/chatter";
nros::create_publisher<std_msgs::msg::Int32>(node, pub, topic);
```

### `std::function` callbacks

```cpp
nros::Timer timer;
nros::create_timer(node, timer, std::chrono::milliseconds(1000), [&count, &pub]() {
    std_msgs::msg::Int32 msg;
    msg.data = ++count;
    pub.publish(msg);
});
```

### `std::chrono` durations

```cpp
using namespace std::chrono_literals;
nros::spin_once(100ms);
nros::spin(5s, 50ms);
```

## Zephyr Integration

### `prj.conf`

```ini
CONFIG_CPP=y
CONFIG_STD_CPP14=y

CONFIG_NROS=y
CONFIG_NROS_CPP_API=y
CONFIG_NROS_ZENOH_LOCATOR="tcp/192.0.2.2:7447"
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

    nros::Result ret = nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID);
    if (!ret.ok()) return 1;

    nros::Node node;
    NROS_TRY(nros::create_node(node, "my_node"));

    // ... create publishers, subscriptions, etc.

    while (true) {
        nros::spin_once(100);
    }
}
```

## Examples

| Directory | Description |
|-----------|-------------|
| `examples/native/cpp/zenoh/talker/` | Publish Int32 on `/chatter` (native Linux) |
| `examples/native/cpp/zenoh/listener/` | Subscribe to `/chatter` (native Linux) |
| `examples/native/cpp/zenoh/service-server/` | AddTwoInts server (native Linux) |
| `examples/native/cpp/zenoh/service-client/` | AddTwoInts client (native Linux) |
| `examples/zephyr/cpp/zenoh/talker/` | Publish Int32 on `/chatter` (Zephyr) |
| `examples/zephyr/cpp/zenoh/listener/` | Subscribe to `/chatter` (Zephyr) |

## See Also

- [creating-examples.md](creating-examples.md) — How to create new examples
- [message-generation.md](message-generation.md) — Message generation details
- [docs/roadmap/phase-66-cpp-api.md](../roadmap/phase-66-cpp-api.md) — Phase 66 roadmap (design decisions)
- [docs/design/cpp-api-design.md](../design/cpp-api-design.md) — Full design rationale

# Getting Started {#getting_started}

## Prerequisites

- A C++14 compiler (GCC 6+, Clang 5+, or `arm-none-eabi-g++`)
- [CMake](https://cmake.org/) >= 3.15
- [Rust](https://rustup.rs/) nightly toolchain (needed to build
  `libnros_cpp.a`)
- zenohd router (`just build-zenohd` in the nano-ros source tree, or
  install a matching version from
  [zenoh releases](https://github.com/eclipse-zenoh/zenoh/releases))

## 1. Build and Install nros

```bash
cd /path/to/nano-ros
just install-local
# CMake-ready package now lives at: build/install/
```

`just install-local` builds both zenoh and XRCE-DDS variants and stages
a config-mode CMake package at `build/install/`. The C++ static library
is `libnros_cpp.a`; headers live under `include/nros/`.

## 2. Create a CMake Project

```bash
mkdir my-cpp-talker && cd my-cpp-talker
mkdir src
```

`CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.15)
project(my_cpp_talker LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(NanoRos REQUIRED CONFIG
    PATHS "/path/to/nano-ros/build/install"
)

# Generate C++ message bindings from .msg files
nano_ros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    LANGUAGE CPP
    SKIP_INSTALL
)

add_executable(my_cpp_talker src/main.cpp)
target_link_libraries(my_cpp_talker PRIVATE
    NanoRos::NanoRosCpp
    std_msgs__cpp
)
```

`find_package(NanoRos)` exports `NanoRos::NanoRosCpp` (the C++ library
target) and the `nano_ros_generate_interfaces(... LANGUAGE CPP)` CMake
function.

## 3. Code Generation

Generated message types use ROS 2 standard namespaces:
`std_msgs::msg::Int32`, `geometry_msgs::msg::Point`, etc. Each generated
type provides:

- `TYPE_NAME` — fully-qualified ROS 2 type name (string literal)
- `TYPE_HASH` — RIHS01 type hash, or `TypeHashNotSupported` on Humble
- `ffi_publish()` / `ffi_take()` — codegen-emitted serialise + FFI calls

You never hand-write CDR serialisation. The generator handles it.

## 4. Write a Talker

`src/main.cpp`:

```cpp
#include <cstdio>
#include <csignal>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

static volatile sig_atomic_t g_running = 1;
static void on_signal(int) { g_running = 0; }

struct Ctx {
    nros::Publisher<std_msgs::msg::Int32>* pub;
    int count;
};

static void on_tick(void* ctx_ptr) {
    auto* ctx = static_cast<Ctx*>(ctx_ptr);
    std_msgs::msg::Int32 msg;
    msg.data = ++ctx->count;
    if (ctx->pub->publish(msg).ok()) {
        std::printf("Published %d\n", ctx->count);
    }
}

int main() {
    NROS_TRY(nros::init("tcp/127.0.0.1:7447"));

    nros::Node node;
    NROS_TRY(nros::create_node(node, "cpp_talker"));

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY(node.create_publisher(pub, "/chatter"));

    Ctx ctx{ &pub, 0 };
    nros::Timer timer;
    NROS_TRY(node.create_timer(timer, 1000, on_tick, &ctx));

    std::signal(SIGINT, on_signal);
    while (g_running && nros::ok()) {
        nros::spin_once(100);
    }

    nros::shutdown();
    return 0;
}
```

`NROS_TRY(expr)` short-circuits on the first error — equivalent to the
Rust `?` operator. Available without `NROS_CPP_STD`.

## 5. Build and Run

```bash
mkdir build && cd build
cmake ..
cmake --build .

# Terminal 1
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2
./my_cpp_talker
```

## 6. Listener

Replace the publisher loop with a subscription:

```cpp
nros::Subscription<std_msgs::msg::Int32> sub;
NROS_TRY(node.create_subscription(sub, "/chatter",
    [](const std_msgs::msg::Int32& msg) {
        std::printf("Received: %d\n", msg.data);
    }));

while (nros::ok()) nros::spin_once(100);
```

## Zephyr Integration

In `prj.conf`:

```ini
CONFIG_NROS=y
CONFIG_NROS_CPP_API=y
CONFIG_NROS_RMW_ZENOH=y       # or CONFIG_NROS_RMW_XRCE=y
```

In your application's `CMakeLists.txt`:

```cmake
nros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    LANGUAGE CPP
)
target_sources(app PRIVATE src/main.cpp)
target_link_libraries(app PRIVATE std_msgs__cpp)
```

See `examples/zephyr/cpp/` for full Zephyr templates.

## Std-Mode Convenience

For host platforms where the STL is available:

```cpp
#define NROS_CPP_STD
#include <nros/nros.hpp>
#include <chrono>

using namespace std::chrono_literals;

nros::create_node(node, std::string("cpp_talker"));
node.create_subscription(sub, "/chatter",
    std::function<void(const std_msgs::msg::Int32&)>{...});
node.create_timer(timer, 100ms, ...);
```

`NROS_CPP_STD` is opt-in; the freestanding surface remains the
canonical API.

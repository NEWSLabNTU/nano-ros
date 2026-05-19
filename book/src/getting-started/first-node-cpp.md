# First Node — C++ (Linux)

Build, run, and verify a single nano-ros publisher node on Linux from
C++14. Uses CMake, the Zenoh backend, and `add_subdirectory`
consumption.

> **Stuck?** See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md) for the common first-build errors.
>
> **Prereqs.** A clone with `just setup tier=default` already run.
> See [Install + first build (Linux)](./installation.md) if you
> haven't.

## Project layout

The talker is a **standalone CMake project** that pulls nano-ros via
`add_subdirectory(<repo-root>)`. Four files matter:

```text
examples/native/cpp/zenoh/talker/
├── CMakeLists.txt      # add_subdirectory + targets
├── package.xml         # ROS-style manifest (drives codegen tooling)
├── config.toml         # runtime locator + domain id (optional)
└── src/
    └── main.cpp        # ~70-line talker
```

CMake preamble — five lines + per-target link:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_talker LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW      zenoh)
add_subdirectory(<rel-path-to-nano-ros> nano_ros)

# Generate C++ bindings (LANGUAGE CPP — separate from the C variant).
nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces
                                  LANGUAGE CPP SKIP_INSTALL)

add_executable(my_talker src/main.cpp)
target_link_libraries(my_talker PRIVATE
    std_msgs__nano_ros_cpp
    NanoRos::NanoRosCpp)
nros_platform_link_app(my_talker)
nano_ros_link_rmw(my_talker RMW zenoh)
```

The C++ entry point is **`int nros_app_main(int argc, char** argv)`**
(same as C); `<nros/app_main.h>` provides the OS-side `main` stub.

The body uses typed `nros::Publisher<M>` / `nros::Subscription<M>`
wrappers over the C ABI:

```cpp
#include <nros/app_main.h>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

#define NROS_TRY_LOG(file, line, expr, ret) \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", file, line, expr, (int)ret)

int nros_app_main(int argc, char** argv) {
    NROS_TRY_RET(nros::init("tcp/127.0.0.1:7447", 0), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "talker"), 1);

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

    // ... register a timer + spin
}
```

`NROS_TRY_RET` short-circuits on any non-OK return code and logs the
expression that failed. Define `NROS_TRY_LOG` once (any sink — here
`std::fprintf`) and reuse it across every call site.

## Configure

Three runtime knobs:

| Knob | Default | Override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | First arg to `nros::init` |
| ROS domain ID | `0` | Second arg to `nros::init` |
| Node name | `talker` | First arg to `nros::create_node` |

Reading from env in C++ is `std::getenv("NROS_LOCATOR")` plus the
same `nros::init` call — see the GitHub source for the full pattern.

## Build

```bash
cd examples/native/cpp/zenoh/talker
cmake -B build
cmake --build build
```

First configure builds nano-ros's Rust staticlibs (~3 minutes).
Re-builds finish in seconds.

## Run

Three terminals.

```bash
# 1. zenoh router:
just zenohd

# 2. Run the talker:
cd examples/native/cpp/zenoh/talker
./build/cpp_talker
# Expected:
#   Published: 1
#   Published: 2
#   …

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

## GitHub source

Canonical, copy-out:
[`examples/native/cpp/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/cpp/zenoh/talker)

## Next

- Add a subscription:
  [`examples/native/cpp/zenoh/listener/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/cpp/zenoh/listener)
- Services + actions:
  [`service-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/cpp/zenoh/service-client),
  [`action-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/cpp/zenoh/action-client)
- Parameters:
  [`parameters/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/cpp/zenoh/parameters)
- Custom `.msg` / `.srv` / `.action`:
  [Message Generation](../user-guide/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section.

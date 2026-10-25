# First Node — C++ (Linux)

Build, run, and verify a single nano-ros publisher node on Linux from
C++. Uses CMake and the Zenoh backend, with one `find_package(nano_ros)`.

> **Stuck?** See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md) for the common first-build errors.

## Prereqs

The `nros` CLI on PATH ([Installation](./installation.md) — `install.sh`
puts it in `~/.nros/bin`), then provision the Linux host (installs the
zenoh client stack into a shared store; the router itself comes from
your ROS 2 install — `ros2 run rmw_zenoh_cpp rmw_zenohd`):

```sh
nros setup linux --rmw zenoh
```

## Scaffold the talker

```sh
nros new cpp_talker --platform linux --lang cpp --rmw zenoh
cd cpp_talker
```

The talker is a **standalone CMake project**. Four files matter:

```text
cpp_talker/
├── system.toml         # WHAT this deploys to — the one file you edit
├── CMakeLists.txt      # find_package + targets; no board facts
├── package.xml         # ROS-style manifest
└── src/
    └── main.cpp        # the talker
```

`system.toml` is the whole deployment statement — `[system]` for the
RMW and domain, and one `[image.<id>]` per target naming its `board`
(RFC-0098). `find_package(nano_ros)`
reads it and derives the platform from the board, so no line of
`CMakeLists.txt` names a platform, a board or an RMW. Switching this
package to a board is editing `[image.linux] board` and re-running
`nros build`; see [First Node — C](first-node-c.md#project-layout) for the
file in full.

Native (host) talkers read the locator + domain from arguments passed to
`rclcpp::init_in(...)`; no other config is needed. An embedded image declares
its `locator` in its own `[image.<id>]` table and its address in the board
network file `os_config` names, and bakes them at build time. The `package.xml`
`<nano_ros deploy=… board=… rmw=…/>` export that used to carry that
tuple is retired and refused.

The CMakeLists is the **ament shape you already know** — this is the
scaffolded file, comments trimmed (RFC-0048: `nano_rosConfig.cmake` at
the SDK root makes `find_package(nano_ros)` resolve source-side, and the
message-package Find-stubs make `find_package(std_msgs)` generate
bindings on the fly):

```cmake
cmake_minimum_required(VERSION 3.22)
project(cpp_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(cpp_talker src/main.cpp)
ament_target_dependencies(cpp_talker std_msgs)

install(TARGETS cpp_talker DESTINATION lib/${PROJECT_NAME})
ament_package()
```

**`LANGUAGES C CXX`** (not `CXX` alone): the per-target register stub
is a C translation unit, so C must be enabled in this directory scope
or the link fails. Backend selection is `[system] rmw` in `system.toml`,
not CMake code and not a configure-line cache variable.

The C++ entry point is **`int main(int argc, char** argv)`**, as in any
rclcpp program; the build reaches it from the platform's entry, as for C.
Write that exact signature: the build declares the renamed function with it,
so another form fails at link naming `nros_ported_main(int, char**)`.

The scaffolded `src/main.cpp` is the smallest rclcpp program that does
something: it creates a node and a `std_msgs/Int32` publisher on
`/chatter`, publishes one message, prints
`cpp_talker: published 0 on /chatter`,
and exits. A talker that keeps publishing adds a timer and a spin, as in
the full
[`examples/linux/cpp/talker/src/main.cpp`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/linux/cpp/talker/src/main.cpp)
on GitHub, which you can paste over the scaffold's `main.cpp`.

The body uses typed `rclcpp::Publisher<M>` / `rclcpp::Subscription<M>`
wrappers over the C ABI:

```cpp
#include <rclcpp/rclcpp.hpp>
#include "std_msgs.hpp"

#define RCLCPP_TRY_LOG(file, line, expr, ret) \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", file, line, expr, (int)ret)

int main(int argc, char** argv) {
    RCLCPP_TRY_RET(rclcpp::init_in(), 1);

    rclcpp::Node node;
    RCLCPP_TRY_RET(rclcpp::create_node(node, "talker"), 1);

    rclcpp::Publisher<std_msgs::msg::String> pub;
    RCLCPP_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

    // ... register a timer + spin
}
```

`RCLCPP_TRY_RET` short-circuits on any non-OK return code and logs the
expression that failed. Define `RCLCPP_TRY_LOG` once (any sink — here
`std::fprintf`) and reuse it across every call site.

## If you write the `rclcpp::Node` constructor instead

`rclcpp::create_node(node, "talker")` above returns a `Result`, and
`RCLCPP_TRY_RET` checks it. The rclcpp-shaped constructor has no return value to
check — and on a `-fno-exceptions` target, which every nano-ros build is
(RFC-0018), it cannot throw the way upstream's does either. It records the
failure and reports it through `ok()`:

```cpp
rclcpp::Node node("talker");
if (!node.ok()) return 1;   // this line replaces upstream's try/catch
```

Skip that line and `main` runs on with a node that never came up. The object
exists either way, `node.create_publisher(...)` on it still compiles and still
returns, and nothing ever reaches the wire. There is no diagnostic and no
crash — not throwing is the point, so asking is your job.

A **generated entry already asks**, and halts naming the node that failed, so a
workspace component needs none of this. It is only on you in a hand-written
`main` like the one above.

The two-phase form carries the same obligation:

```cpp
rclcpp::Node node;
if (!node.init("talker").ok()) return 1;
```

with one difference in your favour: `init()` returns a `Result`, which is
`[[nodiscard]]`, so dropping it on the floor is a compiler warning. `ok()` on a
constructed node is a question nothing can force you to ask.

Full compatibility notes, including what `shared_from_this()` and
`rclcpp::TimerBase` do differently here: [Porting a ROS 2 C++
node](./porting-a-cpp-node.md).

## Configure

Three runtime knobs:

| Knob | Default | Override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | First arg to `rclcpp::init_in` |
| ROS domain ID | `0` | Second arg to `rclcpp::init_in` |
| Node name | `talker` | First arg to `rclcpp::create_node` |

With no arguments, `rclcpp::init_in()` (and `rclcpp::init(argc, argv)`)
takes the locator and domain from the build, then the `NROS_LOCATOR` and
`ROS_DOMAIN_ID` environment variables on a hosted target, so an example
needs neither argument.

## Build

```bash
nros build
```

`nros build` reads `[image.linux]`, configures the package with CMake
into `build/linux/cmake` (passing the SDK root and the board's
toolchain file), and builds it; the binary lands at
`build/linux/cmake/cpp_talker`. No `nros sync` here: a C++ leaf's
message bindings are generated inside CMake, at configure time. (A Rust
leaf needs sync, because its message crates and its build settings have
to exist before cargo parses anything.) In a
[multi-node workspace](workspace-cpp.md) `nros build` is the only build
verb.

Driving CMake yourself works too; name the SDK root:

```bash
cmake -B build -Dnano_ros_ROOT="$(nros sdk-root)"
cmake --build build
```

First configure builds nano-ros's Rust staticlibs (~3 minutes).
Re-builds finish in seconds.

## Run

Three terminals.

```bash
# 1. zenoh router (ROS 2's own):
source /opt/ros/humble/setup.bash
ros2 run rmw_zenoh_cpp rmw_zenohd

# 2. Run the talker:
cd cpp_talker
./build/linux/cmake/cpp_talker
# Expected from the scaffold (it publishes once, then exits):
#   cpp_talker: published 0 on /chatter
# Expected from the full examples/linux/cpp/talker main.cpp:
#   nros C++ Talker
#   ===================
#   Node created: talker
#
#   Publishing messages (Ctrl+C to exit)...
#
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
#   …

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

**Readiness signal.** The scaffold prints its one line as soon as the
session opens. The full example, within ~6 seconds (session open + the
first 1 s timer tick), prints `Publishing: 'Hello World: 1'` — Rust + C
+ C++ all start the count at 1, matching the official ROS 2 demo
talker. If neither appears in 30 seconds:

1. Confirm the router is running (terminal 1). Without it,
   `rclcpp::init_in` returns `-100` (TransportError) — the
   `RCLCPP_TRY_RET` macro logs the failed call to stderr.
2. Check stderr for `[nros] …/main.cpp:LINE rclcpp::init_in(...) -> -N`
   diagnostics. `-3` / `-100` both indicate transport open failed.
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

The full talker, to read or to paste over the scaffold:
[`examples/linux/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/cpp/talker)

## Next

- Add a subscription:
  [`examples/linux/cpp/listener/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/cpp/listener)
- Services + actions:
  [`service-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/cpp/service-client),
  [`action-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/cpp/action-client)
- Parameters:
  [`parameters/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/cpp/parameters)
- Custom `.msg` / `.srv` / `.action`:
  [Message Generation](../user-guide/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section.

# First Node — C (Linux)

Build, run, and verify a single nano-ros publisher node on Linux from
C. Uses CMake, the Zenoh backend, and `add_subdirectory` consumption.

> **Stuck?** See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md) for the common first-build errors.
>
> **Prereqs.** A clone with `just setup tier=default` already run.
> See [Install + first build (Linux)](./installation.md) if you
> haven't.

## Project layout

The talker is a **standalone CMake project** that pulls nano-ros via
`add_subdirectory(<repo-root>)`. Four files matter:

```text
examples/native/c/zenoh/talker/
├── CMakeLists.txt      # add_subdirectory + targets
├── package.xml         # ROS-style manifest (drives codegen tooling)
├── config.toml         # runtime locator + domain id (optional)
└── src/
    └── main.c          # ~100-line talker
```

The CMake preamble is **four lines** plus the per-target link:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_talker LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Pull nano-ros in. Adjust the relative path to your repo root.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW      zenoh)
add_subdirectory(<rel-path-to-nano-ros> nano_ros)

# Generate C bindings for std_msgs (transitively depends on
# builtin_interfaces; the dependency is declared explicitly).
nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces
                                  SKIP_INSTALL)

add_executable(my_talker src/main.c)
target_link_libraries(my_talker PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(my_talker)
nano_ros_link_rmw(my_talker RMW zenoh)
```

`nano_ros_link_rmw` emits the per-target
`nros_app_register_backends()` strong stub that calls
`nros_rmw_zenoh_register()` at startup — the auto-registration path
for the C build.

The C entry point is **`int nros_app_main(int argc, char **argv)`**
(not `main`); `<nros/app_main.h>` provides the OS-side `main`
stub that wires signal handling and forwards to your function.

```c
#include <nros/app_main.h>
#include <nros/init.h>
#include <nros/executor.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>
#include "std_msgs.h"

int nros_app_main(int argc, char** argv) {
    // 1. nros_init() — opens the zenoh session
    // 2. nros_executor_init() / nros_node_init()
    // 3. std_msgs_msg_int32_publisher_init() — typed publisher
    // 4. nros_timer_init() with a 1 Hz period + publish callback
    // 5. nros_executor_spin() until SIGINT
}
```

## Configure

Three runtime knobs:

| Knob | Default | Env override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | `NROS_LOCATOR` |
| ROS domain ID | `0` | `ROS_DOMAIN_ID` |
| Node name | `talker` | hard-coded in source |

`config.toml` (optional) accepts the same `[zenoh]` table as the
Rust starter; the C runtime reads it only when wired explicitly via
`nros_config_load()` (see the example source).

## Build

```bash
cd examples/native/c/zenoh/talker
cmake -B build
cmake --build build
```

The first configure pulls and builds nano-ros's Rust staticlibs
(~3 minutes). Re-builds finish in seconds.

## Run

Three terminals.

```bash
# 1. Start the in-tree zenoh router:
just zenohd                          # or: ./build/zenohd/zenohd

# 2. Run the talker:
cd examples/native/c/zenoh/talker
./build/c_talker
# Expected output:
#   nros C Talker
#   =================
#   Published: 1
#   Published: 2
#   …

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

**Readiness signal.** Within 5 seconds of `./build/c_talker`, the
binary should print `Published: 1` on stdout. If no `Published:`
line in 30 seconds:

1. Confirm `zenohd` is running (terminal 1). Without it, `nros_init`
   blocks indefinitely.
2. Check `nros_init -> -3` / `-100` in stderr — both indicate
   transport open failed (wrong locator or zenohd unreachable).
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

Canonical, copy-out:
[`examples/native/c/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/c/zenoh/talker)

## Next

- Add a subscription:
  [`examples/native/c/zenoh/listener/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/c/zenoh/listener)
- Service / action shapes:
  [`service-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/c/zenoh/service-client),
  [`action-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/c/zenoh/action-client)
- Custom `.msg` / `.srv` / `.action`:
  [Message Generation](../user-guide/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section.

# First Node — C (Linux)

Build, run, and verify a single nano-ros publisher node on Linux from
C. Uses CMake, the Zenoh backend, and `add_subdirectory` consumption.

> **Stuck?** See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md) for the common first-build errors.
>
> **Prereqs.** Install the `nros` CLI and provision the native host.
> `nros setup native` installs the zenoh router (`zenohd`) into a
> shared store — no ROS 2 needed.
>
> ```bash
> # Install the nros CLI once per machine:
> curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
> export PATH="$HOME/.nros/bin:$PATH"
>
> # Provision the native host for the zenoh RMW:
> nros setup native --rmw zenoh
> ```
>
> See [Install + first build (Linux)](./installation.md) for more.

## Project layout

The talker is a **standalone CMake project** that pulls nano-ros via
`add_subdirectory(<repo-root>)`. Four files matter:

```text
examples/native/c/talker/
├── CMakeLists.txt      # add_subdirectory + targets
└── src/
    └── main.c          # ~100-line talker (locator + domain via env vars)
```

The CMake preamble matches the canonical
[`examples/native/c/talker/CMakeLists.txt`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/native/c/talker/CMakeLists.txt)
shape — RMW chosen via the `NROS_RMW` cache var (overridable on the
configure line with `-DNROS_RMW=<rmw>`), registration wired
transitively by `nros_platform_link_app`:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_talker LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Pull nano-ros in. Adjust the relative path to your repo root.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into my_talker.")
set(NANO_ROS_RMW "${NROS_RMW}")
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
```

On hosted POSIX the RMW backend registers itself transitively when
`nros_platform_link_app` brings in the backend rlib's CGU — no
explicit `nano_ros_link_rmw(my_talker RMW zenoh)` call is needed.
(That helper still exists and works; it's the only registration path
on bare-metal targets without `.init_array`. POSIX builds inherit it
for free.)

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

Embedded C examples (FreeRTOS / NuttX / ThreadX / bare-metal /
ESP32) carry a sidecar `nros.toml` with `[node]` + `[[transport]]`
sections — see the embedded starter pages. The native POSIX C
talker has neither: it reads `NROS_LOCATOR` / `ROS_DOMAIN_ID` from
env vars at startup (see `examples/native/c/talker/src/main.c`).

## Build

```bash
cd examples/native/c/talker
cmake -B build
cmake --build build
```

The first configure pulls and builds nano-ros's Rust staticlibs
(~3 minutes). Re-builds finish in seconds.

## Run

Three terminals.

```bash
# 1. Start the zenoh router:
zenohd                               # installed by `nros setup native`

# 2. Run the talker:
cd examples/native/c/talker
./build/c_talker
# Expected output:
#   nros C Talker
#   =================
#   Locator: tcp/127.0.0.1:7447
#   Published: 0
#   Published: 1
#   …

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

**Readiness signal.** Within 5 seconds of `./build/c_talker`, the
binary should print `Published: 0` on stdout. If no `Published:`
line in 30 seconds:

1. Confirm `zenohd` is running (terminal 1). Without it,
   `nros_support_init` returns immediately with `-4`
   (`NROS_RET_NOT_FOUND` — connection refused).
2. Wrong locator / unreachable host → same `-4` signature in stderr.
   Reachable host but mismatched port → talker hangs on session-open
   handshake rather than returning a code.
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

Canonical, copy-out:
[`examples/native/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/c/talker)

## Next

- Add a subscription:
  [`examples/native/c/listener/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/c/listener)
- Service / action shapes:
  [`service-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/c/service-client),
  [`action-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/c/action-client)
- Custom `.msg` / `.srv` / `.action`:
  [Message Generation](../user-guide/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section.

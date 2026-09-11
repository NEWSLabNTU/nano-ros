# First Node — C (Linux)

Build, run, and verify a single nano-ros publisher node on Linux from
C. Uses CMake and the Zenoh backend, with one `find_package(nano_ros)`.

> **Stuck?** See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md) for the common first-build errors.

## Prereqs

Pick one path from a fresh checkout — `just` is NOT a prereq.

**A. Front door** (bare machine OK — no Rust, no `just`):
```sh
./scripts/bootstrap.sh
```
Installs rustup if needed and builds the in-tree `nros` CLI from
source at `packages/cli/target/release/nros`, leaving it on PATH for
this shell (in a checkout, the tree's own build is the binary this tree
accepts — RFC-0090 / phase-431).

**B. Already have cargo** (equivalent — same build, same binary):
```sh
git submodule update --init packages/cli/third-party/play_launch
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
export PATH="$PWD/packages/cli/target/release:$PATH"
```

Every subsequent shell sources the workspace env via one of:
```sh
direnv allow                  # if you use direnv
source ./activate.sh          # bash / zsh
source ./activate.fish        # fish
```

Then provision the native host (installs the zenoh client stack into a
shared store; the router itself comes from your ROS 2 install —
`ros2 run rmw_zenoh_cpp rmw_zenohd`):
```sh
nros setup native --rmw zenoh
```

See [Install + first build (Linux)](./installation.md) for more.

## Project layout

The talker is a **standalone CMake project**. Four files matter:

```text
examples/native/c/talker/
├── system.toml         # WHAT this deploys to — the one file you edit
├── CMakeLists.txt      # find_package + targets; no board facts
├── package.xml         # ROS-style manifest (drives codegen tooling)
└── src/
    └── main.c          # ~100-line talker
```

`system.toml` is the whole deployment statement — the same schema a
Rust leaf and a multi-node workspace bringup use (RFC-0098):

```toml
[system]
name      = "native_c_talker"
rmw       = "zenoh"
domain_id = 0

[[component]]
pkg   = "native_c_talker"
class = "native_c_talker::Talker"
name  = "talker"

[image.native]
board = "native"
```

`find_package(nano_ros)` reads it, derives the platform from the board,
and configures the build accordingly. Nothing in `CMakeLists.txt` names
a platform, a board or an RMW — and the `package.xml`
`<nano_ros deploy=… board=… rmw=…/>` export that used to carry that
tuple is retired and refused. Switching this example to a board is
editing `[image.native] board` and re-running cmake; a C or C++ leaf
names no board crate, so that is the only edit to the project — a cross
build still passes its `-DCMAKE_TOOLCHAIN_FILE` on the command line.

On native, locator + domain come from env vars (`NROS_LOCATOR`,
`ROS_DOMAIN_ID`) with built-in defaults; an embedded image declares
them in its own `[image.<id>]` table (`locator`, `ip`, `gateway`,
`netmask`) and they are baked at build time (see
[Configuration](../user-guide/configuration.md)).

The `CMakeLists.txt` is, verbatim from
[`examples/native/c/talker/CMakeLists.txt`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/native/c/talker/CMakeLists.txt):

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(c_talker src/main.c)
ament_target_dependencies(c_talker std_msgs)

install(TARGETS c_talker DESTINATION lib/${PROJECT_NAME})
ament_package()
```

`nano_ros_add_executable` does the wiring the old five-`set()` preamble
did by hand: it generates the C bindings the package declares, links
`NanoRos::NanoRos` and the platform, and pulls in the active RMW's
strong `nros_app_register_backends()` stub (calling
`nros_rmw_zenoh_register()` for the zenoh image above). Which RMW that
is comes from `[system] rmw`, not from a `-D` on the command line.

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
    // 1. nros_support_init() — opens the zenoh session
    // 2. nros_executor_init() / rclc_node_init_default()
    // 3. std_msgs_msg_int32_publisher_init() — typed publisher
    // 4. nros_timer_init() with a 1 Hz period + publish callback
    // 5. rclc_executor_spin() until SIGINT
}
```

## Configure

Three runtime knobs:

| Knob | Default | Env override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | `NROS_LOCATOR` |
| ROS domain ID | `0` | `ROS_DOMAIN_ID` |
| Node name | `talker` | hard-coded in source |

On an embedded image the same knobs are compile-baked from that image's
`[image.<id>]` table in `system.toml` (see
[Configuration](../user-guide/configuration.md)).

## Build

```bash probe=40
cd examples/native/c/talker
cmake -B build
cmake --build build
```

**No `nros sync` here, and that is a property of the language, not an
oversight:** a C or C++ leaf's message bindings are generated *inside*
CMake, at configure time, from the package's own declared dependencies.
A Rust leaf needs sync because its message crates and its build settings
must exist before cargo parses anything. Measured: a copy of this
directory outside the checkout configures and builds with no `nros sync`
at all. See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md).

`nros build`, the workspace verb, does not yet work in a single-package
C or C++ leaf — the synthesised bringup and the CMake driver derive its
name from two different facts and disagree
([issue 1296](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1296-nros-build-c-leaf-bringup-name-mismatch.md)).
The two `cmake` lines above are the supported path here; `nros build` is
the path for a [multi-node workspace](workspace-cpp.md).

Outside a sourced checkout, tell cmake where nano-ros is —
`cmake -B build -Dnano_ros_ROOT=/path/to/nano-ros`, or export
`NROS_REPO_DIR` once.

The first configure pulls and builds nano-ros's Rust staticlibs
(~3 minutes). Re-builds finish in seconds.

## Run

Three terminals.

```bash
# 1. Start the zenoh router (ROS 2's own):
source /opt/ros/humble/setup.bash
ros2 run rmw_zenoh_cpp rmw_zenohd    # or: just zenohd

# 2. Run the talker:
cd examples/native/c/talker
./build/c_talker
# Expected output:
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
#   Publishing: 'Hello World: 3'
#   …

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

**Readiness signal.** Within ~6 seconds of `./build/c_talker` (session
open + the first 1 s timer tick), the binary should print
`Publishing: 'Hello World: 1'` on stdout — Rust + C + C++ all start
the count at 1, matching the official ROS 2 demo talker. If no
`Publishing:` line in 30 seconds:

1. Confirm the router is running (terminal 1). Without it,
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

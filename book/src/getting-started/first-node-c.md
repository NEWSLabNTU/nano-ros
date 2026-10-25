# First Node — C (Linux)

Build, run, and verify a single nano-ros publisher node on Linux from
C. Uses CMake and the Zenoh backend, with one `find_package(nano_ros)`.

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

```sh probe=38
nros new c_talker --platform linux --lang c --rmw zenoh
cd c_talker
```

The talker is a **standalone CMake project**. Four files matter:

```text
c_talker/
├── system.toml         # WHAT this deploys to — the one file you edit
├── CMakeLists.txt      # find_package + targets; no board facts
├── package.xml         # ROS-style manifest (drives codegen tooling)
└── src/
    └── main.c          # the talker
```

`system.toml` is the whole deployment statement — the same schema a
Rust leaf and a multi-node workspace bringup use (RFC-0098):

```toml
[system]
name = "c_talker"
rmw = "zenoh"
domain_id = 0

[image.linux]
board = "linux"
```

`find_package(nano_ros)` reads it, derives the platform from the board,
and configures the build accordingly. Nothing in `CMakeLists.txt` names
a platform, a board or an RMW — and the `package.xml`
`<nano_ros deploy=… board=… rmw=…/>` export that used to carry that
tuple is retired and refused. Switching this example to a board is
editing `[image.linux] board` and re-running `nros build`; a C or C++ leaf
names no board crate, so that is the only edit to the project — a cross
build still passes its `-DCMAKE_TOOLCHAIN_FILE` on the command line.

`nano_ros_add_executable` still accepts `DEPLOY` and `BOARD` keywords, for a
project with no `system.toml`. Beside one they are redundant at best: a
keyword that names a different deploy or board than the leaf's
`[image.<id>]` stops the configure and names both files, rather than
quietly building the `CMakeLists.txt` one.

On native, locator + domain come from env vars (`NROS_LOCATOR`,
`ROS_DOMAIN_ID`) with built-in defaults; an embedded image declares
them in its own `[image.<id>]` table (`locator`, and its address in the board
network file `os_config` names) and they are baked at build time (see
[Configuration](../user-guide/configuration.md)).

The scaffolded `CMakeLists.txt` is (comments trimmed):

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker VERSION 0.1.0 LANGUAGES C CXX)

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

The scaffolded `src/main.c` is the smallest rclc program that does
something: it opens a session, creates a node and a `std_msgs/Int32`
publisher on `/chatter`, publishes one message, prints
`c_talker: published 0 on /chatter` and exits. A talker that keeps
publishing adds a timer and an executor, as in the outline below and in
the full
[`examples/linux/c/talker/src/main.c`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/linux/c/talker/src/main.c)
on GitHub, which you can paste over the scaffold's `main.c`.

The C entry point is **`int main(int argc, char **argv)`**, as in any rclc
program. On a board the C runtime does not call `main`, so the build compiles
yours under another name and reaches it from the platform's own entry
(Zephyr's `main(void)`, an RTOS startup task); on Linux it is the process
entry. You write `main` either way.

```c
#include <rcl/rcl.h>
#include <rclc/executor.h>
#include <rclc/rclc.h>
#include <std_msgs/msg/int32.h>

int main(int argc, char** argv) {
    // 1. rclc_support_init() — opens the zenoh session
    // 2. rclc_executor_init() / rclc_node_init_default()
    // 3. rclc_publisher_init_default(&pub, &node,
    //        ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter")
    // 4. rclc_timer_init_default() with a 1 Hz period; the callback calls
    //        rcl_publish(&pub, &msg, NULL)
    // 5. rclc_executor_spin() until SIGINT
}
```

Generated messages take rosidl's C names and member shape
(`std_msgs__msg__String` in `<std_msgs/msg/string.h>`, `msg.data.data` /
`.size` / `.capacity`). Their strings and sequences point at memory **you**
own: wire them with the generated `<Msg>__init_in(&msg, &storage)` over a
`<Msg>__Storage`, or point `data` at your own buffer as micro-ROS code does.
Nothing on this path allocates, so rosidl's allocating `<Msg>__init` /
`__fini` are not declared.

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
nros build
```

`nros build` reads `[image.linux]`, configures the package with CMake
into `build/linux/cmake` (passing the SDK root and the toolchain file
the board names), and builds it. The binary lands at
`build/linux/cmake/c_talker`.

**No `nros sync` here, and that is a property of the language, not an
oversight:** a C or C++ leaf's message bindings are generated *inside*
CMake, at configure time, from the package's own declared dependencies.
A Rust leaf needs sync because its message crates and its build settings
must exist before cargo parses anything. See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md).
In a [multi-node workspace](workspace-cpp.md) `nros build` is the only
build verb.

When it finishes, `nros build` prints what it built and how to run it:

```text
nros build: next for `c_talker:linux`:
  artifact: /home/you/c_talker/build/linux/cmake/c_talker
  run:
    /home/you/c_talker/build/linux/cmake/c_talker
```

**Escape hatch — driving CMake yourself.** For a CMake tree that is not a
nano-ros package, or a build you want to steer by hand, configure it the way
`nros build` does (`nros build --dry-run` prints its exact line). It must be
told where nano-ros is; a cross board also needs the toolchain file its
starter page names:

```bash
cmake -S . -B build/linux/cmake -DNROS_IMAGE=linux -Dnano_ros_ROOT="$(nros sdk-root)"
cmake --build build/linux/cmake
```

Without a `system.toml` at all, see
[Build as a CMake subdirectory](./build-as-subdirectory.md).

`nros sdk-root` prints the SDK root the `nros` on PATH resolves (the
one its release carries). `nros build`, and the CMakePresets
`nros init` writes, pass it for you.

The first configure pulls and builds nano-ros's Rust staticlibs
(~3 minutes). Re-builds finish in seconds.

## Run

Three terminals.

```bash
# 1. Start the zenoh router (ROS 2's own):
source /opt/ros/humble/setup.bash
ros2 run rmw_zenoh_cpp rmw_zenohd

# 2. Run the talker:
cd c_talker
./build/linux/cmake/c_talker
# Expected output (the scaffold publishes once, then exits):
#   c_talker: published 0 on /chatter
# The full examples/linux/c/talker publishes once a second:
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
#   …

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive. With the full example's main.c
# (std_msgs/String, once a second):
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
# With the scaffold (one std_msgs/Int32): start the echo FIRST, then
# terminal 2, with the type it publishes:
#   ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

**Readiness signal.** The scaffold prints its one line as soon as the
session opens. The full example, within ~6 seconds (session open + the
first 1 s timer tick), prints `Publishing: 'Hello World: 1'` on stdout —
Rust + C + C++ all start the count at 1, matching the official ROS 2
demo talker. If neither appears in 30 seconds:

1. Confirm the router is running (terminal 1). Without it,
   `rclc_support_init` returns immediately with `-4`
   (`RCLC_RET_NOT_FOUND` — connection refused).
2. Wrong locator / unreachable host → same `-4` signature in stderr.
   Reachable host but mismatched port → talker hangs on session-open
   handshake rather than returning a code.
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

The full talker, to read or to paste over the scaffold:
[`examples/linux/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/c/talker)

## Next

- Add a subscription:
  [`examples/linux/c/listener/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/c/listener)
- Service / action shapes:
  [`service-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/c/service-client),
  [`action-client/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/c/action-client)
- Custom `.msg` / `.srv` / `.action`:
  [Message Generation](../user-guide/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section.

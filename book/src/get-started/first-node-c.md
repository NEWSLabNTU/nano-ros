# First Node — C (Linux)

Build, run, and verify a single nano-ros publisher node on Linux from
C. The node is a C library built with CMake; the program around it is
generated. Uses the Zenoh backend.

> **Stuck?** See [Troubleshooting — First 10 Minutes](first-10-minutes.md) for the common first-build errors.

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
nros new c_talker_ws --board linux --template talker --lang c --rmw zenoh
cd c_talker_ws
```

That wrote a workspace with two packages:

```text
c_talker_ws/
└── src/
    ├── c_talker_node/            # node package: the code
    │   ├── package.xml           # ROS manifest: <depend>std_msgs</depend>
    │   ├── CMakeLists.txt        # a library and its registration; no board facts
    │   └── src/Talker.c          # the talker: a 1 s timer publishing /chatter
    └── c_talker_node_bringup/    # bringup package: what runs where
        ├── package.xml
        └── system.toml           # the board, the RMW, the node it runs
```

### The bringup package

`c_talker_node_bringup/system.toml` is the whole deployment statement,
the same schema whatever language the node is written in:

```toml
[system]
name = "c_talker_node"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "c_talker_node"
class = "c_talker_node::Talker"
name = "c_talker_node"

[image.linux]
board = "linux"
```

The `[[component]]` row says which node runs; `[image.linux]` says it runs on
this machine. Nothing in the node package names a platform, a board or an RMW
— and the `package.xml` `<nano_ros deploy=… board=… rmw=…/>` export that used
to carry that tuple is retired and refused. Moving the talker to a board is
another `[image.<id>]` block here, with no edit to `src/c_talker_node/`.

On native, locator + domain come from env vars (`NROS_LOCATOR`,
`ROS_DOMAIN_ID`) with built-in defaults; an embedded image declares
them in its own `[image.<id>]` table (`locator`, and its address in the board
network file `os_config` names) and they are baked at build time (see
[Configuration](../build/configuration.md)).

### The node package

A C node package is a library, not a program. The scaffolded
`CMakeLists.txt` is (comments trimmed):

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker_node VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)

nros_components_register_node(talker_lib
    PLUGIN c_talker_node::Talker
    EXECUTABLE c_talker_node
    SHAPE configure
    TYPED)
```

`nano_ros_auto_add_library` generates the C bindings the package declares
and links nano-ros. `nros_components_register_node` registers the class the
bringup's `[[component]]` row names, the CMake spelling of ROS 2's
`rclcpp_components_register_node`.

There is no `main()`. The build generates one for each image, which opens the
session and executor, creates the node, and hands both to your code. Your code
is a state struct and a `configure` function, exported with
`RCLC_C_COMPONENT`:

```c
#include <stdio.h>
#include <rclc/rclc.h>
#include <std_msgs/msg/string.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
    char text[32];
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    self->count++;
    int n = snprintf(self->text, sizeof self->text, "Hello World: %d", (int)self->count);
    std_msgs__msg__String msg;
    msg.data.data = self->text;          // your buffer; nothing allocates
    msg.data.size = (size_t)n;
    msg.data.capacity = sizeof self->text;
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("Publishing: '%s'\n", self->text);
    }
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor,
                                  talker_t* self) {
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter");
    if (rc != RCL_RET_OK) return rc;
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 1000000000ULL, on_tick);
    if (rc != RCL_RET_OK) return rc;
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
```

`configure` runs once at startup, on the generated entry's own node and
executor; after that the executor calls `on_tick` once a second. Because the
entry owns startup, the same file runs on Linux and on a board whose C runtime
never calls `main` (Zephyr, an RTOS startup task).

`--template` picks a different node: `listener`, `server` / `client`
(AddTwoInts) or `action-server` / `action-client` (Fibonacci).

Generated messages take rosidl's C names and member shape
(`std_msgs__msg__String` in `<std_msgs/msg/string.h>`, `msg.data.data` /
`.size` / `.capacity`). Their strings and sequences point at memory **you**
own: wire them with the generated `<Msg>__init_in(&msg, &storage)` over a
`<Msg>__Storage`, or point `data` at your own buffer as above and as micro-ROS
code does. Nothing on this path allocates, so rosidl's allocating
`<Msg>__init` / `__fini` are not declared.

## Configure

Three runtime knobs:

| Knob | Default | Env override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | `NROS_LOCATOR` |
| ROS domain ID | `0` | `ROS_DOMAIN_ID` |
| Node name | `c_talker_node` | the `[[component]]` row's `name` (a launch file can remap it) |

On an embedded image the same knobs are compile-baked from that image's
`[image.<id>]` table in `system.toml` (see
[Configuration](../build/configuration.md)).

## Build

```bash probe=40
nros build
```

`nros build` prepares the build from the bringup (the generated `main()` for
`[image.linux]` and a CMake root that adds your package), configures it with
the SDK root and the toolchain file the board names, and builds it. The C
message bindings are generated during that CMake configure, from the
package's declared dependencies. There is no step to run before it.

When it finishes, `nros build` prints what it built and how to run it:

```text
nros build: next for `c_talker_node_bringup:linux`:
  artifact: /home/you/c_talker_ws/build/posix-zenoh-linux/cmake/linux_entry
  run:
    nros run c_talker_node
```

**Driving CMake yourself.** The CMake root `nros build` configures is
generated under `build/`, never at the workspace root, and
`nros build --dry-run` prints the exact `cmake` lines it would run. To have
that root in place without building (before opening the workspace in an IDE,
or to run those lines by hand), prepare it on its own:

```bash
nros configure
```

For a CMake tree that is
not a nano-ros workspace at all, see
[Build as a CMake subdirectory](../build/cmake-subdirectory.md).

The first configure pulls and builds nano-ros's Rust staticlibs
(~3 minutes). Re-builds finish in seconds.

## Run

Three terminals.

```bash
# 1. Start the zenoh router (ROS 2's own):
source /opt/ros/humble/setup.bash
ros2 run rmw_zenoh_cpp rmw_zenohd

# 2. Run the talker:
cd c_talker_ws
nros run c_talker_node
# Expected output, once a second:
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

**Readiness signal.** Within ~6 seconds (session open + the first 1 s
timer tick), the talker prints `Publishing: 'Hello World: 1'` on stdout —
Rust + C + C++ all start the count at 1, matching the official ROS 2
demo talker. If it does not appear in 30 seconds:

1. Confirm the router is running (terminal 1). Without it, the
   generated entry's session open fails at once with `-4`
   (`RCLC_RET_NOT_FOUND` — connection refused) and the program stops,
   naming the step.
2. Wrong locator / unreachable host → same `-4` signature in stderr.
   Reachable host but mismatched port → talker hangs on session-open
   handshake rather than returning a code.
3. See [Troubleshooting — First 10 Minutes](first-10-minutes.md).

## Next

- Add a subscriber: from the workspace root,
  `nros new c_listener_node --lang c --template listener --to c_talker_node_bringup`
  adds a second node package and its component row to the bringup
  ([Growing your workspace](../build/growing-your-workspace.md)).
- Service / action shapes, and a workspace of several C node packages:
  [`examples/workspaces/c/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/c)
- Custom `.msg` / `.srv` / `.action`:
  [Message Generation](../build/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section.

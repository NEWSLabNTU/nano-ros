# First Node — C++ (Linux)

<!-- type: tutorial · level: 1 -->

Build, run, and verify a single nano-ros publisher node on Linux from
C++. The node is a C++ library built with CMake, in the ament shape; the
program around it is generated. Uses the Zenoh backend.

> **Stuck?** See [Troubleshooting — First 10 Minutes](../get-started/first-10-minutes.md) for the common first-build errors.

## Prereqs

The `nros` CLI on PATH ([Installation](../start/install.md) — `install.sh`
puts it in `~/.nros/bin`), then provision the Linux host (installs the
zenoh client stack into a shared store; the router itself comes from
your ROS 2 install — `ros2 run rmw_zenoh_cpp rmw_zenohd`):

```sh
nros setup linux --rmw zenoh
```

## Scaffold the talker

```sh
nros new cpp_talker_ws --board linux --template talker --lang cpp --rmw zenoh
cd cpp_talker_ws
```

That wrote a workspace with two packages:

```text
cpp_talker_ws/
└── src/
    ├── cpp_talker_node/            # node package: the code
    │   ├── package.xml             # ROS manifest: <depend>std_msgs</depend>
    │   ├── CMakeLists.txt          # a library and its registration; no board facts
    │   ├── include/cpp_talker_node/Talker.hpp
    │   └── src/Talker.cpp          # the talker: a 1 s timer publishing /chatter
    └── cpp_talker_node_bringup/    # bringup package: what runs where
        ├── package.xml
        └── system.toml             # the board, the RMW, the node it runs
```

This is the same project as
[`examples/linux/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/cpp/talker)
on GitHub, the copy-out example for C++ on Linux: one node package and a
bringup with no launch file of its own, since the build generates it.

`cpp_talker_node_bringup/system.toml` is the whole deployment statement —
`[system]` for the RMW and domain, one `[[component]]` row naming the node,
and one `[image.<id>]` per target naming its `board`. No line of the node
package names a platform, a board or an RMW, so moving the talker to a board
is another `[image.<id>]` block in the bringup;
[First Node — C](add-a-second-node-c.md#the-bringup-package) shows the file in full.
The `package.xml` `<nano_ros deploy=… board=… rmw=…/>` export that used to
carry that tuple is retired and refused.

## The node package

The CMakeLists is the **ament shape you already know** — this is the
scaffolded file, comments trimmed (the message-package Find-stubs make
`find_package(std_msgs)` generate bindings on the fly):

```cmake
cmake_minimum_required(VERSION 3.22)
project(cpp_talker_node VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.cpp)

nros_components_register_node(talker_lib
    PLUGIN cpp_talker_node::Talker
    EXECUTABLE cpp_talker_node
    SHAPE configure)
```

`nros_components_register_node` has `rclcpp_components_register_node`'s
keywords: the class the bringup's `[[component]]` row names, and its
executable name.

**`LANGUAGES C CXX`** (not `CXX` alone): the per-target register stub
is a C translation unit, so C must be enabled in this directory scope
or the link fails. Backend selection is `[system] rmw` in the bringup,
not CMake code and not a configure-line cache variable.

There is no `main()`. The node is a class with a `configure(rclcpp::Node&)`
method, the shape of a composable node: the generated entry initialises
nano-ros, creates the node, constructs your class in static storage and
calls `configure` on it, then spins the executor.

```cpp
// include/cpp_talker_node/Talker.hpp
#pragma once
#include <rclcpp/rclcpp.hpp>
#include "std_msgs.hpp"

namespace cpp_talker_node {

class Talker {
    ::rclcpp::Publisher<std_msgs::msg::String> pub_;
    ::rclcpp::Timer timer_;
    int count_ = 0;

    void on_tick();   // bound by &Talker::on_tick, not by a name

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

}  // namespace cpp_talker_node
```

```cpp
// src/Talker.cpp
#include "cpp_talker_node/Talker.hpp"
#include <cstdio>

namespace cpp_talker_node {

void Talker::on_tick() {
    char payload[64];
    std::snprintf(payload, sizeof payload, "Hello World: %d", ++count_);
    std_msgs::msg::String m;
    m.data = payload;
    if (pub_.publish(m).ok()) std::printf("Publishing: '%s'\n", m.data.c_str());
}

::rclcpp::Result Talker::configure(::rclcpp::Node& node) {
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
}

}  // namespace cpp_talker_node
```

The publisher and timer are typed wrappers over the C ABI, and every call
returns a `Result` rather than throwing: every nano-ros build is
`-fno-exceptions`. `configure` returns the first failure, and the generated
entry halts naming the node that failed, so there is no unchecked
construction to forget. [Porting a ROS 2 C++ node](../guide/port/port-a-cpp-node.md)
covers the rclcpp-shaped constructors, `shared_from_this()` and
`rclcpp::TimerBase`.

`--template` picks a different node: `listener`, `server` / `client`
(AddTwoInts) or `action-server` / `action-client` (Fibonacci).

## Configure

Three runtime knobs:

| Knob | Default | Override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | `NROS_LOCATOR` |
| ROS domain ID | `0` | `ROS_DOMAIN_ID` |
| Node name | `cpp_talker_node` | the `[[component]]` row's `name` (a launch file can remap it) |

On a hosted target the generated entry takes the locator and domain from the
build, then from the environment variables. An embedded image declares its
`locator` in its own `[image.<id>]` table and its address in the board network
file `os_config` names, and bakes them at build time.

## Build

The build generates `std_msgs` from your ROS 2 install, so source it first.
This first build records the ROS 2 prefix in the workspace; later builds and
runs of `cpp_talker_ws` need no sourced shell.

```bash
source /opt/ros/humble/setup.bash
nros build
```

`nros build` prepares the build from the bringup (the generated `main()` for
`[image.linux]` and a CMake root that adds your package), configures it with
the SDK root and the board's toolchain file, and builds it. The C++ message
bindings are generated during that CMake configure. There is no step to run
before it.

When it finishes, `nros build` prints what it built and how to run it:

```text
nros build: next for `cpp_talker_node_bringup:linux`:
  artifact: /home/you/cpp_talker_ws/build/posix-zenoh-linux/cmake/linux_entry
  run:
    nros run cpp_talker_node
```

**Driving CMake yourself.** The CMake root is generated under `build/`, and
`nros build --dry-run` prints the exact `cmake` lines it would run. To have
that root in place without building (before opening the workspace in an
IDE, or to run those lines by hand), prepare it on its own:

```bash
nros configure
```

For a CMake tree that is
not a nano-ros workspace at all, see
[Build as a CMake subdirectory](../guide/build/cmake-subdirectory.md).

First configure builds nano-ros's Rust staticlibs (~3 minutes).
Re-builds finish in seconds.

## Run

Three terminals.

```bash
# 1. zenoh router (ROS 2's own):
source /opt/ros/humble/setup.bash
ros2 run rmw_zenoh_cpp rmw_zenohd

# 2. Run the talker:
cd cpp_talker_ws
nros run cpp_talker_node
# Expected output:
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
timer tick), the talker prints `Publishing: 'Hello World: 1'` — Rust + C
+ C++ all start the count at 1, matching the official ROS 2 demo
talker. If it does not appear in 30 seconds:

1. Confirm the router is running (terminal 1). Without it, the generated
   entry's session open fails with `-100` (TransportError) and the
   program stops, naming the step.
2. `-3` / `-100` on stderr both mean the transport did not open: check
   `NROS_LOCATOR` and the router's port.
3. See [Troubleshooting — First 10 Minutes](../get-started/first-10-minutes.md).

## Next

- Add a subscriber: from the workspace root,
  `nros new cpp_listener_node --lang cpp --template listener --to cpp_talker_node_bringup`
  adds a second node package and its component row
  ([Growing your workspace](grow-your-workspace.md)).
- Services, actions and several C++ node packages in one workspace:
  the C++ language workspace,
  [`examples/workspaces/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/cpp),
  where each paired example is an image row under its behaviour's bringup
  (`pubsub_bringup`, `service_bringup`, `action_bringup`), and
  [C / C++ multi-node workspaces](../guide/contracts/cpp-workspaces.md).
- Custom `.msg` / `.srv` / `.action`:
  [Message Generation](../build/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section.

## Design background

- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)
- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The copy-out talker kept for each platform and language, and the language
  workspaces that hold the paired examples.

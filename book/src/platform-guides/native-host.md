# Native (Host) Build

Use the host (`linux`) board for first experiments, CI smoke tests, and
ROS 2 interoperability checks on Linux. It uses OS sockets, the
host process environment, and the same project layout as the
embedded targets.

The host board is named `linux` (`[image.<id>] board = "linux"`) because
that is all it supports: the platform layer (`nros-platform-posix`) is
POSIX, but the host board crate's `apply_tier_affinity` calls
`sched_setaffinity` with `cpu_set_t`, which libc does not define for
macOS, Windows or FreeBSD. **Only Linux is supported** (AGENTS.md,
phase-260, phase-500). Neither `native` (every general-purpose host) nor
`posix` (the platform) names a board: `board = "…"`, `nros setup`,
`nros new --platform` and the `just` lane/scope words refuse both with a
hint naming `linux`.

## When to Use It

- You want the shortest path to a working nano-ros publisher or
  subscriber.
- You are validating message generation, CMake integration, or ROS 2
  interop before moving to an RTOS.
- Your deployment target is Linux or an embedded Linux system.

## Setup

Build the in-tree `nros` CLI (Phase 218), then provision the native
host. For a host build there is no cross-toolchain to fetch — `nros
setup linux` installs the client stack (and for xrce the
Micro-XRCE-DDS agent) into a shared store. The zenoh router is NOT
installed: it comes from a ROS 2 install
(`ros2 run rmw_zenoh_cpp rmw_zenohd`, RFC-0075). No ROS 2 on this
machine? Use `--rmw cyclonedds` — it needs no daemon.

With the `nros` CLI on PATH ([Installation](../getting-started/installation.md) —
`install.sh` puts it in `~/.nros/bin`):

```bash
# Provision the Linux host (zenoh RMW is the default):
nros setup linux --rmw zenoh        # or: --rmw xrce / --rmw cyclonedds
```

`linux` is this board's one name; `posix` names the platform layer underneath
it, and `native` names no board at all
([Retired names](../reference/retired-names.md#retired-board-names)).

For a colcon consumer workspace that already has nano-ros under
`src/`:

```bash
colcon build && source install/setup.bash
```

## Package Layout

For a host application package:

```text
my_native_node/
├── package.xml
├── system.toml         # board, rmw, domain, components
├── Cargo.toml          # Rust path
├── CMakeLists.txt      # C / C++ path
└── src/
    └── main.rs         # or main.c / main.cpp
```

`nros new <name> --platform linux --lang <rust|c|cpp>` scaffolds that
shape, and the examples on GitHub are copy-out ready: nothing in one names
a path into the SDK. For Rust, run `nros sync` and `nros build` (`nros`
finds its own SDK; `--nano-ros-path` names another). For C/C++, run
`nros build`, or configure a raw `cmake` with
`-Dnano_ros_ROOT="$(nros sdk-root)"` — a single-package C/C++
leaf needs no `nros sync`, because
its message bindings are a CMake-time output.

## Code Example

Rust publisher skeleton:

```rust,ignore
use core::fmt::Write as _;
use nros::prelude::*;
use std_msgs::msg::String as StringMsg;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ExecutorConfig::from_env().node_name("talker");
    let mut executor = Executor::open(&config)?;
    let mut node = executor.create_node("talker")?;
    let publisher = node.create_publisher::<StringMsg>("/chatter")?;

    let mut count = 0i32;
    loop {
        count += 1;
        let mut msg = StringMsg::default();
        let _ = write!(msg.data, "Hello World: {count}");
        publisher.publish(&msg)?;
        executor.spin_once(100);
    }
}
```

Use the full [First Node — Rust](../getting-started/first-node-rust.md)
walkthrough for generated messages and runnable commands.

## Build and Run

Start the RMW host daemon (installed by `nros setup linux`). For
zenoh:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
```

Run the node directly — `nros build` then
`./build/<image-id>/target/debug/<binary>` (Rust), or
`cmake --build build && ./build/<binary>` (C/C++) — or via
`colcon build && source install/setup.bash && ros2 run …` if the
package lives in a colcon consumer workspace.

## Configuration

Native examples can read runtime settings from the shell:

```bash
export ROS_DOMAIN_ID=0
export NROS_LOCATOR=tcp/127.0.0.1:7447
```

The same fields are compiled into embedded targets through their
platform-specific configuration files. See
[Configuration](../user-guide/configuration.md) for the full layering.

## Deployment

Host deployment is normal process deployment: install the workspace,
source `install/setup.bash`, set environment, and run the binary. For
ROS 2 interop, run [ROS 2 Interoperability](../getting-started/ros2-interop.md)
and set the ROS side to `rmw_zenoh_cpp`.

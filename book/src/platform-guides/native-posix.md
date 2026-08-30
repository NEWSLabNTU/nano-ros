# Native (Host) Build

Use the `native` (host) target for first experiments, CI smoke tests, and
ROS 2 interoperability checks on Linux or *BSD. It uses OS sockets, the
host process environment, and the same source checkout layout as the
embedded targets.

`native` is a *role* — the host build rather than a cross build — not a
reach claim. The reach is `linux`: the platform layer
(`nros-platform-posix`) is POSIX-clean, but the host board crate's
`apply_tier_affinity` calls `sched_setaffinity` with `cpu_set_t`, which
libc does not define for macOS, and only Linux is tested in CI. **macOS is
not supported** (AGENTS.md, phase-260); *BSD is a supported path that no
lane exercises.

## When to Use It

- You want the shortest path to a working nano-ros publisher or
  subscriber.
- You are validating message generation, CMake integration, or ROS 2
  interop before moving to an RTOS.
- Your deployment target is Linux, *BSD, or an embedded Linux system.

## Setup

Build the in-tree `nros` CLI (Phase 218), then provision the native
host. For a host build there is no cross-toolchain to fetch — `nros
setup native` installs the client stack (and for xrce the
Micro-XRCE-DDS agent) into a shared store. The zenoh router is NOT
installed: it comes from a ROS 2 install
(`ros2 run rmw_zenoh_cpp rmw_zenohd`, RFC-0075). No ROS 2 on this
machine? Use `--rmw cyclonedds` — it needs no daemon.

```bash
# Build the in-tree nros CLI:
./scripts/bootstrap.sh      # builds packages/cli/target/release/nros
source ./activate.sh        # OR: direnv allow / source ./activate.fish

# Provision the native host (zenoh RMW is the default):
nros setup native --rmw zenoh        # or: --rmw xrce / --rmw cyclonedds
```

`native` and `linux` are the two names of this board (`native` the role,
`linux` the reach); `posix` names the platform layer underneath it, not the
board.

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
├── Cargo.toml          # Rust path
├── CMakeLists.txt      # C / C++ path
└── src/
    └── main.rs         # or main.c / main.cpp
```

Examples are copy-out ready: for Rust, `cp -r` the example, run
`NROS_REPO_DIR=<nano-ros checkout> nros sync`, and `cargo build`;
for C/C++, `cp -r` and configure with
`-DNANO_ROS_ROOT=<nano-ros checkout>` (or export `NROS_REPO_DIR`) —
the example `CMakeLists.txt` resolves the checkout via its
`NANO_ROS_ROOT` guard.

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

Start the RMW host daemon (installed by `nros setup native`). For
zenoh:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
```

Run the node directly via `cargo run` (Rust) or
`cmake --build build && ./build/<binary>` (C/C++), or via
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

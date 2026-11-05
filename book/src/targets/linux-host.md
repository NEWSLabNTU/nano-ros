# Native (Host) Build

Use the host (`linux`) board for first experiments, CI smoke tests, and
ROS 2 interoperability checks on Linux. It uses OS sockets, the
host process environment, and the same project layout as the
embedded targets.

The host board is named `linux` (`[image.<id>] board = "linux"`) because
that is all it supports: the platform layer (`nros-platform-posix`) is
POSIX, but the host board crate's `apply_tier_affinity` calls
`sched_setaffinity` with `cpu_set_t`, which libc does not define for
macOS, Windows or FreeBSD. **Only Linux is supported.** Neither `native` (every general-purpose host) nor
`posix` (the platform) names a board: `board = "…"`, `nros setup`,
and `nros new --board` refuse both with a
hint naming `linux`.

## When to Use It

- You want the shortest path to a working nano-ros publisher or
  subscriber.
- You are validating message generation, CMake integration, or ROS 2
  interop before moving to an RTOS.
- Your deployment target is Linux or an embedded Linux system.

## Setup

Install the `nros` CLI ([Installation](../get-started/installation.md)),
then provision the native host. For a host build there is no cross-toolchain to fetch — `nros
setup linux` installs the client stack (and for xrce the
Micro-XRCE-DDS agent) into a shared store. The zenoh router is NOT
installed: it comes from a ROS 2 install
(`ros2 run rmw_zenoh_cpp rmw_zenohd`). Rather not run a router? Use
`--rmw cyclonedds` — it needs no daemon. Either way the first build
generates the messages from your ROS 2 install, so it needs ROS 2 sourced.

With the `nros` CLI on PATH ([Installation](../get-started/installation.md) —
`install.sh` puts it in `~/.nros/bin`):

```bash
# Provision the Linux host (zenoh RMW is the default):
nros setup linux --rmw zenoh        # or: --rmw xrce / --rmw cyclonedds
```

`linux` is this board's one name; `posix` names the platform layer underneath
it, and `native` names no board at all
([Retired names](../migrating/retired-names.md#retired-board-names)).

For a colcon consumer workspace that already has nano-ros under
`src/`:

```bash
colcon build && source install/setup.bash
```

## Workspace Layout

A host project is a workspace, like every other board's:

```bash
nros new host_ws --board linux --template talker --lang rust   # or --lang c / cpp
cd host_ws
```

```text
host_ws/
└── src/
    ├── host_node/              # node package: package.xml + Cargo.toml (or CMakeLists.txt), the node
    └── host_node_bringup/      # package.xml and system.toml: board, rmw, domain, components
```

The node package is a library with no `main()`; the bringup's
`[image.linux]` block (`board = "linux"`) is what makes it a host program.
The examples on GitHub are copy-out ready: nothing in one names a path into
the SDK. `nros` finds its own SDK; `--nano-ros-path` names another.

## Code Example

A Rust node package's `src/lib.rs` declares what the node creates and what
its callbacks do, and exports the node; the entry that opens the executor and
spins it is generated:

```rust,ignore
pub struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let chatter = node.create_publisher_for_topic::<StringMsg>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick").publishes_entity(&chatter)?;
        Ok(())
    }
}

// impl ExecutableNode for Talker { … the timer callback publishes "Hello World: N" … }

nros::node!(Talker);
```

Use the full [First Node — Rust](../get-started/first-node-rust.md)
walkthrough for generated messages and runnable commands, and
[Growing your workspace](../build/growing-your-workspace.md#srclibrs) for each
line of `lib.rs`.

## Build and Run

Start the RMW host daemon (the router comes from ROS 2). For
zenoh:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
```

Then build and run the node from the workspace root, in a shell that has
sourced ROS 2. The first build generates the messages from it and records
the prefix, so later runs need no sourced shell:

```bash
source /opt/ros/humble/setup.bash
nros run host_node
```

`nros run` builds first if anything changed. `nros build` alone prints the
binary's path under `build/`, which runs directly too. For a workspace that
lives in a colcon consumer workspace, `colcon build && source
install/setup.bash && ros2 run …` works as well.

## Configuration

Native examples can read runtime settings from the shell:

```bash
export ROS_DOMAIN_ID=0
export NROS_LOCATOR=tcp/127.0.0.1:7447
```

The same fields are compiled into embedded targets through their
platform-specific configuration files. See
[Configuration](../build/configuration.md) for the full layering.

## Deployment

Host deployment is normal process deployment: install the workspace,
source `install/setup.bash`, set environment, and run the binary. For
ROS 2 interop, run [ROS 2 Interoperability](../communicate/ros2-interop.md)
and set the ROS side to `rmw_zenoh_cpp`.

## Design background

- [RFC-0075: zenoh router provenance](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md)

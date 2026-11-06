# Native (Host) Build

<!-- type: how-to · level: 2 -->

**Needs first:** [Get started on the Linux host](../../starters/linux-host.md)

Use the host (`linux`) board for first experiments, CI smoke tests, and
ROS 2 interoperability checks on Linux. It uses OS sockets, the
host process environment, and the same project layout as the
embedded targets.

The host board is named `linux` (`[image.<id>] board = "linux"`) because
that is all it supports: the platform layer (`nros-platform-posix`) is
POSIX, but the host board crate's `apply_sched_task_affinity` calls
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

The [starter](../../starters/linux-host.md) provisions the host for Cyclone
DDS, which needs no daemon. `nros setup linux` installs the client stack (and
for xrce the Micro-XRCE-DDS agent) into a shared store; it installs no zenoh
router, which comes from a ROS 2 install
(`ros2 run rmw_zenoh_cpp rmw_zenohd`). Either way the first build generates
the messages from your ROS 2 install, so it needs ROS 2 sourced.

`linux` is this board's one name; `posix` names the platform layer underneath
it, and `native` names no board at all
([Retired names](../../migrating/retired-names.md#retired-board-names)).

For a colcon consumer workspace that already has nano-ros under
`src/`:

```bash
colcon build && source install/setup.bash
```

## Workspace Layout

A host project is a workspace, like every other board's. The starter's
scaffold wrote one (`--lang rust`, `c` or `cpp`):

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

The Tutorial's [Add a second node](../../tutorial/add-a-second-node.md)
walks through a node package, generated messages and runnable commands, and
[its listener section](../../tutorial/add-a-second-node.md#read-the-listener)
reads a node's `lib.rs` line by line.

## Run with zenoh

With `--rmw zenoh`, the image dials a router on this machine's loopback, so
`nros run host_node` starts the router, ROS's `rmw_zenohd`, before the node
and stops it with the node. Run it from the workspace root, in a shell that
has sourced ROS 2 for the first build. `nros build` alone prints the binary's
path under `build/`, which runs directly too. For a workspace that lives in a
colcon consumer workspace, `colcon build && source install/setup.bash && ros2
run …` works as well.

## Configuration

Native examples can read runtime settings from the shell:

```bash
export ROS_DOMAIN_ID=0
export NROS_LOCATOR=tcp/127.0.0.1:7447
```

The same fields are compiled into embedded targets through their
platform-specific configuration files. See
[Configuration](../build/configure-an-image.md) for the full layering.

## Deployment

Host deployment is normal process deployment: install the workspace,
source `install/setup.bash`, set environment, and run the binary. For
ROS 2 interop, run [ROS 2 Interoperability](../../tutorial/talk-to-ros2.md)
and set the ROS side to `rmw_zenoh_cpp`.

## Design background

- [RFC-0075: zenoh router provenance](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md)

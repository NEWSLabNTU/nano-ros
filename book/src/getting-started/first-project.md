# First Project

One command scaffolds a working multi-node workspace; two more build and
run it. C++ and CMake, publishing on your machine, with **nothing else
running** — no router, no daemon, no ROS 2 installation.

This assumes the two-step [install](installation.md) is done (`nros` on
your PATH, `nros setup native --rmw cyclonedds` run once).

## Scaffold

```bash
nros new my_robot --workspace
cd my_robot
```

That wrote a complete workspace, not a hello-world stub:

```text
my_robot/
├── .colcon_workspace           # marks the workspace root; there is no root build file
└── src/
    ├── talker_pkg/             # a C++ node: publishes std_msgs/Int32 on /chatter
    ├── listener_pkg/           # a C++ node: subscribes to /chatter
    └── demo_bringup/           # launch file + system.toml — no code
```

There is no entry package and no root `CMakeLists.txt`: the binary is
*generated* from the bringup's `system.toml` when you build. What each
directory is *for* is the next chapter,
[Anatomy of What You Just Built](anatomy.md). You don't need it to run.

## Build

```bash
nros sync
nros build
```

`nros sync` resolves the launch file into the system model; `nros build`
generates the entry for each `[image.*]` in `system.toml` and drives
CMake under `build/`. The first build compiles nano-ros's runtime into
the build tree (~3 minutes); rebuilds are seconds. Outside a nano-ros
checkout, `nros` finds the SDK itself; inside one, `direnv`/`activate.sh`
exports `NROS_REPO_DIR` and that is used. `nros sdk-root --explain`
prints the root it would use and where that came from.

## Run

```bash
./build/posix-native/cmake/native_entry
```

```text
Published: 0
Received: 0
Published: 1
Received: 1
Published: 2
Received: 2
```

One process, two nodes: the talker publishes `std_msgs/Int32` on
`/chatter` every 500 ms and the listener prints each one it receives —
the interleaved `Received:` lines are your proof of delivery — exactly as a ROS 2
composition container would run two composable nodes. `Ctrl-C` stops it.

Why nothing else needed to be running: the default RMW is
**CycloneDDS**, which discovers peers directly — there is no router or
daemon in the picture. The zenoh backend (for talking to a ROS 2 system)
and XRCE (for the smallest targets) are one page away:
[Choosing an RMW](../user-guide/rmw-choosing.md), and switching is a
one-word edit — [Switching RMW in Config](../user-guide/rmw-switching.md).

## Same thing in Rust

```bash
nros new my_rust_robot --workspace --lang rust
cd my_rust_robot
nros sync
nros build
RUST_LOG=info ./build/posix/native_entry/target/debug/native_entry
```

```text
[INFO  talker_pkg] Publishing: 0
[INFO  listener_pkg] I heard: 0
[INFO  talker_pkg] Publishing: 1
```

(The Rust talker ticks at 1 s.)

> **Known issue ([1295](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1295-rust-cyclonedds-workspace-entry-no-publisher.md)).**
> On CycloneDDS — the scaffold's default RMW — the generated Rust entry
> currently stops at startup with
> `NodeError::Transport(PublisherCreationFailed)`. The C++ path above is
> not affected.

Same workspace shape, same `system.toml`, same launch file, same two
commands — only the node implementations and the driver `nros build`
picks (cargo instead of CMake) differ. Neither workspace has a root build
file: the entry is generated per image under `build/`.

## If it didn't work

- `nros: RMW session open failed` on a busy machine usually means the
  DDS port range on your `ROS_DOMAIN_ID` is contended — pick another:
  `ROS_DOMAIN_ID=57 ./build/posix-native/cmake/native_entry`.
- Everything else: [Troubleshooting — First 10 Minutes](troubleshooting-first-10-min.md).

## Where to go next

- **Understand the four directories** — [Anatomy](anatomy.md).
- **Talk to a real ROS 2 system** — [ROS 2 Interoperability](ros2-interop.md).
- **Put it on a board** — [How Integration Works](how-integration-works.md).
- **Add nodes, parameters, more targets** —
  [Project layout](workspace-from-app-node.md) and the Multi-Node
  Projects section.

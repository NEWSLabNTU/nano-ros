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

That wrote 21 files — a complete workspace, not a hello-world stub:

```text
my_robot/
├── CMakeLists.txt              # workspace root — names the packages, picks the RMW
└── src/
    ├── talker_pkg/             # a C++ node: publishes std_msgs/Int32 on /chatter
    ├── listener_pkg/           # a C++ node: subscribes to /chatter
    ├── demo_bringup/           # launch file + system.toml — no code
    └── robot_entry/            # the binary: boots the launch topology
```

What each directory is *for* is the next chapter,
[Anatomy of What You Just Built](anatomy.md). You don't need it to run.

## Build

```bash
cmake -S . -B build -DNANO_ROS_ROOT=<path-to-your-nano-ros-checkout>
cmake --build build
```

The first configure compiles nano-ros's runtime into the build tree
(~3 minutes); rebuilds are seconds. If you use `direnv`/`activate.sh`
from the nano-ros checkout, `-DNANO_ROS_ROOT` can be omitted — the
`NROS_REPO_DIR` env it exports is picked up automatically.

## Run

```bash
./build/src/robot_entry/robot_entry
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
NROS_REPO_DIR=<path-to-nano-ros> nros sync   # Rust-side codegen; once per checkout location
RUST_LOG=info cargo run -p robot_entry
```

```text
[INFO  talker_pkg] Publishing: 0
[INFO  listener_pkg] I heard: 0
[INFO  talker_pkg] Publishing: 1
```

(The workspace root is a virtual manifest — `cargo run` alone has no
default binary, so name the entry package with `-p robot_entry`. The
Rust talker ticks at 1 s.)

Same workspace shape, same `system.toml`, same launch file — only the
node implementations and the build tool differ. The extra `nros sync`
step is the Rust-side message codegen; C++ does the equivalent inside
CMake, which is why the C++ path doesn't have it.

## If it didn't work

- `nros: RMW session open failed` on a busy machine usually means the
  DDS port range on your `ROS_DOMAIN_ID` is contended — pick another:
  `ROS_DOMAIN_ID=57 ./build/src/robot_entry/robot_entry`.
- Everything else: [Troubleshooting — First 10 Minutes](troubleshooting-first-10-min.md).

## Where to go next

- **Understand the four directories** — [Anatomy](anatomy.md).
- **Talk to a real ROS 2 system** — [ROS 2 Interoperability](ros2-interop.md).
- **Put it on a board** — [How Integration Works](how-integration-works.md).
- **Add nodes, parameters, more targets** —
  [Project layout](workspace-from-app-node.md) and the Multi-Node
  Projects section.

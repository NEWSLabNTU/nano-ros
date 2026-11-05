# Welcome

<!-- type: explanation · level: 1 -->

nano-ros runs ROS 2 nodes on microcontrollers and real-time operating
systems, and those nodes talk to an ordinary ROS 2 system on your computer.
In your first hour you install the `nros` command, scaffold a talker for an
Arm Cortex-M3 board, run it in an emulator, and watch its messages arrive in
`ros2 topic echo`. You need no board and you write no code to get there.

## The talker you will run

The first Tutorial chapter scaffolds a workspace with one node package. Its
code is a ROS 2 talker. This excerpt is the part that does the work:

```rust
fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
    let mut node = ctx.create_node(nros::NodeOptions::new("talker"))?;
    node.create_publisher_for_topic::<StringMsg>("/chatter")?;
    node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
    Ok(())
}

fn on_callback(state: &mut i32, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
    if callback.as_str() == "on_tick" {
        *state = state.wrapping_add(1);
        let mut msg = StringMsg::default();
        let _ = write!(msg.data, "Hello World: {}", *state);
        if ctx.publish_to_topic::<StringMsg, 64>("/chatter", &msg).is_ok() {
            log::info!("Publishing: '{}'", msg.data);
        }
    }
}
```

The node creates a publisher on `/chatter` and a one-second timer. Each tick
publishes a `std_msgs/String`. You run it with `nros run sensor_node`, which
builds the firmware, boots it in QEMU, and prints the board's console:

```text
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
Publishing: 'Hello World: 3'
```

In a second terminal, the ROS 2 command line sees the same messages:

```text
data: 'Hello World: 2'
---
data: 'Hello World: 3'
---
```

Nothing on the ROS 2 side knows that the talker is firmware on an emulated
microcontroller. It is a ROS 2 node publishing a ROS 2 message type, and the
ROS 2 tools you already use receive it. The same chapter also runs the talker
as a process on your Linux host, for when you have no emulator set up.

## Who nano-ros is for

nano-ros serves three kinds of work. Each has a budget that the book is
written and tested against:

- **Education.** Courses and labs on robotics and embedded systems. The
  install takes 15 minutes, a lab fits a two-hour session, and every seat
  gets the same result.
- **Field solutions.** Engineers and technicians who build a system for each
  customer, often on a board they were handed. The target is 30 minutes from
  an unopened box to `ros2 topic echo` showing the board's data.
- **Industrial.** Teams who build products where safety and robustness
  matter. The same inputs give the same image, every fact about a board is on
  a page, and a build runs with no network.

Two more readers use the book: ROS 2 developers moving existing nodes onto a
microcontroller, and porters who add a board or a platform.
[Choose your path](choose-your-path.md) gives each of the five a route
through the book, in order, with its budget.

## What you get

- **One node, any supported board.** You write a node once. Moving it to
  another board is one line in the workspace's configuration, the board's
  name. The cross-compiler, linker flags, middleware and memory sizes follow
  from that line, so neither your code nor your build files change.
- **Your ROS 2 graph as it is.** Your nodes join a stock ROS 2 system over
  Zenoh, through `rmw_zenoh_cpp`, or over DDS, through `rmw_cyclonedds_cpp`.
  Topics, services and actions work in both directions.
- **The middleware you choose, with the same code.** Zenoh needs no agent.
  XRCE-DDS works with the micro-ROS agent. Cyclone DDS speaks plain RTPS.
  Switching between them is a one-line change and a rebuild.
- **A fit for a small microcontroller.** The core runs with no operating
  system, and the heap is optional: whether an image needs one depends on the
  middleware you choose. Memory for nodes, topics and messages is sized when
  you build, in fixed pools.
- **Rust, C or C++.** The core is Rust, with C and C++ APIs over it. The C
  API follows `rclc` conventions, and the C++ API uses `rclcpp`'s names. One
  workspace can mix all three.
- **Messages that match your peers.** Message bindings are generated from
  the ROS 2 install you build against, so the types and their hashes are the
  ones your ROS 2 nodes use.
- **Claims you can check.** Each board's support level, where it runs, and
  how often it is tested come from one registry. The book prints them on
  generated pages, and `nros board show <board>` prints the same facts from
  the `nros` you installed.

## Where nano-ros differs from ROS 2

nano-ros keeps ROS 2's vocabulary (nodes, publishers, subscriptions,
services, actions, timers, parameters, QoS) and its wire format. It is not a
port of `rclcpp`, and some things you rely on in ROS 2 work differently:

- **Choices happen at build time.** The middleware, the board and the memory
  sizes are fixed when you build. A running image cannot load a different
  middleware or create more topics than it was built for.
- **The executor comes first.** An executor opens the middleware session and
  owns the nodes, instead of a global `init` followed by nodes.
- **A node is code you build into an image.** On a microcontroller there is
  no process to start with `ros2 run`. The workspace says which board each
  image runs on, and `nros run` builds and boots it.
- **Parameters are smaller.** Each node keeps its own parameter store, with
  no descriptors or change callbacks yet.
- **Some ROS 2 tools need the host.** `ros2 topic echo` and `ros2 node list`
  run on your computer and see the board's nodes. Runtime introspection on the
  board itself is limited.

[Differences from standard ROS 2](../background/differences-from-ros2.md)
gives each difference and its reason, and
[nano-ros vs micro-ROS](../background/vs-micro-ros.md) compares nano-ros with
the other ROS 2 stack for microcontrollers.

## Does it run on my board?

[Supported Boards](../reference/supported-boards.md) lists every board the
release knows, with its support level and where it runs: on your host, in an
emulator, or on hardware. [Board support levels](../reference/board-support-levels.md)
says what each level means. These pages are generated from the board
registry, so they always match the release.

Every board in this book has a way to run without its hardware: on your
host, in an emulator, or in an emulated twin of the board. The first chapter uses
QEMU's model of the MPS2-AN385, an Arm Cortex-M3 board, because it boots in
seconds and needs nothing but your computer. Boards with a hardware twin, such
as the [Nucleo-F767ZI](../starters/nucleo-f767zi.md), run the same firmware in
an emulator first and then on the board.

A board that is not in the registry can still run nano-ros. You add it from
your own tree, without changing nano-ros: [Extending nano-ros](../extending/overview.md)
shows how.

## How this book is organized

The sidebar follows what you are doing:

- **Start here:** this page, [Choose your path](choose-your-path.md) and
  [Install](install.md).
- **Get started on your board:** one page per board, from install to
  `ros2 topic echo`.
- **Tutorial:** one project, built up chapter by chapter, starting with
  [the talker above](../tutorial/run-a-talker.md).
- **User guide:** task pages, grouped by area: communication, contracts and
  sizing, build and configuration, platforms, real-time, deployment, and
  porting from ROS 2.
- **Background:** why nano-ros works the way it does.
- **Reference:** the CLI, the APIs, configuration keys and the boards,
  generated from their sources where possible.
- **Extending nano-ros:** adding a board, a platform, a transport or a
  middleware backend.
- **Releases:** moving from an older release.

Each page shows its kind and level under its title. Level 1 pages assume
nothing beyond this section. A page at a higher level assumes the levels
below it:

| Level | What it covers |
| --- | --- |
| 1. First contact | install, a board's first run, the first Tutorial chapters |
| 2. Building real applications | your own messages, services, launch files, ROS 2 interop |
| 3. Production and tuning | configuration, offline builds, deployment, timing and memory |
| 4. Extending nano-ros | new boards, platforms, transports and middleware |

Changing nano-ros itself is a different job, with its own book:
[nano-ros Internals](../internals/orientation/start-here.html).

## Next

Go to [Choose your path](choose-your-path.md) to find the route for your
work, or go straight to [Install](install.md).

## Design background

The three purposes and their budgets are RFC-0116 D1. The book's sections,
page types and topic levels are RFC-0124.

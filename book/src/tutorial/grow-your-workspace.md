# Growing your workspace

<!-- type: tutorial · level: 2 -->

[First Project](run-a-talker.md) left you with a workspace of
two packages: a node and the bringup that says what it runs on. A project
grows from there by one path, in four steps, and you take only the ones you
need:

1. [add a node package](#1-add-a-node-package);
2. [add its component row](#2-add-a-component-row) to the bringup;
3. [add `launch/`](#3-add-a-launch-file-when-you-need-one) when you need a
   launch argument, a remap or a parameter;
4. [add an image](#4-add-an-image) for another board.

Each step adds a package or a few lines to the bringup. The shape of the
workspace never changes, so nothing you did in an earlier step has to be
undone.

## Where you start

```text
sensor_ws/
└── src/
    ├── sensor_node/            # node package: package.xml, Cargo.toml (a library), src/lib.rs
    └── sensor_node_bringup/    # package.xml and system.toml; no launch/ yet
```

The two roles, and what each one never holds:

| Package | Holds | Never holds |
|---|---|---|
| **Node package** | one node: publishers, subscriptions, timers, services, callback bodies | a board, an RMW, a `main()` |
| **Bringup package** | `system.toml`: which nodes run, on which boards, with which RMW; optionally `launch/` | compiled code |

There is no third package to write. For each `[image.*]` block in the
bringup's `system.toml`, `nros build` and `nros run` generate the program's
entry (its `main()`) under `build/`. A workspace has no root `Cargo.toml` or
`CMakeLists.txt` either: like a colcon workspace, it is a directory of
packages.

`system.toml` lives only in a bringup package. A `system.toml` beside a
`Cargo.toml` or `CMakeLists.txt` is refused, and the message says where to
move it.

## 1. Add a node package

From the workspace root:

```bash
nros new monitor_node --template listener --to sensor_node_bringup
```

Inside a workspace, `nros new` adds a package under `src/` instead of
writing a new workspace. `--to` also adds the node's component row to the
bringup (step 2). Pass `--lang c` or `--lang cpp` for a C or C++ node; the
rest of this page is the same in every language.

A node package is a library with three files:

```text
src/monitor_node/
├── package.xml          # ROS 2 manifest: one <depend> per message package
├── Cargo.toml           # a [lib] crate and its dependencies, nothing else
└── src/lib.rs           # the node; its last line is nros::node!(Listener);
```

It has no `fn main()` and names no board, RMW or board crate. That is what
lets one node package go into a Cortex-M image and a Linux image unchanged:
the difference between them is an `[image.*]` block, not a second copy of
the node.

### The manifests

`package.xml` is a standard ROS 2 manifest. Each message package the node
uses is one `<depend>`:

```xml
<depend>std_msgs</depend>
```

That line is all you write for a message package. The build turns each
`<depend>` on a message package into a dependency row in `Cargo.toml`, tagged
`# nros-managed`, pointing at the crate it generates under `generated/`.
Removing the `<depend>` removes the row. A row without the tag is yours and
is never touched, so to override a generated row, delete its tag.

`Cargo.toml` registers the node class the package exports:

```toml
[lib]
path = "src/lib.rs"

[package.metadata.nros.node]
class = "monitor_node::Listener"
name = "monitor_node"

[dependencies]
nros = { version = "*", default-features = false, features = [
    "alloc",
    "rmw-cffi", "macros"] }
std_msgs = { path = "../../generated/std_msgs", default-features = false }  # nros-managed
```

This registration is the Rust spelling of
`nros_components_register_node(... PLUGIN pkg::Class ...)` in a C or C++
package's `CMakeLists.txt`. A package registers its class once, however many
bringups run it. Which bringup runs it is the component row's business
(step 2). `nros` is never fetched from a registry: the build points it at
your installed nano-ros, so no path in the manifest names where nano-ros
lives and the package copies anywhere.

### `src/lib.rs`

A Rust node implements `Component` (what the node creates, declared once at
startup) and `ExecutableNode` (what each callback does), then exports both
with `nros::node!`. The essential shape, from
[`examples/workspaces/rust/src/talker_pkg/src/lib.rs`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/workspaces/rust/src/talker_pkg/src/lib.rs):

```rust
use nros::{
    CallbackCtx, ExecutableNode, Component, NodeContext, DeclarativeNode, NodeOptions, NodeResult,
    TimerDuration,
};

pub struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";

    // Per-instance slots, in order: publishers, service servers,
    // service clients, action clients, action servers.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let chatter = node.create_publisher_for_topic::<MyMsg>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    type State = i32;

    fn init() -> Self::State { 0 }

    // See the full example for the callback body.
}

nros::node!(Talker);   // exports the node; this is the last line
```

- **`register` declares; it does not run.** It is called once at startup to
  declare publishers, subscriptions, timers and which callback publishes
  what. No message flows here.
- **`on_callback` is the body.** The image's executor calls it each time a
  callback fires; `state` is the node's own mutable storage.
- **The node is the same type everywhere.** `ctx.create_node(...)` returns
  the `nros::Node` a standalone program gets, with rclrs's methods
  (`create_publisher`, `create_subscription`, timers, `logger()`, the graph
  queries). `use nros::DeclarativeNode;` adds the constructors that bind a
  callback by name, which is what lets the board run callbacks from its own
  tasks and lets the build measure the node without running it.
- **`ENTITY_BOUNDS` is static RAM.** `nros::node!` emits a `static` sized
  from it, and a publisher slot carries a loan arena. The default (8 of each
  kind) measured 50,824 bytes of `.bss` for a one-service node, against 568
  bytes when the node declared what it creates. Count the five kinds in your
  `register()`; timers and subscriptions need no slot here. Declaring fewer
  than `register()` creates is an error at boot, never a silent drop, so the
  exact number is safe.
- **Timers.** In a timer callback, `ctx.timer_info()` returns the activation's
  `expected_call_time` and `actual_call_time` (`lateness_ns()` is their
  difference). `create_timer_for_callback_name_with_autostart(name, period,
  clock, false)` declares a stopped timer; any callback starts it with
  `ctx.reset_timer(nros::EntityId::new("on_tick"))` and stops it with
  `ctx.cancel_timer(..)`. To control timers by id, add `.with_timers(n)` to
  `ENTITY_BOUNDS` (16 bytes a timer).

### Calling a service from a node

A node cannot make a blocking service or action call from `on_callback`: the
executor is mid-dispatch there, and the call would deadlock it. Client calls
go in `ExecutableNode::tick`, which the executor runs once per spin between
callbacks. Arm the call in the callback and make it in `tick`:

```rust
fn on_callback(state: &mut State, cb: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
    state.want_call = true;          // decide; do not call here
}

fn tick(state: &mut State, ctx: &mut TickCtx<'_>) {
    if state.want_call {
        state.want_call = false;
        let reply = ctx.call_for_name("add_two_ints", &request);  // blocking is safe here
    }
}
```

A service server needs no such split: requests arrive in `on_callback` like
any other callback. In C++, `rclcpp::bind_service<Svc, T, &T::method>(node,
"/add_two_ints", this)` binds a typed member that takes the generated
`Svc::Request` and returns `Svc::Response`.

## 2. Add a component row

`--to sensor_node_bringup` added this to
`src/sensor_node_bringup/system.toml`, beside the row the scaffold wrote for
`sensor_node`:

```toml
[[component]]
pkg = "monitor_node"
class = "monitor_node::Listener"
name = "monitor_node"
```

| Field | Meaning |
|---|---|
| `pkg` | the node package (its `<name>` in `package.xml`) |
| `class` | the type the package registers |
| `name` | the node's name at runtime; a launch file can remap it |
| `dispatch` | optional: the dispatch shape, when it is not the default |

It also added `<exec_depend>monitor_node</exec_depend>` to the bringup's
`package.xml`. To add a row by hand (for a node package you copied in), write
both.

Run the system. The workspace's first build, in First Project, recorded
your ROS 2 prefix, so a shell that has not sourced ROS 2 builds it too; a
workspace you start fresh needs ROS 2 sourced for its first build, because
the messages are generated from it:

```bash
source /opt/ros/humble/setup.bash   # only needed for a fresh workspace's first build
nros run
```

With nothing named, `nros run` runs the workspace's only image, which now
now holds both nodes in one program. `nros run sensor_node` no longer works. That spelling
means "the bringup whose only node is `sensor_node`", and this bringup now
has two, so it refuses and prints the spellings that would resolve, here
`sensor_node_bringup:baremetal`.

The bringup still has no `launch/`, so the build generates its launch file
from the two rows. After a build, it is at:

```bash
cat build/nros/launch/sensor_node_bringup/launch/system.launch.xml
```

Before a build, two checks catch most mistakes in a bringup (a
`[[component]]` row naming a package or class that does not exist, a stray
build file in the bringup):

```bash
nros check --bringup src/sensor_node_bringup
nros check --workspace .
```

## 3. Add a launch file when you need one

The generated launch file starts every component with its default name and
nothing else. When you need a launch argument, a remap, a namespace or a
parameter, the bringup gets a `launch/` directory. Start from the generated
file, so that adding it changes nothing until you edit it:

```bash
mkdir -p src/sensor_node_bringup/launch
cp build/nros/launch/sensor_node_bringup/launch/system.launch.xml \
   src/sensor_node_bringup/launch/system.launch.xml
```

From then on `launch/system.launch.xml` is used instead of the generated one.
Deleting `launch/` goes back to generation.

Keep each `<node>`'s `pkg` and `exec` as the generated file wrote them; the
examples below write `exec="…"` for that value.

The file is the ROS 2 launch XML schema, read with the same parser, so a
launch file from another ROS 2 project pastes in. For example, a launch
argument that moves both nodes to another topic:

```xml
<launch>
  <arg name="topic" default="/chatter"/>

  <node pkg="sensor_node" exec="…" name="sensor_node">
    <remap from="/chatter" to="$(var topic)"/>
  </node>
  <node pkg="monitor_node" exec="…" name="monitor_node">
    <remap from="/chatter" to="$(var topic)"/>
  </node>
</launch>
```

| Tag | Purpose |
|---|---|
| `<arg name="…" default="…"/>` | declare a launch argument |
| `<node pkg="…" exec="…" name="…"/>` | start a node |
| `<param name="…" value="…"/>` | set one parameter (inside `<node>`) |
| `<param from="….yaml"/>` | load a ROS parameter file (inside `<node>`) |
| `<remap from="…" to="…"/>` | remap a topic or service (inside `<node>`) |
| `<group ns="…">` | put a group of nodes in a namespace |
| `<include file="…"/>` | include another launch file |

Substitutions: `$(var <arg>)` expands a launch argument, `$(find <pkg>)` a
package's path, `$(env <name>)` an environment variable.

There is no `ros2 launch` step. The launch file is resolved when the image is
built, and its result is compiled into the program. An image binds launch
arguments, and can name another launch file in the same bringup:

```toml
[image.baremetal]
board = "mps2-an385-baremetal"
args = { topic = "/sensor" }

[image.baremetal_sim]
board = "mps2-an385-baremetal"
launch = "sim.launch.xml"
```

### Parameters

`<param from="…"/>` takes a standard ROS parameter file, relative to the
launch file:

```yaml
# launch/sensor.param.yaml
/**:                      # every node
  ros__parameters:
    use_sim_time: false
sensor_node:              # this node only
  ros__parameters:
    rate_hz: 25
    limits:
      max_accel: 1.5      # reaches the node as `limits.max_accel`
```

```xml
<node pkg="sensor_node" exec="…" name="sensor_node">
  <param from="sensor.param.yaml"/>
  <param name="rate_hz" value="50"/>   <!-- wins over the file -->
</node>
```

Precedence is ROS's: the `/**` block, then the node's own block (keyed
`sensor_node`, `/ns/sensor_node` or `/**/sensor_node`), then inline
`<param name= value=>`. The values are baked into the program at build time,
so an embedded board reads no file at run time; editing the YAML rebuilds. A
`from=` naming a missing file is a build error.

Two capabilities in `[system] features` decide what the image does with them:

| declared | parameter store (`declare_parameter`, launch values, `--ros-args -p`) | `ros2 param` services |
| --- | --- | --- |
| `params` | yes | none |
| `param_services` | yes | six per node (`get`, `set`, `list`, `describe`, …) |

`params` suits a small board whose nodes only read their configuration: each
of the six services costs a queryable per node. What it gives up is
`ros2 param get|set|list|describe|dump` and the `use_sim_time` switch.

## 4. Add an image

An image is one program: the bringup's nodes, built for one board with one
RMW. It is a block in `system.toml`, not a package. Your bringup has one,
written by the scaffold:

```toml
[image.baremetal]
board   = "mps2-an385-baremetal"
locator = "serial/UART_0#baudrate=115200"
```

Running the same two nodes on your Linux machine is one more block:

```toml
[image.linux]
board = "linux"
rmw   = "cyclonedds"       # this image only; needs no router on a host
```

```bash
nros run linux
```

Nothing in either node package changes. The board's descriptor supplies the
target triple, linker flags, cross compiler and memory budgets, so retargeting
is the `board` line and nothing else.

| Key | Meaning |
|---|---|
| `board` | the nano-ros board id; [Supported Boards](../reference/supported-boards.md) lists them |
| `rmw` | this image's RMW, when it differs from `[system] rmw` |
| `locator`, `net` | an embedded image's network identity (where its router is, its own address) |
| `launch` | a launch file in the bringup; default `[system] default_launch` |
| `args` | launch arguments bound for this image |
| `features` | capability axes added to `[system] features` |
| `os_config` | RTOS configuration files, in order: Zephyr `prj-*.conf` and overlays, or a board network file |
| `panic` | panic policy: `platform`, `halt` or `own` |
| `profile` | a cargo or CMake build profile |
| `before_boot` | a function to call before the board boots ([below](#customising-an-entry)) |

`[image_defaults]` holds what every image shares, so eight images do not
repeat their RMW eight times. A specific block's values replace it; `args`
merge, and `os_config` and `features` add up:

```toml
[image_defaults]
rmw = "zenoh"
```

The RMW, like the board, is chosen at build time; embedded targets have no
`RMW_IMPLEMENTATION` switch. On a Linux image, `ROS_DOMAIN_ID` and
`NROS_LOCATOR` still work at run time; an embedded image bakes the values from
its block. See [Configuration](../guide/build/configure-an-image.md).

### Naming what to run

With two images, `nros run` alone refuses and lists them. The target argument
of `nros run`, `nros build` and `nros flash` accepts, in this order:

| You type | It means |
|---|---|
| `sensor_node_bringup:linux` | that bringup's image; always works |
| `linux` | the image named `linux`, when only one bringup declares it |
| `sensor_node` | the bringup whose only node is `sensor_node`, with its only image |
| nothing | the workspace's only image |

Anything else refuses and lists the `<bringup>:<image>` spellings that would
resolve; a shortcut never picks one of several. `--why` says which row
resolved. To make a bare `nros build` build a chosen set, declare it:

```toml
[system]
default_images = ["linux"]
```

### Zephyr images

A Zephyr image also carries Zephyr's own configuration (`prj.conf`, overlays),
which nothing can derive. It lives in the bringup, one directory per board,
and the image names the fragments it adds:

```text
src/sensor_node_bringup/boards/native_sim_native_64/
├── prj.conf
└── prj-zenoh.conf               # named by [image.<id>] os_config
```

`nros build` writes the Zephyr application itself under `build/` and runs
`west build` on it. The board id is still nano-ros's (`zephyr`), never
Zephyr's spelling. [Configuring an image](../guide/platforms/zephyr.md#configuring-an-image-systemtoml-not-prjconf)
has the rest. A Zephyr application that keeps its own `CMakeLists.txt` and
`prj.conf` and is built with plain `west build` is not a workspace image; it
has [its own chapter](../guide/platforms/zephyr-west-app.md).

## What `nros build` generates

Everything generated lands under `build/`, and none of it is yours to edit or
commit:

- the generated launch file of each bringup without `launch/`;
- the resolved system for each image (`build/nros/models/`);
- the generated message crates;
- each image's entry and the build root that compiles it: a cargo package
  when every node is Rust, a CMake project when any node is C or C++, a
  Zephyr application for a Zephyr board.

`nros build` then runs that host tool, so compiler errors are the compiler's.
`nros build --dry-run` prints the command for an image, and running it
yourself works too. Where each binary lands is
[What `nros build` Produces](../build/build-artifacts.md).

## Customising an entry

The generated entry is one `nros::main!(…)` call, and the macro does a lot: it
opens the executor at the size the image declares, registers every node in the
launch file, and adds the lifecycle, parameter services and tiers the system
asks for. Take the smallest step that does what you need:

| You need | Use | Entry stays generated |
|---|---|---|
| another panic policy, build profile, Kconfig fragment or capability | an `[image.*]` key | yes |
| one step before the runtime starts (open a device, check a calibration) | `before_boot` | yes |
| hardware setup every image on a board needs (clocks, DMA) | the board's `init_hardware`, in a [board package](../extending/board-trait.md) | yes |
| startup none of the above can express | `nros materialize` | no |

**`before_boot`** names a function in one of the image's node packages:

```toml
[image.linux]
board = "linux"
before_boot = "sensor_node::prepare"
```

```rust
// src/sensor_node/src/lib.rs
pub fn prepare() -> Result<(), &'static str> {
    // open the device, check the calibration, wait for the peripheral
    Ok(())
}
```

The entry calls it first, before the board does anything, so nothing has
booted when it runs: no executor, no session, and on bare metal no hardware
setup yet. Returning `Err("reason")` stops the image with that reason. It
works in Rust images only; a C or C++ image that sets it is refused, and so
is a board whose framework owns the first statement. The
worked example is image `linux_rust_before_boot` in
[`examples/workspaces/features`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/features).

**`nros materialize`** is the last resort. It copies an image's generated
entry into `src/<image>_entry/`, and from then on `nros build` uses your copy
and does not regenerate it:

```console
$ nros build linux
$ nros materialize linux
wrote …/src/linux_entry

This entry is YOURS now — `nros build` will not regenerate it.
```

What you get is the same `nros::main!(…)` call inside its crate root. While
you keep the macro, launch-file changes still reach the program; what you own
is the shell around it. Replacing the macro with your own `main` means
writing the boot, node registration, executor sizing and every capability
yourself, and keeping them in step with the launch file by hand. Deleting
`src/<image>_entry/` returns the image to generation.

## Coming from ROS 2

| ROS 2 | nano-ros |
|---|---|
| `ros2 pkg create` | `nros new <pkg>` inside the workspace; `--to <bringup>` adds its component row |
| `colcon build` | `nros build` (every default image) or `nros build <image>` |
| `ros2 run <pkg> <exe>` | `nros run <node>` |
| `ros2 launch <pkg> <file>` | `nros run <bringup>:<image>`; the launch file is compiled into the image |

One image is one program, which runs its nodes in one process on one board.
Several processes are several images.

## Other languages

Nothing on this page is Rust-specific except the node's source file. C and
C++ node packages, and workspaces that mix languages, are
[C / C++ multi-node workspaces](../guide/contracts/cpp-workspaces.md) and
[Mixed-language workspaces](../guide/build/mix-languages.md). [Role reference](../guide/build/roles.md)
lists every field of the three roles.

## Design background

- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md):
  why every project is a workspace, why `system.toml` lives only in a bringup,
  and the target spellings `nros run` accepts.
- [RFC-0120: one resolver for every build road](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0120-nros-configure-one-resolver.md):
  why `nros build` prepares the build itself.

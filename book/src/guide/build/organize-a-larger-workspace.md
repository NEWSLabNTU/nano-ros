# Organize a larger workspace

<!-- type: how-to · level: 2 -->

**Needs first:** [Add a second node](../../tutorial/add-a-second-node.md)

Use this page when a workspace grows past one node and one board: several
node packages, several images, and a bringup per behaviour.

A nano-ros workspace has two kinds of package you write, and one thing that is
not a package:

- A **node package** is a library that says what a node does: its publishers,
  subscriptions, timers and callbacks. It has no `main()`, no board and no
  deployment settings, so one node package runs on every board.
- A **bringup package** says what runs and where. It holds a `package.xml` and
  a `system.toml`, and optionally `launch/` and `config/`. It has no
  `Cargo.toml`, no `CMakeLists.txt` and no code. Name it `<system>_bringup`,
  as nav2 and Autoware do.
- An **image** is one row, `[image.<id>]`, in that `system.toml`: a board, a
  launch topology and a backend. `nros build <id>` generates the program's
  entry under `build/` and hands off to cargo, CMake or west.

The workspace root has no build file at all. Where ROS 2 has you write a
launch file and a `main()` per process, nano-ros generates the entry from the
image row, so adding a board is adding a row, never a package.

## Before you start

- nano-ros installed and the board provisioned, for example
  `nros setup linux`.
- ROS 2 sourced for the first build, which generates message bindings from it.

## Steps

1. Start from a one-node workspace:

   ```bash
   nros new host_ws --board linux --template talker
   cd host_ws
   ```

   ```text
   ✓ Created the workspace host_ws
     src/host_node/   the node: a rust talker (library)
     src/host_node_bringup/   what it runs on: `linux`, image `linux`, zenoh
   ```

2. Add a node package. Run `nros new` from the workspace root; `--to` adds its
   `[[component]]` row to the bringup, and `--lang` picks its language:

   ```bash
   nros new listener_pkg --lang cpp --template listener --to host_node_bringup
   ```

   ```text
   nros new: added the node package src/listener_pkg/ (listener, cpp) to …/host_ws
     declared it in …/host_ws/src/host_node_bringup: a [[component]] row and an <exec_depend>
   ```

Each language states the node in its own build file, and none of them names
a board:

<!-- tabs -->

### Rust

```toml
# src/host_node/Cargo.toml: a library; `nros::node!(Talker);` ends src/lib.rs
[lib]
path = "src/lib.rs"
```

### C++

```cmake
# src/talker_pkg/CMakeLists.txt
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)
nano_ros_auto_add_library(talker_lib STATIC src/Talker.cpp)
nros_components_register_node(talker_lib
    PLUGIN talker_pkg::Talker
    EXECUTABLE talker
    SHAPE configure)
```

The class has a `configure(::rclcpp::Node&)` method that creates its
entities and binds member callbacks by identity, with no callback name and no
allocation; the generated entry constructs it and calls `configure`:

```cpp
::rclcpp::Result Talker::configure(::rclcpp::Node& node) {
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    // Member-fn-pointer-as-template-param → no-alloc trampoline; `this` is ctx.
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
}
```

<!-- /tabs -->

The C shape, and packages of several languages in one image, are in
[Mix languages](mix-languages.md).

3. Add an image for a second board:

   ```bash
   nros new --image freertos --board mps2-an385-freertos
   ```

   ```text
   nros new: added [image.freertos] to …/src/host_node_bringup/system.toml
     build it with `nros build freertos`
   ```

4. Build. With two images and no default, a bare `nros build` lists them and
   stops:

   ```bash
   nros build
   ```

   ```text
   Error: this workspace declares 2 images and no default.

     host_node_bringup:freertos
     host_node_bringup:linux

     build one:   nros build freertos
     build all:   nros build --all
     or declare:  [system] default_images = ["freertos"]
   ```

   Name one image, build them all, or set `[system] default_images` in the
   bringup. `nros build linux --dry-run` prints the handoff and runs nothing:

   ```text
   nros build: host_node_bringup:linux -> board linux (platform posix), driver cmake
   ```

   The driver is CMake here because the image now holds a C++ package.

5. Run one image. Once a bringup has a second node, the node's name no longer
   names one image, so name `<bringup>:<image>`:

   ```bash
   nros run host_node_bringup:linux
   ```

6. Select a machine with launch arguments. Several images can share one
   launch file and differ only in its arguments, which is how one launch tree
   describes several hosts:

   ```toml
   [image.linux_robot1]
   board = "linux"
   launch = "multihost.launch.xml"
   args = { host = "robot1" }

   [image.linux_robot2]
   board = "linux"
   launch = "multihost.launch.xml"
   args = { host = "robot2" }
   ```

   `[image.<id>]` says what is built. `[host.<name>]` says which machine a
   node runs on, with `launch` and `nodes` keys. An image needs no host, and a
   host needs no image.

7. Give each behaviour its own bringup when one workspace holds several
   programs. The language workspaces in the examples keep `pubsub_bringup`,
   `service_bringup` and `action_bringup` side by side, with one image row
   per platform and role:

   ```bash
   nros run pubsub_bringup:linux_listener
   ```

   A variant with different code is its own node package with its own image
   row. A variant that only changes a setting, such as a serial link, is an
   image row over the same packages.

8. Check what the workspace holds. `nros list` prints each package and its
   kind, and `nros graph` prints the dependency order:

   ```bash
   nros graph
   ```

   ```text
   host_node
   listener_pkg
   host_node_bringup  <- host_node, listener_pkg
   ```

## Example

The bringup after the steps above:

```toml
# src/host_node_bringup/system.toml
[system]
name = "host_node"
rmw = "zenoh"
ros_edition = "humble"
domain_id = 0

[[component]]
pkg = "host_node"
class = "host_node::Talker"
name = "host_node"

[[component]]
pkg = "listener_pkg"
class = "listener_pkg::Listener"
name = "listener_pkg"

[image.linux]
board = "linux"

[image.freertos]
board = "mps2-an385-freertos"
```

```bash
nros build --all
nros run host_node_bringup:linux
```

The bringup has no `launch/`, so its launch file is generated from the
`[[component]]` rows. Add `launch/system.launch.xml` when you need a launch
argument, a remap or a parameter, as in
[Parameters and launch files](../../tutorial/grow-your-workspace.md#3-add-a-launch-file-when-you-need-one).
The workspaces under
[`examples/workspaces/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces)
(`rust`, `c`, `cpp` and `mixed`) have this shape, Zephyr images included.

## When the generated entry is not enough

Reach for these in order, smallest first:

1. An image key: `panic`, `profile`, `os_config` or `features`.
2. `before_boot = "<pkg>::<fn>"` on the image, for one step before the
   runtime starts. Rust images only.
3. The board's own hardware setup, for what every image on that board needs.
4. `nros materialize <bringup>:<image>`, which copies the generated entry to
   `src/<image>_entry/` as a package you own. `nros build` then leaves it
   alone; delete the directory to go back to a generated one.

## If it doesn't work

- **`nros build` refuses a `system.toml`.** It sits beside a `Cargo.toml` or
  `CMakeLists.txt`. A `system.toml` lives only in a bringup package; the
  message says where to move it.
- **`nros run <node>` lists images instead of running.** The bringup has
  more than one node or image. Name `<bringup>:<image>`.
- **An edit to the image row changes nothing.** A hand-written
  `src/<image>_entry/` with a `Cargo.toml` or `CMakeLists.txt` takes
  precedence. Move its settings to the `[image.*]` row and delete it.

## See also

- [system.toml keys](../../reference/system-toml.md): every `[image.<id>]`
  key and how blocks fold over `[image_defaults]`.
- [Build outputs](../../reference/build-outputs.md): what `nros build`
  generates under `build/`, and where each artifact lands.
- [Build inside an existing CMake or vendor tree](existing-cmake-or-vendor-tree.md):
  projects that own their build and have no `system.toml`.
- [How a build becomes an image](../../background/how-a-build-becomes-an-image.md):
  the model behind the generated entry.

## Design background

- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)
- [RFC-0065: colcon-like workspace builder (images, hosts, generated entries)](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0065-colcon-like-workspace-builder.md)
- [RFC-0057: ament-parity component registration](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0057-ament-parity-component-registration.md)
- [RFC-0122: behaviour bringups, single-node witnesses, and cadence-backed CI](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md)

# Move a ROS 2 project over

<!-- type: how-to · level: 3 -->

**Needs first:** [Organize a larger workspace](../build/organize-a-larger-workspace.md)

Use this page to move an existing ROS 2 workspace (a `src/` of packages built
with `colcon`, run with `ros2 launch`) to nano-ros, before you port the nodes
one by one.

nano-ros keeps the workspace and package vocabulary: a `src/` of packages
with `package.xml` manifests, launch files in the ROS 2 schema, and the
ROS-shaped names of nodes, topics, services, actions and QoS. It changes the
setup boundary, because it targets embedded builds that have no shared
libraries and no runtime plugin loading:

| You have (ROS 2) | You get (nano-ros) |
| --- | --- |
| a distro install, then `rosdep install` | the `nros` CLI, then `nros setup <board> --rmw <rmw>` |
| `colcon build` | `nros build <image>`, which drives cmake, cargo or west |
| `ros2 launch pkg file`, `ros2 run` | `nros run <bringup>:<image>`: the launch topology is compiled in |
| `RMW_IMPLEMENTATION=…` at start-up | `[system] rmw = "…"` in the bringup's `system.toml`, at build time |
| `package.xml` | kept as it is |
| `.launch.xml`, `.launch.py` | kept, in the bringup package; resolved at build time |
| message packages from the ament index | bindings generated under `build/` by `nros build` |
| `find_package(rclcpp)`, `ament_target_dependencies` | kept: nano-ros's own find modules and ament verbs |

The full account of the differences is in
[How nano-ros differs from ROS 2](../../background/differences-from-ros2.md).

## Before you start

- A ROS 2 Humble workspace that builds with `colcon build`.
- ROS 2 Humble installed. nano-ros generates its message bindings from it,
  and its zenoh router is ROS 2's `rmw_zenohd`.

## Steps

1. Install the CLI and provision the host, in place of the distro install
   and `rosdep`:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh
   export PATH="$HOME/.nros/bin:$PATH"
   nros setup linux --rmw zenoh
   ```

   For a board, name it: `nros setup mps2-an385-freertos --rmw zenoh` fetches
   its cross compiler, emulator and SDK sources into `~/.nros/sdk`. Nothing
   is installed system-wide.

2. Add a bringup package. It replaces the `ros2 launch` command line: a
   `package.xml`, a `system.toml`, and your launch files under `launch/`:

   ```bash
   cd src
   nros new system my_robot_bringup --components talker_pkg,listener_pkg
   ```

   ```text
   nros new system: scaffolded bringup pkg my_robot_bringup at …/src/my_robot_bringup (2 component(s))
   ```

   It writes a `[[component]]` row per package with `class = "<pkg>::TODO"`;
   set each to the node's class. Move the launch files you run into
   `my_robot_bringup/launch/`, replacing the generated `system.launch.xml`. Stock XML
   launch files work as they are; `.launch.py` files are resolved at build
   time too.

3. State the backend and the target once, in the bringup's `system.toml`.
   These two lines replace `RMW_IMPLEMENTATION` and any cross-compile
   recipe:

   ```toml
   [system]
   name = "my_robot"
   rmw  = "zenoh"                 # zenoh | cyclonedds | xrce

   [image.linux]
   board = "linux"

   [image.freertos]
   board = "mps2-an385-freertos"  # the platform follows from the board
   ```

   On Linux, `ROS_DOMAIN_ID` and `NROS_LOCATOR` still work at run time. On a
   board the same values come from the image row, because a board has no
   environment to read them from.

4. Port the nodes. A C++ node keeps its source and its `CMakeLists.txt`; see
   [Port a C++ node](port-a-cpp-node.md). For a Rust or C node, the
   [Tutorial](../../tutorial/add-a-second-node.md) shows the node package
   shapes.

5. Build and run one image:

   ```bash
   source /opt/ros/humble/setup.bash
   nros build linux
   nros run my_robot_bringup:linux
   ```

   The first build records the ROS 2 prefix, so later shells need not source
   ROS 2. A POSIX C++ workspace can still be built with `colcon build` for
   its existing consumers.

6. Check it from stock ROS 2:

   ```bash
   export RMW_IMPLEMENTATION=rmw_zenoh_cpp
   ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
   ```

## Example

The whole move for a two-node project that ran as
`ros2 launch my_robot system.launch.xml` with Cyclone DDS:

```text
my_robot_ws/
└── src/
    ├── talker_pkg/             # unchanged
    ├── listener_pkg/           # unchanged
    └── my_robot_bringup/
        ├── package.xml
        ├── system.toml
        └── launch/system.launch.xml   # moved here, unchanged
```

```toml
# src/my_robot_bringup/system.toml
[system]
name = "my_robot"
rmw  = "cyclonedds"
default_launch = "system.launch.xml"

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::Talker"
name = "talker"

[[component]]
pkg = "listener_pkg"
class = "listener_pkg::Listener"
name = "listener"

[image.linux]
board = "linux"
```

```bash
nros setup linux --rmw cyclonedds
nros build linux
nros run my_robot_bringup:linux
```

Cyclone DDS needs no router, so this interoperates with `rmw_cyclonedds_cpp`
nodes directly.

## If it doesn't work

- **Changing `RMW_IMPLEMENTATION` changes nothing.** The backend is compiled
  in. Edit `[system] rmw` and build again.
- **A stock ROS 2 node does not hear the image.** The two use different
  domain IDs or QoS reliability, the ROS 2 side is on a different RMW, or no
  zenoh router runs. See
  [Talk to ROS 2](../../tutorial/talk-to-ros2.md#troubleshooting).
- **A `<depend>` stops `nros setup`.** It names something that is neither a
  workspace package, a message package, a system key nor a package your ROS
  install provides. Remove it or add the package.
- **The store keeps growing.** Provisioning never overwrites. `nros toolchain
  list` shows each entry and when it was last read, and
  `nros toolchain gc --older-than 90d` proposes what to remove; it removes
  nothing without `--delete`, and never an entry a pin names.

## See also

- [Port a C++ node](port-a-cpp-node.md).
- [How nano-ros differs from ROS 2 and micro-ROS](../../background/differences-from-ros2.md).
- [Switch the RMW per image](../rmw/switch-the-rmw-per-image.md) and
  [Bridge between backends](../rmw/bridge-between-backends.md).

## Design background

- [RFC-0075: zenoh router provenance](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md)
- [RFC-0098: generated leaf build config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)

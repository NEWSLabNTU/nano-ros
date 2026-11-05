# Anatomy of What You Just Built

Two packages, two roles. Everything you add to this project later is another
package with one of these roles, or a few more lines in the bringup. The
shape never changes; it only gains packages.

If you come from ROS 2, each role is something you already know:

| Your workspace | Role | In ROS 2 terms |
|---|---|---|
| `src/sensor_node/` | **Node package** | a composable node (`rclcpp_components`) |
| `src/sensor_node_bringup/` | **Bringup package** | `<pkg>_bringup`: configuration and launch, no code |
| *(generated under `build/`)* | **Entry** | the composition container, the `main()` |

## The node package: the code

A node package is one node, as a library. It has no `main()` and names no
board:

```text
src/sensor_node/
├── Cargo.toml          # a library crate; dependencies only
├── package.xml         # ROS package manifest: std_msgs is declared here
└── src/lib.rs          # the Talker: a 1 s timer that publishes on /chatter
```

`lib.rs` declares what the node creates (a publisher, a timer) and what its
callback does. Message types come from `package.xml`'s dependencies and are
generated during the build; you never write them by hand.

The same node builds for any board. Nothing in this package says
`mps2-an385`, `zenoh` or `serial`: those are the bringup's.

## The bringup package: what runs where

`src/sensor_node_bringup/` holds no code, only `package.xml` and
`system.toml`:

```toml
[system]
name = "sensor_node"
rmw = "zenoh"                        # which middleware
domain_id = 0

[[component]]                        # which nodes run
pkg = "sensor_node"
class = "sensor_node::Talker"
name = "sensor_node"

[image.baremetal]                    # one block per thing you deploy
board = "mps2-an385-baremetal"
locator = "serial/UART_0#baudrate=115200"
```

Every configuration choice lives here, never in a node package. Moving the
talker to another board is a new `[image.*]` block; switching middleware is
the `rmw` line. Neither touches `src/sensor_node/`.

## The launch file: generated until you need one

ROS 2 projects describe what to start in a launch file. This bringup has no
`launch/` directory, so `nros run` generated one from the `[[component]]`
rows. To see it:

```bash
nros plan explain sensor_node_bringup
```

It prints the launch file and says it was generated. When you need what only
a launch file can say (a parameter, a remap, a launch argument), add
`launch/system.launch.xml` to the bringup and it replaces the generated one.
The file is the ROS 2 launch XML schema, so a launch file from another ROS 2
project pastes in unchanged.

## The entry: the binary, also generated

There is no entry package under `src/`. `nros run` (and `nros build`) reads
each `[image.*]` block and generates the entry for it under `build/`: a
`main()` that constructs each component, applies the launch file's
parameters, and runs them on one executor. Running the entry is launching
the system; there is no separate `ros2 launch` step.

## What is yours and what is generated

Everything under `src/` is yours to edit and commit. `build/` holds
everything derived: generated message code, the launch file, the resolved
system and the entry. Nothing in `build/` is committed; if you can
regenerate it, it does not belong in git.

## Next

- [Your own message package](your-own-msg-package.md)
- [Growing your workspace](../build/growing-your-workspace.md): a second
  node, a second board, a launch file.
- [First Node in C or C++](first-node-cpp.md): the same node package in
  another language.

## Design background

[RFC-0121](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)
D1 to D3.

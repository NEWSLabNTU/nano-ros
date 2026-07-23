# `multi-node-workspace` — the canonical 3-role template

This is the **canonical nano-ros workspace shape**: a colcon-style
`src/`-rooted workspace with the three package roles from
[`docs/design/0024-multi-node-workspace-layout.md` §11](../../../docs/design/0024-multi-node-workspace-layout.md).
Copy the whole directory out and rename the packages.

```
multi-node-workspace/
├── Cargo.toml              # [workspace] members = the Rust pkgs; default_system = "demo_bringup"
└── src/
    ├── talker_pkg/         # Node pkg  — lib, nros::node!(Talker),   publishes /chatter
    ├── listener_pkg/       # Node pkg  — lib, nros::node!(Listener), subscribes /chatter
    ├── demo_bringup/       # Bringup pkg — declarative (package.xml + system.toml + launch/);
    │                       #               NO Cargo.toml, NO src/. Not a workspace member.
    └── robot_entry/        # Entry pkg — bin, nros::main!(model = "demo_bringup")
```

## The three roles

| Role        | What it is | Marker |
| ----------- | ---------- | ------ |
| **Node pkg**    | A library implementing `nros::Node` (+ optional `ExecutableNode`), stamped with `nros::node!(T)`. One per node. | `Cargo.toml [package.metadata.nros.node]` |
| **Bringup pkg** | Pure declarative — owns the launch topology + per-target deploy config. No compiled code. **Optional**: required only when ≥2 Entry pkgs share one topology; a single-Entry workspace folds `launch/` + `system.toml` into the Entry pkg. | `package.xml` + `system.toml` + `launch/*.launch.xml` (no `Cargo.toml`) |
| **Entry pkg**   | A binary that boots a topology against a `Board`, via `nros::main!(...)`. One per deploy target. | `Cargo.toml [package.metadata.nros.entry] deploy = "<board>"` |

## ROS 2 ↔ nano-ros map

| ROS 2 (rclcpp / ament)                      | nano-ros                                            |
| ------------------------------------------- | --------------------------------------------------- |
| Composable node (`rclcpp_components`)       | **Node pkg** (`nros::node!`)                         |
| `<pkg>_bringup` with `launch/*.launch.xml`  | **Bringup pkg** (declarative — same launch XML)     |
| Per-target launch + deploy config           | `system.toml` (`[system]`, `[[component]]`, `[deploy.*]`) |
| `ros2 launch <pkg> <file>` (ament install)  | `cargo run -p <entry_pkg>`; the Entry binary is the launch product |
| Composition container / main               | **Entry pkg** (`nros::main!(model = "...")`)         |

The resolved SystemModel (`src/demo_bringup/config/system_model.yaml`,
emitted by `play_launch resolve` from the launch file) is the
**ROS 2 launch schema, verbatim** — `<launch>`, `<arg>`, `<node>`,
`<param>`, `<remap>`, `<group>`, `<include>` with `$(find)` / `$(var)` /
`$(env)` substitutions. nav2 / Autoware / turtlebot3 XML pastes in and
Just Works. (Python `.launch.py` is not supported yet.)

## Build

```bash
nros ws sync
nros codegen-system --bringup demo_bringup
cargo build
```

Builds the two Node pkg rlibs + the `robot_entry` binary. The
`nros::main!(model = "demo_bringup")` macro in
`robot_entry/src/main.rs` walks the workspace package index, parses the
launch XML, and emits one `<node_pkg>::register(runtime)?;` call per
`<node>` entry — so `robot_entry` links and boots both nodes in a single
process. The Node pkgs use generated `std_msgs::msg::Int32`, so run
`nros ws sync` before the first build and after changing message
dependencies.

## Validate the workspace

```bash
nros check --bringup src/demo_bringup   # asserts demo_bringup is pure declarative
nros check --workspace .                # lints <pkg>::<Class> rows, stray system.toml, etc.
```

Both pass for this template.

## Run

The runnable path is the macro-composed Entry binary (one process boots
the whole topology):

```bash
# start a Zenoh router (in another shell)
zenohd --listen tcp/127.0.0.1:7447 &

# boot the demo system
cargo run -p robot_entry
```

`robot_entry` opens the executor against the router, registers `talker`
+ `listener`, and runs the topology.

### Caveat on plan generation

- **`nros plan demo_bringup` / `nros check <plan.json>`** need
  pre-collected source-metadata sidecars (`record.json` +
  per-pkg `_metadata/*.json`) for lib-only Node pkgs in this release —
  the host metadata-mode auto-build (`nros metadata --build`) and the
  launch record parser are not yet wired for this shape. The
  `nros check --bringup` / `nros check --workspace` lints above do work
  and are the relevant gates for a template. See
  `packages/testing/nros-tests/fixtures/orchestration_e2e/` for the
  pre-collected-sidecar plan pipeline.

## Note on C / C++

C and C++ workspaces use the same Node / Bringup / Entry roles through
CMake. See the sibling
[`multi-node-workspace-cpp/`](../multi-node-workspace-cpp/) and
[`c-and-cpp-mixed-workspace/`](../c-and-cpp-mixed-workspace/) templates.

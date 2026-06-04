# `multi-node-workspace` — the canonical 3-role template

This is the **canonical nano-ros workspace shape**: a colcon-style
`src/`-rooted workspace with the three package roles from
[`docs/design/multi-node-workspace-layout.md` §11](../../../docs/design/multi-node-workspace-layout.md).
Copy the whole directory out and rename the packages.

```
multi-node-workspace/
├── Cargo.toml              # [workspace] members = the Rust pkgs; default_system = "demo_bringup"
└── src/
    ├── talker_pkg/         # Node pkg  — lib, nros::node!(Talker),   publishes /chatter
    ├── listener_pkg/       # Node pkg  — lib, nros::node!(Listener), subscribes /chatter
    ├── demo_bringup/       # Bringup pkg — declarative (package.xml + system.toml + launch/);
    │                       #               NO Cargo.toml, NO src/. Not a workspace member.
    └── robot_entry/        # Entry pkg — bin, nros::main!(launch = "demo_bringup:system.launch.xml")
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
| `ros2 launch <pkg> <file>` (ament install)  | `nros launch <bringup>` (no ament install) — see caveat below |
| Composition container / main               | **Entry pkg** (`nros::main!(launch = "...")`)        |

The launch file (`src/demo_bringup/launch/system.launch.xml`) is the
**ROS 2 launch schema, verbatim** — `<launch>`, `<arg>`, `<node>`,
`<param>`, `<remap>`, `<group>`, `<include>` with `$(find)` / `$(var)` /
`$(env)` substitutions. nav2 / Autoware / turtlebot3 XML pastes in and
Just Works. (Python `.launch.py` is not supported yet.)

## Build

```bash
cargo build
```

Builds the two Node pkg rlibs + the `robot_entry` binary. The
`nros::main!(launch = "demo_bringup:system.launch.xml")` macro in
`robot_entry/src/main.rs` walks the workspace package index, parses the
launch XML, and emits one `<node_pkg>::register(runtime)?;` call per
`<node>` entry — so `robot_entry` links and boots both nodes in a single
process.

> The Node pkgs ship a `PlaceholderInt32` message (a 4-byte LE `i32`,
> the wire shape of `std_msgs/Int32`) so the template compiles with a
> plain `cargo build`, no codegen step. To use the real typed `Int32`,
> run `nros generate-rust` for each Node pkg and swap the placeholder for
> `std_msgs::msg::Int32` (see `examples/native/rust/talker/`).

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

### Caveats on the orchestration CLI (nros 0.3.7)

- **`nros launch <bringup>` is not yet aligned with this composed-binary
  shape.** It implements a *one-process-per-`[[component]]`* model — it
  tries to spawn `target/debug/talker_pkg` + `target/debug/listener_pkg`,
  but here the Node pkgs are **libraries** composed into the single
  `robot_entry` binary. Use `cargo run -p robot_entry` instead. (To use
  `nros launch`, you would instead give each Node pkg its own `[[bin]]` —
  the separate-process deployment shape.)
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

`NROS_NODE` / `NROS_MAIN` C / C++ entry macros are **future work**
(Phase 216 / 219). Today the Node + Entry pkg roles are Rust-only; a
polyglot app-node workspace (Rust / C / C++) is demonstrated by the
sibling [`multi-package-workspace/`](../multi-package-workspace/).

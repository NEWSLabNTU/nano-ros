# From an App Node to a Workspace

You built the single-file talker in
[`examples/native/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/rust/talker):
one `main.rs`, one `Cargo.toml`, one package — open a terminal, run
`cargo run`, done. That shape covers a lot of ground: demos, driver
tests, single-purpose microcontroller apps.

At some point it stops being enough.

## When you outgrow one app

Three common triggers:

1. **Two or more nodes.** You want a talker *and* a listener, or a
   sensor driver alongside a control loop. Splitting them into one
   entry binary per node file works until you need to tune their
   wiring or deploy them to different boards.

2. **A shared launch topology.** You want to describe *once* how nodes
   are named, remapped, and parameterised — and reuse that description
   across a dev laptop, a hardware bring-up board, and a sim target.

3. **Multiple deploy targets.** The same talker logic goes on a native
   Linux host for integration testing and on an STM32F4 for
   production. The node logic is identical; only the boot and board
   differ.

That's when you split your project into the three-role model.

## The three roles

**Node pkg** — a `lib` crate that contains one node's logic. It
declares `nros::node!(T)` and carries
`[package.metadata.nros.node]` in its `Cargo.toml`. It has no
`fn main()` — that lives in the Entry pkg. One Node pkg per node.
Think of it as a composable building block: the same `talker_pkg` lib
can be assembled into a native binary *and* an embedded binary without
any source change.

**Bringup pkg** — a purely declarative directory that owns the launch
topology. It contains a `package.xml`, a `system.toml` (listing which
nodes run, how they're wired, per-target deploy config), and a
`launch/` directory with ROS 2 launch XML. **No `Cargo.toml`, no
compiled code.** Naming convention `<system>_bringup`, matching nav2 /
Autoware / turtlebot3. This role is *optional* — a workspace with a
single deploy target can fold the launch files and `system.toml`
directly into the Entry pkg.

**Entry pkg** — a `bin` crate that boots a topology on one specific
board. It carries `[package.metadata.nros.entry]` with `deploy =
"<board>"` and a `src/main.rs` that is just `nros::main!(...)`. One
Entry pkg per deploy target. The Entry pkg links in all Node pkg libs,
links in the board crate, and hands control to the nano-ros runtime.

The app-node shape you already know (`examples/native/rust/talker/`) is
effectively a *fused* Entry + Node pkg: a single package that is both
the logic and the boot point. That fusion is fine — and encouraged —
for single-node work. Only split when you actually need the
flexibility.

## ROS 2 ↔ nano-ros command map

If you're coming from ROS 2, here's the mapping of the commands you
already know:

| ROS 2 | nano-ros | Notes |
|---|---|---|
| `ros2 pkg create` | `nros new <name> --platform <plat> [--lang <lang>]` | scaffolds a Node pkg |
| `colcon build` | `cargo build` (Rust) / `cmake --build build` (C++) | `nros build` delegates to these |
| `ros2 launch <pkg> <file>` | `nros launch <bringup> [--launch <file>]` | host-side; no ament install |
| (plan/validate) | `nros plan` → `nros check` | resolve + statically check the topology |
| `ros2 run <pkg> <exe>` | run the Entry pkg binary (`cargo run`) | one Entry pkg per board |

`nros build` and `nros deploy` exist in the CLI but **delegate** to
the underlying build system — `cargo` for Rust, `cmake` for C/C++,
`west` for Zephyr. Use those tools directly when you need fine-grained
control; `nros build` is the convenience wrapper.

## The app-node shape stays valid

There is no obligation to restructure. If your project is one node on
one board, the app-node shape (`src/main.rs` + one package = both
logic and boot) is perfectly idiomatic and has no runtime penalty. The
three-role split is a *tool* for when you need it, not a gatekeeping
requirement.

## Where to go next

Walk through the three-role model step by step:

1. [Prepare node packages](./workspace-node-pkgs.md) — scaffold and
   implement Node pkgs with `nros::node!`.
2. [Bringup: launch + system.toml](./workspace-bringup.md) — declare
   your topology in a Bringup pkg.
3. [Entry package: boot on a board](./workspace-entry-pkg.md) — write
   the Entry pkg that boots everything together.

For the full API reference covering all three roles, see
[Node, Bringup & Entry Packages](../user-guide/component-and-entry-pkg.md).

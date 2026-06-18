# Role Reference: Node, Bringup, and Entry Packages

nano-ros multi-node workspaces split into **three kinds of package**:

- **Node pkg** — a reusable, board-agnostic node library. Defines what a node *does* (publishers, subscribers, timers, services, actions) and registers itself with the `nros::node!(T)` macro. No `main()`, no board pick, no deploy config. (Previously called a *component package*; renamed to *Node pkg* to match ROS 2 composable-node naming.)
- **Bringup pkg** — pure declarative: owns the launch topology and per-target deploy config. Contains a `package.xml`, `system.toml`, `launch/*.launch.xml`, and optional `config/`. **No** `Cargo.toml`, no compiled code. Named `<system>_bringup`. **Optional** — only required when ≥2 Entry pkgs share one topology; a single-Entry workspace folds `launch/` + `system.toml` into the Entry pkg directly.
- **Entry pkg** — a per-board binary that *composes* one or more Node pkgs into a runnable system. Owns the board choice (via the `Board` trait family), the launch file reference, and the deploy/domain/bridge config. Typically ~30 LoC of `main.rs`.

The split exists because a node's logic is portable across boards, but boot + transport + deploy config is not. One Node pkg can be reused across native POSIX, FreeRTOS, and Zephyr targets by writing one Entry pkg per target.

> **single-Node convenience:** for a single-Node workspace on native hosts, a Node pkg can declare `[package.metadata.nros.entry] deploy = "<board>"` in its own `Cargo.toml` and skip the Entry pkg directory entirely — see [Single-Node-pkg convenience](#single-node-pkg-convenience-cargo-run-just-works) below. Embedded boards still need a hand-written Entry pkg.

## Node pkg

A Node pkg is a normal Rust library (or C++ static library) with a few nano-ros-specific knobs:

```
src/talker_pkg/
├── Cargo.toml          # [lib] crate-type = ["rlib", "staticlib"]
│                       # [package.metadata.nros.node]
│                       #     class = "talker_pkg::Talker"
├── package.xml         # ROS 2 package manifest (<exec_depend> etc.)
├── src/
│   └── lib.rs          # impl Node for Talker { … }
│                       # nros::node!(Talker);
└── launch/             # OPTIONAL — per-node launch fragment
    └── talker.launch.xml
```

`src/lib.rs` declares the user class, implements `Node` +
`ExecutableNode` (`init` / `on_callback` / optional `tick`), and
ends with `nros::node!(Talker);` to emit the register trampoline.
Codegen owns the spin loop — your code only describes what the node
*has* and what its callbacks *do*.

Key rules:

- **No `fn main()`.** A Node pkg builds as `rlib + staticlib` and is *linked into* an Entry pkg's binary. Codegen synthesises the spin driver; you never hand-write one.
- **`class` field must start with the pkg dir name.** `nros check` rejects `class = "foo::Talker"` inside `src/talker_pkg/` — the pkg name and the class prefix are the same identity. (Phase 212.L.4.)
- **C++ / C analogue:** `nano_ros_node_register(NAME … CLASS … SOURCES …)` cmake fn + a typed component in the source — C++ a `configure(::nros::Node&)` method, C a `NROS_C_COMPONENT(StateT, configure_fn)` seam (RFC-0043). Same conceptual shape, no Cargo.toml.
- **`package.xml` is mandatory.** Even pure-Rust Node pkgs ship one — `<exec_depend>` lines drive ROS 2 launch discovery when the system runs through `ros2 launch` outside the nano-ros toolchain.

## Bringup pkg (optional)

A Bringup pkg is **pure declarative** — it owns the launch topology and
per-target deploy config, and contains no compiled code:

```
src/demo_bringup/
├── package.xml          # <name>demo_bringup</name>, <exec_depend> per node
├── system.toml          # [system] + [[component]] + [deploy.<target>] (+ [[domain]]/[[bridge]])
├── launch/
│   └── system.launch.xml   # ROS 2 launch schema, verbatim
└── config/                 # optional — params.yaml, etc.
```

No `Cargo.toml`, no `CMakeLists.txt`, no `src/`. Naming convention
`<system>_bringup` (alias `<system>_launch`), matching nav2 / Autoware /
turtlebot3. It is **optional**: required only when two or more Entry pkgs
share one topology. A single-Entry workspace folds `launch/` + `system.toml`
into the Entry pkg directly.

`launch/*.launch.xml` is the ROS 2 launch schema verbatim — `<launch>`,
`<arg>`, `<node>`, `<param>`, `<remap>`, `<group>`, `<include>`, with
`$(find <pkg>)` / `$(var)` / `$(env)` substitutions. Stock nav2/Autoware
XML pastes in and Just Works (Python `.launch.py` is not supported yet).
See [the workspace bringup tutorial](../getting-started/workspace-bringup.md).

## Entry pkg

An Entry pkg is a binary crate that combines one or more Node pkgs with a board choice, a launch file, and per-board deploy config:

```
src/robot_entry/
├── Cargo.toml          # [[bin]] name = "robot_entry"
│                       # [dependencies]
│                       #     talker_pkg   = { path = "../talker_pkg" }
│                       #     listener_pkg = { path = "../listener_pkg" }
│                       #     nros-board-posix = { … }            # or another family
│                       # [package.metadata.nros.entry]
│                       #     deploy = "native"
│                       # [package.metadata.nros.deploy.native]
│                       #     board     = "posix"
│                       #     rmw       = "zenoh"
│                       #     domain_id = 0
│                       #     locator   = "tcp/127.0.0.1:7447"
├── package.xml         # <exec_depend>talker_pkg</exec_depend>, listener_pkg, …
└── src/
    └── main.rs         # one line: `nros::main!(launch = "demo_bringup");`
```

The `nros::main!()` proc-macro (Phase 212.N.9) reads the launch file
at compile time, walks the workspace pkg-index for each `<node pkg=…>`
entry, and expands to a `fn main()` that delegates to
`<Board as BoardEntry>::run(...)`, dispatching one
`<pkg>::register(runtime)?` call per launch row. The macro has four
forms; pick whichever matches your composition shape:

```rust,ignore
nros::main!();                                          // single-node self-bringup (reads [..nros.entry] deploy)
nros::main!(board = NativeBoard);                       // single-node, explicit board
nros::main!(launch = "demo_bringup");                   // multi-node, default launch from system.toml
nros::main!(launch = "demo_bringup:sim.launch.xml");    // multi-node, explicit file
nros::main!(                                            // all explicit
    board  = NativeBoard,
    launch = "demo_bringup:sim.launch.xml",
    args   = [("use_sim", "true")],
);
```

Form-1 (no args) reads
`[package.metadata.nros.entry] deploy = "<board>"` from this Entry
pkg's own `Cargo.toml` and maps the board key
(`"native"`/`"freertos"`/`"zephyr"`/…) to the right board crate
via a small lookup table. Forms 2–4 use the user-supplied path
verbatim. Forms 3/4 reference a Bringup pkg by `<bringup>[:<file>]` —
the Bringup pkg's dir hosts `launch/<file>.launch.xml` plus an optional
`system.toml` naming the default file (`[system] default_launch = "..."`).
The `nros::main!()` expansion replaces the older
`build.rs + include!(env!("OUT_DIR")/run_plan.rs)` shape end-to-end;
new Entry pkgs no longer need a `build.rs` or a `nros-build`
build-dep — just `nros` + the target board crate.

**Escape hatch:** skip the macro entirely and call
`<NativeBoard as BoardEntry>::run(|runtime| { ... })`, or go fully manual
with `nros::Executor::open(&ExecutorConfig::default())`.

Key rules:

- **One Entry pkg per board target.** Want to run the same nodes on native POSIX, on a QEMU-MPS2-AN385 FreeRTOS target, and on a real ThreadX board? Three Entry pkgs (`robot_entry_native`, `robot_entry_qemu_freertos`, `robot_entry_acme_threadx`) sharing the same Node pkgs and (usually) the same `launch/system.launch.xml` via symlink or `<include>`.
- **`launch/system.launch.xml` is the canonical name.** `nros plan` resolution order: `--file <path>` → `<dir>/launch/<pkg>.launch.xml` → `<dir>/launch/system.launch.xml` → the single `<dir>/launch/*.launch.xml` → synth (only for non-Entry, single-Node pkgs).
- **Deploy config lives in `Cargo.toml`.** `[package.metadata.nros.deploy.<target>]` holds board / RMW / domain / locator per target; `[[package.metadata.nros.domain]]` and `[[package.metadata.nros.bridge]]` carry multi-domain topology.
- **C++ analogue:** cmake fn `nano_ros_entry(NAME <name> LANGUAGE CXX LAUNCH …)` plus `NROS_MAIN(...)`. Metadata flows through `${BUILD}/nros-metadata.json` rather than a sidecar TOML.

## Workspace shape

A typical multi-Node workspace, with one Entry pkg per supported board:

```
my_ws/
├── Cargo.toml          # [workspace] members = ["src/talker_pkg", "src/listener_pkg", "src/robot_entry"]
│                       # [workspace.metadata.nros] default_system = "demo_bringup"
└── src/
    ├── talker_pkg/         # Node pkg (lib, nros::node!)
    ├── listener_pkg/       # Node pkg
    ├── demo_bringup/       # Bringup pkg (declarative; no Cargo.toml)
    └── robot_entry/        # Entry pkg (bin, nros::main!(launch = "demo_bringup"))
```

`cargo build` at the workspace root builds everything via cargo's native scheduler. `nros plan` reads `[workspace.metadata.nros] default_system` to pick the Entry pkg (or you pass `nros plan robot_entry` explicitly).

For C++-majority or mixed workspaces, CMake is the top-level driver instead — see [the multi-node workspace design doc](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0024-multi-node-workspace-layout.md).

## Single-Node-pkg convenience (`cargo run` Just Works)

For tiny fixtures and host-side dev loops, a Node pkg can declare itself as its own Entry pkg by adding `[package.metadata.nros.entry] deploy = "<board>"` to its `Cargo.toml`, alongside the usual `[package.metadata.nros.node]` and `[package.metadata.nros.deploy.<target>]` tables. `src/main.rs` collapses to one line:

```rust,ignore
// src/main.rs
nros::main!();
```

The macro reads `deploy = "<board>"` from this pkg's own `Cargo.toml`,
maps it to the right board crate, and emits
`fn main()` + `<this_pkg>::register(runtime)?;` — the latter resolves
through the companion `src/lib.rs` cargo auto-wires alongside the
binary target. No `build.rs`, no launch file (one is synthesised
in-memory), no hand-written boot glue. This is the L.7 self-entry
planner path (Phase 212.L.7 + N.5 + N.9).

**Limits of the convenience:**

- **Native only.** Embedded boards (FreeRTOS, ThreadX, Zephyr, bare-metal) still require a hand-written Entry pkg — board init is non-trivial enough that hiding it behind a one-liner does more harm than good.
- **One Node.** Two or more Node pkgs in the same workspace = author an Entry pkg. The point of the convenience is to skip ceremony for tiny single-node fixtures, not to grow into a multi-node composition root.

## Quick reference

| You want… | Use |
|---|---|
| Reusable node logic, board-independent | Node pkg (`nros::node!()`) |
| Per-board binary that runs N nodes | Entry pkg (`main.rs` calls `BoardEntry::run`) |
| `cargo run` on host for a single-node fixture | Node pkg with `[package.metadata.nros.entry] deploy = "native"` |
| Same nodes on multiple boards | One Node pkg set + one Entry pkg per board |
| Launch topology + per-target deploy config | Bringup pkg (declarative; optional, folds into Entry pkg when single-target) |
| Board hardware bringup | `Board` trait family (see [porting chapter](../porting/board-trait.md)) |

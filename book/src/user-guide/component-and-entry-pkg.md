# Component + Entry Pkg Cookbook

nano-ros multi-node workspaces split into **two kinds of package**:

- **Component pkg** — a reusable, board-agnostic node library. Defines what a node *does* (publishers, subscribers, timers, services, actions) and registers itself with the `nros::component!()` macro. No `main()`, no board pick, no deploy config.
- **Entry pkg** — a per-board binary that *composes* one or more Component pkgs into a runnable system. Owns the board choice (via the `Board` trait family), the launch file, and the deploy/domain/bridge config. Typically ~30 LoC of `main.rs`.

The split exists because a node's logic is portable across boards, but boot + transport + deploy config is not. One Component pkg can be reused across native POSIX, FreeRTOS, and Zephyr targets by writing one Entry pkg per target.

> **Convenience shortcut:** for a single-Component workspace on native hosts, a Component pkg can declare `[package.metadata.nros.entry] deploy = "<board>"` in its own `Cargo.toml` and skip the Entry pkg directory entirely — see [Single-Component-pkg convenience](#single-component-pkg-convenience-cargo-run-just-works) below. Embedded boards still need a hand-written Entry pkg.

## Component pkg

A Component pkg is a normal Rust library (or C++ static library) with a few nano-ros-specific knobs:

```
pkgs/talker/
├── Cargo.toml          # [lib] crate-type = ["rlib", "staticlib"]
│                       # [package.metadata.nros.component]
│                       #     class = "talker::Talker"
├── package.xml         # ROS 2 package manifest (<exec_depend> etc.)
├── src/
│   └── lib.rs          # impl Component for Talker { … }
│                       # nros::component!(Talker);
└── launch/             # OPTIONAL — per-component launch fragment
    └── talker.launch.xml
```

`src/lib.rs` declares the user class, implements `Component` +
`ExecutableComponent` (`init` / `on_callback` / optional `tick`), and
ends with `nros::component!(Talker);` to emit the register trampoline.
Codegen owns the spin loop — your code only describes what the node
*has* and what its callbacks *do*.

Key rules:

- **No `fn main()`.** A Component pkg builds as `rlib + staticlib` and is *linked into* an Entry pkg's binary. Codegen synthesises the spin driver; you never hand-write one.
- **`class` field must start with the pkg dir name.** `nros check` rejects `class = "foo::Talker"` inside `pkgs/talker/` — the pkg name and the class prefix are the same identity. (Phase 212.L.4.)
- **C++ analogue:** `nano_ros_component_register(NAME … CLASS … SOURCES … DEPLOY …)` cmake fn + `NROS_COMPONENT_REGISTER(UserClass, "<pkg>::UserClass")` in the source. Same conceptual shape, no Cargo.toml.
- **`package.xml` is mandatory.** Even pure-Rust Component pkgs ship one — `<exec_depend>` lines drive ROS 2 launch discovery when the system runs through `ros2 launch` outside the nano-ros toolchain.

## Entry pkg

An Entry pkg is a binary crate that combines one or more Component pkgs with a board choice, a launch file, and per-board deploy config:

```
pkgs/robot_entry/
├── Cargo.toml          # [[bin]] name = "robot_entry"
│                       # [dependencies]
│                       #     talker   = { path = "../talker" }
│                       #     listener = { path = "../listener" }
│                       #     nros-board-posix = { … }            # or another family
│                       # [package.metadata.nros.entry]
│                       #     deploy = "native"
│                       # [package.metadata.nros.deploy.native]
│                       #     board     = "posix"
│                       #     rmw       = "zenoh"
│                       #     domain_id = 0
│                       #     locator   = "tcp/127.0.0.1:7447"
├── package.xml         # <exec_depend>talker</exec_depend>, listener, …
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
nros::main!();                                          // single-node self-bringup
nros::main!(board = NativeBoard);                       // single-node, explicit board
nros::main!(launch = "demo_bringup");                   // multi-node, default launch
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
verbatim. Forms 3/4 expect a separate "bringup" pkg whose dir
hosts `launch/<file>.launch.xml` plus an optional `system.toml`
naming the default file (`[system] default_launch = "..."`).
The `nros::main!()` expansion replaces the older
`build.rs + include!(env!("OUT_DIR")/run_plan.rs)` shape end-to-end;
new Entry pkgs no longer need a `build.rs` or a `nros-build`
build-dep — just `nros` + the target board crate.

Key rules:

- **One Entry pkg per board target.** Want to run the same components on native POSIX, on a QEMU-MPS2-AN385 FreeRTOS target, and on a real ThreadX board? Three Entry pkgs (`robot_entry_native`, `robot_entry_qemu_freertos`, `robot_entry_acme_threadx`) sharing the same Component pkgs and (usually) the same `launch/system.launch.xml` via symlink or `<include>`.
- **`launch/system.launch.xml` is the canonical name.** `nros plan` resolution order: `--file <path>` → `<dir>/launch/<pkg>.launch.xml` → `<dir>/launch/system.launch.xml` → the single `<dir>/launch/*.launch.xml` → synth (only for non-Entry, single-Component pkgs).
- **Deploy config lives in `Cargo.toml`.** `[package.metadata.nros.deploy.<target>]` holds board / RMW / domain / locator per target; `[[package.metadata.nros.domain]]` and `[[package.metadata.nros.bridge]]` carry multi-domain topology. The retired `system.toml` is gone — `nros check` rejects it.
- **C++ analogue:** cmake fn `nano_ros_entry(NAME <name> SOURCES … BOARD <board> DEPLOY …)` plus `nano_ros_deploy(TARGET … RMW … DOMAIN_ID … LOCATOR …)`. (`nano_ros_entry` is renamed from the older `nano_ros_application` per Phase 212.N.6.) Metadata flows through `${BUILD}/nros-metadata.json` rather than a sidecar TOML.

## Workspace shape

A typical multi-Component workspace, with one Entry pkg per supported board:

```
my_robot/
├── Cargo.toml          # [workspace] members = [
│                       #     "pkgs/talker",
│                       #     "pkgs/listener",
│                       #     "pkgs/robot_entry_native",
│                       #     "pkgs/robot_entry_freertos",
│                       # ]
│                       # [workspace.metadata.nros]
│                       #     default_system = "robot_entry_native"
├── pkgs/
│   ├── talker/                  # Component pkg (lib)
│   ├── listener/                # Component pkg (lib)
│   ├── robot_entry_native/      # Entry pkg (bin, board = posix)
│   └── robot_entry_freertos/    # Entry pkg (bin, board = qemu-mps2-an385-freertos)
└── .gitignore          # /target/  /build/
```

`cargo build` at the workspace root builds everything via cargo's native scheduler. `nros plan` reads `[workspace.metadata.nros] default_system` to pick the Entry pkg (or you pass `nros plan robot_entry_freertos` explicitly).

For C++-majority or mixed workspaces, CMake is the top-level driver instead — see [the multi-node workspace design doc](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/multi-node-workspace-layout.md).

## Single-Component-pkg convenience (`cargo run` Just Works)

For tiny fixtures and host-side dev loops, a Component pkg can declare itself as its own Entry pkg by adding `[package.metadata.nros.entry] deploy = "<board>"` to its `Cargo.toml`, alongside the usual `[package.metadata.nros.component]` and `[package.metadata.nros.deploy.<target>]` tables. `src/main.rs` collapses to one line:

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
- **One Component.** Two or more Component pkgs in the same workspace = author an Entry pkg. The point of the convenience is to skip ceremony for tiny single-node fixtures, not to grow into a multi-component composition root.

## Quick reference

| You want… | Use |
|---|---|
| Reusable node logic, board-independent | Component pkg (`nros::component!()`) |
| Per-board binary that runs N components | Entry pkg (`main.rs` calls `BoardEntry::run`) |
| `cargo run` on host for a single-node fixture | Component pkg with `[package.metadata.nros.entry] deploy = "native"` |
| Same components on multiple boards | One Component pkg set + one Entry pkg per board |
| Composition root (launch file + deploy config) | Entry pkg (replaces the retired Bringup pkg) |
| Board hardware bringup | `Board` trait family (see [porting chapter](../porting/board-trait.md)) |

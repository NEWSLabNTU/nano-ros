# Rust, C, and Mixed-Language Workspaces

The [anatomy](anatomy.md) does not change with the language: node
packages hold code, the bringup holds configuration, an entry per image.
This page is what changes — per language — inside that fixed shape.

## Rust

Scaffold the whole workspace in Rust, or add Rust node packages to an
existing one:

```bash
nros new my_robot --workspace --lang rust      # whole workspace
nros new my_node --component --lang rust       # one node pkg, added under src/
```

A Rust node package is a library crate carrying `nros::node!(YourNode)`;
the entry collapses to one line:

```rust
nros::main!(launch = "demo_bringup", spin = "forever");
```

Two Rust-specific facts:

- **`nros sync` before the first build**, and again after editing `.msg`
  files or moving the checkout. It generates the message crates and the
  `[patch.crates-io]` table that maps `nros` dependencies to your
  checkout — cargo cannot resolve the workspace without it. C and C++
  never need this; their codegen runs inside CMake.
- **The RMW is spelled in three places** (the bringup's `system.toml`,
  the entry's board-crate feature, the `nros` facade feature). The
  scaffold bakes all three from `--rmw`; changing it later is the
  checklist in [Switching RMW in Config](../user-guide/rmw-switching.md).

## C

C node packages join a workspace the same way:

```bash
nros new my_c_node --component --lang c
```

A C node package is the declarative component shape — a `configure`
function binding callbacks through `nros/nros.h` — built by the same
per-package CMake as C++. There is no C workspace scaffold; start from
the C++ workspace (`nros new --workspace`) and add C packages, which is
also what the in-tree reference does.

## Mixed — and why it just works

Languages mix per *package*, not per file, and the bringup does not care:
a `[[component]]` row names a package and a class/entry point, whatever
implements it. The in-tree reference workspace
(`examples/workspaces/mixed/`) runs a C talker, a C++ listener, and a
Rust heartbeat under one C++ entry:

```text
mixed/src/
├── c_talker_pkg/          # C — publishes std_msgs/Int32 on /chatter
├── cpp_listener_pkg/      # C++ — subscribes
├── rust_heartbeat_pkg/    # Rust — a timer callback
├── demo_bringup/          # one system.toml for all three
└── native_entry/          # one C++ entry boots them together
```

Copy it out as a starting point if your project is mixed from day one.

## Next

- [Mixed-language workspaces, in depth](workspace-mixed-language.md)
- [C / C++ multi-node workspaces](workspace-cpp.md)
- [Node packages](workspace-node-pkgs.md)

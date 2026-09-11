# Rust, C, and Mixed-Language Workspaces

The [anatomy](anatomy.md) does not change with the language: node
packages hold code, the bringup holds configuration, and `nros build`
generates one entry per image under `build/`. This page is what changes —
per language — inside that fixed shape.

Two commands build any of them, in this order:

```bash
nros sync
nros build
```

## Rust

Scaffold the whole workspace in Rust, or add Rust node packages to an
existing one:

```bash
nros new my_robot --workspace --lang rust      # whole workspace
nros new my_node --component --lang rust       # one node pkg, added under src/
```

A Rust node package is a library crate carrying `nros::node!(YourNode)`.
You write no entry: the one `nros build` generates for each image collapses
to one line, and this is what it holds —

```rust
nros::main!(launch = "demo_bringup", spin = "forever");
```

Two Rust-specific facts:

- **`nros sync` before the first build**, and again after editing `.msg`
  files or moving the checkout. It generates the message crates, and — per
  image — the settings file cargo is handed with `--config`: the board's
  rustc triple, the link flags, the `[env]`, the patch rows (RFC-0098 D1).
  Nothing of that is written beside your packages, and cargo cannot resolve
  the build without it. A C or C++ workspace runs `nros sync` too; what it
  does not get from it is the message codegen, which runs inside CMake.
- **The RMW is spelled once** — `[system] rmw = "zenoh"` in the bringup's
  `system.toml`, or `rmw =` on a single `[image.*]` row when one image
  differs. The board crate's feature and the `nros` facade feature are
  generated from it, so switching backend is a one-word edit plus
  `nros sync`: [Switching RMW in Config](../user-guide/rmw-switching.md).

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
Rust heartbeat, composed into one binary:

```text
mixed/src/
├── c_talker_pkg/          # C — publishes std_msgs/Int32 on /chatter
├── cpp_listener_pkg/      # C++ — subscribes
├── rust_heartbeat_pkg/    # Rust — a timer callback
├── …                      # more C and C++ node pkgs: service + action pairs
├── demo_bringup/          # one system.toml for all of them
└── zephyr_entry/          # the one hand-written application: west needs a directory
```

There is no entry package for the native images and no root build file: the
entry for each `[image.*]` is generated under `build/`, and the driver is cmake
because the package graph crosses languages. Copy the workspace out as a
starting point if your project is mixed from day one.

## Next

- [Mixed-language workspaces, in depth](workspace-mixed-language.md)
- [C / C++ multi-node workspaces](workspace-cpp.md)
- [Node packages](workspace-node-pkgs.md)

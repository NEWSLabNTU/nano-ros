# Node packages

> **This page is part of the Multi-Node Projects group.**
> Previous: [Project layout](./workspace-from-app-node.md) —
> Next: [Bringup packages](./workspace-bringup.md)

A **Node pkg** is the unit of reusable behaviour in a multi-node workspace.
It is a Rust library — a `[lib]` crate — that implements one node and
registers it with `nros::node!(T)`.
The Entry pkg is what boots the binary; the Node pkg is what runs inside it.

---

## Prereqs

Pick one path from a fresh checkout — `just` is NOT a prereq.

**A. Bare machine** (no Rust, no `just`, no cargo):
```sh
./scripts/bootstrap.sh base
```
Installs rustup, just, builds the in-tree `nros` CLI at
`packages/cli/target/release/nros`, leaves the binary on PATH for
this shell.

**B. Already have cargo** (most contributors):
```sh
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
export PATH="$PWD/packages/cli/target/release:$PATH"
```

**C. Tagged release, no Rust at all**:
```sh
./scripts/install-nros-prebuilt.sh
```
Downloads the matching `nros-<triple>.tar.gz` from the GitHub release,
sha256-verifies, installs to `packages/cli/target/release/nros`.

Every subsequent shell sources the workspace env via one of:
```sh
direnv allow                  # if you use direnv
source ./activate.sh          # bash / zsh
source ./activate.fish        # fish
```

Then provision the native host:
```sh
nros setup native --rmw zenoh
```

---

## Scaffolding a Node pkg

Use `nros new` to create a skeleton:

```bash
nros new talker --platform native --lang rust
```

`nros new` creates a project skeleton.
For a workspace, move (or create) the result under `src/talker_pkg/` so the
workspace root's `Cargo.toml` can include it as a member:

```toml
# workspace Cargo.toml
[workspace]
resolver = "2"
members = ["src/talker_pkg", "src/listener_pkg", "src/native_entry"]
```

---

## Node pkg anatomy

A Node pkg has three files:

```
src/talker_pkg/
├── package.xml          # ROS 2 manifest — <exec_depend> per message package
├── Cargo.toml           # [lib] + [package.metadata.nros.node] metadata
└── src/lib.rs           # impl Node + ExecutableNode; ends with nros::node!(Talker);
```

No `fn main()` here — a Node pkg is a library linked into an Entry pkg.
The Entry pkg's macro-generated runtime owns `nros::init`, executor open,
RMW registration, and the spin/yield loop.

---

## `Cargo.toml` — the `[package.metadata.nros.node]` block

The metadata block is what the `nros` CLI reads to discover, name, and
wire this node into a topology.
From [`examples/stm32f4/rust/talker_pkg/Cargo.toml`](../../../../examples/stm32f4/rust/talker_pkg/Cargo.toml):

```toml
[lib]
crate-type = ["rlib"]

[dependencies]
nros = { path = "../../../../packages/core/nros", default-features = false,
         features = ["alloc", "rmw-cffi", "platform-bare-metal", "ros-humble"] }

[package.metadata.nros.node]
class = "stm32f4_talker_pkg::Talker"
name = "talker"
default_namespace = "/"
```

The three fields in `[package.metadata.nros.node]`:

| Field | Purpose |
|---|---|
| `class` | Fully-qualified Rust path to the type that `impl`s `Node + ExecutableNode` |
| `name` | Default ROS 2 node name (remappable at launch) |
| `default_namespace` | Default namespace (remappable at launch) |

For a native workspace the `nros` dep would use `features = ["std", "rmw-cffi", "platform-posix", "ros-humble"]` instead of `platform-bare-metal`. The RMW feature (`rmw-zenoh`, `rmw-xrce`, `rmw-cyclonedds`) is chosen at build time — it is not baked into the Node pkg itself.

---

## `src/lib.rs` — the node implementation

A Node pkg implements two traits: `Node` (declarative registration) and
`ExecutableNode` (per-callback body), then calls `nros::node!` to export the
trampolines the Entry macro expects.

Here is the essential shape, drawn from
[`examples/stm32f4/rust/talker_pkg/src/lib.rs`](../../../../examples/stm32f4/rust/talker_pkg/src/lib.rs)
(see that file for the full worked version):

```rust
use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node,
    NodeContext, NodeId, NodeOptions, NodeResult, TimerDuration,
};

pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeId::new("node"), NodeOptions::new("talker"))?;
        let _pub = node.create_publisher::<MyMsg>(EntityId::new("pub_chatter"), "/chatter")?;
        let _timer = node.create_timer(
            EntityId::new("timer_tick"),
            CallbackId::new("on_tick"),
            TimerDuration::from_millis(1000),
        )?;
        node.callback(CallbackId::new("on_tick"))
            .publishes(EntityId::new("pub_chatter"))?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    type State = i32;

    fn init() -> Self::State { 0 }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            // publish ...
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);   // <-- exports the trampolines; this is the last line
```

Key points:

- `Node::register` is **declarative** — it runs once at startup to declare
  publishers, subscriptions, timers, and callback edges. No message bytes
  flow here.
- `ExecutableNode::on_callback` is the **body** — called by the Entry pkg's
  executor each time a callback fires. `state` is your per-node mutable storage.
- `nros::node!(Talker)` **must be the last public API call** in the file.
  It generates the `extern "C"` trampolines the Entry macro imports.
- There is **no `fn main()`** in a Node pkg.

---

## `package.xml` — the ROS 2 manifest

A Node pkg that uses generated message types lists them as `<exec_depend>`
entries. Minimal example:

```xml
<?xml version="1.0"?>
<package format="3">
  <name>talker_pkg</name>
  <version>0.1.0</version>
  <description>Talker node</description>
  <maintainer email="dev@example.com">Developer</maintainer>
  <license>MIT OR Apache-2.0</license>

  <depend>std_msgs</depend>

  <export>
    <build_type>ament_cargo</build_type>
  </export>
</package>
```

If your node uses no external message packages, the `<depend>` line can be
omitted.

---

## Building

From the workspace root, sync generated interfaces first, then let Cargo
compile the Node pkgs and Entry pkg:

```bash
# From examples/workspaces/rust/ (or your workspace root):
nros ws sync
nros codegen-system --bringup demo_bringup
cargo build -p native_entry
```

No per-Node-pkg invocation is needed — the workspace resolver handles
dependency ordering.

To cross-compile for an embedded target, pass `--target` and ensure
`.cargo/config.toml` in the workspace root sets the right linker and target:

```bash
cargo build --target thumbv7em-none-eabihf
```

---

## Next steps

- **[Bringup packages](./workspace-bringup.md)** — wire the Node
  pkgs together into a topology.
- **[Entry packages](./workspace-entry-pkg.md)** — build the
  binary that boots the topology on real hardware or a host process.
- **[Role reference](../user-guide/component-and-entry-pkg.md)** —
  the full reference for all three roles, metadata fields, and the
  `nros::main!()` four forms.

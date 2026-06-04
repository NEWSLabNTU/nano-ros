# Bringup: launch + system.toml

A **Bringup pkg** is the declarative glue that ties your Node packages together
into a runnable topology. It owns the launch file, the wiring between nodes, and
the per-target deploy config — all without any compiled code of its own.

> **Pre-requisite:** You've scaffolded your Node packages following the
> [Prepare node packages](./workspace-node-pkgs.md) guide. This page adds the
> `demo_bringup` layer between them and the Entry package that boots everything.

---

## What a Bringup pkg is

A Bringup pkg is **pure declarative** — no `Cargo.toml`, no `CMakeLists.txt`,
no `src/`. Its job is to describe *which* nodes run, how they're wired, and
where they deploy. Naming convention: `<system>_bringup` (aliased `<system>_launch`),
matching nav2 / Autoware / turtlebot3.

It is **optional**: required only when two or more Entry packages share one
topology. A single-Entry workspace can fold `launch/` + `system.toml` directly
into the Entry pkg.

---

## Anatomy

```
src/demo_bringup/
├── package.xml          # ROS 2 manifest; <exec_depend> per node pkg
├── system.toml          # [system] + [[component]] + [deploy.<target>]
├── launch/
│   └── system.launch.xml   # ROS 2 launch schema, verbatim
└── config/                 # optional — params.yaml, per-target overrides
```

---

## `system.toml` — node wiring + deploy targets

The `system.toml` is the machine-readable topology. It lists every node the
system runs, its class path, and one or more deploy targets (which board, which
RMW, which domain).

Below is a minimal two-node example adapted from the real fixture at
`packages/testing/nros-tests/fixtures/orchestration_e2e/demo_pkg_bringup/system.toml`:

```toml
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::Talker"
name = "talker"

[[component]]
pkg = "listener_pkg"
class = "listener_pkg::Listener"
name = "listener"

[deploy.native]
kind = "self"
target = "x86_64-unknown-linux-gnu"
```

Key fields:

| Field | Meaning |
|---|---|
| `[system] name` | Logical system name; used by `nros plan`/`check`/`launch` |
| `[system] rmw` | Default RMW for all components (`zenoh`, `xrce`, `cyclonedds`) |
| `[system] domain_id` | ROS 2 domain (compile-time on embedded, runtime env on host) |
| `[[component]] pkg` | The ROS package name (matches `<name>` in `package.xml`) |
| `[[component]] class` | Fully-qualified Rust type (`crate::TypeName`) |
| `[[component]] name` | Node name at runtime |
| `[deploy.<target>]` | Deploy target block; read by `nros deploy`/`nros launch` |
| `[deploy.<t>] kind` | `"self"` = host native binary; `"flash"` = embedded target |
| `[deploy.<t>] target` | Rust target triple |

For multi-domain setups or cross-domain bridges add `[[domain]]` and
`[[bridge]]` sections — see `docs/design/multi-node-workspace-layout.md` §11
for the full schema.

---

## `launch/system.launch.xml` — ROS 2 launch schema

The launch file uses the **ROS 2 launch XML schema verbatim** — nano-ros reads
it with the same parser so existing nav2/Autoware/turtlebot3 XML pastes in and
Just Works.

```xml
<launch>
  <node pkg="talker_pkg" exec="talker" name="talker"/>
  <node pkg="listener_pkg" exec="listener" name="listener"/>
</launch>
```

### v1 tag set

| Tag | Purpose |
|---|---|
| `<launch>` | Root element |
| `<arg name="…" default="…"/>` | Declare a launch argument |
| `<node pkg="…" exec="…" name="…"/>` | Instantiate a node |
| `<param name="…" value="…"/>` | Set a parameter (nested inside `<node>`) |
| `<remap from="…" to="…"/>` | Topic/service remapping (nested inside `<node>`) |
| `<group ns="…">` | Namespace a group of nodes |
| `<include file="…"/>` | Nest another launch file |

### Substitutions

- `$(find <pkg>)` — resolves to the package's install/source path
- `$(var <arg>)` — expands a launch argument
- `$(env <name>)` — reads an environment variable

A richer example using args and remapping (taken from the real fixture):

```xml
<launch>
  <arg name="talker_name" default="talker" />

  <node pkg="talker_pkg" exec="talker" name="$(var talker_name)" output="screen">
    <param name="rate_hz" value="25" />
    <remap from="chatter" to="/chatter" />
  </node>

  <node pkg="listener_pkg" exec="listener" name="listener"/>
</launch>
```

> **Note:** Python `.launch.py` files are not yet supported in v1 — use the XML
> schema above.

---

## `package.xml`

A standard ROS 2 manifest. List each Node package as an `<exec_depend>`:

```xml
<?xml version="1.0"?>
<package format="3">
  <name>demo_bringup</name>
  <version>0.1.0</version>
  <description>Bringup package for the demo system</description>
  <maintainer email="you@example.com">Your Name</maintainer>
  <license>Apache-2.0</license>

  <exec_depend>talker_pkg</exec_depend>
  <exec_depend>listener_pkg</exec_depend>

  <export>
    <build_type>ament_cmake</build_type>
  </export>
</package>
```

No `<build_depend>` entries — there is nothing to compile.

---

## Workflow: plan → check → launch

Once your Bringup pkg is written, use the `nros` CLI to validate and run the
topology:

```bash
# 1. Resolve the topology — emits plan.json
nros plan demo_bringup

# 2. Static-check the resolved plan (type wiring, QoS mismatches, missing deps)
nros check

# 3. Spawn all nodes on the host (native / native_sim)
nros launch demo_bringup

# — or pass an explicit launch file —
nros launch demo_bringup --launch system.launch.xml
```

> **Caveat — host `nros launch` e2e path:** The `nros plan` + `nros check`
> commands validate your topology regardless of platform. The host-side
> `nros launch` spawner (native/`native_sim` equivalent of `ros2 launch`) may
> not be fully wired in your `nros` build. If `nros launch` does not yet start
> the nodes, run the Entry package binary directly (see the next page) — the
> topology has already been validated by `plan` + `check`.

---

## Runnable copy-out

`examples/templates/multi-node-workspace/src/demo_bringup/` is the canonical
3-role template that pairs with this guide. It is being built concurrently with
this page — once landed, `cargo build` from the workspace root builds all Node
pkgs and the Entry pkg; `nros plan demo_bringup && nros check` validates the
topology.

---

## When you don't need a Bringup pkg

If you have a single Entry pkg and don't plan to share the topology across
multiple boards, fold `launch/` and `system.toml` directly into the Entry pkg.
The `nros::main!` macro accepts a `launch =` argument that names the bringup
package:

```rust
// Multi-node: reads launch/system.launch.xml from demo_bringup
nros::main!(launch = "demo_bringup");

// Explicit file within a bringup pkg
nros::main!(launch = "demo_bringup:sim.launch.xml");
```

If the launch files live inside the Entry pkg itself, point at it by name. The
Entry package page covers this in full.

---

## Where to go next

- [Entry package: boot on a board](./workspace-entry-pkg.md) — the `nros::main!` macro and `robot_entry`
- [Node, Bringup & Entry Packages](../user-guide/component-and-entry-pkg.md) — full reference for all three roles
- [From an app node to a workspace](./workspace-from-app-node.md) — start here if you haven't read it yet

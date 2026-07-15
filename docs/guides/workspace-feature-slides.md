# 1. From One App to a Workspace

nano-ros workspace feature: the three-role model for growing from one
app node to a reusable system.

The app-node shape is still valid:

```text
one package = node logic + main() + board/runtime setup
```

Split into a workspace when you need:

- multiple nodes in one topology
- shared launch naming, remaps, and parameters
- one node reused across native and embedded targets
- mixed implementation languages, such as C nodes hosted by C++ or Rust

Workspace model:

```text
Node pkg(s)  +  Bringup pkg  +  Entry pkg(s)
```

---

## 2. Node Pkg: Reusable Node Logic

A Node pkg is a library. It declares behavior, but it does not boot.

```text
src/talker_pkg/
├── package.xml
├── Cargo.toml or CMakeLists.txt
└── src/lib.rs / src/Talker.c / src/Talker.cpp
```

Rust shape:

```rust
impl Node for Talker {
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        /* declare publishers, subscriptions, timers */
        Ok(())
    }
}

nros::node!(Talker);
```

C/C++ shape:

```cmake
nano_ros_node_register(
    NAME talker
    CLASS talker_pkg::Talker
    LANGUAGE C|CPP
    SOURCES src/Talker.c
    DEPLOY native)
```

Key idea: the Entry pkg links Node pkgs through exported register
trampolines.

---

## 3. Bringup Pkg: Declarative Topology

A Bringup pkg has no compiled code. It owns the launch graph.

```text
src/demo_bringup/
├── package.xml
├── system.toml
└── launch/
    └── system.launch.xml
```

Launch XML uses ROS 2 launch syntax:

```xml
<launch>
  <node pkg="talker_pkg" exec="talker" name="talker"/>
  <node pkg="listener_pkg" exec="listener" name="listener"/>
</launch>
```

`package.xml` lists Node pkgs as runtime dependencies:

```xml
<exec_depend>talker_pkg</exec_depend>
<exec_depend>listener_pkg</exec_depend>
```

Key idea: Bringup describes which nodes run and how they are named,
remapped, parameterized, and grouped.

---

## 4. Entry Pkg: Board-Specific Boot

An Entry pkg is the binary. It chooses the board and boots a Bringup
topology.

```text
src/robot_entry/
├── package.xml
├── Cargo.toml or CMakeLists.txt
└── src/main.rs / src/main.c / src/main.cpp
```

Rust:

```rust
nros::main!(launch = "demo_bringup:system.launch.xml");
```

C/C++:

```cmake
nano_ros_entry(
    NAME robot_entry
    SOURCES src/main.c
    BOARD native
    LAUNCH "demo_bringup:system.launch.xml"
    LANG c
    DEPLOY native)
```

Key idea: one Entry pkg per deploy target; Node pkgs stay reusable and
board-agnostic.

---

## 5. Build Process: Launch Scan to Codegen

Workspace build turns declarative packages into a linked Entry binary.

```text
src/demo_bringup/launch/system.launch.xml
        │
        ▼
play_launch_parser
        │
        ▼
record.json
        │
        ▼
nros plan + package/source metadata scan
        │
        ├── finds Node pkgs from package.xml + Cargo.toml/CMakeLists.txt
        ├── matches <node pkg="..." exec="..."> to Node-pkg metadata
        └── produces ordered component list
        │
        ▼
nros codegen entry
        │
        ├── emits generated main TU
        └── emits link sidecar for Node-pkg static libs
        │
        ▼
cargo build / cmake --build
```

Key idea: launch XML decides *which* nodes enter the binary; Entry
codegen decides the registration order and link set.

---

## 6. Entry Code Examples

Rust Entry pkg: the macro scans workspace metadata and launch inputs at
build time.

```rust
// src/robot_entry/src/main.rs
nros::main!(launch = "demo_bringup:system.launch.xml");
```

C++ Entry pkg: CMake shells out to `nros codegen entry --lang cpp`.

```cpp
// src/robot_entry/src/main.cpp
#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard,
          "demo_bringup:system.launch.xml");
```

```cmake
nano_ros_entry(
    NAME    robot_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    DEPLOY  native)
```

Key idea: the handwritten Entry source is intentionally tiny; generated
code carries the launch-specific register calls.

---

## 7. Supported Workspace Shapes

Canonical templates:

```text
examples/templates/multi-node-workspace/        # Rust Node + Rust Entry
examples/templates/multi-node-workspace-cpp/    # C++ Node + C++ Entry
examples/templates/c-and-cpp-mixed-workspace/   # C Node + C++ Node + C++ Entry
examples/templates/pure-c-workspace/            # C Node + C Entry
```

Build examples:

```sh
# Rust workspace
cargo build

# C/C++ workspace
cmake -S . -B build
cmake --build build
```

Validation path:

```sh
nros check --workspace .
nros plan --workspace . --out-dir build/plan demo_bringup src/demo_bringup/launch/system.launch.xml
```

Takeaway: Node pkgs own behavior, Bringup owns topology, and Entry pkgs
turn a topology into a deployable binary.

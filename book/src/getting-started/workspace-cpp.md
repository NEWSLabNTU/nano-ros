# C / C++ multi-node workspaces

The four previous chapters
([project layout](./workspace-from-app-node.md),
[node packages](./workspace-node-pkgs.md), [bringup packages](./workspace-bringup.md),
[entry packages](./workspace-entry-pkg.md)) describe the canonical three-role
node + bringup + entry shape against the Rust path
(`nros::node!(…)` + `nros::main!(launch = …)`). This chapter shows the
**C and C++ path** through the same shape, role-for-role.

Phase 219 closed the parity gap. Same launch.xml, same `package.xml`,
same `system.toml`, same workspace pkg-index — the only thing that
changes language-side is the cmake-fn / macro surface.

## TL;DR — side-by-side

| Role | Rust | C / C++ |
|---|---|---|
| **Node pkg** | `lib.rs` with `nros::node!(MyNode)` + `[package.metadata.nros.node]` in `Cargo.toml` | `Talker.{hpp,cpp}` with a `configure(::nros::Node&)` component method (C++) / `NROS_C_COMPONENT` (C); `CMakeLists.txt` calling `nano_ros_node_register(NAME … CLASS … SOURCES …)` |
| **Bringup pkg** | `package.xml` + `system.toml` + `launch/*.launch.xml` (no `Cargo.toml`) | identical (language-agnostic) |
| **Entry pkg** | `src/main.rs` with `nros::main!(launch = "demo_bringup:system.launch.xml")` | `src/main.cpp` with `NROS_MAIN(nros::board::NativeBoard, "demo_bringup:system.launch.xml")`; `CMakeLists.txt` calling `nano_ros_entry(NAME … LAUNCH "demo_bringup:system.launch.xml" DEPLOY native)` |
| **Workspace root** | `Cargo.toml [workspace] members = […]` | `CMakeLists.txt` calling `nano_ros_workspace(BACKEND zenoh PLATFORM posix SUBDIRS src/talker_pkg src/listener_pkg src/native_entry)` |
| **Build** | `nros ws sync` + `cargo build -p native_entry` | `nros ws sync` + `cmake -S . -B build` + `cmake --build build` |
| **Boot** | `cargo run -p native_entry` | `./build/.../native_entry` |

The reference C++ workspace ships in-tree at
[`examples/workspaces/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/cpp).
Copy the whole directory, rename the packages.

## Workspace layout

Identical structure to the Rust template, swapping `Cargo.toml` →
`CMakeLists.txt`:

```text
my_ws/
├── CMakeLists.txt                # nano_ros_workspace(SUBDIRS …)
└── src/
    ├── talker_pkg/               # Node pkg (C++)
    │   ├── package.xml
    │   ├── CMakeLists.txt        # nano_ros_node_register(…)
    │   └── src/{Talker.hpp,Talker.cpp}
    ├── listener_pkg/             # Node pkg (C++)
    │   ├── package.xml
    │   ├── CMakeLists.txt
    │   └── src/{Listener.hpp,Listener.cpp}
    ├── demo_bringup/             # Bringup pkg (language-agnostic — copy/paste
    │   ├── package.xml           #          works between Rust and C++ workspaces)
    │   ├── system.toml
    │   └── launch/system.launch.xml
    └── native_entry/             # Entry pkg (C++)
        ├── package.xml
        ├── CMakeLists.txt        # nano_ros_entry(LAUNCH …)
        └── src/main.cpp          # NROS_MAIN(…) one-liner
```

## Workspace root

Four declarations:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_ws LANGUAGES C CXX)
include(<nano-ros>/cmake/NanoRosWorkspace.cmake)

nano_ros_workspace(
    BACKEND        zenoh                # zenoh | xrce | cyclonedds
    PLATFORM       posix                # posix | … (default posix)
    NANO_ROS_ROOT  <path-to-nano-ros>   # also: -D cache var, $NANO_ROS_ROOT,
                                        # or auto-walk for nros-sdk-index.toml
    SUBDIRS        src/talker_pkg
                   src/listener_pkg
                   src/native_entry
)
```

`nano_ros_workspace()` (Phase 219.I) does the heavy lifting in one call:

1. Sets `NANO_ROS_PLATFORM=posix` + `NANO_ROS_RMW=zenoh`.
2. `add_subdirectory(<nano-ros>)` **once** at root scope (so per-pkg
   subdirs don't collide on re-include).
3. `include(NanoRosNodeRegister.cmake)` + `include(NanoRosEntry.cmake)`
   once.
4. `add_subdirectory(<each member>)` for each `SUBDIRS` entry.

Subdir CMakeLists begin with the dual call:

```cmake
nano_ros_workspace_pkg_guard()  # no-op inside a workspace; bootstraps standalone solo
```

— the cmake equivalent of cargo `[workspace]` discipline. Every member
compiles standalone (with `-DNANO_ROS_ROOT=<path>`) or as part of the
workspace; the per-pkg CMakeLists doesn't change between modes.

## Node pkg

A **typed component** (RFC-0043) — no `main()`. The pkg ships a class with a
`configure(::nros::Node&)` method that creates real entities (a `Publisher`, a
`Timer`) and binds member callbacks **by identity** (member-fn-pointer template
param, no string callback name, no interpreter). The Entry pkg constructs the
object and calls `configure(node)`; the executor dispatches the callbacks.

```cmake
# src/talker_pkg/CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(talker_pkg LANGUAGES C CXX)
nano_ros_workspace_pkg_guard()
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_node_register(
    NAME    talker
    CLASS   talker_pkg::Talker     # §212.L.4 — class prefix must equal PROJECT_NAME
    SOURCES src/Talker.cpp)

target_link_libraries(talker_pkg_talker_component PUBLIC std_msgs__nano_ros_cpp)
```

```cpp
// src/talker_pkg/include/talker_pkg/Talker.hpp
#pragma once
#include <nros/component.hpp>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

namespace talker_pkg {

class Talker {
    ::nros::Publisher<std_msgs::msg::Int32> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick();  // real body; bound via &Talker::on_tick (no name)

  public:
    ::nros::Result configure(::nros::Node& node);
};

}  // namespace talker_pkg
```

```cpp
// src/talker_pkg/src/Talker.cpp
#include "talker_pkg/Talker.hpp"

namespace talker_pkg {

void Talker::on_tick() {
    std_msgs::msg::Int32 m;
    m.data = count_++;
    (void)pub_.publish(m);
}

::nros::Result Talker::configure(::nros::Node& node) {
    ::nros::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    // Member-fn-pointer-as-template-param → no-alloc trampoline; `this` is ctx.
    return ::nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 1000, this);
}

}  // namespace talker_pkg
```

The Entry pkg constructs `Talker` in static storage and calls `configure(node)`
on the real executor — the same component model the Rust `nros::node!(Talker)` +
the C `NROS_C_COMPONENT` paths use, so C++, C, and Rust Node pkgs interoperate in
one launch graph.

Scaffold a C++ Node pkg with:

```bash
$ nros new --component my-talker --lang cpp --use-case talker
✓ Created nano-ros C++ Node pkg 'my-talker'
  Class     : my_talker::Talker
  Node      : talker
  Kind      : typed component (RFC-0043)
```

## Bringup pkg

Language-agnostic — copy verbatim from the
[bringup chapter](./workspace-bringup.md). `package.xml` +
`system.toml` + `launch/system.launch.xml`. No `Cargo.toml`, no
`CMakeLists.txt`. Stock ROS 2 launch.xml from nav2 / Autoware /
turtlebot3 pastes in modulo unsupported tags.

```xml
<!-- src/demo_bringup/launch/system.launch.xml -->
<launch>
  <node pkg="talker_pkg" exec="talker" name="talker"/>
  <node pkg="listener_pkg" exec="listener" name="listener"/>
</launch>
```

## Entry pkg

The C++ Entry pkg's `CMakeLists.txt` calls
`nano_ros_entry(LAUNCH …)` — Phase 219.D added the `LAUNCH` keyword:

```cmake
# src/native_entry/CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(native_entry LANGUAGES C CXX)
nano_ros_workspace_pkg_guard()

nano_ros_entry(
    NAME    native_entry
    SOURCES src/main.cpp                              # user-authored
    LAUNCH  "demo_bringup:system.launch.xml"          # Phase 219.D
    DEPLOY  native)
```

At configure time the cmake fn shells `nros codegen entry --lang cpp --typed`,
emits `${CMAKE_BINARY_DIR}/native_entry_nros_main_generated.cpp` (the canonical
`int main()` body that constructs each launch node's component + calls
`configure(node)` on the real executor via `NativeBoard::run_components`),
appends it to the target's sources, and auto-links every
`<pkg>_<exec>_component` static lib the launch XML named (Phase 219.J). The
user's `main.cpp` is a single declarative line:

```cpp
// src/native_entry/src/main.cpp
#include <nros/main.hpp>
NROS_MAIN(nros::board::NativeBoard, "demo_bringup:system.launch.xml")
```

`NROS_MAIN(...)` is a sentinel macro — the cmake fn owns the generated
body, the user's TU is documentation + IDE hint.

`nros new` scaffolds an Entry pkg:

```bash
$ nros new my-entry --lang cpp --platform native
```

## Build + boot

```bash
nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build -DNANO_ROS_ROOT=<path-to-nano-ros>
cmake --build build
./build/src/native_entry/native_entry
```

The build produces:

- `src/talker_pkg/libtalker_pkg_talker_component.a` — the Node pkg static lib
  (`_component` is the compatibility target suffix).
- `src/listener_pkg/liblistener_pkg_listener_component.a` — ditto.
- `src/native_entry/native_entry` — the Entry exe, with the generated
  `int main()` + register-call sequence + Board boot stub linked in.

`cmake configure` is incremental — pinned `CMAKE_CONFIGURE_DEPENDS` on
every file `nros codegen entry` reads (depfile from the CLI), so any
launch.xml / `package.xml` / Node pkg edit re-runs codegen.

## What runs where

| Concern | Lives in |
|---|---|
| Node entities + real callbacks | Node pkg `configure(::nros::Node&)` |
| Topology + launch args + per-target deploy | Bringup pkg `system.toml` + `launch/*.launch.xml` |
| `int main()` + executor init + spin | Generated TU emitted by the Entry pkg's cmake fn |
| Board + RMW selection | Entry pkg's `nano_ros_entry(BOARD …)` arg |

Same partition as the Rust track — the only thing that changes is the
syntax the user types into the three pkgs.

## C / C++ scaffolding via `nros new`

| Command | Output |
|---|---|
| `nros new <name> --lang cpp --platform native` | C++ Entry pkg (single-Node self-bringup; swap in a multi-Node `LAUNCH` arg post-219.D) |
| `nros new <name> --lang c --platform native` | C Entry pkg (same shape) |
| `nros new --component <name> --lang cpp --use-case talker` | C++ Node pkg; `--component` is the compatibility scaffold flag |
| `nros new system <name>_bringup --components a,b` | Bringup pkg (language-agnostic — works for both Rust and C++ workspaces) |

The C-side compatibility scaffold (`nros new --component … --lang c`) is
available for pure-C Node pkgs. Pure-C and mixed C/C++ workspace examples
live under `examples/workspaces/`.

## See also

- [`examples/workspaces/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/cpp)
  — the canonical reference workspace (talker + listener Node pkgs +
  Bringup pkg + Entry pkg, all C++).
- [Phase 219 roadmap](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-219-cpp-entry-pkg.md)
  — full landing order + acceptance bar.
- [Multi-node workspace layout design](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0024-multi-node-workspace-layout.md)
  §11 — LOCKED canonical shape (`Bringup + Node + Entry`).

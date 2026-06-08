---
rfc: 0025
title: "nano-ros Phase 212 — Multi-Node Workspace Layout Reference"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# nano-ros Phase 212 — Multi-Node Workspace Layout Reference

Canonical layout reference for nano-ros user workspaces. Matches design decisions in `docs/design/0024-multi-node-workspace-layout.md`. Five cases: single rust, single cpp, multi rust, multi cpp, mixed.

## Quick matrix

| case       | top-level build tool             | user-facing verb                        | orchestration pkg?     |
|------------|----------------------------------|-----------------------------------------|------------------------|
| single rust| cargo                            | `cargo build` / `cargo run`             | no                     |
| single cpp | cmake                            | `cmake --build`                         | no                     |
| multi rust | cargo (workspace)                | `cargo build` + `nros plan/deploy`| yes (`demo_bringup`)   |
| multi cpp  | cmake (superbuild)               | `cmake --build` + `nros plan/deploy`    | yes (`demo_bringup`)   |
| mixed      | cmake (corrosion bridges cargo)  | `cmake --build` + `nros plan/deploy`    | yes (`demo_bringup`)   |

Caveman rule: pure-Rust speak cargo. Pure-C++ speak cmake. Mix → cmake boss, cargo slave via Corrosion. Many node → add bringup pkg. One node → no bringup.

---

## Case 1 — Single Rust node

One bin crate. No workspace, no bringup. `nros-build` build-dep auto-runs codegen.

```
my_talker/
├── Cargo.toml
├── package.xml
├── .gitignore
└── src/main.rs
```

Key files (diff vs sibling cases — main.rs/package.xml boilerplate omitted):

```toml
# Cargo.toml — single-bin shape, RMW via cargo feature
[package]
name = "my_talker"
edition = "2024"

[dependencies]
nros-node     = { path = "../../packages/core/nros-node", default-features = false }
nros-std-msgs = { path = "../../packages/interfaces/rcl-interfaces/generated/humble/nros-std-msgs" }

[build-dependencies]
nros-build = { path = "../../packages/nros-build" }

[features]
default = ["rmw-zenoh"]
rmw-zenoh      = ["nros-node/rmw-zenoh"]
rmw-xrce       = ["nros-node/rmw-xrce"]
rmw-cyclonedds = ["nros-node/rmw-cyclonedds"]

[package.metadata.nros.component]
default_namespace = "/"

[package.metadata.ament]
build_depend = ["rosidl_default_generators"]
exec_depend  = ["rosidl_default_runtime", "std_msgs"]
```

Commands:

```bash
# 1. cargo — scaffold
cargo new my_talker --bin
# Created binary (application) `my_talker` package

# 2. editor — paste Cargo.toml + package.xml (no stdout)

# 3. cargo — build (nros-build invokes `nros` CLI for codegen)
cargo build
# Compiling my_talker v0.1.0
# Finished `dev` profile [unoptimized + debuginfo]

# 4. cargo — RMW flip (edit default = ["rmw-cyclonedds"])
cargo build --no-default-features --features rmw-cyclonedds
# Compiling cyclonedds-sys v…  Finished `dev` profile

# 5. cargo — run
cargo run
# Published: 0
# Published: 1
```

---

## Case 2 — Single C++ node

One executable. CMake top-level. No bringup. Pulls nano-ros via `add_subdirectory($NANO_ROS_DIR)`.

```
my_talker/
├── CMakeLists.txt
├── package.xml
├── .gitignore
└── src/main.cpp
```

Key files (diff vs multi-cpp: no `nano_ros_workspace_metadata`, no nested `add_subdirectory(talker_pkg)`):

```cmake
# CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(my_talker CXX)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

set(NANO_ROS_PLATFORM posix CACHE STRING "")
set(NANO_ROS_RMW      zenoh CACHE STRING "")
add_subdirectory($ENV{NANO_ROS_DIR} nano_ros)

add_executable(my_talker src/main.cpp)
nros_find_interfaces(LANGUAGE CPP)  # reads package.xml <depend> rows
target_link_libraries(my_talker PRIVATE NanoRos::NanoRos my_talker_msgs)
nros_platform_link_app(my_talker)
```

```xml
<!-- package.xml: ament_cmake build_type (vs ament_cargo for Rust) -->
<export><build_type>ament_cmake</build_type></export>
```

Commands:

```bash
# 1. editor — write CMakeLists.txt, src/main.cpp, package.xml (no stdout)

# 2. shell — point at nano-ros
export NANO_ROS_DIR=$HOME/repos/nano-ros

# 3. cmake — configure (per-RMW build dir; no source edits to swap)
cmake -S . -B build -DNANO_ROS_PLATFORM=posix -DNANO_ROS_RMW=zenoh
# -- Configuring nano-ros (platform=posix, rmw=zenoh)
# -- Generating interfaces: std_msgs (CPP)
# -- Configuring done

# 4. cmake — build
cmake --build build -j
# [100%] Linking CXX executable my_talker

# 5. shell — run (zenohd router started separately on :7447)
./build/my_talker
# Published: 0
# Published: 1
```

---

## Case 3 — Multi-Node Rust

Cargo workspace + `demo_bringup` Path A pkg (no `Cargo.toml`). Components are `staticlib`+`rlib` library crates with `#[nros::component]` entry.

```
robot_ws/
├── Cargo.toml                  # workspace; excludes demo_bringup
├── .gitignore
├── talker_pkg/   {Cargo.toml, package.xml, src/lib.rs}
├── listener_pkg/ {Cargo.toml, package.xml, src/lib.rs}
└── demo_bringup/               # Path A — NO Cargo.toml
    ├── package.xml
    ├── system.toml
    └── launch/system.launch.xml
```

Key files (diff vs single-rust: lib crate not bin; workspace root; bringup pkg):

```toml
# Cargo.toml (workspace root) — bringup is excluded; default_system pointer
[workspace]
resolver = "2"
members  = ["talker_pkg", "listener_pkg"]
exclude  = ["demo_bringup"]

[workspace.metadata.nros]
default_system = "demo_bringup"
```

```toml
# talker_pkg/Cargo.toml — staticlib+rlib (vs bin in single-rust)
[lib]
crate-type = ["staticlib", "rlib"]

[package.metadata.nros.component]
class     = "talker_pkg::TalkerNode"
node_name = "talker"

[package.metadata.ament]
build_type  = "ament_cargo"
exec_depend = ["std_msgs"]
```

```toml
# demo_bringup/system.toml — declarative system spec
[system]
name = "demo"
rmw  = "zenoh"
domain_id = 0

[[component]]
pkg = "talker_pkg"   ; class = "talker_pkg::TalkerNode"   ; name = "talker"
[[component]]
pkg = "listener_pkg" ; class = "listener_pkg::ListenerNode"; name = "listener"

[deploy.native]
launch = "launch/system.launch.xml"
```

```xml
<!-- demo_bringup/package.xml — exec_depend only, no build deps -->
<export><build_type>ament_nros</build_type></export>
<exec_depend>talker_pkg</exec_depend>
<exec_depend>listener_pkg</exec_depend>
```

Commands:

```bash
# 1. nros — scaffold workspace + bringup
nros new system robot_bringup --components talker_pkg,listener_pkg
# created robot_ws/{Cargo.toml,talker_pkg,listener_pkg,demo_bringup}

# 2. cargo — build component crates (bringup excluded)
cargo build
# Compiling talker_pkg v0.1.0
# Compiling listener_pkg v0.1.0
# Finished `dev` profile

# 3. nros — sanity check the system wiring
nros check
# OK 2 components, 0 unresolved

# 4. nros — emit deploy plan from workspace.metadata.nros pointer
nros plan
# wrote build/demo_bringup/plan.json

# 5. nros — launch native
nros deploy native
# [listener] hi 0
# [listener] hi 1
```

---

## Case 4 — Multi-Node C++

CMake superbuild + `demo_bringup` Path A pkg. Components are executables. Root `CMakeLists.txt` calls `nano_ros_workspace_metadata(SYSTEM …)` so `nros` finds binaries via `build/.nros/workspace.json`.

```
robot_ws/
├── CMakeLists.txt              # superbuild driver
├── .gitignore
├── talker_pkg/   {CMakeLists.txt, package.xml, src/talker.cpp}
├── listener_pkg/ {CMakeLists.txt, package.xml, src/listener.cpp}
└── demo_bringup/
    ├── package.xml
    ├── system.toml
    └── launch/system.launch.xml
```

Key files (diff vs single-cpp: workspace metadata call + nested `add_subdirectory`s; diff vs multi-rust: cmake driver, no Cargo):

```cmake
# CMakeLists.txt (root) — workspace metadata is the bridge to `nros`
cmake_minimum_required(VERSION 3.22)
project(robot_ws LANGUAGES CXX)
find_package(NanoRos REQUIRED)
nano_ros_workspace_metadata(SYSTEM demo_bringup)
add_subdirectory(talker_pkg)
add_subdirectory(listener_pkg)
```

```cmake
# talker_pkg/CMakeLists.txt — no project(), no add_subdirectory(nano_ros)
add_executable(talker src/talker.cpp)
nros_find_interfaces(LANGUAGE CPP)  # reads package.xml <depend> rows
target_link_libraries(talker PRIVATE NanoRos::NanoRos)
nros_platform_link_app(talker)
```

```toml
# demo_bringup/system.toml — [[domain]]/[[node]] shape (vs Rust [[component]])
[[domain]]
name = "default" ; rmw = "cyclonedds" ; id = 0

[[node]]
package = "talker_pkg"   ; executable = "talker"   ; domain = "default"
[[node]]
package = "listener_pkg" ; executable = "listener" ; domain = "default"
```

Commands:

```bash
# 1. nros — scaffold C++ system
nros new system robot_ws --lang cpp
# created robot_ws/{CMakeLists.txt,talker_pkg,listener_pkg,demo_bringup}

# 2. cmake — configure superbuild
cmake -S . -B build -DNANO_ROS_PLATFORM=posix -DNANO_ROS_RMW=cyclonedds \
      -DNanoRos_DIR=$HOME/repos/nano-ros
# -- nano_ros: workspace system = demo_bringup
# -- nano_ros: writing build/.nros/workspace.json

# 3. cmake — build all components
cmake --build build -j
# [ 40%] Built target talker
# [ 80%] Built target listener
# [100%] Built nros_workspace_manifest

# 4. nros — emit plan (reads build/.nros/workspace.json — NOT `cmake nros`)
nros plan demo_bringup
# resolved 2 nodes across 1 domain (default/cyclonedds/0)

# 5. nros — launch native
nros deploy native
# [listener] got: hello 0
# [listener] got: hello 1
```

Asymmetry: there is **no** `cmake nros …` subcommand. `cargo` dispatches to `cargo-<x>` binaries on PATH; cmake has no equivalent. Phase 212 accepts this — C++ users always call `nros plan` / `nros deploy` directly, bridged via `nano_ros_workspace_metadata(…)`.

---

## Case 5 — Mixed Rust + C++

CMake boss. Corrosion imports Rust component as staticlib. Root `Cargo.toml` is **IDE-only** (rust-analyzer discovery) — `cargo build` at root fails by design.

```
demo_ws/
├── CMakeLists.txt              # top-level — DO build via cmake
├── Cargo.toml                  # IDE-only — DO NOT cargo build here
├── .gitignore
├── talker_pkg/                 # Rust component (staticlib+rlib)
│   {Cargo.toml, build.rs, package.xml, src/lib.rs}
├── listener_pkg/               # C++ component (SHARED lib)
│   {CMakeLists.txt, package.xml, src/listener.cpp}
└── demo_bringup/
    ├── package.xml
    ├── system.toml
    └── launch/system.launch.xml
```

Key files (diff vs multi-rust: cmake at root, Corrosion bridge; diff vs multi-cpp: Rust component imported via Corrosion):

```cmake
# CMakeLists.txt (root)
cmake_minimum_required(VERSION 3.22)
project(demo_ws LANGUAGES C CXX)
set(NANO_ROS_PLATFORM posix      CACHE STRING "")
set(NANO_ROS_RMW      cyclonedds CACHE STRING "")
add_subdirectory($ENV{NANO_ROS_DIR} nano_ros)

include(FetchContent)
FetchContent_Declare(Corrosion GIT_REPOSITORY https://github.com/corrosion-rs/corrosion GIT_TAG v0.5)
FetchContent_MakeAvailable(Corrosion)

corrosion_import_crate(MANIFEST_PATH talker_pkg/Cargo.toml CRATES talker_pkg)
corrosion_link_libraries(talker_pkg NanoRos::NanoRos)

add_subdirectory(listener_pkg)
```

```toml
# Cargo.toml (root) — virtual workspace, IDE only
# Run `cmake --build build`, NOT `cargo build`.
[workspace]
members  = ["talker_pkg"]
resolver = "2"
```

```rust
// talker_pkg/src/lib.rs — #[nros::component] expands to no_mangle extern "C" trampoline
#[nros::component(name = "talker")]
pub fn run(node: &Node) -> Result<()> { /* ... */ }
```

Commands:

```bash
# 1. shell — point at nano-ros (mandatory; .envrc template ships with bringup)
export NANO_ROS_DIR=$HOME/repos/nano-ros

# 2. cmake — configure (Corrosion finds cargo, imports talker_pkg)
cmake -S . -B build -DNANO_ROS_PLATFORM=posix -DNANO_ROS_RMW=cyclonedds
# -- Corrosion: Found cargo 1.85
# -- Importing crate talker_pkg (staticlib)

# 3. cmake — build (cargo errors surface verbatim with [ NN%] prefix)
cmake --build build -j
# [ 12%] Building Rust crate talker_pkg
# [ 60%] Building CXX object listener_pkg/.../listener.cpp.o
# [100%] Linking CXX shared library liblistener_pkg.so

# 4. nros — plan (one shared `nros generate` → matching CDR + type hash both sides)
nros plan demo_bringup
# plan: 2 nodes, 1 topic (chatter: std_msgs/Int32)

# 5. nros — launch native
nros deploy native
# [listener] heard: 0
# [listener] heard: 1
```

Cross-language pub/sub works because both components link `NanoRos::NanoRos` with the same `NANO_ROS_RMW` and consume one shared `nros generate` output — wire-identical descriptors on the bus.

---

## Decision rules

- **Pure Rust → cargo top-level.** Workspace root `Cargo.toml`, components as `staticlib+rlib`. Build with `cargo build`; orchestrate with `nros plan/deploy`.
- **Pure C++ → cmake top-level.** Superbuild root `CMakeLists.txt` with `nano_ros_workspace_metadata(SYSTEM …)`; components are executables. Build with `cmake --build`; orchestrate with `nros plan/deploy` (no `cmake nros`).
- **Mixed Rust + C++ → cmake top-level via Corrosion.** Root `Cargo.toml` ships **only** for rust-analyzer; loud-fail if someone runs `cargo build` at root.
- **Multi-node → add `<system>_bringup` pkg.** Carries `package.xml` + `system.toml` + `launch/`. No source, no `Cargo.toml`, no `CMakeLists.txt`. Workspace pointer (`workspace.metadata.nros.default_system` in Rust, `nano_ros_workspace_metadata(SYSTEM …)` in CMake) names it.
- **Single-node → no bringup pkg.** One crate / one cmake project, that is the whole repo.
- **Component pkg → never carries launch files.** Launch lives in bringup. Component pkgs carry one `[package.metadata.nros.component]` (Rust) or one `add_executable` + `target_link_libraries(NanoRos::NanoRos)` (C++).
- **RMW pick is build-time, not source.** Cargo feature (`--features rmw-cyclonedds`) or cmake cache (`-DNANO_ROS_RMW=cyclonedds`). Per-RMW build dirs (`build-zenoh/`, `build-cdds/`) — no source edits to swap.
- **`nros generate` is single-source-of-truth for type descriptors.** One invocation per workspace; both languages read the same `generated/` tree. Skip this and Cyclone silently diverges on type hash.

## Anti-patterns

- **Don't ship `Cargo.toml` inside a bringup pkg.** Bringup is `package.xml` + `system.toml` + `launch/` only — no Rust, no source, no build deps. Adding `Cargo.toml` invites someone to `cargo build` it and pollute the workspace with a phantom crate.
- **Don't put `src/` in a bringup pkg.** Bringup is declarative. If code is needed it belongs in a component pkg referenced from `system.toml`.
- **Don't `cargo build` at the root of a mixed workspace.** The root `Cargo.toml` is a virtual-workspace stub for rust-analyzer; `nros_platform_*` symbols come from the C++/CMake side and won't resolve. README must lead with `cmake --build build`; the stub `Cargo.toml`'s top comment must say so.
- **Don't hand-write `package.xml` for a Rust pkg.** It's regenerated from `[package.metadata.ament]` by `nros emit package-xml`. Hand-editing drifts the moment someone bumps the metadata block.
- **Don't add hardcoded SDK pins or `git submodule update <path>` in `just <module> setup` recipes.** Add the pin to `nros-sdk-index.toml`; call `nros setup --source <s>` / `--tool <t>`. The recipe stays a thin caller.
- **Don't add a new Rust component to a mixed workspace by editing only `Cargo.toml`.** Each new Rust component needs **both** a `members` entry in root `Cargo.toml` **and** a `corrosion_import_crate` line in root `CMakeLists.txt`. Phase 212 follow-up: provide a `nros_add_rust_component()` cmake macro that does both.

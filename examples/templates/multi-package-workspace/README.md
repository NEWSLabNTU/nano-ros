# Multi-Package Workspace Demo

> For the canonical 3-role (Node + Bringup + Entry) workspace pattern, see [`multi-node-workspace/`](../multi-node-workspace/). This template demonstrates a polyglot (Rust/C/C++) app-node workspace.

**Phase 123.A.10** — concrete Pattern A workspace with mixed C / C++ / Rust
packages sharing one nano-ros source tree.

## Layout

```
multi-package-workspace/
├── README.md
├── build-all.sh                   # one-shot build driver
└── src/
    ├── pkg_c_talker/              # C package — publishes /chatter
    │   ├── package.xml
    │   ├── CMakeLists.txt
    │   └── src/Talker.c
    ├── pkg_cpp_listener/          # C++ package — subscribes /chatter
    │   ├── package.xml
    │   ├── CMakeLists.txt
    │   └── src/{Listener.cpp,Listener.hpp}
    └── pkg_rust_publisher/        # Rust package — alt publisher
        ├── package.xml
        ├── Cargo.toml
        ├── system.toml            # the board/RMW/domain it deploys to
        └── src/{lib.rs,main.rs}
```

There is no `.cargo/config.toml` in the Rust package and no workspace-root
`Cargo.toml` (RFC-0098 D1/D9): `nros sync` writes what the board choice implies
into `build/<image>/`, and the build reads it from there.

In a real Pattern A workspace, `src/nano-ros/` is the nano-ros checkout
(or symlink) alongside the user packages. This in-repo demo
references the parent checkout via relative paths
(`../../../../../packages/api/nros` etc.) so the example tree stays
self-contained inside the nano-ros source repo.

## What it shows

* **One nano-ros source per workspace.** Both CMake packages
  `add_subdirectory(<path-to-nano-ros>)` (Phase 140) — there is no
  install prefix to populate up front. The Rust package names nano-ros
  crates registry-style and `nros sync` resolves them against the same
  checkout — the resolution is generated, never hand-written.
* **Three audiences, one entry.** C (rclc-shaped), C++ (rclcpp-shaped),
  Rust (rclrs-shaped) packages co-exist; their build files differ by
  ~10 lines of CMake / Cargo each.
* **Workspace-shared codegen cache.** `NANO_ROS_GEN_CACHE_DIR` lets
  `std_msgs__nano_ros_c` and `std_msgs__nano_ros_cpp` build **once**
  across the C + C++ packages. Without the cache, each package would
  regenerate the bindings independently. See Phase 123.A.7.
* **`NanoRos::NanoRos` umbrella target.** `add_subdirectory(nano-ros)`
  exposes `NanoRos::NanoRos` / `NanoRos::NanoRosCpp` INTERFACE
  targets that transitively wire the RMW staticlib + platform shim
  + the per-app fixup hook `nros_platform_link_app(target)`.

## Prerequisites

Bootstrap the nano-ros checkout once:

```bash
cd <nano-ros-checkout>
./scripts/bootstrap.sh                  # builds the `nros` CLI from source
source ./activate.sh
nros setup native --rmw zenoh           # host toolchains + the zenoh router
```

No install step — Phase 140 removed `just install-local`. The
source tree IS the consumption surface.

## Build the three packages

Each package is built by its own language's driver; the board, the RMW and the
domain are never on a command line.

```bash
cd examples/templates/multi-package-workspace

# C and C++ — their own CMake. No `nros sync`: a C/C++ package's message
# bindings are a CMake-time output.
cmake -S src/pkg_c_talker -B src/pkg_c_talker/build \
      -DNANO_ROS_GEN_CACHE_DIR="$PWD/build/nros-gen-cache"
cmake --build src/pkg_c_talker/build
cmake -S src/pkg_cpp_listener -B src/pkg_cpp_listener/build \
      -DNANO_ROS_GEN_CACHE_DIR="$PWD/build/nros-gen-cache"
cmake --build src/pkg_cpp_listener/build

# Rust — its `system.toml` declares `[image.native]`.
cd src/pkg_rust_publisher && nros sync && nros build
```

`build-all.sh` is the one-shot driver for the same three builds; it also sets
`NANO_ROS_GEN_CACHE_DIR` to a shared scratch dir so the `std_msgs` C/C++
bindings are generated once across both CMake packages.

Per-package output:

* `src/pkg_c_talker/build/pkg_c_talker`
* `src/pkg_cpp_listener/build/pkg_cpp_listener`
* `src/pkg_rust_publisher/build/native/target/debug/pkg_rust_publisher`

## Run

In separate terminals:

```bash
# 1. zenoh router (background)
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd &

# 2. C talker
./src/pkg_c_talker/build/pkg_c_talker

# 3. C++ listener (in another terminal)
./src/pkg_cpp_listener/build/pkg_cpp_listener
```

Each listener should print `received: N` once per second.

For a Rust↔C interop demo, swap step 2 with:

```bash
./src/pkg_rust_publisher/build/native/target/debug/pkg_rust_publisher
```

The C++ listener picks up either publisher's stream — both round-trip
through the same `std_msgs/Int32` wire format.

## Colcon integration

The packages are declared via `package.xml` so a workspace that uses
`colcon build` discovers them. The standard
`colcon build --packages-select pkg_c_talker pkg_cpp_listener
pkg_rust_publisher` invocation works once a `colcon` environment is
sourced; the in-repo `build-all.sh` exists for users without a
ROS 2 distro on hand (or who don't want to install colcon).

## Open follow-ups

* A workspace-level `Cargo.toml` is **not** the plan and will not
  arrive: RFC-0098 D9 makes a workspace a directory of packages with
  no root build file, and the cargo settings a package needs are
  generated per image under `build/`. The old `.cargo/config.toml`
  shim this template used to carry is gone for the same reason.
* Rust codegen is still per-package — the `NANO_ROS_GEN_CACHE_DIR`
  sharing that the C and C++ packages get has no cargo-side
  equivalent yet (Phase 123.A.7 follow-up).

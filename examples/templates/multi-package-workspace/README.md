# Multi-Package Workspace Demo

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
    │   └── src/main.c
    ├── pkg_cpp_listener/          # C++ package — subscribes /chatter
    │   ├── package.xml
    │   ├── CMakeLists.txt
    │   └── src/main.cpp
    └── pkg_rust_publisher/        # Rust package — alt publisher
        ├── package.xml
        ├── Cargo.toml
        ├── .cargo/config.toml
        └── src/main.rs
```

In a real Pattern A workspace, `src/nano-ros/` is the nano-ros checkout
(or symlink) alongside the user packages. This in-repo demo
references the parent checkout via relative paths
(`../../../../../packages/core/nros` etc.) so the example tree stays
self-contained inside the nano-ros source repo.

## What it shows

* **One nano-ros source / install per workspace.** All three packages
  consume the same prebuilt install (`CMAKE_PREFIX_PATH` for C/C++,
  `[patch.crates-io]` for Rust).
* **Three audiences, one entry.** C (rclc-shaped), C++ (rclcpp-shaped),
  Rust (rclrs-shaped) packages co-exist; their build files differ by
  ~10 lines of CMake / Cargo each.
* **Workspace-shared codegen cache.** `NANO_ROS_GEN_CACHE_DIR` lets
  `std_msgs__nano_ros_c` and `std_msgs__nano_ros_cpp` build **once**
  across the C + C++ packages. Without the cache, each package would
  regenerate the bindings independently. See Phase 123.A.7.
* **Symbol-decoupled archives.** `NanoRos::NanoRos` /
  `NanoRos::NanoRosCpp` pull `libnros_platform_posix.a` +
  `libnros_rmw_<rmw>.a` transitively via the imported targets —
  user CMake declares neither manually. Per-target overrides go
  through `nano_ros_link_platform(target [PLATFORM <p>])` /
  `nano_ros_link_rmw(target [RMW <r>])`. See Phase 123.A.1.x.5.

## Prerequisites

Build + install nano-ros first:

```bash
cd <nano-ros-checkout>
./tools/setup.sh --target=posix-zenoh        # one-time
just install-local                            # builds + installs
```

This populates `<nano-ros-checkout>/build/install/`.

## Build all three packages

```bash
cd examples/multi-package-workspace
./build-all.sh
```

`build-all.sh` configures each CMake package with
`CMAKE_PREFIX_PATH` pointing at the nano-ros install + sets
`NANO_ROS_GEN_CACHE_DIR` to a shared scratch dir, then builds the
Rust package via `cargo build`.

Per-package output:

* `src/pkg_c_talker/build/pkg_c_talker`
* `src/pkg_cpp_listener/build/pkg_cpp_listener`
* `src/pkg_rust_publisher/target/release/pkg_rust_publisher`

## Run

In separate terminals:

```bash
# 1. zenoh router (background)
<nano-ros-checkout>/build/zenohd/zenohd --listen tcp/127.0.0.1:7447 &

# 2. C talker
./src/pkg_c_talker/build/pkg_c_talker

# 3. C++ listener (in another terminal)
./src/pkg_cpp_listener/build/pkg_cpp_listener
```

Each listener should print `received: N` once per second.

For a Rust↔C interop demo, swap step 2 with:

```bash
./src/pkg_rust_publisher/target/release/pkg_rust_publisher
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

* `tools/setup.sh --rust-workspace` writes a workspace-level
  `Cargo.toml` + `[patch.crates-io]` so the per-package
  `.cargo/config.toml` shim isn't needed. Currently the Rust
  package carries its own patch table for the standalone-build
  path. See Phase 123.A.3's `--rust-workspace` flag (impl
  deferred).

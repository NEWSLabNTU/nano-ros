# Setup Compared to Standard ROS 2

This page is for ROS 2 users who already know the normal desktop flow:
install a ROS distro, create a workspace, run `rosdep`, build with
`colcon`, and select an RMW at runtime with `RMW_IMPLEMENTATION`.

nano-ros keeps the workspace and package vocabulary, but changes the
setup boundary because it targets embedded and RTOS builds.

## Standard ROS 2 Flow

A typical ROS 2 application starts from a distro install:

```bash
source /opt/ros/humble/setup.bash
mkdir -p ~/ros2_ws/src
cd ~/ros2_ws
rosdep install --from-paths src --ignore-src -y
colcon build
source install/setup.bash
```

The middleware implementation is selected at process startup:

```bash
export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp
ros2 run my_pkg my_node
```

That model assumes shared libraries, a hosted OS, and runtime plugin
loading.

## nano-ros Flow

nano-ros is **shipped as source** (Phase 111 archive — no crates.io,
no precompiled SDK, no binary tarball). Clone, run `just setup`, then
build the example tree (or your own package) directly:

```bash
git clone --branch=v<X.Y.Z> https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros

# One-shot SDK fetch. `tier` controls how much gets installed.
just setup tier=default        # full coverage for `just ci`
# just setup tier=minimal      # workspace + verification + zenohd
# just setup tier=extended     # default + esp_idf + px4

# Build + run an example (POSIX):
cd examples/native/rust/zenoh/talker
cargo run
```

For embedded targets, the per-platform `just <plat> build` /
`just <plat> run` recipes drive the right cross-toolchain
(see [Build Commands](../reference/build-commands.md)):

```bash
just freertos build-fixtures   # QEMU FreeRTOS Cortex-M3 examples
just zephyr  build-fixtures    # west + Zephyr-SDK
just nuttx   build-fixtures    # NuttX kernel + ARM Cortex-M3
```

SDK tier matrix (Phase 142 — strict supersets):

- `minimal` — workspace + verification + zenohd. Rust-only.
- `default` — `minimal` + QEMU + FreeRTOS + NuttX + ThreadX(Linux/RV64) +
  ESP32 + Zephyr + XRCE + rmw_zenoh + Orin SPE + Cyclone DDS + PlatformIO.
  Covers everything `just ci` exercises.
- `extended` — `default` + ESP-IDF + PX4. Every Phase 139 integration
  shell runnable.

Override the default via `NROS_SETUP_TIER=<tier>` or by passing
`tier=<tier>` to `just setup`.

## Choosing platform + RMW

Unlike standard ROS 2, RMW and platform are **compile-time** choices —
there is no runtime `RMW_IMPLEMENTATION` switch on embedded targets
(no `dlopen`). The pair is selected via CMake cache vars + Cargo
features:

```cmake
# Each example is a standalone CMake project that pulls nano-ros in
# via add_subdirectory (Phase 144).
set(NANO_ROS_PLATFORM freertos)
set(NANO_ROS_RMW      zenoh)
set(NANO_ROS_BOARD    mps2-an385-freertos)
add_subdirectory(<repo-root>  nano_ros)

target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
nros_platform_link_app(my_app)
nano_ros_link_rmw(my_app RMW zenoh)
```

Multi-RMW bridges (one binary, two or more backends) use
`Executor::open_with_rmw("<name>", ...)` + `node_builder.rmw("<name>")`
— see [Cross-backend Bridges](../user-guide/cross-backend-bridges.md).

## What Stays Familiar

- Workspace layout: one source checkout next to your packages.
- Package metadata: downstream packages still use `package.xml`.
- ROS vocabulary: nodes, publishers, subscriptions, services, actions,
  QoS profiles, parameters, and message packages keep ROS-shaped names.
- `colcon build` still works as a consumer-side build for POSIX
  workspaces that already use it; embedded targets use `cmake`,
  `cargo`, `west`, or `idf.py` directly.
- Interop: POSIX nano-ros nodes can communicate with standard ROS 2
  nodes through compatible RMW backends (Zenoh, Cyclone DDS, XRCE).

## What Changes

- **Source-only.** No binary SDK tarball, no crates.io umbrella crate,
  no Arduino zip / ESP-IDF binary component / PlatformIO library /
  GitHub Releases artifact. The locked policy is `git clone --branch=v<X.Y.Z>` +
  in-tree build.
- **Target-aware setup.** `just setup tier=<tier>` fetches only the
  submodules + toolchains needed for the requested tier.
- **Compile-time RMW + platform.** Embedded targets can't `dlopen`,
  so the RMW and platform combination is locked in by CMake cache
  vars (`NANO_ROS_PLATFORM`, `NANO_ROS_RMW`) and Cargo features at
  build time.
- **No install prefix.** Phase 140 removed `just install-local` and
  every `install(...)` rule; consumers pull nano-ros into their build
  via `add_subdirectory(<repo-root>)` (Phase 144). The Phase 139
  integration shells under `integrations/<rtos>/` re-export the same
  root CMake under each RTOS's native package manager.
- **Generated bindings in-tree.** Message codegen lands under
  `<your-package>/generated/` (or `OUT_DIR` for Cargo builds), not in
  an installed ROS message library.
- **Configuration is build-time on embedded.** Runtime env vars
  (`ROS_DOMAIN_ID`, `ZENOH_LOCATOR`, …) work on POSIX; embedded
  targets resolve config via CMake cache, Kconfig (Zephyr), Cargo
  features, or `config.toml`.

## Next Step

Continue with [Installation](../getting-started/installation.md), then
run the [ROS 2 Interoperability](../getting-started/ros2-interop.md)
example before moving to a platform-specific guide.

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

Where standard ROS 2 installs a distro and resolves system packages with
`rosdep`, nano-ros provisions a **per-board toolchain** with one command.
`nros setup` replaces the distro install + `rosdep`: it ships **prebuilt
toolchains per platform per RMW** — the cross-compiler, emulator, RMW host
daemon, and SDK sources for a board are fetched from a pinned index into a
shared store (`${NROS_HOME:-~/.nros}/sdk`). You do not install cross-toolchains by hand,
and you do not need a ROS distro on the machine.

```bash
# 1. Build the in-tree nros CLI (analogous to installing a ROS distro, Phase 218):
source ./activate.sh        # OR: direnv allow / source ./activate.fish
just setup-cli              # builds packages/cli/target/release/nros

# 2. Provision a board + RMW (analogous to `rosdep install`):
nros setup native --rmw zenoh

# 3. Build + run an example (the nano-ros source is vendored in your project):
cd examples/native/rust/talker
cargo run
```

For embedded targets, name the board instead of `native`; `nros setup`
fetches the matching prebuilt cross-toolchain + emulator + SDK:

```bash
nros setup qemu-arm-freertos --rmw zenoh     # arm-none-eabi-gcc, qemu, FreeRTOS+lwIP
nros setup zephyr            --rmw zenoh     # Zephyr west workspace + SDK bits
nros setup qemu-arm-nuttx    --rmw zenoh     # arm-none-eabi-gcc, qemu, NuttX
```

Useful flags: `nros setup --list` (every package + version),
`nros setup <board> --dry-run` (resolve + print the plan, fetch nothing),
`nros setup --licenses` (license-gated packages). See
[Supported Boards](../reference/supported-boards.md) for the board list and
[`nros` CLI](../reference/cli.md) for every subcommand.

> Contributors working on nano-ros itself drive the same index through
> `just` — `just <module> setup` calls `nros setup <board>` under the hood,
> so the provisioned toolchains are identical.

## Choosing platform + RMW

Unlike standard ROS 2, RMW and platform are **compile-time** choices —
there is no runtime `RMW_IMPLEMENTATION` switch on embedded targets
(no `dlopen`). The pair is selected via CMake cache vars + Cargo
features:

```cmake
# Each example is a standalone CMake project that pulls nano-ros in
# via add_subdirectory.
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
  no Arduino zip / ESP-IDF binary component / GitHub Releases artifact.
  The locked policy is `git clone --branch=v<X.Y.Z>` +
  in-tree build.
- **Per-board provisioning, no `rosdep`.** `nros setup <board> --rmw <rmw>`
  is the single setup command. It fetches prebuilt toolchains (cross-gcc,
  emulator), the RMW host daemon, and SDK sources for exactly that board+RMW
  into `~/.nros/sdk` — no system-wide package install, no ROS distro. (The
  `just <module> setup` recipes call the same command for contributors.)
- **Compile-time RMW + platform.** Embedded targets can't `dlopen`,
  so the RMW and platform combination is locked in by CMake cache
  vars (`NANO_ROS_PLATFORM`, `NANO_ROS_RMW`) and Cargo features at
  build time.
- **No install prefix.** removed `just install-local` and
  every `install(...)` rule; consumers pull nano-ros into their build
  via `add_subdirectory(<repo-root>)`. The
  integration shells under `integrations/<rtos>/` re-export the same
  root CMake under each RTOS's native package manager.
- **Generated bindings in-tree.** Message codegen lands under
  `<your-package>/generated/` (or `OUT_DIR` for Cargo builds), not in
  an installed ROS message library.
- **Configuration is build-time on embedded.** Runtime env vars
  (`ROS_DOMAIN_ID`, `NROS_LOCATOR` — legacy alias `ZENOH_LOCATOR`,
  …) work on POSIX; embedded targets bake config from
  `[package.metadata.nros.deploy.<t>]` (Rust) / the package.xml
  `<nano_ros deploy=…/>` tuple (C/C++)
  (CMake), plus Kconfig on Zephyr.

## Next Step

Continue with [Installation](../getting-started/installation.md), then
run the [ROS 2 Interoperability](../getting-started/ros2-interop.md)
example before moving to a platform-specific guide.

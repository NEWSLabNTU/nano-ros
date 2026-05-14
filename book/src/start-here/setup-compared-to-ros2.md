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

The middleware implementation is usually selected at process startup:

```bash
export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp
ros2 run my_pkg my_node
```

That model assumes shared libraries, a hosted OS, and runtime plugin
loading.

## nano-ros Flow

nano-ros is distributed as source and built inside the user's workspace:

```bash
mkdir -p ~/ros2_ws/src
cd ~/ros2_ws/src
git clone --depth=1 --branch=v1.0.0 https://github.com/NEWSLabNTU/nano-ros.git
cd ~/ros2_ws
./src/nano-ros/tools/setup.sh --target=posix-zenoh
colcon build
source install/setup.bash
```

The `--target=<platform>-<rmw>` tuple replaces runtime RMW selection.
It tells setup which submodules, Rust target, C/C++ dependencies, and
platform support are needed. Examples include `posix-zenoh`,
`freertos-xrce`, `zephyr-dds`, and `threadx-zenoh`.

## What Stays Familiar

- Workspace layout: one source checkout under `~/ros2_ws/src/`.
- Package metadata: downstream packages still use `package.xml`.
- Build entry point: `colcon build` remains the recommended consumer
  build for ROS 2 workspaces.
- ROS vocabulary: nodes, publishers, subscriptions, services, actions,
  QoS profiles, parameters, and message packages keep ROS-shaped names.
- Interop: POSIX nano-ros nodes can communicate with standard ROS 2
  nodes through compatible RMW backends.

## What Changes

- nano-ros is source-first. There is no binary SDK tarball or crates.io
  umbrella crate.
- Setup is target-aware. `tools/setup.sh` fetches only the submodules
  required by the selected platform and RMW.
- RMW and platform are compile-time choices. Embedded targets usually
  cannot use `dlopen` or shared-library RMW plugins.
- Message bindings are generated into the workspace or build tree
  instead of relying on installed ROS message libraries.
- Runtime environment variables are POSIX conveniences. Embedded
  targets usually resolve configuration at build time through CMake,
  Kconfig, Cargo features, or `config.toml`.

## Next Step

Continue with [Installation](../getting-started/installation.md), then
run the [ROS 2 Interoperability](../getting-started/ros2-interop.md)
example before moving to a platform-specific guide.

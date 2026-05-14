# Native POSIX

Use the native POSIX target for first experiments, CI smoke tests, and
ROS 2 interoperability checks on Linux or macOS. It uses OS sockets,
the host process environment, and the same source checkout layout as the
embedded targets.

## When to Use It

- You want the shortest path to a working nano-ros publisher or
  subscriber.
- You are validating message generation, CMake integration, or ROS 2
  interop before moving to an RTOS.
- Your deployment target is Linux, macOS, or an embedded Linux system.

## Setup

From a ROS 2 workspace root:

```bash
./src/nano-ros/tools/setup.sh --target=posix-zenoh
colcon build
source install/setup.bash
```

For direct repository development, use:

```bash
just setup posix-zenoh
just build
```

## Run a First Node

Follow [First Native Rust Node](../getting-started/native.md) for a
publisher/listener walkthrough. For ROS 2 interop, run
[ROS 2 Interoperability](../getting-started/ros2-interop.md) and set
the ROS side to `rmw_zenoh_cpp`.

## Configuration

Native examples can read runtime settings from the shell:

```bash
export ROS_DOMAIN_ID=0
export NROS_LOCATOR=tcp/127.0.0.1:7447
```

The same fields are compiled into embedded targets through their
platform-specific configuration files. See
[Configuration](../user-guide/configuration.md) for the full layering.

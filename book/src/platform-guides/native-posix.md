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

## Package Layout

For a POSIX application package:

```text
my_posix_node/
├── package.xml
├── Cargo.toml          # Rust path
├── CMakeLists.txt      # C / C++ path
└── src/
    └── main.rs         # or main.c / main.cpp
```

Keep package beside `nano-ros` in workspace `src/`. Use path
dependencies for Rust or `add_subdirectory(<path-to-nano-ros>)` for
C/C++.

## Code Example

Rust publisher skeleton:

```rust,ignore
use nros::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ExecutorConfig::from_env().node_name("talker");
    let mut executor = Executor::open(&config)?;
    let mut node = executor.create_node("talker")?;
    let publisher = node.create_publisher::<std_msgs::msg::Int32>("/chatter")?;

    let mut msg = std_msgs::msg::Int32 { data: 0 };
    loop {
        publisher.publish(&msg)?;
        msg.data += 1;
        executor.spin_once(100);
    }
}
```

Use the full [First Native Rust Node](../getting-started/native.md)
walkthrough for generated messages and runnable commands.

## Build and Run

Start router:

```bash
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447
```

Run node through colcon install or directly with Cargo/CMake,
depending on package type.

## Configuration

Native examples can read runtime settings from the shell:

```bash
export ROS_DOMAIN_ID=0
export NROS_LOCATOR=tcp/127.0.0.1:7447
```

The same fields are compiled into embedded targets through their
platform-specific configuration files. See
[Configuration](../user-guide/configuration.md) for the full layering.

## Deployment

POSIX deployment is normal process deployment: install the workspace,
source `install/setup.bash`, set environment, and run the binary. For
ROS 2 interop, run [ROS 2 Interoperability](../getting-started/ros2-interop.md)
and set the ROS side to `rmw_zenoh_cpp`.

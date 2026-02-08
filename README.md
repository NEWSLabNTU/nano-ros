# nano-ros

A lightweight ROS 2 client library for embedded real-time systems, written in Rust.

## Features

- `no_std` compatible for bare-metal and RTOS targets (Zephyr, RTIC, Embassy)
- ROS 2 interoperability via rmw_zenoh protocol
- Zero-copy CDR serialization
- C API for integration with C/C++ projects
- Runs on Linux, QEMU bare-metal (Cortex-M3), Zephyr RTOS, and STM32F4

## Status

| Feature         | Status   |
|-----------------|----------|
| Pub/Sub         | Complete |
| Services        | Complete |
| Actions         | Complete |
| Parameters      | Complete |
| ROS 2 Interop   | Complete |
| Zephyr Support  | Complete |
| QEMU Bare-Metal | Complete |
| C API           | Complete |
| Message Codegen | Complete |

## Requirements

- Rust nightly (edition 2024)
- zenohd 1.6.2 router (built from submodule via `just build-zenohd`)
- (Optional) ROS 2 Humble with rmw_zenoh_cpp for interop
- (Optional) cmake for C examples

## Quick Start (Rust)

### As a Git Dependency

Add nano-ros to your project's `Cargo.toml`:

```toml
[dependencies]
nano-ros = { git = "https://github.com/jerry73204/nano-ros", default-features = false, features = ["std"] }
std_msgs = { version = "*", default-features = false }
```

Generate message bindings with `cargo nano-ros`:

```bash
# Install the binding generator
cargo install --git https://github.com/jerry73204/nano-ros --path colcon-nano-ros/packages/cargo-nano-ros

# Source ROS 2 and generate bindings
source /opt/ros/humble/setup.bash
cargo nano-ros generate --config --nano-ros-git
```

This creates `generated/` with Rust types for your ROS 2 messages and a `.cargo/config.toml` with the necessary patch entries.

See [Getting Started](docs/guides/getting-started.md) for a complete walkthrough.

### From the Repository

```bash
git clone https://github.com/jerry73204/nano-ros.git
cd nano-ros
just setup         # Install toolchains + tools
just build-zenohd  # Build zenohd 1.6.2 from submodule
```

Run the demo:

```bash
# Terminal 1: Zenoh router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Talker
cd examples/native/rs-talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: Listener
cd examples/native/rs-listener && RUST_LOG=info cargo run --features zenoh
```

## Quick Start (C API)

Build the nano-ros C library and link against it with CMake:

```bash
# Build the static library
cd nano-ros
cargo build -p nano-ros-c --release

# Build a C example
cd examples/native/c-talker
mkdir -p build && cd build
cmake -DNANO_ROS_ROOT=/path/to/nano-ros ..
make
```

A `FindNanoRos.cmake` module is provided at `cmake/FindNanoRos.cmake` for easy integration:

```cmake
list(APPEND CMAKE_MODULE_PATH "/path/to/nano-ros/cmake")
find_package(NanoRos REQUIRED)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
```

See [Getting Started](docs/guides/getting-started.md) for a complete C walkthrough.

## ROS 2 Interoperability

nano-ros communicates with ROS 2 nodes via the rmw_zenoh protocol:

```bash
# Terminal 1: zenohd
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nano-ros talker
cd examples/native/rs-talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## Project Structure

```
crates/
├── nano-ros/              # Unified API (re-exports all sub-crates)
├── nano-ros-core/         # Core types, traits, node abstraction
├── nano-ros-serdes/       # CDR serialization
├── nano-ros-macros/       # #[derive(RosMessage)] proc macros
├── nano-ros-params/       # Parameter server
├── nano-ros-transport/    # Transport abstraction (zenoh backend)
├── nano-ros-node/         # High-level node API + parameter services
├── nano-ros-c/            # C API (rclc-style)
├── rcl-interfaces/        # Generated ROS 2 interface types
├── zenoh-pico-shim/       # Safe Rust API for zenoh-pico
└── zenoh-pico-shim-sys/   # FFI + C shim + zenoh-pico submodule
```

## Message Generation

nano-ros uses `cargo nano-ros generate` to create Rust bindings from ROS 2 `.msg`/`.srv`/`.action` files. See [Message Generation](docs/guides/message-generation.md) for details.

## Documentation

| Topic                  | Location                                                     |
|------------------------|--------------------------------------------------------------|
| Getting started        | [docs/guides/getting-started.md](docs/guides/getting-started.md)           |
| Message generation     | [docs/guides/message-generation.md](docs/guides/message-generation.md)     |
| ROS 2 interop protocol | [docs/reference/rmw_zenoh_interop.md](docs/reference/rmw_zenoh_interop.md)       |
| Testing                | [tests/README.md](tests/README.md)                           |
| Zephyr setup           | [docs/guides/zephyr-setup.md](docs/guides/zephyr-setup.md)                 |
| Embedded integration   | [docs/reference/embedded-integration.md](docs/reference/embedded-integration.md) |
| Troubleshooting        | [docs/guides/troubleshooting.md](docs/guides/troubleshooting.md)           |

## License

Apache-2.0 OR MIT

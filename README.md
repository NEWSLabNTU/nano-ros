# nros

A `no_std` ROS 2 client library for bare-metal and RTOS targets, written in Rust. Built on [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico) for lightweight pub/sub, services, and actions over TCP, serial, or raw Ethernet.

nros runs directly on microcontrollers without an OS, on RTOS kernels like Zephyr, and on Linux — using the same API. It interoperates with standard ROS 2 nodes via the rmw_zenoh protocol. QEMU emulation is provided for Cortex-M3 and ESP32-C3, enabling full integration testing without hardware.

The project integrates formal verification (Kani bounded model checking, CBMC for the C API) and WCET measurement (DWT cycle counters, static stack analysis) into the build pipeline, providing a foundation for schedulability analysis in safety-critical systems.

## Features

- **Bare-metal and RTOS**: runs on Cortex-M3, STM32F4, ESP32-C3, and Zephyr with no heap allocator required
- **ROS 2 interoperability**: communicates with ROS 2 Humble nodes via rmw_zenoh
- **QEMU emulation**: Cortex-M3 (MPS2-AN385) and ESP32-C3 targets with TAP networking for CI
- **Customizable platform/transport**: swap platform crates (clock, heap, RNG) and transport crates (TCP via smoltcp, serial, raw Ethernet) independently
- **Formal verification ready**: Kani proofs for panic-freedom, CBMC harnesses for C API pointer safety, DWT cycle counting for WCET baselines
- **Zero-copy CDR serialization**: `no_std` serializer with compile-time buffer bounds
- **C API**: rclc-style interface for integration with C/C++ projects
- **Code generation**: `cargo nano-ros generate` produces Rust bindings from `.msg`/`.srv`/`.action` files

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

Add nros to your project's `Cargo.toml`:

```toml
[dependencies]
nros = { git = "https://github.com/jerry73204/nano-ros", default-features = false, features = ["std"] }
std_msgs = { version = "*", default-features = false }
```

Generate message bindings with `cargo nano-ros`:

```bash
# Install the binding generator
cargo install --git https://github.com/jerry73204/nano-ros --path packages/codegen/packages/cargo-nano-ros

# Source ROS 2 and generate bindings
source /opt/ros/humble/setup.bash
cargo nano-ros generate --config --nano-ros-git
```

This creates `generated/` with Rust types for your ROS 2 messages and a `.cargo/config.toml` with the necessary patch entries.

See [Getting Started](docs/guides/getting-started.md) for a complete walkthrough.

### From the Repository

```bash
git clone https://github.com/jerry73204/nano-ros.git
cd nros
just setup         # Install toolchains + tools
just build-zenohd  # Build zenohd 1.6.2 from submodule
```

Run the demo:

```bash
# Terminal 1: Zenoh router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Talker
cd examples/native/rust/zenoh/talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: Listener
cd examples/native/rust/zenoh/listener && RUST_LOG=info cargo run --features zenoh
```

## Quick Start (C API)

Build the nros C library and link against it with CMake:

```bash
# Build the static library
cd nros
cargo build -p nros-c --release

# Build a C example
just install-local  # Build libraries + create CMake package
cd examples/native/c/zenoh/talker
mkdir -p build && cd build
cmake ..
make
```

A config-mode CMake package is provided for easy integration:

```cmake
find_package(NanoRos REQUIRED CONFIG)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
```

See [Getting Started](docs/guides/getting-started.md) for a complete C walkthrough.

## ROS 2 Interoperability

nros communicates with ROS 2 nodes via the rmw_zenoh protocol:

```bash
# Terminal 1: zenohd
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nros talker
cd examples/native/rust/zenoh/talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## Project Structure

```
packages/
├── core/                      # The nros library stack
│   ├── nros/              # Unified API (re-exports all sub-crates)
│   ├── nros-core/         # Core types, traits, node abstraction
│   ├── nros-serdes/       # CDR serialization
│   ├── nros-macros/       # #[derive(RosMessage)] proc macros
│   ├── nros-params/       # Parameter server
│   ├── nros-rmw/          # Transport abstraction (middleware traits)
│   ├── nros-node/         # High-level node API + parameter services
│   └── nros-c/            # C API (rclc-style)
├── zpico/                     # Zenoh-pico transport backend
│   ├── nros-rmw-zenoh/        # Safe Rust API for zenoh-pico
│   ├── zpico-sys/             # FFI + C shim + zenoh-pico submodule
│   └── zpico-smoltcp/         # TCP/UDP via smoltcp IP stack
├── interfaces/                # Generated ROS 2 types
│   └── rcl-interfaces/        # rcl_interfaces + builtin_interfaces
├── bsp/                       # Board Support Packages
├── drivers/                   # Hardware drivers (lan9118, openeth)
├── testing/                   # Integration test infrastructure
├── reference/                 # Low-level platform reference implementations
└── codegen/                   # Message binding generator (cargo nano-ros)
```

## Message Generation

nros uses `cargo nano-ros generate` to create Rust bindings from ROS 2 `.msg`/`.srv`/`.action` files. See [Message Generation](docs/guides/message-generation.md) for details.

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

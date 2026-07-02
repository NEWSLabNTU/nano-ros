# nano-ros

[![CI](https://github.com/NEWSLabNTU/nano-ros/actions/workflows/pr-checks.yml/badge.svg)](https://github.com/NEWSLabNTU/nano-ros/actions/workflows/pr-checks.yml)
[![Book](https://img.shields.io/badge/docs-book-blue)](https://newslabntu.github.io/nano-ros-book/)
![no_std](https://img.shields.io/badge/no__std-yes-success)
![Rust](https://img.shields.io/badge/rust-edition%202024-orange)
![ROS 2](https://img.shields.io/badge/ROS%202-Humble%20%7C%20Iron-22314E)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

A `no_std` ROS 2 client library for bare-metal and RTOS targets, written in Rust. Built on [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico) for lightweight pub/sub, services, and actions over TCP, serial, or raw Ethernet.

nano-ros runs directly on microcontrollers without an OS, on RTOS kernels like Zephyr, and on Linux — using the same API. It interoperates with standard ROS 2 nodes via the rmw_zenoh protocol. QEMU emulation is provided for Cortex-M3 and ESP32-C3, enabling full integration testing without hardware.

The project integrates formal verification (Kani bounded model checking, CBMC for the C API) and WCET measurement (DWT cycle counters, static stack analysis) into the build pipeline, providing a foundation for schedulability analysis in safety-critical systems.

## Features

- **Bare-metal and RTOS**: runs on Cortex-M3, STM32F4, ESP32-C3, and Zephyr with no heap allocator required
- **ROS 2 interoperability**: communicates with ROS 2 Humble nodes via rmw_zenoh
- **QEMU emulation**: Cortex-M3 (MPS2-AN385) and ESP32-C3 targets with TAP networking for CI
- **Customizable platform/transport**: swap platform crates (clock, heap, RNG) and transport crates (TCP via smoltcp, serial, raw Ethernet) independently
- **Formal verification ready**: Kani proofs for panic-freedom, CBMC harnesses for C API pointer safety, DWT cycle counting for WCET baselines
- **Zero-copy CDR serialization**: `no_std` serializer with compile-time buffer bounds
- **C API**: rclc-style interface for integration with C/C++ projects
- **Code generation**: `nros generate rust` produces Rust bindings from `.msg`/`.srv`/`.action` files

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
- `nros setup native --rmw <zenoh|xrce|cyclonedds>` provisions the RMW host
  daemon (zenohd, Micro-XRCE-DDS Agent, or Cyclone DDS) — no manual build step
- (Optional) ROS 2 Humble with rmw_zenoh_cpp for interop
- (Optional) cmake for C examples

## Quick Start (Rust)

nano-ros is distributed as **source** — nothing is published to crates.io.
Consumers either build in-tree (below) or add a path dependency (see
[Rust-only consumers](book/src/getting-started/installation.md#rust-only-consumers)).

### 1. Get the `nros` CLI

```bash
git clone https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros
./scripts/bootstrap.sh base
```

Already have cargo?
`cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros`.
Tagged release, no Rust at all? `./scripts/install-nros-prebuilt.sh`.

### 2. Activate the workspace (every new shell)

```bash
source ./activate.sh          # or: direnv allow / source ./activate.fish
```

### 3. Provision a board + RMW

```bash
nros setup native --rmw zenoh
```

Installs the zenoh router (`zenohd`) into `~/.nros/sdk`. See
[Supported Boards](book/src/reference/supported-boards.md) for cross
targets (Zephyr, FreeRTOS, NuttX, ThreadX, ESP32, bare-metal).

### 4. Run the demo

```bash
# Terminal 1: Zenoh router (installed by `nros setup native --rmw zenoh`)
export PATH="$(dirname "$(ls -d ~/.nros/sdk/zenohd/*/bin/zenohd | tail -1)")":$PATH
zenohd

# Terminal 2: Talker
cd examples/native/rust/talker && RUST_LOG=info cargo run

# Terminal 3: Listener
cd examples/native/rust/listener && RUST_LOG=info cargo run
```

See [Installation](book/src/getting-started/installation.md) and
[First Node — Rust](book/src/getting-started/first-node-rust.md) for the
complete walkthrough.

## Quick Start (C API)

Consume nano-ros from a CMake project via `add_subdirectory`:

```bash
# Clone alongside (or as a submodule of) your project.
cd examples/native/c/talker
cmake -B build -S .
cmake --build build
./build/c_talker
```

The example's `CMakeLists.txt` is 20 lines:

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker LANGUAGES C)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory(<path-to-nano-ros> nano_ros)

add_executable(c_talker src/main.c)
target_link_libraries(c_talker PRIVATE NanoRos::NanoRos)
nros_platform_link_app(c_talker)
```

See [First Node — C](book/src/getting-started/first-node-c.md) for a complete C walkthrough.

## On Zephyr (west module)

nano-ros is consumable as a Zephyr **module** from your own west workspace,
on both **Zephyr 3.7 LTS and 4.x**: import via `west.yml`, apply patches
(`west patch apply` on 4.x), pick an RMW (`-S nros-<rmw>` snippet on 4.x),
and copy out a worked example. See
[Zephyr (west module)](book/src/getting-started/integration-zephyr.md) for
the version-spanning consumption guide + compatibility matrix.

## ROS 2 Interoperability

nano-ros communicates with ROS 2 nodes via the rmw_zenoh protocol:

```bash
# Terminal 1: zenohd (installed by `nros setup native --rmw zenoh`)
export PATH="$(dirname "$(ls -d ~/.nros/sdk/zenohd/*/bin/zenohd | tail -1)")":$PATH
zenohd

# Terminal 2: nano-ros talker
cd examples/native/rust/talker && RUST_LOG=info cargo run

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
└── codegen/                   # Message binding generator (`nros`)
```

## Message Generation

nano-ros uses `nros generate rust` to create Rust bindings from ROS 2 `.msg`/`.srv`/`.action` files. See [Message Generation](docs/guides/message-generation.md) for details.

## Documentation

| Topic                  | Location                                                     |
|------------------------|--------------------------------------------------------------|
| Getting started        | [book/src/getting-started/installation.md](book/src/getting-started/installation.md) |
| Message generation     | [docs/guides/message-generation.md](docs/guides/message-generation.md)     |
| ROS 2 interop protocol | [docs/reference/rmw_zenoh_interop.md](docs/reference/rmw_zenoh_interop.md)       |
| Testing                | [tests/README.md](tests/README.md)                           |
| Zephyr setup           | [docs/guides/zephyr-setup.md](docs/guides/zephyr-setup.md)                 |
| Embedded integration   | [book/src/concepts/board-integration.md](book/src/concepts/board-integration.md) |
| Troubleshooting        | [docs/guides/troubleshooting.md](docs/guides/troubleshooting.md)           |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option (SPDX `MIT OR Apache-2.0`).

A few crates derived from Apache-2.0 ROS 2 sources are **Apache-2.0 only** —
`rcl-interfaces` and `lifecycle-msgs` (generated from ROS 2 message
definitions) and `nros-c` (rclc-compatible C API). Each crate's `Cargo.toml`
declares its own SPDX `license`.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

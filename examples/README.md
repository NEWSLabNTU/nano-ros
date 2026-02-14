# nros Examples

This directory contains examples demonstrating nros on various platforms.

## Directory Structure

```
examples/
├── native/              # Desktop/Linux examples
├── qemu/                # QEMU bare-metal ARM (MPS2-AN385)
├── stm32f4/             # STM32F4 microcontrollers
├── zephyr/              # Zephyr RTOS
└── platform-integration/  # Low-level reference implementations
```

## Example Categories

### Native (`native/`)

Desktop/Linux examples using the full nros Rust API. Best for learning nros concepts and developing applications before deploying to embedded targets.

| Example | Language | Description |
|---------|----------|-------------|
| `rs-talker` | Rust | Publishes Int32 messages to `/chatter` |
| `rs-listener` | Rust | Subscribes to `/chatter` |
| `rs-service-server` | Rust | ROS 2 service server example |
| `rs-service-client` | Rust | ROS 2 service client example |
| `rs-action-server` | Rust | ROS 2 action server example |
| `rs-action-client` | Rust | ROS 2 action client example |
| `rs-custom-msg` | Rust | Custom message types |
| `c-talker` | C | C language talker using nros-c |
| `c-listener` | C | C language listener using nros-c |
| `cpp-talker` | C++ | C++ talker using nros-cpp |
| `cpp-listener` | C++ | C++ listener using nros-cpp |

**Running native examples:**
```bash
# Terminal 1: Start zenoh router
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Run talker
cd examples/native/rs-talker && cargo run

# Terminal 3: Run listener
cd examples/native/rs-listener && cargo run
```

### QEMU (`qemu/`)

Bare-metal ARM Cortex-M examples running on QEMU MPS2-AN385. Uses `nano-ros-platform-qemu` for simplified setup.

| Example | Description |
|---------|-------------|
| `bsp-talker` | Simplified publisher using platform crate (recommended starting point) |
| `bsp-listener` | Simplified subscriber using platform crate |
| `rs-talker` | Full publisher example |
| `rs-listener` | Full subscriber example |
| `rs-test` | Integration tests for CI |

**Running QEMU examples:**
```bash
# Terminal 1: Start zenoh router
zenohd --listen tcp/192.0.2.1:7447

# Terminal 2: Set up TAP networking
sudo ./scripts/zephyr/setup-network.sh

# Terminal 3: Run in Docker (recommended)
just docker-qemu-test
```

### STM32F4 (`stm32f4/`)

STM32F4 microcontroller examples using `nano-ros-platform-stm32f4`.

| Example | Description |
|---------|-------------|
| `bsp-talker` | Publisher using STM32F4 platform crate (for NUCLEO-F429ZI) |

**Building STM32F4 examples:**
```bash
cd examples/stm32f4/bsp-talker
cargo build --release --target thumbv7em-none-eabihf
# Flash with probe-rs
cargo run --release
```

### Zephyr (`zephyr/`)

Zephyr RTOS examples using `nano-ros-bsp-zephyr` (C) or the Rust API.

| Example | Language | Description |
|---------|----------|-------------|
| `c-talker` | C | Publisher using C BSP API |
| `c-listener` | C | Subscriber using C BSP API |
| `rs-talker` | Rust | Publisher using Rust API on Zephyr |
| `rs-listener` | Rust | Subscriber using Rust API on Zephyr |
| `rs-service-server` | Rust | Service server on Zephyr |
| `rs-service-client` | Rust | Service client on Zephyr |
| `rs-action-server` | Rust | Action server on Zephyr |
| `rs-action-client` | Rust | Action client on Zephyr |

**Running Zephyr examples:**
```bash
# Set up Zephyr workspace
./scripts/zephyr/setup.sh
source ~/nano-ros-workspace/env.sh

# Build for native_sim
west build -b native_sim/native/64 nros/examples/zephyr/c-talker

# Run
./build/zephyr/zephyr.exe
```

### Platform Integration (`platform-integration/`)

Low-level reference implementations for BSP developers. These examples show how to integrate nros with different network stacks and hardware platforms.

| Example | Description |
|---------|-------------|
| `qemu-smoltcp-bridge` | smoltcp-to-zenoh-pico bridge library |
| `qemu-lan9118` | LAN9118 Ethernet driver validation |
| `stm32f4-smoltcp` | smoltcp TCP echo server for STM32F4 |
| `stm32f4-rtic` | RTIC-based networking example |
| `stm32f4-polling` | Polling-based networking example |
| `stm32f4-embassy` | Embassy async framework example |

**Note:** These examples are for advanced users developing platform support. Most users should start with the platform-crate-based examples above.

## Quick Start

1. **New to nros?** Start with `native/rs-talker` and `native/rs-listener`
2. **Targeting QEMU?** Use `qemu/bsp-talker` and `qemu/bsp-listener`
3. **Targeting STM32F4?** Use `stm32f4/bsp-talker`
4. **Targeting Zephyr?** Use `zephyr/c-talker` (C) or `zephyr/rs-talker` (Rust)

## ROS 2 Interoperability

nros examples are compatible with ROS 2 nodes using rmw_zenoh. To test interop:

```bash
# Terminal 1: zenoh router
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nros talker
cd examples/native/rs-talker && cargo run

# Terminal 3: ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## See Also

- [CLAUDE.md](../CLAUDE.md) - Development guidelines
- [docs/zephyr-setup.md](../docs/guides/zephyr-setup.md) - Zephyr workspace setup
- [docs/rmw_zenoh_interop.md](../docs/reference/rmw_zenoh_interop.md) - ROS 2 protocol details

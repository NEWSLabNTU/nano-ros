# nros Examples

This directory contains examples demonstrating nros on various platforms.

## Directory Structure

```
examples/
├── native/              # Desktop/Linux examples
│   ├── rust/zenoh/      # Rust + zenoh transport
│   ├── rust/xrce/       # Rust + XRCE-DDS transport
│   └── c/zenoh/         # C + zenoh transport
├── qemu-arm-baremetal/            # QEMU bare-metal ARM (MPS2-AN385)
│   └── rust/
│       ├── zenoh/       # Networked examples (talker, listener)
│       ├── core/        # nros-core only (cdr-test, wcet-bench)
│       └── standalone/  # No nros deps (lan9118 driver test)
├── qemu-esp32-baremetal/          # QEMU ESP32-C3 (RISC-V)
│   └── rust/zenoh/      # Networked examples
├── esp32/               # ESP32-C3 hardware
│   └── rust/
│       ├── zenoh/       # Networked examples
│       └── standalone/  # No nros deps (hello-world)
├── stm32f4/             # STM32F4 microcontrollers
│   └── rust/
│       ├── zenoh/       # Networked examples (talker, polling, rtic)
│       ├── core/        # nros-core only (embassy)
│       └── standalone/  # No nros deps (smoltcp)
└── zephyr/              # Zephyr RTOS
    ├── rust/zenoh/      # Rust + zenoh transport
    └── c/zenoh/         # C + zenoh transport
```

## Example Categories

### Native (`native/`)

Desktop/Linux examples using the full nros Rust or C API. Best for learning nros concepts and developing applications before deploying to embedded targets.

| Example | Language | Description |
|---------|----------|-------------|
| `rust/zenoh/talker` | Rust | Publishes Int32 messages to `/chatter` |
| `rust/zenoh/listener` | Rust | Subscribes to `/chatter` |
| `rust/zenoh/service-server` | Rust | ROS 2 service server example |
| `rust/zenoh/service-client` | Rust | ROS 2 service client example |
| `rust/zenoh/action-server` | Rust | ROS 2 action server example |
| `rust/zenoh/action-client` | Rust | ROS 2 action client example |
| `rust/zenoh/custom-msg` | Rust | Custom message types |
| `rust/xrce/talker` | Rust | XRCE-DDS publisher on `/chatter` |
| `rust/xrce/listener` | Rust | XRCE-DDS subscriber on `/chatter` |
| `rust/xrce/service-server` | Rust | XRCE-DDS service server |
| `rust/xrce/service-client` | Rust | XRCE-DDS service client |
| `rust/xrce/action-server` | Rust | XRCE-DDS action server (Fibonacci) |
| `rust/xrce/action-client` | Rust | XRCE-DDS action client (Fibonacci) |
| `c/zenoh/talker` | C | C language talker using nros-c |
| `c/zenoh/listener` | C | C language listener using nros-c |
| `c/zenoh/custom-msg` | C | C custom message types |

**Running native examples:**
```bash
# Terminal 1: Start zenoh router
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Run talker
cd examples/native/rust/zenoh/talker && cargo run

# Terminal 3: Run listener
cd examples/native/rust/zenoh/listener && cargo run
```

### QEMU ARM Bare-Metal (`qemu-arm-baremetal/`)

Bare-metal ARM Cortex-M3 examples running on QEMU MPS2-AN385. Uses `nros-board-mps2-an385` for simplified setup.

| Example | Description |
|---------|-------------|
| `rust/zenoh/talker` | Publisher using platform crate |
| `rust/zenoh/listener` | Subscriber using platform crate |
| `rust/core/cdr-test` | CDR serialization integration tests |
| `rust/core/wcet-bench` | WCET cycle counting benchmarks |
| `rust/standalone/lan9118` | LAN9118 Ethernet driver validation |

**Running QEMU examples:**
```bash
# Run in Docker (recommended)
just docker-qemu-test
```

### STM32F4 (`stm32f4/`)

STM32F4 microcontroller examples using `nros-board-stm32f4`.

| Example | Description |
|---------|-------------|
| `rust/zenoh/talker` | Publisher for NUCLEO-F429ZI |
| `rust/zenoh/polling` | Polling-based networking |
| `rust/zenoh/rtic` | RTIC-based networking |
| `rust/core/embassy` | Embassy async framework |
| `rust/standalone/smoltcp` | smoltcp TCP echo server |

### Zephyr (`zephyr/`)

Zephyr RTOS examples using `zpico-zephyr` (C) or the Rust API.

| Example | Language | Description |
|---------|----------|-------------|
| `c/zenoh/talker` | C | Publisher using C BSP API |
| `c/zenoh/listener` | C | Subscriber using C BSP API |
| `rust/zenoh/talker` | Rust | Publisher using Rust API on Zephyr |
| `rust/zenoh/listener` | Rust | Subscriber using Rust API on Zephyr |
| `rust/zenoh/service-server` | Rust | Service server on Zephyr |
| `rust/zenoh/service-client` | Rust | Service client on Zephyr |
| `rust/zenoh/action-server` | Rust | Action server on Zephyr |
| `rust/zenoh/action-client` | Rust | Action client on Zephyr |

**Running Zephyr examples:**
```bash
# Set up Zephyr workspace
just zephyr setup
source ~/nano-ros-workspace/env.sh

# Build for native_sim
west build -b native_sim/native/64 nros/examples/zephyr/c/zenoh/talker

# Run
./build/zephyr/zephyr.exe
```

## Quick Start

1. **New to nros?** Start with `native/rust/zenoh/talker` and `native/rust/zenoh/listener`
2. **Targeting QEMU?** Use `qemu-arm-baremetal/rust/zenoh/talker` and `qemu-arm-baremetal/rust/zenoh/listener`
3. **Targeting STM32F4?** Use `stm32f4/rust/zenoh/talker`
4. **Targeting Zephyr?** Use `zephyr/c/zenoh/talker` (C) or `zephyr/rust/zenoh/talker` (Rust)

## ROS 2 Interoperability

nros examples are compatible with ROS 2 nodes using rmw_zenoh. To test interop:

```bash
# Terminal 1: zenoh router
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nros talker
cd examples/native/rust/zenoh/talker && cargo run

# Terminal 3: ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## See Also

- [CLAUDE.md](../CLAUDE.md) - Development guidelines
- [docs/zephyr-setup.md](../docs/guides/zephyr-setup.md) - Zephyr workspace setup
- [docs/rmw_zenoh_interop.md](../docs/reference/rmw_zenoh_interop.md) - ROS 2 protocol details

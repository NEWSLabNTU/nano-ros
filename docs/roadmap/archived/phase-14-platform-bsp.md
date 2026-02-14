# Phase 14: Platform BSP Libraries

## Overview

Create Board Support Package (BSP) libraries that hide platform-specific details (smoltcp, Ethernet drivers, network configuration) behind simple, high-level APIs. Users should be able to write nano-ros applications without understanding the underlying network stack.

**Status**: Complete

## Problem Statement

Current bare-metal and RTOS examples expose too many low-level details:

| Example           | Issues                                                 |
|-------------------|--------------------------------------------------------|
| `qemu-rs-talker`  | Hardcoded MAC/IP, manual smoltcp setup, ~200 lines     |
| `zephyr-c-talker` | Direct `zenoh_shim_*` calls instead of nano-ros C API  |
| `stm32f4-rs-*`    | Platform integration code mixed with application logic |

Users must understand smoltcp, zenoh-pico internals, and hardware initialization just to publish a message.

## Goals

1. **Zero-config startup**: `run_node(|node| { ... })` with no manual setup
2. **Platform abstraction**: Same application code across QEMU, STM32, Zephyr
3. **Sensible defaults**: Auto-detect hardware, use DHCP/link-local, generate unique IDs
4. **Progressive disclosure**: Simple API by default, escape hatches for customization

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      User Application                            │
│              (20-50 lines of ROS-focused code)                  │
└─────────────────────────────┬───────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────┐
│                     nano-ros / nano-ros-c                        │
│              (Node, Publisher, Subscriber, Executor)             │
└─────────────────────────────┬───────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────┐
│                    Platform BSP Libraries                        │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐             │
│  │nano-ros-bsp- │ │nano-ros-bsp- │ │nano-ros-bsp- │             │
│  │    qemu      │ │   stm32f4    │ │    zephyr    │             │
│  │   (Rust)     │ │    (Rust)    │ │     (C)      │             │
│  └──────────────┘ └──────────────┘ └──────────────┘             │
└─────────────────────────────┬───────────────────────────────────┘
                              │ (hidden from users)
┌─────────────────────────────▼───────────────────────────────────┐
│   smoltcp │ zenoh-pico │ lan9118 │ stm32-eth │ Zephyr APIs     │
└─────────────────────────────────────────────────────────────────┘
```

## New Crates

### 14.1 `nano-ros-bsp-qemu` (Rust)

QEMU MPS2-AN385 with LAN9118 Ethernet.

**Target API**:
```rust
#![no_std]
#![no_main]

use nano_ros_bsp_qemu::prelude::*;

#[entry]
fn main() -> ! {
    // Zero-config: auto MAC, link-local IP, discovers zenoh router
    run_node(|node| {
        let publisher = node.create_publisher("demo/topic")?;

        for i in 0..10 {
            node.spin_once(100);
            publisher.publish(format_args!("Hello {}", i))?;
        }
        Ok(())
    })
}
```

**With configuration**:
```rust
run_node_with_config(
    NodeConfig::new()
        .zenoh_locator("tcp/192.168.1.1:7447")
        .ip_address([192, 168, 1, 100])
        .node_name("my_node"),
    |node| { ... }
)
```

**Implementation**:
- Absorbs `nano-ros-baremetal` functionality
- Absorbs `qemu-rs-common` bridge code
- Provides `run_node()` entry point that handles all setup
- Auto-generates MAC from random seed or device ID
- Supports static IP or link-local (169.254.x.x) auto-configuration
- Zenoh locator from compile-time config or mDNS discovery (future)

**Files**:
```
packages/bsp/nano-ros-bsp-qemu/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API, prelude
│   ├── platform.rs      # LAN9118 + smoltcp setup
│   ├── config.rs        # NodeConfig builder
│   ├── network.rs       # IP auto-configuration
│   └── entry.rs         # run_node() implementation
```

### 14.2 `nano-ros-bsp-stm32f4` (Rust)

STM32F4 family with Ethernet (STM32F407, STM32F429, etc.).

**Target API**:
```rust
#![no_std]
#![no_main]

use nano_ros_bsp_stm32f4::prelude::*;

#[entry]
fn main() -> ! {
    // Auto-detects STM32 variant, configures Ethernet
    run_node(|node| {
        let subscriber = node.create_subscriber("sensor/cmd", |msg: &[u8]| {
            // Handle command
        })?;

        loop {
            node.spin_once(10);
        }
    })
}
```

**With pin configuration**:
```rust
run_node_with_config(
    Stm32Config::new()
        .phy_mode(PhyMode::RMII)
        .phy_address(0)
        .pins(PinConfig::nucleo_f429zi()),
    |node| { ... }
)
```

**Implementation**:
- Wraps `stm32-eth` crate
- Auto-detects STM32 variant via device ID
- Provides common pin configurations (Nucleo, Discovery boards)
- Handles DMA descriptor setup internally
- Supports both polling and interrupt-driven modes
- Optional RTIC integration via feature flag

**Files**:
```
packages/bsp/nano-ros-bsp-stm32f4/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API
│   ├── platform.rs      # STM32 peripheral setup
│   ├── config.rs        # Stm32Config builder
│   ├── pins.rs          # Pin configurations for common boards
│   ├── phy.rs           # PHY detection and configuration
│   └── rtic.rs          # Optional RTIC integration
```

### 14.3 `nano-ros-bsp-zephyr` (C library)

Zephyr RTOS integration.

**Target API**:
```c
#include <nano_ros_bsp_zephyr.h>
#include <std_msgs/msg/int32.h>

void main(void) {
    // Uses Kconfig for zenoh locator, network settings
    nano_ros_bsp_context_t ctx;
    nano_ros_bsp_init(&ctx);

    nano_ros_node_t node;
    nano_ros_bsp_create_node(&ctx, &node, "talker");

    nano_ros_publisher_t pub;
    NANO_ROS_CREATE_PUBLISHER(&node, &pub, std_msgs__msg__Int32, "/chatter");

    std_msgs__msg__Int32 msg = {0};
    while (1) {
        msg.data++;
        nano_ros_publish(&pub, &msg);
        nano_ros_bsp_spin_once(&ctx, K_MSEC(1000));
    }
}
```

**Implementation**:
- Wraps `nano-ros-c` with Zephyr-specific initialization
- Reads configuration from Kconfig:
  - `CONFIG_NANO_ROS_ZENOH_LOCATOR`
  - `CONFIG_NANO_ROS_NODE_NAME`
  - `CONFIG_NANO_ROS_DOMAIN_ID`
- Handles Zephyr networking stack setup
- Manages zenoh-pico thread lifecycle
- Provides Zephyr-native error codes

**Files**:
```
packages/bsp/nano-ros-bsp-zephyr/
├── CMakeLists.txt
├── Kconfig
├── include/
│   └── nano_ros_bsp_zephyr.h
├── src/
│   ├── bsp_zephyr.c     # Main implementation
│   ├── config.c         # Kconfig integration
│   └── network.c        # Zephyr network setup
└── zephyr/
    └── module.yml       # Zephyr module definition
```

## Example Refactoring

### 14.4 Refactor `qemu-rs-talker`

**Before** (~200 lines):
```rust
// Manual MAC, IP, gateway configuration
const MAC_ADDRESS: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00];
mod net_config {
    pub const IP_ADDRESS: [u8; 4] = [192, 168, 100, 10];
    pub const GATEWAY: [u8; 4] = [192, 168, 100, 1];
    pub const ZENOH_LOCATOR: &[u8] = b"tcp/172.20.0.2:7447\0";
}

// Manual Ethernet driver setup
let mut eth = qemu_mps2::create_ethernet(MAC_ADDRESS)?;

// Manual smoltcp interface setup
let mut iface = create_interface(&mut eth);
let mut sockets = unsafe { create_socket_set() };

// Manual node configuration
let config = NodeConfig::new(IP_ADDRESS, GATEWAY, ZENOH_LOCATOR);
let mut node = BaremetalNode::new(&mut eth, &mut iface, &mut sockets, config)?;

// Finally, application logic...
let publisher = node.create_publisher(TOPIC)?;
```

**After** (~25 lines):
```rust
#![no_std]
#![no_main]

use nano_ros_bsp_qemu::prelude::*;

#[entry]
fn main() -> ! {
    run_node(|node| {
        hprintln!("QEMU Talker started");

        let publisher = node.create_publisher("demo/qemu")?;

        for i in 0u32..10 {
            node.spin_once(500);

            let msg = i.to_le_bytes();
            publisher.publish(&msg)?;
            hprintln!("Published: {}", i);
        }

        hprintln!("Done!");
        Ok(())
    })
}
```

### 14.5 Refactor `zephyr-c-talker`

**Before** (~130 lines with direct zenoh_shim calls):
```c
// Direct zenoh-pico shim usage
zenoh_shim_init(locator);
zenoh_shim_open();
int pub_handle = zenoh_shim_declare_publisher(keyexpr);

// Manual CDR serialization
uint8_t buffer[64];
// ... manual serialization code ...

zenoh_shim_publish(pub_handle, buffer, len);
```

**After** (~30 lines):
```c
#include <nano_ros_bsp_zephyr.h>
#include <std_msgs/msg/int32.h>

void main(void) {
    nano_ros_bsp_context_t ctx;
    nano_ros_bsp_init(&ctx);

    nano_ros_node_t node;
    nano_ros_bsp_create_node(&ctx, &node, "zephyr_talker");

    nano_ros_publisher_t pub;
    NANO_ROS_CREATE_PUBLISHER(&node, &pub, std_msgs__msg__Int32, "/chatter");

    std_msgs__msg__Int32 msg = {0};

    while (1) {
        msg.data++;
        nano_ros_publish(&pub, &msg);
        printk("Published: %d\n", msg.data);
        nano_ros_bsp_spin_once(&ctx, K_MSEC(1000));
    }
}
```

### 14.6 Reorganize Examples Directory

**New structure**:
```
examples/
├── README.md                      # Overview and categorization
│
├── native/                        # Desktop/Linux examples (clean ROS API)
│   ├── rs-talker/
│   ├── rs-listener/
│   ├── rs-service-client/
│   ├── rs-service-server/
│   ├── rs-action-client/
│   ├── rs-action-server/
│   ├── c-talker/
│   ├── c-listener/
│   ├── cpp-talker/
│   └── cpp-listener/
│
├── qemu/                          # QEMU bare-metal (uses bsp-qemu)
│   ├── rs-talker/
│   ├── rs-listener/
│   └── rs-test/
│
├── stm32f4/                       # STM32F4 (uses bsp-stm32f4)
│   ├── rs-talker/
│   └── rs-listener/
│
├── zephyr/                        # Zephyr RTOS (uses bsp-zephyr)
│   ├── c-talker/
│   ├── c-listener/
│   ├── rs-talker/
│   └── rs-listener/
│
└── platform-integration/          # Low-level reference implementations
    ├── README.md                  # Explains these are for BSP developers
    ├── qemu-smoltcp-bridge/       # Current qemu-rs-common
    ├── stm32f4-smoltcp/           # Current stm32f4-rs-smoltcp
    ├── stm32f4-rtic/              # Current stm32f4-rs-rtic
    └── stm32f4-polling/           # Current stm32f4-rs-polling
```

## Implementation Plan

### Phase 14.1: `nano-ros-bsp-qemu` (Priority: High)

**Status**: Complete ✓

**Tasks**:
1. [x] Create crate structure (`packages/bsp/nano-ros-bsp-qemu/`)
2. [x] Implement `run_node()` entry point (wraps `nano-ros-baremetal`)
3. [x] Implement `Config` builder with sensible defaults
4. [x] Add `Config::listener()` preset for subscriber nodes
5. [x] Create `qemu-bsp-talker` example demonstrating simplified API
6. [x] Create `qemu-bsp-listener` example demonstrating subscriber API
7. [x] Refactor `qemu-rs-talker` to use BSP (~90 lines → cleaner API)
8. [x] Refactor `qemu-rs-listener` to use BSP (~100 lines → cleaner API)
9. [x] Move `nano-ros-baremetal` internals to BSP (absorbed completely)
10. [x] Move `qemu-rs-common` bridge to internal module (absorbed completely)
11. [x] Add IP auto-configuration (link-local fallback)
12. [x] Update Docker Compose tests
13. [x] Delete `nano-ros-baremetal` (fully merged into bsp-qemu)

**Acceptance Criteria**:
- [x] New `qemu-bsp-talker` example demonstrates BSP API
- [x] New `qemu-bsp-listener` example demonstrates subscriber API
- [x] `qemu-rs-talker` refactored to use BSP (91 lines with message formatting)
- [x] `qemu-rs-listener` refactored to use BSP (98 lines with callback handling)
- [x] `just test-rust-qemu-baremetal` and `just test-rust-qemu-baremetal-bsp` available
- [x] BSP hides smoltcp/zenoh references from examples
- [x] Examples use `Config::default()` or `Config::listener()` presets
- [x] Link-local IP auto-configuration (`Config::link_local()`) available

### Phase 14.2: `nano-ros-bsp-zephyr` (Priority: High)

**Status**: Complete ✓

**Tasks**:
1. [x] Create Zephyr module structure (`packages/bsp/nano-ros-bsp-zephyr/`)
2. [x] Create Kconfig with zenoh locator, domain ID, init delay options
3. [x] Implement `nano_ros_bsp_init()` and `nano_ros_bsp_init_with_locator()`
4. [x] Implement `nano_ros_bsp_create_node()` and `_with_domain()`
5. [x] Implement `nano_ros_bsp_create_publisher()` and `nano_ros_bsp_publish()`
6. [x] Implement `nano_ros_bsp_create_subscriber()` with wildcard keyexpr
7. [x] Implement `nano_ros_bsp_spin_once()` and `nano_ros_bsp_spin()`
8. [x] Refactor `zephyr-c-talker` to use BSP (131 → 106 lines)
9. [x] Refactor `zephyr-c-listener` to use BSP (135 → 111 lines)
10. [x] Refactor `zephyr-rs-talker` and `zephyr-rs-listener` to use BSP FFI
11. [x] Update Zephyr test infrastructure

**Acceptance Criteria**:
- [x] `zephyr-c-talker` uses only `nano_ros_bsp_*` functions
- [x] `zephyr-c-listener` uses only `nano_ros_bsp_*` functions
- [x] No direct `zenoh_shim_*` calls in C Zephyr examples
- [x] Test infrastructure updated for new directory structure
- [x] Kconfig options documented for future integration

### Phase 14.3: `nano-ros-bsp-stm32f4` (Priority: Medium)

**Status**: Complete ✓

**Tasks**:
1. [x] Create crate structure (`packages/bsp/nano-ros-bsp-stm32f4/`)
2. [x] Implement STM32F4 peripheral initialization (clocks, DWT)
3. [x] Implement PHY auto-detection (scans common addresses, detects LAN8742A/DP83848/KSZ8081/LAN8720)
4. [x] Add pin configurations for common boards (NucleoF429ZI, DiscoveryF407)
5. [x] Implement `run_node()` entry point
6. [x] Create new `stm32f4-bsp-talker` example
7. [x] Move existing examples to `platform-integration/`
8. [ ] TODO: RTIC integration module

**Acceptance Criteria**:
- [x] New `stm32f4-bsp-talker` example ~75 lines (vs ~400+ in polling example)
- [x] Builds for `thumbv7em-none-eabihf`
- [ ] Works on STM32F407 Discovery or Nucleo-F429ZI (requires hardware test)

### Phase 14.4: Example Reorganization (Priority: High)

**Status**: Complete ✓

**Tasks**:
1. [x] Create new directory structure
2. [x] Move native examples to `examples/native/`
3. [x] Move QEMU examples to `examples/qemu/`
4. [x] Move STM32F4 examples to `examples/stm32f4/`
5. [x] Move Zephyr examples to `examples/zephyr/`
6. [x] Move low-level examples to `packages/reference/`
7. [x] Create `examples/README.md` with categorization
8. [x] Create `packages/reference/README.md`
9. [x] Update `justfile` recipes for new paths
10. [x] Update test scripts (`c-tests.sh`, `c-msg-gen-tests.sh`, etc.)
11. [x] Update `zephyr/setup.sh` and `setup-network.sh` paths
12. [x] Update root `Cargo.toml` exclude list
13. [x] Update CLAUDE.md workspace structure

**Acceptance Criteria**:
- [x] All examples organized by platform
- [x] README explains example categories
- [x] `justfile` recipes reference new paths
- [x] No broken paths in scripts

## Dependencies

```
Phase 14.1 (bsp-qemu) ─────┬─────► Phase 14.4 (reorganize)
                           │
Phase 14.2 (bsp-zephyr) ───┤
                           │
Phase 14.3 (bsp-stm32f4) ──┘
```

Phases 14.1-14.3 can proceed in parallel. Phase 14.4 depends on all BSP libraries being ready.

## Success Metrics

| Metric                           | Before | After   |
|----------------------------------|--------|---------|
| QEMU talker lines                | ~200   | <30     |
| Zephyr C talker lines            | ~130   | <30     |
| Direct zenoh calls in examples   | 15+    | 0       |
| Direct smoltcp refs in examples  | 20+    | 0       |
| Time to first publish (new user) | Hours  | Minutes |

## Risks and Mitigations

| Risk                                | Impact | Mitigation                                  |
|-------------------------------------|--------|---------------------------------------------|
| Breaking existing users             | High   | No external users; `nano-ros-baremetal` fully merged into BSP |
| Platform-specific bugs hidden       | Medium | Comprehensive integration tests             |
| Over-abstraction limits flexibility | Medium | Provide `_with_config()` variants           |
| Maintenance burden of multiple BSPs | Medium | Share code via internal traits              |

## Future Extensions

- `nano-ros-bsp-esp32`: ESP32 with WiFi → **[Phase 22](phase-22-esp32-support.md)**
- Precompiled Arduino library → **[Phase 23](phase-23-arduino-precompiled.md)**
- `nano-ros-bsp-pico-w`: Raspberry Pi Pico W with WiFi → **[Phase 24](phase-24-rpi-pico-w.md)**
- `nano-ros-bsp-nrf52`: Nordic nRF52 with BLE transport
- mDNS-based zenoh router discovery
- DHCP client for automatic IP configuration

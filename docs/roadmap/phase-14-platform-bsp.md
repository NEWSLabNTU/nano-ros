# Phase 14: Platform BSP Libraries

## Overview

Create Board Support Package (BSP) libraries that hide platform-specific details (smoltcp, Ethernet drivers, network configuration) behind simple, high-level APIs. Users should be able to write nano-ros applications without understanding the underlying network stack.

**Status**: Planning

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
crates/nano-ros-bsp-qemu/
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
crates/nano-ros-bsp-stm32f4/
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
crates/nano-ros-bsp-zephyr/
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

**Tasks**:
1. Create crate structure
2. Move `nano-ros-baremetal` internals to `platform.rs`
3. Move `qemu-rs-common` bridge to internal module
4. Implement `run_node()` entry point
5. Implement `NodeConfig` builder
6. Add IP auto-configuration (link-local fallback)
7. Refactor `qemu-rs-talker` and `qemu-rs-listener`
8. Update Docker Compose tests
9. Deprecate `nano-ros-baremetal` (re-export from bsp-qemu for compatibility)

**Acceptance Criteria**:
- [ ] `qemu-rs-talker` reduced to <30 lines
- [ ] `qemu-rs-listener` reduced to <40 lines
- [ ] `just docker-qemu-test` passes
- [ ] No direct smoltcp/zenoh references in examples

### Phase 14.2: `nano-ros-bsp-zephyr` (Priority: High)

**Tasks**:
1. Create Zephyr module structure
2. Implement `nano_ros_bsp_init()` with Kconfig integration
3. Implement `nano_ros_bsp_create_node()`
4. Implement `nano_ros_bsp_spin_once()`
5. Add Kconfig options for zenoh locator, domain ID
6. Refactor `zephyr-c-talker` and `zephyr-c-listener`
7. Refactor `zephyr-rs-talker` and `zephyr-rs-listener` (if applicable)
8. Update Zephyr test infrastructure

**Acceptance Criteria**:
- [ ] `zephyr-c-talker` uses only `nano_ros_*` functions
- [ ] No direct `zenoh_shim_*` calls in Zephyr examples
- [ ] `just test-zephyr` passes
- [ ] Configuration via Kconfig works

### Phase 14.3: `nano-ros-bsp-stm32f4` (Priority: Medium)

**Tasks**:
1. Create crate structure
2. Implement STM32F4 peripheral initialization
3. Implement PHY auto-detection
4. Add pin configurations for common boards
5. Implement `run_node()` entry point
6. Create new `stm32f4-rs-talker` example
7. Move existing examples to `platform-integration/`
8. Optional: RTIC integration module

**Acceptance Criteria**:
- [ ] New `stm32f4/rs-talker` example <40 lines
- [ ] Builds for `thumbv7em-none-eabihf`
- [ ] Works on STM32F407 Discovery or Nucleo-F429ZI

### Phase 14.4: Example Reorganization (Priority: High)

**Tasks**:
1. Create new directory structure
2. Move native examples to `examples/native/`
3. Move QEMU examples to `examples/qemu/`
4. Move low-level examples to `examples/platform-integration/`
5. Create `examples/README.md` with categorization
6. Update `justfile` recipes for new paths
7. Update CI/CD pipelines
8. Update CLAUDE.md workspace structure

**Acceptance Criteria**:
- [ ] All examples organized by platform
- [ ] README explains example categories
- [ ] `just test-*` commands work with new paths
- [ ] No broken links in documentation

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

| Metric | Before | After |
|--------|--------|-------|
| QEMU talker lines | ~200 | <30 |
| Zephyr C talker lines | ~130 | <30 |
| Direct zenoh calls in examples | 15+ | 0 |
| Direct smoltcp refs in examples | 20+ | 0 |
| Time to first publish (new user) | Hours | Minutes |

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking existing users | High | Keep `nano-ros-baremetal` as re-export shim |
| Platform-specific bugs hidden | Medium | Comprehensive integration tests |
| Over-abstraction limits flexibility | Medium | Provide `_with_config()` variants |
| Maintenance burden of multiple BSPs | Medium | Share code via internal traits |

## Future Extensions

- `nano-ros-bsp-esp32`: ESP32 with WiFi
- `nano-ros-bsp-rp2040`: Raspberry Pi Pico with Ethernet hat
- `nano-ros-bsp-nrf52`: Nordic nRF52 with BLE transport
- mDNS-based zenoh router discovery
- DHCP client for automatic IP configuration

# Phase 22: ESP32-C3 Platform Support

**Goal**: Add native Rust support for ESP32-C3 (RISC-V) using esp-hal + esp-wifi, enabling WiFi-connected nano-ros nodes on the most popular IoT chip family.

**Status**: Not Started
**Priority**: High
**Depends on**: Phase 14 (Platform BSP)

## Overview

The ESP32-C3 is a RISC-V-based WiFi/BLE SoC from Espressif. It is the most accessible WiFi-capable microcontroller for nano-ros because:

- **Upstream Rust**: RISC-V target (`riscv32imc-unknown-none-elf`) uses standard `rustc` — no forked compiler
- **WiFi built-in**: No external module or shield needed
- **Affordable**: ~$8 for ESP32-C3-DevKitC, ~$20 for Arduino Nano ESP32 (S3)
- **Huge community**: ESP32 is the most popular IoT chip, massive ecosystem
- **smoltcp integration**: `esp-wifi` uses smoltcp internally — matches nano-ros's existing `platform_smoltcp` backend

### Target Hardware

| Board              | Chip     | Arch   | RAM   | Flash | WiFi           | Price |
|--------------------|----------|--------|-------|-------|----------------|-------|
| ESP32-C3-DevKitC   | ESP32-C3 | RISC-V | 400KB | 4MB   | 802.11 b/g/n   | ~$8   |
| Arduino Nano ESP32 | ESP32-S3 | Xtensa | 512KB | 16MB  | 802.11 b/g/n   | ~$20  |
| ESP32-C6-DevKitC   | ESP32-C6 | RISC-V | 512KB | 4MB   | WiFi 6 + BLE 5 | ~$12  |

ESP32-C3 is the primary target. ESP32-S3 (Xtensa) requires the Espressif forked compiler and is a secondary target.

### Resource Budget

| Component              | Size        | ESP32-C3 Available |
|------------------------|-------------|--------------------|
| zenoh-pico heap        | ~12 KB      | 400 KB RAM         |
| zenoh-pico code        | ~80 KB      | 4 MB flash         |
| smoltcp buffers        | ~8 KB       | 400 KB RAM         |
| WiFi driver (esp-wifi) | ~60 KB      | 4 MB flash         |
| Application + nano-ros | ~40 KB      | 4 MB flash         |
| **Total RAM**          | **~80 KB**  | **400 KB**         |
| **Total Flash**        | **~180 KB** | **4 MB**           |

Fits comfortably. ESP32-C3 has 20x more flash and 5x more RAM than needed.

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
│                     nano-ros-bsp-esp32                           │
│                                                                  │
│  - WiFi initialization (esp-wifi + smoltcp)                     │
│  - Zenoh session setup via platform_smoltcp backend             │
│  - run_node() / run_node_with_config() entry points             │
│  - DHCP or static IP configuration                              │
│  - Hardware RNG for zenoh-pico                                  │
└─────────────────────────────┬───────────────────────────────────┘
                              │ (hidden from users)
┌─────────────────────────────▼───────────────────────────────────┐
│  esp-hal │ esp-wifi │ smoltcp │ zenoh-pico-shim-sys             │
│  (HAL)   │ (WiFi)   │ (TCP)   │ (zenoh-pico + C shim)          │
└─────────────────────────────────────────────────────────────────┘
```

### Key Integration Point

`esp-wifi` already uses smoltcp internally for its network stack. nano-ros has an existing `platform_smoltcp` backend in `zenoh-pico-shim-sys/c/platform_smoltcp/`. The ESP32 BSP bridges these two by:

1. Initializing WiFi via `esp-wifi` to get a connected smoltcp interface
2. Passing that interface to `zenoh-pico-shim-sys` platform_smoltcp functions
3. Polling both WiFi and zenoh-pico in a unified loop

### Comparison with QEMU BSP

| Aspect | bsp-qemu | bsp-esp32 |
|--------|----------|-----------|
| Network | LAN9118 Ethernet (wired) | ESP32 WiFi (wireless) |
| smoltcp integration | Via `lan9118-smoltcp` driver | Via `esp-wifi` WiFi driver |
| Clock source | DWT cycle counter | ESP32 hardware timer |
| RNG | Software PRNG (seeded) | Hardware RNG peripheral |
| Debug output | Semihosting (`hprintln!`) | UART (`esp_println!`) |
| Entry point | `#[entry]` (cortex-m-rt) | `#[entry]` (esp-hal) |

## Target API

### Simple Example (WiFi Talker)

```rust
#![no_std]
#![no_main]

use nano_ros_bsp_esp32::prelude::*;

#[entry]
fn main() -> ! {
    // Connect to WiFi + zenoh router, create node
    run_node(
        WifiConfig::new("MyNetwork", "password123"),
        |node| {
            let publisher = node.create_publisher("demo/esp32")?;

            for i in 0u32..100 {
                node.spin_once(1000);
                publisher.publish(&i.to_le_bytes())?;
                esp_println::println!("Published: {}", i);
            }
            Ok(())
        },
    )
}
```

### With Configuration

```rust
run_node_with_config(
    WifiConfig::new("MyNetwork", "password123"),
    NodeConfig::new()
        .zenoh_locator("tcp/192.168.1.1:7447")
        .node_name("esp32_sensor")
        .ip_mode(IpMode::Dhcp),  // or IpMode::Static([192,168,1,50])
    |node| { ... },
)
```

### Listener Example

```rust
#![no_std]
#![no_main]

use nano_ros_bsp_esp32::prelude::*;

#[entry]
fn main() -> ! {
    run_node(
        WifiConfig::new("MyNetwork", "password123"),
        |node| {
            node.create_subscriber("demo/esp32", |data: &[u8]| {
                esp_println::println!("Received: {:?}", data);
            })?;

            loop {
                node.spin_once(100);
            }
        },
    )
}
```

## Implementation Plan

### 22.1: Development Environment Setup

**Status**: Not Started

**Tasks**:
1. [ ] Add `riscv32imc-unknown-none-elf` target via rustup
2. [ ] Install `espflash` and `espmonitor` tools
3. [ ] Create `examples/esp32/` directory structure
4. [ ] Verify bare `esp-hal` blink example compiles and runs on ESP32-C3
5. [ ] Verify `esp-wifi` connects to WiFi and gets DHCP address
6. [ ] Document ESP32-C3 development setup in `docs/esp32-setup.md`

**Acceptance Criteria**:
- [ ] ESP32-C3 blink example runs via `espflash`
- [ ] WiFi connects and DHCP works
- [ ] Setup documented

### 22.2: Cross-Compile zenoh-pico for RISC-V

**Status**: Not Started

**Tasks**:
1. [ ] Create `scripts/build-zenoh-pico-riscv.sh` (similar to `build-zenoh-pico-arm.sh`)
2. [ ] Configure CMake cross-compilation for `riscv32imc-unknown-none-elf`
3. [ ] Set zenoh-pico features: `Z_FEATURE_MULTI_THREAD=0`, `Z_FEATURE_LINK_TCP=1`, `Z_FEATURE_LINK_SERIAL=0`
4. [ ] Build `libzenohpico.a` for RISC-V
5. [ ] Add `just build-zenoh-pico-riscv` recipe
6. [ ] Verify library links with esp-hal binary

**Acceptance Criteria**:
- [ ] `build/esp32-zenoh-pico/libzenohpico.a` built for RISC-V
- [ ] Library links cleanly with esp-hal test binary
- [ ] Build script documented

### 22.3: Create `nano-ros-bsp-esp32` Crate

**Status**: Not Started

**Tasks**:
1. [ ] Create crate structure:
   ```
   crates/nano-ros-bsp-esp32/
   ├── Cargo.toml
   ├── src/
   │   ├── lib.rs           # Public API, prelude
   │   ├── wifi.rs           # WiFi initialization (esp-wifi)
   │   ├── config.rs         # WifiConfig, NodeConfig, IpMode
   │   ├── network.rs        # smoltcp bridge (WiFi → zenoh-pico)
   │   ├── clock.rs          # ESP32 hardware timer for smoltcp timestamps
   │   ├── rng.rs            # Hardware RNG for zenoh-pico
   │   ├── node.rs           # run_node() / run_node_with_config()
   │   ├── publisher.rs      # Publisher wrapper
   │   ├── subscriber.rs     # Subscriber wrapper
   │   └── error.rs          # Error types
   ```
2. [ ] Implement `WifiConfig` and `NodeConfig` types
3. [ ] Implement WiFi initialization using `esp-wifi`
4. [ ] Bridge esp-wifi's smoltcp interface to zenoh-pico-shim-sys platform_smoltcp
5. [ ] Implement `run_node()` entry point with WiFi + zenoh setup
6. [ ] Implement hardware RNG callbacks for zenoh-pico `z_random_*`
7. [ ] Implement hardware timer for `z_clock_*` functions
8. [ ] Add DHCP support (esp-wifi provides this via smoltcp)
9. [ ] Add `Cargo.toml` with dependencies:
   ```toml
   [dependencies]
   esp-hal = { version = "1.0", features = ["esp32c3"] }
   esp-wifi = { version = "0.13", features = ["esp32c3", "wifi", "smoltcp"] }
   esp-alloc = "0.7"
   smoltcp = { version = "0.12", default-features = false, features = [
       "medium-ethernet", "proto-ipv4", "socket-tcp", "proto-dhcpv4",
   ] }
   zenoh-pico-shim-sys = { path = "../zenoh-pico-shim-sys", features = ["smoltcp"] }
   ```

**Acceptance Criteria**:
- [ ] Crate compiles for `riscv32imc-unknown-none-elf`
- [ ] WiFi connects and gets IP address
- [ ] zenoh-pico session opens to router over WiFi
- [ ] `run_node()` API works end-to-end

### 22.4: Create ESP32 Examples

**Status**: Not Started

**Tasks**:
1. [ ] Create `examples/esp32/rs-talker/` — WiFi publisher
2. [ ] Create `examples/esp32/rs-listener/` — WiFi subscriber
3. [ ] Create `examples/esp32/bsp-talker/` — Simplified BSP publisher
4. [ ] Add `just build-examples-esp32` recipe
5. [ ] Add `just flash-esp32-talker` recipe (uses `espflash`)
6. [ ] Create `examples/esp32/README.md` with setup instructions

**Acceptance Criteria**:
- [ ] Talker publishes messages over WiFi to zenohd
- [ ] Listener receives messages over WiFi from zenohd
- [ ] BSP example demonstrates simplified API (<30 lines)
- [ ] Examples documented

### 22.5: Integration Testing

**Status**: Not Started

**Tasks**:
1. [ ] Test ESP32 talker → native listener (via zenohd)
2. [ ] Test native talker → ESP32 listener (via zenohd)
3. [ ] Test ESP32 ↔ ESP32 communication (two boards)
4. [ ] Test ESP32 ↔ ROS 2 interop (via rmw_zenoh)
5. [ ] Measure WiFi latency and throughput
6. [ ] Measure power consumption (active + sleep modes)
7. [ ] Add QEMU ESP32-C3 testing (if available in espflash/wokwi)
8. [ ] Document test results in `docs/esp32-performance.md`

**Acceptance Criteria**:
- [ ] Bidirectional pub/sub works over WiFi
- [ ] ROS 2 interop verified
- [ ] Performance documented

### 22.6: Documentation and CI

**Status**: Not Started

**Tasks**:
1. [ ] Create `docs/esp32-setup.md` — Development environment setup
2. [ ] Create `docs/esp32-performance.md` — Benchmarks and measurements
3. [ ] Update `CLAUDE.md` — Add ESP32 to workspace structure and build commands
4. [ ] Update `docs/micro-ros-comparison.md` — Add ESP32 to platform support table
5. [ ] Update Phase 14 roadmap — Reference ESP32 BSP
6. [ ] Add clippy check for ESP32 target to `just quality` (optional)

**Acceptance Criteria**:
- [ ] Setup guide enables new users to get running in <30 minutes
- [ ] All documentation updated
- [ ] CI checks ESP32 build (if feasible)

## Dependencies

```
22.1 (Dev setup) ─────────────────────────────────────────────┐
                                                               │
22.2 (Cross-compile zenoh-pico) ──────────────────────────────┤
                                                               │
                                                               ▼
22.3 (BSP crate) ─────────────────────────────────────────────┤
                                                               │
                                                               ▼
22.4 (Examples) ──────────────────────────────────────────────┤
                                                               │
                                                               ▼
22.5 (Integration testing) ───────────────────────────────────┤
                                                               │
                                                               ▼
22.6 (Documentation) ─────────────────────────────────────────┘
```

Phases 22.1 and 22.2 can proceed in parallel. All subsequent phases are sequential.

## Risks and Mitigations

| Risk                                    | Impact | Mitigation                                            |
|-----------------------------------------|--------|-------------------------------------------------------|
| esp-wifi API instability                | Medium | Pin exact versions, test before upgrading             |
| WiFi reliability (disconnects, retries) | Medium | Add reconnection logic in BSP                         |
| zenoh-pico RISC-V alignment issues      | Low    | zenoh-pico is well-tested on RISC-V (ESP-IDF support) |
| Flash size constraints on C3            | Low    | 4MB is 20x our needs, not a concern                   |
| esp-wifi smoltcp version mismatch       | Medium | Pin smoltcp version to match esp-wifi's dependency    |
| No QEMU for ESP32 testing               | Medium | Use Wokwi simulator or require physical hardware      |

## ESP32 Chip Comparison

For future expansion beyond C3:

| Feature       | ESP32-C3     | ESP32-S3     | ESP32-C6 |
|---------------|--------------|--------------|----------|
| Arch          | RISC-V       | Xtensa       | RISC-V   |
| Upstream Rust | Yes          | No (forked)  | Yes      |
| WiFi          | 802.11 b/g/n | 802.11 b/g/n | WiFi 6   |
| BLE           | 5.0          | 5.0          | 5.3      |
| RAM           | 400 KB       | 512 KB       | 512 KB   |
| Flash         | 4 MB         | up to 16 MB  | 4 MB     |
| USB           | No           | USB-OTG      | No       |
| Price         | ~$8          | ~$12         | ~$12     |
| Priority      | **Primary**  | Secondary    | Future   |

## Future Extensions

- ESP32-S3 support (requires Xtensa compiler)
- ESP32-C6 support (WiFi 6, same RISC-V toolchain as C3)
- BLE transport (zenoh-pico over BLE serial link)
- Deep sleep + wake-on-message patterns
- OTA firmware update via zenoh
- ESP-IDF integration (alternative to bare-metal esp-hal)

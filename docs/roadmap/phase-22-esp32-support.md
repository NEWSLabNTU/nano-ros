# Phase 22: ESP32-C3 Platform Support

**Goal**: Add native Rust support for ESP32-C3 (RISC-V) using esp-hal + esp-wifi, enabling WiFi-connected nano-ros nodes on the most popular IoT chip family.

**Status**: In Progress (22.1–22.4 complete, 22.5a–d complete, 22.6 deferred)
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

| Aspect              | bsp-qemu                     | bsp-esp32                  |
|---------------------|------------------------------|----------------------------|
| Network             | LAN9118 Ethernet (wired)     | ESP32 WiFi (wireless)      |
| smoltcp integration | Via `lan9118-smoltcp` driver | Via `esp-wifi` WiFi driver |
| Clock source        | DWT cycle counter            | ESP32 hardware timer       |
| RNG                 | Software PRNG (seeded)       | Hardware RNG peripheral    |
| Debug output        | Semihosting (`hprintln!`)    | UART (`esp_println!`)      |
| Entry point         | `#[entry]` (cortex-m-rt)     | `#[entry]` (esp-hal)       |

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

**Status**: Complete (compile-verified; flash/WiFi require hardware)

**Tasks**:
1. [x] Add `riscv32imc-unknown-none-elf` target via rustup (in `just setup`)
2. [x] Install `espflash` tool (in `just setup`)
3. [x] Create `examples/esp32/` directory structure
4. [x] Verify bare `esp-hal` blink example compiles for ESP32-C3 (`examples/esp32/hello-world/`)
5. [ ] Verify `esp-wifi` connects to WiFi and gets DHCP address (requires hardware)
6. [x] Document ESP32-C3 development setup in `docs/guides/esp32-setup.md`

**Acceptance Criteria**:
- [x] ESP32-C3 hello-world example compiles to RISC-V ELF
- [ ] ESP32-C3 blink example runs via `espflash` (requires hardware)
- [ ] WiFi connects and DHCP works (requires hardware)
- [x] Setup documented

**Implementation Notes**:
- esp-hal 1.0.0 requires nightly Rust (`build-std = ["core"]`)
- `unstable` feature needed on esp-hal for `delay` module
- ESP32 examples excluded from workspace (standalone packages)
- picolibc-riscv64-unknown-elf needed for C library headers in zenoh-pico build

### 22.2: Cross-Compile zenoh-pico for RISC-V

**Status**: Complete

**Tasks**:
1. [x] Create `scripts/esp32/build-zenoh-pico.sh` (based on `scripts/qemu/build-zenoh-pico.sh`)
2. [x] Direct GCC cross-compilation for RISC-V RV32IMC (no CMake needed)
3. [x] Set zenoh-pico features: `Z_FEATURE_MULTI_THREAD=0`, `Z_FEATURE_LINK_TCP=1`, `Z_FEATURE_LINK_SERIAL=0`
4. [x] Build `libzenohpico.a` for RISC-V (120 sources, 6.5 MiB)
5. [x] Add `just build-zenoh-pico-riscv` and `just clean-zenoh-pico-riscv` recipes
6. [x] Add RISC-V cross-compilation flags in `zenoh-pico-shim-sys/build.rs`
7. [x] Verify library links with esp-hal binary (verified in Phase 22.3/22.4)

**Acceptance Criteria**:
- [x] `build/esp32-zenoh-pico/libzenohpico.a` built for RISC-V (`elf32-littleriscv`)
- [x] Library links cleanly with esp-hal test binary (Phase 22.3)
- [x] Build script documented in `docs/guides/esp32-setup.md`

**Implementation Notes**:
- Toolchain auto-detection: `riscv64-unknown-elf-gcc` or `riscv32-esp-elf-gcc` (ESP-IDF)
- picolibc specs auto-detected for system GCC headers
- Same source file set and defines as the ARM build (platform_smoltcp backend)

### 22.3: Create `nano-ros-bsp-esp32` Crate

**Status**: Complete (compile-verified; WiFi/zenoh require hardware)

**Tasks**:
1. [x] Create crate structure:
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
2. [x] Implement `WifiConfig` and `NodeConfig` types
3. [x] Implement WiFi initialization using `esp-radio`
4. [x] Bridge esp-radio's smoltcp interface to zenoh-pico-shim-sys platform_smoltcp
5. [x] Implement `run_node()` entry point with WiFi + zenoh setup
6. [x] Implement hardware RNG callbacks for zenoh-pico `z_random_*`
7. [x] Implement hardware timer for `z_clock_*` functions
8. [x] Add DHCP support (via smoltcp dhcpv4 socket)
9. [x] Add `Cargo.toml` with dependencies:
   ```toml
   [dependencies]
   esp-hal = { version = "~1.0.0", features = ["esp32c3", "unstable"] }
   esp-backtrace = { version = "~0.18.0", features = ["esp32c3", "panic-handler", "println"] }
   esp-bootloader-esp-idf = { version = "~0.4.0", features = ["esp32c3"] }
   esp-println = { version = "~0.16.0", features = ["esp32c3"] }
   esp-alloc = { version = "~0.9.0" }
   esp-radio = { version = "~0.17.0", features = ["esp32c3", "wifi"] }
   smoltcp = { version = "0.12", default-features = false, features = [
       "medium-ethernet", "proto-ipv4", "socket-tcp", "proto-dhcpv4",
   ] }
   zenoh-pico-shim-sys = { path = "../zenoh-pico-shim-sys", features = ["smoltcp"] }
   ```
   Note: `esp-wifi` has been split into `esp-radio` (WiFi/BLE) + `esp-rtos` (task scheduler) in the 1.0 ecosystem.

**Acceptance Criteria**:
- [x] Crate compiles for `riscv32imc-unknown-none-elf`
- [ ] WiFi connects and gets IP address (requires hardware)
- [ ] zenoh-pico session opens to router over WiFi (requires hardware)
- [ ] `run_node()` API works end-to-end (requires hardware)

### 22.4: Create ESP32 Examples

**Status**: Complete (compile-verified; runtime requires hardware)

**Tasks**:
1. [ ] Create `examples/esp32/rs-talker/` — WiFi publisher (deferred to 22.5)
2. [ ] Create `examples/esp32/rs-listener/` — WiFi subscriber (deferred to 22.5)
3. [x] Create `examples/esp32/bsp-talker/` — Simplified BSP publisher
4. [x] Create `examples/esp32/bsp-listener/` — Simplified BSP subscriber
5. [x] Add `just build-examples-esp32` recipe
6. [ ] Add `just flash-esp32-talker` recipe (uses `espflash`)
7. [ ] Create `examples/esp32/README.md` with setup instructions

**Acceptance Criteria**:
- [ ] Talker publishes messages over WiFi to zenohd (requires hardware)
- [ ] Listener receives messages over WiFi from zenohd (requires hardware)
- [x] BSP example demonstrates simplified API (<30 lines)
- [ ] Examples documented

### 22.5: QEMU ESP32-C3 Integration Testing (OpenETH)

**Status**: Complete (22.5a–d all done)

**Goal**: Run ESP32-C3 interop tests in QEMU using the OpenCores Ethernet MAC (OpenETH), eliminating the need for physical hardware or WiFi. Espressif's QEMU fork (`qemu-system-riscv32 -M esp32c3`) emulates OpenETH at `0x600CD000` with TAP networking, giving the emulated ESP32-C3 full IP connectivity to zenohd.

**Architecture**:
```
┌──────────────────┐         ┌─────────┐         ┌──────────────────┐
│ QEMU ESP32-C3    │  TAP    │ zenohd  │  TAP    │ QEMU ESP32-C3    │
│  bsp-talker      │◄───────►│ (host)  │◄───────►│  bsp-listener    │
│                  │  eth    │         │  eth    │                  │
│ OpenETH MAC      │         │         │         │ OpenETH MAC      │
│ smoltcp TCP/IP   │         │         │         │ smoltcp TCP/IP   │
│ zenoh-pico       │         │         │         │ zenoh-pico       │
└──────────────────┘         └─────────┘         └──────────────────┘
```

Same pattern as QEMU ARM (LAN9118) Docker E2E tests, but RISC-V with OpenETH.

#### 22.5a: OpenETH smoltcp Driver

**Tasks**:
1. [x] Create `crates/openeth-smoltcp/` crate (like `crates/lan9118-smoltcp/`)
2. [x] Define OpenETH register constants (`MODER`, `INT_SOURCE`, `TX_BD_NUM`, etc. — ~15 registers at base `0x600CD000`)
3. [x] Define TX/RX buffer descriptor structs (8 bytes each, at base + `0x400`)
4. [x] Implement init: reset (`MODER.RST`), configure descriptors, allocate static DMA buffers (1600 bytes each), set MAC address, enable TX/RX
5. [x] Implement `smoltcp::phy::Device` trait (polling: scan RX descriptors for `e==0`, TX descriptors for `rd==0`)
6. [x] Unit tests for register definitions and descriptor layout

**Reference**: ESP-IDF C driver at `components/esp_eth/src/esp_eth_mac_openeth.c` + `openeth.h` ([GitHub](https://github.com/espressif/esp-idf/blob/master/components/esp_eth/src/openeth/openeth.h))

**Acceptance Criteria**:
- [x] Crate compiles for `riscv32imc-unknown-none-elf`
- [x] Register layout matches ESP-IDF reference
- [x] `smoltcp::phy::Device` trait implemented with RxToken/TxToken

**Notes**:
- OpenETH is simpler than LAN9118: ~15 registers, DMA descriptor ring (vs FIFO), no PHY negotiation needed in QEMU
- Estimated ~300-400 lines of Rust (LAN9118 driver is ~670 lines)
- DMA buffers must be in ESP32-C3 DRAM (`0x3FC80000+`)
- QEMU transmits instantly when `rd=1` is set — no TX completion wait needed

#### 22.5b: ESP32-C3 QEMU BSP Variant

**Tasks**:
1. [x] Create `crates/nano-ros-bsp-esp32-qemu/` (separate crate, no WiFi deps)
2. [x] `node.rs`: Use OpenETH + smoltcp instead of WiFi (`esp-radio`) — skip WiFi init, DHCP, `esp-rtos`
3. [x] `bridge.rs`: Reuse smoltcp↔zenoh-pico bridge (copied from WiFi BSP)
4. [x] `clock.rs`: Reuse `esp_hal::time::Instant` (works in QEMU with `-icount 3`)
5. [x] Static IP configuration (QEMU TAP network uses `192.0.3.x`)
6. [x] Create `examples/esp32/qemu-talker/` and `examples/esp32/qemu-listener/`

**Acceptance Criteria**:
- [x] BSP compiles for `riscv32imc-unknown-none-elf` without `esp-radio`/`esp-rtos` dependencies
- [x] Examples compile and produce flash images

**Notes**:
- May still need `esp-hal` for basic init (clocks, heap allocator, UART output) and `esp-alloc` for heap
- Does NOT need `esp-radio`, `esp-rtos`, or WiFi — OpenETH replaces the entire WiFi stack
- Decide: separate crate vs feature flag — separate crate avoids pulling WiFi deps into QEMU builds

#### 22.5c: Espressif QEMU Tooling

**Status**: Complete

**Tasks**:
1. [x] Add script to download/build Espressif QEMU fork (`scripts/esp32/install-espressif-qemu.sh`)
2. [x] Add flash image build step: `espflash save-image --chip esp32c3 --merge` in `just build-examples-esp32-qemu`
3. [x] Add `just build-examples-esp32-qemu` recipe (build examples + create flash images)
4. [x] Add `scripts/esp32/launch-esp32c3.sh` launch script with TAP networking support
5. [x] Add `just test-qemu-esp32-basic` recipe (boot test)
6. [x] Add `qemu-system-riscv32` check to `just setup`
7. [x] Fix picolibc TLS errno crash (shadow `errno.h` in zenoh-pico build)
8. [x] Verify full boot → OpenETH init → zenoh connect → publish → shutdown

**Acceptance Criteria**:
- [x] Espressif QEMU installs via script
- [x] Flash images generated from compiled examples
- [x] QEMU boots and shows UART output from example
- [x] Zenoh session connects over TAP networking (with zenohd 1.6.2)
- [x] Talker publishes messages end-to-end

**Notes**:
- Espressif QEMU requires `-icount 3` for instruction timing (simulates 125MHz)
- `espflash save-image --merge` required: merges bootloader + partition table + app into 4MB image
- picolibc on RISC-V declares `extern __thread int errno` (TLS via tp register); bare-metal ESP32-C3 never initializes tp → null pointer crash. Fix: shadow `errno.h` without TLS in `build/esp32-zenoh-pico/include/`
- Must use zenohd 1.6.2 (matching zenoh-pico version); system zenohd 1.7.2 is incompatible

#### 22.5d: QEMU ESP32-C3 E2E and Interop Tests

**Status**: Complete

**Goal**: Automated E2E and interop tests using QEMU ESP32-C3, same pattern as existing QEMU ARM tests in `crates/nano-ros-tests/tests/emulator.rs`.

**Tasks**:
1. [x] Add `esp32_emulator.rs` test suite in `crates/nano-ros-tests/tests/`
2. [x] Add ESP32-C3 QEMU process management (`crates/nano-ros-tests/src/esp32.rs`)
3. [x] Test: QEMU ESP32-C3 talker boots (BSP banner test, no networking)
4. [x] Test: ESP32-C3 talker → ESP32-C3 listener (two QEMU instances, via zenohd)
5. [x] Add `just test-qemu-esp32` recipe and nextest `max-threads=1` config
6. [x] Document QEMU networked test practices in `tests/README.md` and `CLAUDE.md`
7. [x] ESP32-C3 ↔ native interop — migrated to CDR Int32 on `/chatter` with `create_ros_publisher`/`create_ros_subscriber`
8. [N/A] ESP32-C3 ↔ QEMU ARM interop — deferred to Phase 26 (ARM QEMU still uses raw `demo/qemu` topic)
9. [ ] Docker Compose setup for ESP32-C3 QEMU tests (optional, for CI)

**Acceptance Criteria**:
- [x] ESP32-C3 ↔ ESP32-C3 pub/sub works in QEMU without physical hardware
- [x] Tests integrated into `just test-qemu-esp32`
- [x] ESP32-C3 ↔ native interop works (CDR Int32 on `/chatter`, both directions)

**Implementation Notes**:
- Guard functions: `require_riscv32_target()`, `require_zenoh_pico_riscv()`, `require_qemu_riscv32()`, `require_espflash()`, `require_tap_network()`
- Networking helpers: `wait_for_port_free()`, `wait_for_addr()` for bridge IP verification
- E2E ordering: listener first (tap-qemu1) → 5s stabilization → talker (tap-qemu0)
- ESP32 firmware hardcodes `tcp/192.0.3.1:7447` → must use fixed port with nextest `max-threads=1`
- Interop: ESP32 BSP has `create_ros_publisher()`/`create_ros_subscriber()` that construct ROS 2 keyexprs with `domain_id`
- CDR encoding: manual 8-byte CDR (4-byte LE header + i32) — no alloc needed
- 9 tests total: 3 detection, 2 build, 1 boot, 1 ESP32↔ESP32 E2E, 2 ESP32↔native interop

### 22.6: Hardware Integration Testing (WiFi)

**Status**: Not Started (deferred until hardware available)

**Tasks**:
1. [ ] Test ESP32 talker → native listener over WiFi (requires ESP32-C3 board)
2. [ ] Test native talker → ESP32 listener over WiFi
3. [ ] Test ESP32 ↔ ROS 2 interop over WiFi (via rmw_zenoh)
4. [ ] Measure WiFi latency and throughput
5. [ ] Measure power consumption (active + sleep modes)
6. [ ] Document test results in `docs/esp32-performance.md`

**Acceptance Criteria**:
- [ ] Bidirectional pub/sub works over WiFi
- [ ] ROS 2 interop verified
- [ ] Performance documented

**Note**: Requires physical ESP32-C3 board. QEMU covers E2E and interop testing via OpenETH (22.5d).

### 22.7: Documentation and CI

**Status**: Not Started

**Tasks**:
1. [ ] Create `docs/guides/esp32-setup.md` — Development environment setup
2. [ ] Create `docs/esp32-performance.md` — Benchmarks and measurements
3. [ ] Update `CLAUDE.md` — Add ESP32 to workspace structure and build commands
4. [ ] Update `docs/reference/micro-ros-comparison.md` — Add ESP32 to platform support table
5. [ ] Update Phase 14 roadmap — Reference ESP32 BSP
6. [ ] Add clippy check for ESP32 target to `just quality` (optional)

**Acceptance Criteria**:
- [ ] Setup guide enables new users to get running in <30 minutes
- [ ] All documentation updated
- [ ] CI checks ESP32 build (if feasible)

## Dependencies

```
22.1 (Dev setup) ──────────┐
                            ├──► 22.3 (BSP crate) ──► 22.4 (Examples)
22.2 (Cross-compile) ──────┘        │
                                    │
                                    └──► 22.5a (OpenETH driver) ✓
                                              │
                                         22.5b (QEMU BSP variant) ✓
                                              │
                                         22.5c (QEMU tooling) ✓
                                              │
                                              ▼
                                         22.5d (E2E + interop tests) ✓
                                              │
                                              ├──► 22.7 (Docs) ◄── NEXT
                                              │
                                         22.6 (HW WiFi tests) ◄── deferred (needs hardware)
```

- 22.5 is complete: all QEMU-based ESP32-C3 tests are implemented
- 22.7 (Docs) is the next focus
- 22.6 (WiFi hardware tests) deferred until physical ESP32-C3 board is available

## Risks and Mitigations

| Risk                                    | Impact | Mitigation                                            |
|-----------------------------------------|--------|-------------------------------------------------------|
| esp-hal/esp-radio API instability       | Medium | Pin versions with `~`, test before upgrading          |
| WiFi reliability (disconnects, retries) | Medium | Add reconnection logic in BSP                         |
| zenoh-pico RISC-V alignment issues      | Low    | zenoh-pico is well-tested on RISC-V (ESP-IDF support) |
| Flash size constraints on C3            | Low    | 4MB is 20x our needs, not a concern                   |
| esp-radio smoltcp version mismatch      | Medium | Pin smoltcp version to match esp-radio's dependency   |
| No QEMU for ESP32 testing               | Resolved | Espressif QEMU fork works; `scripts/esp32/install-espressif-qemu.sh` |
| Nightly Rust required for build-std     | Low    | ESP32 examples are standalone (don't affect workspace) |
| picolibc needed for RISC-V C headers    | Low    | Documented in setup; auto-detected in build script    |
| picolibc TLS errno crashes bare-metal   | Resolved | Shadow `errno.h` without TLS in zenoh-pico build     |
| zenoh version mismatch (pico vs router) | Low    | Must use zenohd 1.6.2 from submodule (`just build-zenohd`) |

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

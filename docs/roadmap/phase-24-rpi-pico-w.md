# Phase 24: Raspberry Pi Pico W Platform Support

**Goal**: Add native Rust support for Raspberry Pi Pico W (RP2040 + CYW43 WiFi) using embassy-rp or rp-hal, enabling the cheapest WiFi-capable nano-ros node (~$6).

**Status**: Not Started
**Priority**: Medium
**Depends on**: Phase 14 (Platform BSP)

## Overview

The Raspberry Pi Pico W is a $6 board with an RP2040 (dual ARM Cortex-M0+) and CYW43439 WiFi/BLE chip. It is the most affordable WiFi-capable board with excellent Rust support.

### Why Pico W

- **Cheapest WiFi board**: ~$6, widely available
- **Upstream Rust**: ARM Cortex-M0+ (`thumbv6m-none-eabi`) is a standard target
- **Excellent Rust ecosystem**: `embassy-rp` provides async HAL, `cyw43` WiFi driver
- **Education favorite**: Raspberry Pi is the most recognized brand in education
- **nano-ros ARM proven**: Existing QEMU tests run on ARM Cortex-M3 — M0+ is similar
- **Existing zenoh-pico support**: Community project `zenoh_pico_rp2040` demonstrates feasibility

### Hardware Specifications

| Feature | Value |
|---------|-------|
| MCU | RP2040 (dual Cortex-M0+ @ 133 MHz) |
| RAM | 264 KB SRAM |
| Flash | 2 MB external QSPI |
| WiFi | CYW43439 (802.11 b/g/n) |
| BLE | Bluetooth 5.2 (BLE) |
| GPIO | 26 multi-function |
| ADC | 3-channel 12-bit |
| Price | ~$6 |
| USB | 1.1 device/host |

### Resource Budget

| Component | Size | Pico W Available |
|-----------|------|------------------|
| zenoh-pico heap | ~12 KB | 264 KB RAM |
| zenoh-pico code | ~80 KB | 2 MB flash |
| smoltcp buffers | ~8 KB | 264 KB RAM |
| CYW43 WiFi firmware | ~230 KB | 2 MB flash (loaded from flash) |
| CYW43 driver RAM | ~40 KB | 264 KB RAM |
| Application + nano-ros | ~30 KB flash, ~20 KB RAM | 2 MB / 264 KB |
| **Total RAM** | **~80 KB** | **264 KB** |
| **Total Flash** | **~340 KB** | **2 MB** |

Fits with comfortable margins. RAM is tighter than ESP32-C3 but still has 3x headroom.

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
│                     nano-ros-bsp-pico-w                          │
│                                                                  │
│  - WiFi initialization (cyw43 driver)                           │
│  - smoltcp TCP stack over CYW43                                 │
│  - Zenoh session setup via platform_smoltcp backend             │
│  - run_node() entry point                                       │
│  - Embassy async executor integration                           │
└─────────────────────────────┬───────────────────────────────────┘
                              │ (hidden from users)
┌─────────────────────────────▼───────────────────────────────────┐
│  embassy-rp │ cyw43 │ smoltcp │ zenoh-pico-shim-sys             │
│  (HAL)      │ (WiFi) │ (TCP)  │ (zenoh-pico + C shim)          │
└─────────────────────────────────────────────────────────────────┘
```

### Integration Approach

The Pico W WiFi stack uses the `cyw43` crate which provides a smoltcp-compatible network device. This is the same pattern as the ESP32 BSP (Phase 22):

1. Initialize CYW43 WiFi chip and connect to AP
2. Create smoltcp interface over CYW43 network device
3. Bridge to zenoh-pico-shim-sys platform_smoltcp
4. Poll WiFi + zenoh-pico in a unified loop

### Embassy vs Polling

The Pico W ecosystem heavily uses Embassy (async Rust for embedded). Two approaches:

| Approach | Pros | Cons |
|----------|------|------|
| **Embassy async** | Idiomatic Pico W, excellent power management, community standard | Requires async runtime, more complex BSP |
| **Polling loop** | Simpler, matches existing BSP pattern | Less idiomatic, worse power efficiency |

Recommendation: Start with **polling loop** (matches bsp-qemu pattern), add Embassy integration as a feature flag later.

## Target API

### Simple Example (WiFi Talker)

```rust
#![no_std]
#![no_main]

use nano_ros_bsp_pico_w::prelude::*;

#[entry]
fn main() -> ! {
    run_node(
        WifiConfig::new("MyNetwork", "password123"),
        |node| {
            let publisher = node.create_publisher("demo/pico")?;

            for i in 0u32..100 {
                node.spin_once(1000);
                publisher.publish(&i.to_le_bytes())?;
                defmt::info!("Published: {}", i);
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
        .node_name("pico_sensor"),
    |node| { ... },
)
```

### Embassy Async (Future)

```rust
#![no_std]
#![no_main]

use nano_ros_bsp_pico_w::embassy::*;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let node = init_node(
        &spawner,
        WifiConfig::new("MyNetwork", "password123"),
    ).await?;

    let publisher = node.create_publisher("demo/pico")?;

    loop {
        node.spin_once(100).await;
        publisher.publish(&data)?;
        Timer::after(Duration::from_secs(1)).await;
    }
}
```

## Implementation Plan

### 24.1: Development Environment Setup

**Status**: Not Started

**Tasks**:
1. [ ] Add `thumbv6m-none-eabi` target via rustup (if not already present)
2. [ ] Install `probe-rs` for flashing and debugging
3. [ ] Install `elf2uf2-rs` for UF2 flashing (no debug probe needed)
4. [ ] Create `examples/pico-w/` directory structure
5. [ ] Verify bare `embassy-rp` blink example compiles and runs
6. [ ] Verify `cyw43` WiFi connection works with smoltcp
7. [ ] Document setup in `docs/pico-w-setup.md`

**Acceptance Criteria**:
- [ ] Pico W blink example runs
- [ ] WiFi connects and DHCP works
- [ ] Both UF2 and probe-rs flashing documented

### 24.2: Cross-Compile zenoh-pico for ARM Cortex-M0+

**Status**: Not Started

The existing `just build-zenoh-pico-arm` builds for Cortex-M3 (`thumbv7m-none-eabi`). Pico W needs Cortex-M0+ (`thumbv6m-none-eabi`).

**Tasks**:
1. [ ] Create `scripts/build-zenoh-pico-m0.sh` or extend existing script
2. [ ] Configure CMake for `thumbv6m-none-eabi` (Cortex-M0+ — no Thumb2 ISA)
3. [ ] Verify zenoh-pico compiles without Thumb2-only instructions
4. [ ] Set features: `Z_FEATURE_MULTI_THREAD=0`, `Z_FEATURE_LINK_TCP=1`
5. [ ] Build `libzenohpico.a` for Cortex-M0+
6. [ ] Add `just build-zenoh-pico-m0` recipe

**Acceptance Criteria**:
- [ ] `build/pico-zenoh-pico/libzenohpico.a` built for Cortex-M0+
- [ ] Library links with embassy-rp test binary
- [ ] No Thumb2 instruction faults on M0+

### 24.3: Create `nano-ros-bsp-pico-w` Crate

**Status**: Not Started

**Tasks**:
1. [ ] Create crate structure:
   ```
   crates/nano-ros-bsp-pico-w/
   ├── Cargo.toml
   ├── build.rs                 # Link prebuilt zenoh-pico
   ├── src/
   │   ├── lib.rs               # Public API, prelude
   │   ├── wifi.rs              # CYW43 WiFi initialization
   │   ├── config.rs            # WifiConfig, NodeConfig
   │   ├── network.rs           # smoltcp bridge (CYW43 → zenoh-pico)
   │   ├── clock.rs             # RP2040 timer for smoltcp timestamps
   │   ├── rng.rs               # RP2040 ROSC-based RNG
   │   ├── node.rs              # run_node() entry point
   │   ├── publisher.rs         # Publisher wrapper
   │   ├── subscriber.rs        # Subscriber wrapper
   │   └── error.rs             # Error types
   ```
2. [ ] Implement CYW43 WiFi initialization
3. [ ] Bridge CYW43's smoltcp device to zenoh-pico-shim-sys platform_smoltcp
4. [ ] Implement `run_node()` with polling loop
5. [ ] Implement clock using RP2040 Timer peripheral
6. [ ] Implement RNG using RP2040 ROSC randomness
7. [ ] Add DHCP support via smoltcp
8. [ ] Add `Cargo.toml` with dependencies:
   ```toml
   [dependencies]
   embassy-rp = { version = "0.4", features = ["rp2040"] }
   cyw43 = "0.4"
   cyw43-pio = "0.4"
   smoltcp = { version = "0.12", default-features = false, features = [
       "medium-ethernet", "proto-ipv4", "socket-tcp", "proto-dhcpv4",
   ] }
   zenoh-pico-shim-sys = { path = "../zenoh-pico-shim-sys", features = ["smoltcp"] }
   cortex-m = "0.7"
   cortex-m-rt = "0.7"
   defmt = "0.3"
   defmt-rtt = "0.4"
   panic-probe = { version = "0.3", features = ["print-defmt"] }
   ```

**Acceptance Criteria**:
- [ ] Crate compiles for `thumbv6m-none-eabi`
- [ ] WiFi connects and gets IP
- [ ] zenoh-pico session opens over WiFi
- [ ] `run_node()` API works end-to-end

### 24.4: Create Pico W Examples

**Status**: Not Started

**Tasks**:
1. [ ] Create `examples/pico-w/rs-talker/` — WiFi publisher
2. [ ] Create `examples/pico-w/rs-listener/` — WiFi subscriber
3. [ ] Create `examples/pico-w/bsp-talker/` — Simplified BSP publisher
4. [ ] Add `just build-examples-pico-w` recipe
5. [ ] Add `just flash-pico-w-talker` recipe (probe-rs or UF2)
6. [ ] Create `examples/pico-w/README.md`

**Acceptance Criteria**:
- [ ] Talker publishes over WiFi to zenohd
- [ ] Listener receives over WiFi from zenohd
- [ ] BSP example <30 lines
- [ ] Both UF2 and probe-rs flashing work

### 24.5: Integration Testing

**Status**: Not Started

**Tasks**:
1. [ ] Test Pico W talker → native listener (via zenohd)
2. [ ] Test native talker → Pico W listener (via zenohd)
3. [ ] Test Pico W ↔ ESP32 communication (cross-platform)
4. [ ] Test Pico W ↔ ROS 2 interop
5. [ ] Measure WiFi latency and throughput
6. [ ] Measure RAM usage (264 KB is tighter than ESP32)
7. [ ] Stress test: sustained pub/sub for >1 hour
8. [ ] Document results

**Acceptance Criteria**:
- [ ] Bidirectional pub/sub works
- [ ] RAM usage documented and within budget
- [ ] Stable for >1 hour continuous operation

### 24.6: Embassy Async Integration (Future)

**Status**: Not Started
**Priority**: Low

**Tasks**:
1. [ ] Add `embassy` feature flag to BSP crate
2. [ ] Implement async `init_node()` using embassy executor
3. [ ] Implement async `spin_once()` that yields to embassy scheduler
4. [ ] Create Embassy-based examples
5. [ ] Document Embassy vs polling trade-offs

**Acceptance Criteria**:
- [ ] Embassy examples compile and run
- [ ] Power consumption improved vs polling

### 24.7: Documentation

**Status**: Not Started

**Tasks**:
1. [ ] Create `docs/pico-w-setup.md` — Development environment
2. [ ] Update `CLAUDE.md` — Add Pico W to workspace structure
3. [ ] Update Phase 14 roadmap — Reference Pico W BSP
4. [ ] Update `docs/micro-ros-comparison.md` — Add Pico W to platform table
5. [ ] Create `docs/pico-w-performance.md` — Benchmarks

**Acceptance Criteria**:
- [ ] Setup guide enables <30 minute start
- [ ] All documentation updated

## Dependencies

```
24.1 (Dev setup) ──────────────────────────────┐
                                                │
24.2 (Cross-compile zenoh-pico) ───────────────┤
                                                │
                                                ▼
24.3 (BSP crate) ─────────────────────────────┤
                                                │
                                                ▼
24.4 (Examples) ──────────────────────────────┤
                                                │
                                                ▼
24.5 (Integration testing) ───────────────────┤
                                                │
                                                ▼
24.6 (Embassy async) ─────── (optional) ──────┤
                                                │
                                                ▼
24.7 (Documentation) ─────────────────────────┘
```

24.1 and 24.2 can proceed in parallel. 24.6 (Embassy) is optional and can be deferred.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| RAM pressure (264 KB total) | High | Profile carefully, minimize buffers, consider `Z_FEATURE_QUERY=0` |
| CYW43 WiFi driver maturity | Medium | `cyw43` crate is well-maintained by Embassy team |
| Cortex-M0+ lacks atomic ops | Medium | Use `portable-atomic` crate for M0+ atomics |
| Flash size with WiFi firmware | Medium | CYW43 firmware is ~230 KB but flash is 2 MB |
| No hardware FPU | Low | zenoh-pico doesn't use floating point |
| embassy-rp version churn | Medium | Pin versions, test before upgrading |

## Comparison with Other Boards

| Feature | Pico W | ESP32-C3 | QEMU (MPS2-AN385) | STM32F429 |
|---------|--------|----------|--------------------|-----------|
| Price | ~$6 | ~$8 | Free (emulated) | ~$25 |
| Arch | ARM M0+ | RISC-V | ARM M3 | ARM M4F |
| RAM | 264 KB | 400 KB | 64 KB (emulated) | 256 KB |
| WiFi | CYW43 | Built-in | N/A (TAP bridge) | External |
| Rust ecosystem | embassy-rp | esp-hal | cortex-m | stm32-hal |
| Debug | probe-rs / UF2 | espflash | QEMU | probe-rs |
| Availability | Excellent | Excellent | Always | Good |
| nano-ros BSP | This phase | Phase 22 | Complete | Complete |

## Future Extensions

- Embassy async integration (Phase 24.6)
- BLE transport (CYW43 supports BLE 5.2)
- Pico 2 W support (RP2350 — ARM M33 or RISC-V, more RAM)
- USB-CDC serial transport (no WiFi needed)
- PIO-based custom transports
- Multi-core: one core for WiFi/zenoh, one for application
- Low-power modes with WiFi sleep

# Phase 12: QEMU Bare-Metal Tests

This phase focuses on creating comprehensive bare-metal testing infrastructure using QEMU emulation, enabling automated testing of nano-ros on embedded targets without physical hardware.

## Goals

1. **Rust bare-metal examples** - Create/revise examples that run in QEMU with networking
2. **C bare-metal examples** - Create C API examples for QEMU bare-metal targets
3. **QEMU test infrastructure** - Build robust test framework for emulated environments
4. **ROS 2 interop tests** - Verify communication between QEMU bare-metal nodes and standard rmw_zenoh ROS 2 nodes

## Decisions

- **Priority**: Rust examples first, then C examples
- **QEMU Target**: mps2-an385 (Cortex-M3 with LAN9118 Ethernet)
- **Architecture**: No RISC-V in this phase
- **CI Strategy**: QEMU tests run separately (not blocking PRs)

## Current State

### What Exists

| Component               | Status                   | Location                                  |
|-------------------------|--------------------------|-------------------------------------------|
| qemu-rs-test            | ✅ Works (no networking) | `examples/qemu-rs-test/`                  |
| qemu-rs-lan9118         | ✅ Driver tests pass     | `examples/qemu-rs-lan9118/`               |
| qemu-rs-talker          | ✅ TCP client (smoltcp)  | `examples/qemu-rs-talker/`                |
| qemu-rs-listener        | ✅ TCP server (smoltcp)  | `examples/qemu-rs-listener/`              |
| lan9118-smoltcp         | ✅ Complete driver       | `packages/drivers/lan9118-smoltcp/`                 |
| stm32f4-rs-* examples   | ✅ Hardware-specific     | `examples/stm32f4-rs-*/`                  |
| native-c-baremetal-demo | ✅ Desktop simulation    | `examples/native-c-baremetal-demo/`       |
| QemuProcess fixture     | ✅ Complete              | `packages/testing/nano-ros-tests/src/qemu.rs`       |
| QEMU emulator tests     | ✅ 14 tests (no network) | `packages/testing/nano-ros-tests/tests/emulator.rs` |
| smoltcp platform layer  | ✅ Exists                | `packages/transport/nano-ros-transport-zenoh-sys/`             |
| QEMU network scripts    | ✅ Complete              | `scripts/qemu/`                           |
| RTIC design             | ✅ Documented            | `docs/design/rtic-integration-design.md`  |

### What's Missing

- zenoh-pico cross-compiled for Cortex-M3 (thumbv7m-none-eabi)
- QEMU example with full zenoh pub/sub (currently TCP only)
- C bare-metal examples targeting QEMU
- Networked QEMU test infrastructure (requires QEMU 7.0+)
- Interop tests: QEMU bare-metal ↔ ROS 2 rmw_zenoh nodes
- CI/CD automation for QEMU tests

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Host System                             │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐  │
│  │  zenohd     │    │ ROS 2 Node  │    │   Test Runner       │  │
│  │  (router)   │    │ (rmw_zenoh) │    │   (cargo test)      │  │
│  └──────┬──────┘    └──────┬──────┘    └──────────┬──────────┘  │
│         │                  │                       │            │
│         └─────────┬────────┴───────────────────────┘            │
│                   │                                             │
│         ┌─────────▼─────────┐                                   │
│         │   TAP Bridge      │  (192.0.2.2)                      │
│         │   (br-qemu)       │                                   │
│         └─────────┬─────────┘                                   │
│                   │                                             │
│    ┌──────────────┼──────────────┐                              │
│    │              │              │                              │
│    ▼              ▼              ▼                              │
│ ┌──────┐      ┌──────┐      ┌──────┐                            │
│ │ TAP0 │      │ TAP1 │      │ TAP2 │                            │
│ └──┬───┘      └──┬───┘      └──┬───┘                            │
└────┼─────────────┼─────────────┼────────────────────────────────┘
     │             │             │
┌────▼────┐   ┌────▼────┐   ┌────▼────┐
│  QEMU   │   │  QEMU   │   │  QEMU   │
│ ARM M3  │   │ ARM M3  │   │ RISC-V  │
│ talker  │   │listener │   │  node   │
│192.0.2.1│   │192.0.2.3│   │192.0.2.4│
└─────────┘   └─────────┘   └─────────┘
```

## Phases

### Phase 12.1: LAN9118 Rust Driver for smoltcp

**Status**: Complete

Implement a Rust driver for the LAN9118 Ethernet controller that integrates with smoltcp.

**Tasks**:

1. **Create driver crate**
   - Location: `packages/drivers/lan9118-smoltcp/`
   - Implement `smoltcp::phy::Device` trait
   - Memory-mapped register definitions
   - No external dependencies (bare-metal compatible)

2. **Register interface**
   - Study LAN9118 datasheet register map
   - Implement CSR (Control/Status Register) access
   - TX/RX FIFO operations
   - PHY management (MDIO)

3. **Driver features**
   - Polling mode (no interrupts initially)
   - Static buffer allocation
   - MAC address configuration
   - Link status detection

4. **Testing**
   - Unit tests for register access (mock)
   - Integration test with QEMU mps2-an385
   - Packet TX/RX verification

**Deliverables**:
- [x] `packages/drivers/lan9118-smoltcp/` - Driver crate
- [x] Register definitions and accessors
- [x] `smoltcp::phy::Device` implementation
- [x] Basic integration test (`examples/qemu-rs-lan9118/`)

**References**:
- [LAN9118 datasheet](https://www.alldatasheet.com/datasheet-pdf/pdf/172074/SMSC/LAN9118.html)
- [eCos driver documentation](https://doc.ecoscentric.com/ref/devs-eth-smsc-lan9118.html)
- Zephyr `drivers/ethernet/eth_smsc911x.c`

---

### Phase 12.2: QEMU Networking Infrastructure

**Status**: Complete

Create QEMU instances with network connectivity via TAP interfaces.

**Tasks**:

1. **Network setup scripts**
   - Enhance `scripts/qemu/setup-qemu-network.sh`
   - Support multiple TAP interfaces for multi-node tests
   - Bridge configuration (`br-qemu` at 192.0.2.2)

2. **QEMU launch wrapper**
   - Script: `scripts/qemu/launch-mps2-an385.sh`
   - Network options: `-netdev tap,id=net0,ifname=tap0,script=no,downscript=no`
   - Device: `-device lan9118,netdev=net0`
   - Semihosting: `-semihosting-config enable=on,target=native`

3. **Multi-node configuration**
   - TAP0: 192.0.2.1 (talker)
   - TAP1: 192.0.2.3 (listener)
   - Bridge: 192.0.2.2 (host/zenohd)

**Deliverables**:
- [x] `scripts/qemu/launch-mps2-an385.sh` - QEMU launcher
- [x] `scripts/qemu/setup-network.sh` - Bridge + TAP setup
- [x] Documentation in `docs/guides/qemu-bare-metal.md`
- [x] Justfile recipes: `setup-qemu-network`, `teardown-qemu-network`, `status-qemu-network`

---

### Phase 12.3: Rust Bare-Metal Examples with Networking

**Status**: Complete

Create Rust examples that run in QEMU with smoltcp networking.

**Tasks**:

1. **qemu-rs-talker** - Publisher example
   - Target: `thumbv7m-none-eabi` (Cortex-M3)
   - Stack: lan9118-smoltcp + smoltcp + nano-ros-transport-zenoh
   - Publishes `std_msgs/Int32` to `/chatter`
   - Static IP: 192.0.2.1

2. **qemu-rs-listener** - Subscriber example
   - Receives from `/chatter`
   - Static IP: 192.0.2.3
   - Prints received values via semihosting

3. **Memory layout**
   - Linker script for mps2-an385
   - Heap: 64KB (embedded-alloc)
   - Stack: 8KB
   - Ethernet buffers: 16KB

4. **Build configuration**
   - `.cargo/config.toml` for QEMU target
   - Feature flags: `qemu`, `smoltcp`, `lan9118`

**Deliverables**:
- [x] `examples/qemu-rs-talker/` - QEMU TCP client with smoltcp
- [x] `examples/qemu-rs-listener/` - QEMU TCP server with smoltcp
- [x] Shared linker script: `examples/qemu-rs-common/mps2-an385.x`
- [ ] Build instructions in example READMEs

**Note**: Current examples demonstrate smoltcp TCP networking with the LAN9118 driver. Full zenoh-pico pub/sub integration is tracked in Phase 12.3a below.

**Dependencies**:
- lan9118-smoltcp (our driver from 12.1)
- smoltcp (no_std TCP/IP stack)
- embedded-alloc (heap allocator)
- cortex-m, cortex-m-rt (runtime)
- panic-semihosting (panic handler)

---

### Phase 12.3a: Full QEMU Talker/Listener with zenoh-pico

**Status**: Complete (Infrastructure Ready)

Upgraded the TCP examples to use zenoh-pico for pub/sub communication. The examples are built and ready for testing with QEMU 7.0+. Note: Automated tests require QEMU 7.0+ for reliable TAP networking (Ubuntu 22.04 ships 6.2).

**Prerequisites**:

1. **QEMU 7.0+ required**
   - Ubuntu 22.04 ships QEMU 6.2 which has TAP networking issues with mps2-an385
   - Install newer QEMU from backports or build from source
   - Verification: `qemu-system-arm --version` should show 7.0+

2. **TAP networking must work**
   - Verify with: `just setup-qemu-network && just status-qemu-network`
   - Test connectivity between host and QEMU guest

**Completed Work**:

1. **QEMU TAP networking documentation**
   - [x] Documented QEMU 7.0+ requirement
   - [x] Created `just test-qemu-zenoh` recipe that shows test instructions
   - [x] Scripts work with existing QEMU when TAP networking functions

2. **Cross-compile zenoh-pico for Cortex-M3**
   - [x] Created `scripts/qemu/build-zenoh-pico.sh` build script
   - [x] Uses arm-none-eabi-gcc toolchain (not CMake for simplicity)
   - [x] Builds zenoh-pico + shim as static library (3.4MB)
   - [x] Includes smoltcp platform layer and zenoh_shim.c
   - [x] Recipe: `just build-zenoh-pico-arm`

3. **smoltcp platform layer integration**
   - [x] Created `qemu-rs-common` crate with SmoltcpZenohBridge
   - [x] Implements poll callback for smoltcp/zenoh-pico integration
   - [x] Provides libc stubs for bare-metal (strlen, memcpy, strtoul, etc.)
   - [x] Clock functions for monotonic time
   - [x] Location: `examples/qemu-rs-common/src/`

4. **Updated qemu-rs-talker for zenoh pub/sub**
   - [x] Uses zenoh_shim API for session/publisher
   - [x] Connects to zenohd at 192.0.2.1:7447
   - [x] Publishes messages to `demo/qemu` topic
   - [x] Uses SmoltcpZenohBridge for network polling

5. **Updated qemu-rs-listener for zenoh pub/sub**
   - [x] Uses zenoh_shim API for session/subscriber
   - [x] Subscribes to `demo/qemu` topic
   - [x] Callback-based message reception
   - [x] Atomic counter for tracking received messages

6. **QEMU-to-QEMU test infrastructure**
   - [x] Examples ready for talker (192.0.2.10) and listener (192.0.2.11)
   - [x] `just test-qemu-zenoh` shows manual test instructions
   - [x] Automated test blocked by QEMU 6.2 TAP issues

**Files Created/Modified**:

| File | Description |
|------|-------------|
| `scripts/qemu/build-zenoh-pico.sh` | Builds zenoh-pico for ARM Cortex-M3 |
| `examples/qemu-rs-common/` | Shared infrastructure crate |
| `examples/qemu-rs-common/src/bridge.rs` | SmoltcpZenohBridge implementation |
| `examples/qemu-rs-common/src/clock.rs` | Monotonic clock for smoltcp |
| `examples/qemu-rs-common/src/libc_stubs.rs` | Minimal libc for bare-metal |
| `examples/qemu-rs-talker/src/main.rs` | Updated for zenoh-pico |
| `examples/qemu-rs-listener/src/main.rs` | Updated for zenoh-pico |

**Technical Challenges Solved**:

| Challenge | Solution |
|-----------|----------|
| zenoh-pico requires heap | Bump allocator in SmoltcpZenohBridge (64KB) |
| No threading on bare-metal | Single-threaded polling with callbacks |
| smoltcp needs clock | AtomicU32 counter (32-bit ARM limitation) |
| C library functions | Minimal stubs in libc_stubs.rs |
| Link zenoh-pico C library | build.rs links pre-built libzenohpico.a |

**Known Limitations**:

1. **QEMU 6.2 TAP issues**: Ubuntu 22.04's QEMU has unreliable TAP networking with mps2-an385. Manual testing shows the examples build and link correctly, but runtime testing requires QEMU 7.0+.

2. **No ROS 2 keyexpr format yet**: Examples use `demo/qemu` topic, not full ROS 2 format (`0/chatter/std_msgs::msg::dds_::Int32_/...`). Adding RMW interop format is future work.

**How to Test (Manual)**:

```bash
# Build everything
just build-zenoh-pico-arm
just build-examples-qemu

# Terminal 1: Start zenohd
zenohd --listen tcp/0.0.0.0:7447

# Terminal 2: Start listener
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 \
    --binary examples/qemu-rs-listener/target/thumbv7m-none-eabi/release/qemu-rs-listener

# Terminal 3: Start talker
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 \
    --binary examples/qemu-rs-talker/target/thumbv7m-none-eabi/release/qemu-rs-talker
```

---

### Phase 12.4: C Bare-Metal Examples for QEMU

**Status**: Not Started

Create C API examples targeting QEMU bare-metal environment.

**Tasks**:

1. **qemu-c-talker** - C publisher for QEMU
   - Uses nano-ros-c static library
   - Platform implementation for QEMU/ARM
   - smoltcp integration via C bindings

2. **qemu-c-listener** - C subscriber for QEMU
   - Callback-based message reception
   - Static allocation patterns

3. **Platform abstraction**
   - `platform_qemu.c` - Time, sleep, atomics for bare-metal
   - Newlib stubs for minimal libc
   - Semihosting for debug output

4. **Build system**
   - CMake toolchain file for ARM cross-compilation
   - Integration with nano-ros-c library
   - QEMU-specific linker script

**Deliverables**:
- [ ] `examples/qemu-c-talker/` - C publisher
- [ ] `examples/qemu-c-listener/` - C subscriber
- [ ] `examples/qemu-c-common/` - Shared platform code
- [ ] CMake toolchain: `cmake/arm-none-eabi.cmake`

---

### Phase 12.5: QEMU Test Infrastructure

**Status**: Not Started

Enhance test framework for networked QEMU testing.

**Tasks**:

1. **Enhanced QemuProcess fixture**
   ```rust
   pub struct NetworkedQemuProcess {
       process: QemuProcess,
       tap_interface: String,
       ip_address: Ipv4Addr,
   }

   impl NetworkedQemuProcess {
       pub fn start_with_network(binary: &Path, tap: &str, ip: Ipv4Addr) -> TestResult<Self>;
       pub fn wait_for_network_ready(&self) -> TestResult<()>;
   }
   ```

2. **Multi-node test harness**
   ```rust
   pub struct QemuTestCluster {
       nodes: Vec<NetworkedQemuProcess>,
       bridge: NetworkBridge,
       zenohd: Option<ZenohRouter>,
   }
   ```

3. **Test utilities**
   - Network readiness detection
   - Message verification helpers
   - Timeout handling for slow emulation

4. **Cached binary builds**
   - Add qemu-rs-talker/listener to build cache
   - Cross-compilation support in fixtures

**Deliverables**:
- [ ] `packages/testing/nano-ros-tests/src/qemu_network.rs` - Network fixtures
- [ ] `packages/testing/nano-ros-tests/src/cluster.rs` - Multi-node harness
- [ ] Enhanced `fixtures/binaries.rs` with QEMU builds

---

### Phase 12.6: Bare-Metal ↔ ROS 2 Interop Tests

**Status**: Not Started

Test communication between QEMU bare-metal nodes and ROS 2 rmw_zenoh nodes.

**Test Scenarios**:

1. **QEMU talker → ROS 2 listener**
   - QEMU node publishes Int32
   - ROS 2 node receives via rmw_zenoh
   - Verify message content and timing

2. **ROS 2 talker → QEMU listener**
   - ROS 2 node publishes
   - QEMU node receives and verifies

3. **Bidirectional communication**
   - Service call from QEMU to ROS 2
   - Service call from ROS 2 to QEMU

4. **Multi-node scenarios**
   - Multiple QEMU nodes + ROS 2 nodes
   - Network partition testing

**Test Implementation**:
```rust
#[test]
fn test_qemu_to_ros2_interop() {
    // Setup
    let bridge = NetworkBridge::create("br-qemu")?;
    let zenohd = ZenohRouter::start()?;

    // Start QEMU talker
    let qemu_talker = NetworkedQemuProcess::start_with_network(
        &qemu_rs_talker_binary(),
        "tap0",
        "192.0.2.1".parse()?,
    )?;

    // Start ROS 2 listener
    let ros2_listener = Ros2Process::start_listener("/chatter")?;

    // Verify communication
    let received = ros2_listener.wait_for_messages(5, Duration::from_secs(10))?;
    assert!(received.len() >= 5);
}
```

**Deliverables**:
- [ ] `packages/testing/nano-ros-tests/tests/qemu_interop.rs` - Interop test suite
- [ ] Test for each direction (QEMU→ROS2, ROS2→QEMU)
- [ ] Documentation of test prerequisites

---

### Phase 12.7: CI/CD Integration

**Status**: Not Started

Automate QEMU tests in CI pipeline. **Note**: QEMU tests run separately, not blocking PRs.

**Tasks**:

1. **GitHub Actions workflow** (scheduled/manual trigger)
   ```yaml
   name: QEMU Bare-Metal Tests
   on:
     schedule:
       - cron: '0 4 * * *'  # Daily at 4am UTC
     workflow_dispatch:      # Manual trigger

   jobs:
     qemu-tests:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4
         - name: Install QEMU
           run: sudo apt-get install -y qemu-system-arm
         - name: Install ARM toolchain
           run: rustup target add thumbv7m-none-eabi
         - name: Build LAN9118 driver
           run: cargo build -p lan9118-smoltcp --target thumbv7m-none-eabi
         - name: Build QEMU examples
           run: |
             cargo build -p qemu-rs-talker --target thumbv7m-none-eabi --release
             cargo build -p qemu-rs-listener --target thumbv7m-none-eabi --release
         - name: Setup network
           run: sudo ./scripts/qemu/setup-qemu-bridge.sh
         - name: Run QEMU tests
           run: cargo test -p nano-ros-tests --test qemu_bare_metal
   ```

2. **Test tiers**
   - **Tier 1 (blocking)**: Unit tests, clippy, format
   - **Tier 2 (non-blocking)**: QEMU bare-metal tests, interop tests
   - Target: ARM Cortex-M3 only (no RISC-V in this phase)

3. **Artifact caching**
   - Cache compiled QEMU binaries between runs
   - Cache ARM cross-compilation artifacts

**Deliverables**:
- [ ] `.github/workflows/qemu-bare-metal.yml` (scheduled workflow)
- [ ] Justfile recipes: `test-qemu-bare-metal`, `test-qemu-interop`
- [ ] Separate status badge for QEMU tests

---

## Technical Challenges

### 1. QEMU Machine and Ethernet Support

**Selected Machine**: `mps2-an385` (ARM Cortex-M3)

| Aspect | Details |
|--------|---------|
| CPU | ARM Cortex-M3 |
| Ethernet | LAN9118 (SMSC) |
| QEMU Support | ✅ Full emulation |
| Zephyr Support | ✅ SMSC911x driver works |
| Rust Driver | ❌ Must be written |

**References**:
- [QEMU mps2 docs](https://www.qemu.org/docs/master/system/arm/mps2.html)
- [LAN9118 datasheet](https://www.alldatasheet.com/datasheet-pdf/pdf/172074/SMSC/LAN9118.html) (126 pages)
- [Zephyr QEMU networking](https://docs.zephyrproject.org/latest/connectivity/networking/qemu_eth_setup.html)

### 2. LAN9118 Rust Driver for smoltcp

**Challenge**: No existing Rust driver for LAN9118. Must implement `smoltcp::phy::Device`.

**Approach**:
1. Study [eCos LAN9118 driver](https://doc.ecoscentric.com/ref/devs-eth-smsc-lan9118.html) (well documented)
2. Reference Zephyr's `drivers/ethernet/eth_smsc911x.c`
3. Implement memory-mapped register access
4. Create TX/RX buffer management

**LAN9118 Key Features** (simplifies driver):
- Simple SRAM-like bus interface
- 32-bit or 16-bit host bus modes
- No DMA required (programmed I/O works)
- Single TX and RX FIFO

**Driver Structure**:
```rust
pub struct Lan9118<'a> {
    base_addr: usize,
    rx_buffer: &'a mut [u8],
    tx_buffer: &'a mut [u8],
}

impl<'a> smoltcp::phy::Device for Lan9118<'a> {
    type RxToken<'b> = Lan9118RxToken<'b> where Self: 'b;
    type TxToken<'b> = Lan9118TxToken<'b> where Self: 'b;

    fn receive(&mut self, _: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)>;
    fn transmit(&mut self, _: Instant) -> Option<Self::TxToken<'_>>;
    fn capabilities(&self) -> DeviceCapabilities;
}
```

**Estimated Effort**: 3-5 days for basic driver

### 3. Timing and Synchronization

**Challenge**: QEMU emulation is slower than real hardware.

**Mitigations**:
- Longer timeouts in tests (10x normal)
- Use semihosting for synchronization signals
- Implement "ready" handshake protocol

### 4. Heap Allocation

**Challenge**: zenoh-pico requires heap (~64KB minimum).

**Solution**:
- Use `embedded-alloc` crate
- Configure heap in linker script
- Monitor heap usage in tests

---

## Success Criteria

1. **Rust examples work**: qemu-rs-talker and qemu-rs-listener communicate
2. **C examples work**: qemu-c-talker and qemu-c-listener communicate
3. **Cross-language**: Rust QEMU ↔ C QEMU communication verified
4. **ROS 2 interop**: QEMU nodes communicate with rmw_zenoh ROS 2 nodes
5. **CI passing**: All QEMU tests run in GitHub Actions
6. **Documentation**: Complete setup and troubleshooting guide

---

## Timeline Estimate

| Phase     | Description                    | Effort         | Priority     | Status      |
|-----------|--------------------------------|----------------|--------------|-------------|
| 12.1      | LAN9118 Rust Driver            | 3-5 days       | P0 (blocker) | ✅ Complete |
| 12.2      | QEMU Networking Infrastructure | 1-2 days       | P0           | ✅ Complete |
| 12.3      | Rust Bare-Metal Examples (TCP) | 2-3 days       | P0           | ✅ Complete |
| 12.3a     | Full zenoh-pico Talker/Listener| 3-5 days       | P0           | Not Started |
| 12.4      | C Bare-Metal Examples          | 3-4 days       | P1           | Not Started |
| 12.5      | Test Infrastructure            | 2-3 days       | P1           | Not Started |
| 12.6      | ROS 2 Interop Tests            | 2-3 days       | P1           | Not Started |
| 12.7      | CI/CD Integration              | 1-2 days       | P2           | Not Started |
| **Total** |                                | **17-27 days** |             |

**Critical Path**: 12.1 → 12.2 → 12.3 → 12.3a (zenoh-pico integration requires working TCP networking)

**Blocker for 12.3a**: QEMU 7.0+ required for reliable TAP networking. Ubuntu 22.04 ships QEMU 6.2.

---

## References

- [QEMU ARM System Emulator](https://www.qemu.org/docs/master/system/target-arm.html)
- [smoltcp - TCP/IP stack](https://github.com/smoltcp-rs/smoltcp)
- [embedded-alloc](https://github.com/rust-embedded/embedded-alloc)
- [RTIC Framework](https://rtic.rs/)
- [LAN9118 Datasheet](https://www.microchip.com/en-us/product/LAN9118)
- Existing docs: `docs/design/rtic-integration-design.md`, `docs/reference/embedded-integration.md`

# Phase 13: Bare-Metal API Simplification

**Goal**: Create a high-level `nano-ros-baremetal` crate that abstracts smoltcp and zenoh-pico details, enabling simple ~50-line examples instead of the current ~500-line boilerplate.

**Status**: Complete

## Progress

- [x] **Phase 13.1**: Move FFI to nano-ros-transport-zenoh
  - `nano-ros-baremetal` now uses FFI from `nano-ros-transport-zenoh-sys`
  - No duplicate extern "C" declarations in examples
  - `ShimCallback` type re-exported for subscriber callbacks
- [x] **Phase 13.2**: Created `nano-ros-baremetal` crate with high-level API
  - `BaremetalNode`, `Publisher`, `Subscriber` types
  - `NodeConfig` for network configuration
  - `create_interface()` and `create_socket_set()` helpers
  - Static buffer management
- [x] **Phase 13.3**: Created platform modules
  - `platform::qemu_mps2` with `create_ethernet()` and `EthernetDevice` impl
  - Exit helpers (`exit_success`, `exit_failure`)
- [x] **Phase 13.4**: Simplified examples
  - `qemu-rs-talker`: Reduced from ~510 lines to ~217 lines (57% reduction)
  - `qemu-rs-listener`: Reduced from ~499 lines to ~210 lines (58% reduction)
  - Docker tests passing with simplified examples
- [x] **Phase 13.5**: Documentation and cleanup
  - Roadmap updated with progress
  - All quality checks passing

## Motivation

Current bare-metal examples (`qemu-rs-talker`, `qemu-rs-listener`, `stm32f4-rs-*`) mix multiple concerns:
- Hardware initialization (LAN9118/STM32 Ethernet driver)
- Network stack setup (smoltcp interface, sockets, static buffers)
- Transport layer (zenoh-pico FFI, poll callbacks, global pointers)
- Application logic (pub/sub)

This results in ~500 lines of boilerplate per example, making it difficult to:
- Understand the ROS-like API patterns
- Port examples to new hardware
- Maintain consistency across examples

## Goals

1. **Create `nano-ros-baremetal` crate** - High-level API for bare-metal ROS nodes
2. **Move FFI to `nano-ros-transport-zenoh`** - Centralize all `extern "C"` declarations
3. **Create platform modules** - Hardware-specific initialization helpers
4. **Simplify examples to ~50 lines** - Focus on application logic only

## Target API

### Ideal Example (50 lines)

```rust
#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use nano_ros_baremetal::{BaremetalNode, NodeConfig};
use nano_ros_baremetal::platform::qemu_mps2;

#[entry]
fn main() -> ! {
    hprintln!("Starting QEMU Talker...");

    // Platform-specific driver (one function call)
    let eth = qemu_mps2::create_ethernet([0x02, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Create node with network config
    let mut node = BaremetalNode::new(eth, NodeConfig {
        ip: [192, 168, 100, 10],
        gateway: [192, 168, 100, 1],
        zenoh_locator: b"tcp/172.20.0.2:7447\0",
    }).expect("Failed to create node");

    // ROS-like API
    let publisher = node.create_publisher(b"demo/qemu\0")
        .expect("Failed to create publisher");

    // Publish loop
    for i in 0..10 {
        node.spin_once(10);

        let msg = format_msg(i);
        publisher.publish(msg.as_bytes());
        hprintln!("Published: {}", i);
    }

    hprintln!("Done!");
    node.shutdown();

    loop { cortex_m::asm::wfi(); }
}

fn format_msg(n: u32) -> heapless::String<32> {
    use core::fmt::Write;
    let mut s = heapless::String::new();
    write!(s, "Hello from QEMU #{}", n).ok();
    s
}
```

### Listener Example

```rust
#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use nano_ros_baremetal::{BaremetalNode, NodeConfig};
use nano_ros_baremetal::platform::qemu_mps2;

#[entry]
fn main() -> ! {
    hprintln!("Starting QEMU Listener...");

    let eth = qemu_mps2::create_ethernet([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);

    let mut node = BaremetalNode::new(eth, NodeConfig {
        ip: [192, 168, 100, 11],
        gateway: [192, 168, 100, 1],
        zenoh_locator: b"tcp/172.20.0.2:7447\0",
    }).expect("Failed to create node");

    let mut msg_count = 0u32;
    let subscriber = node.create_subscriber(b"demo/qemu\0", |data: &[u8]| {
        msg_count += 1;
        if let Ok(s) = core::str::from_utf8(data) {
            hprintln!("Received [{}]: {}", msg_count, s);
        }
    }).expect("Failed to create subscriber");

    // Receive loop
    while msg_count < 10 {
        node.spin_once(10);
    }

    hprintln!("Received 10 messages, done!");
    node.shutdown();

    loop { cortex_m::asm::wfi(); }
}
```

## Architecture

### Crate Structure

```
packages/
├── nano-ros-baremetal/              # NEW: High-level bare-metal API
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                   # BaremetalNode, Publisher, Subscriber
│   │   ├── config.rs                # NodeConfig, NetworkConfig
│   │   ├── node.rs                  # BaremetalNode implementation
│   │   ├── publisher.rs             # Publisher wrapper
│   │   ├── subscriber.rs            # Subscriber wrapper
│   │   ├── buffers.rs               # Static buffer management
│   │   ├── error.rs                 # Error types
│   │   └── platform/
│   │       ├── mod.rs               # Platform trait + re-exports
│   │       ├── qemu_mps2.rs         # QEMU mps2-an385 (LAN9118)
│   │       └── stm32f4.rs           # STM32F4 (stm32-eth)
│   └── README.md
│
├── nano-ros-transport-zenoh/                 # EXISTING: Add FFI declarations
│   └── src/
│       ├── lib.rs
│       └── ffi.rs                   # NEW: All extern "C" declarations
│
├── nano-ros-transport-zenoh-sys/             # EXISTING: C code
│   └── c/
│       └── shim/
│           └── zenoh_shim.h         # Public C API (already exists)
```

### Layer Responsibilities

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Application                                   │
│                  (examples/qemu-rs-talker)                          │
│                                                                      │
│  - ~50 lines of code                                                │
│  - Application logic only                                           │
│  - No smoltcp imports                                               │
│  - No zenoh FFI calls                                               │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     nano-ros-baremetal                               │
│                                                                      │
│  Provides:                                                          │
│  - BaremetalNode (session + network management)                     │
│  - Publisher, Subscriber (simple wrappers)                          │
│  - NodeConfig (IP, gateway, locator)                                │
│  - Static buffer allocation (hidden from user)                      │
│  - Poll callback registration (hidden from user)                    │
│  - Platform trait for hardware abstraction                          │
└─────────────────────────────────────────────────────────────────────┘
                                 │
              ┌──────────────────┴──────────────────┐
              ▼                                     ▼
┌─────────────────────────────┐   ┌─────────────────────────────────┐
│  platform::qemu_mps2        │   │  platform::stm32f4              │
│                             │   │                                 │
│  - create_ethernet()        │   │  - create_ethernet()            │
│  - LAN9118 driver init      │   │  - STM32 Ethernet init          │
│  - MPS2-AN385 specifics     │   │  - NUCLEO-F429ZI specifics      │
└─────────────────────────────┘   └─────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      nano-ros-transport-zenoh                                 │
│                                                                      │
│  Existing:                                                          │
│  - ShimContext, ShimPublisher, ShimSubscriber                       │
│                                                                      │
│  New (src/ffi.rs):                                                  │
│  - All extern "C" FFI declarations (moved from examples)            │
│  - Safe wrappers for FFI calls                                      │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    nano-ros-transport-zenoh-sys                               │
│                                                                      │
│  - zenoh_shim.c (C implementation)                                  │
│  - platform_smoltcp/*.c (platform layer)                            │
│  - zenoh-pico submodule                                             │
└─────────────────────────────────────────────────────────────────────┘
```

### What Each Layer Hides

| Layer | Hides From User |
|-------|-----------------|
| `nano-ros-baremetal` | smoltcp setup, socket buffers, poll callbacks, global pointers |
| `platform::*` | Driver initialization, memory-mapped addresses, hardware specifics |
| `nano-ros-transport-zenoh` | FFI declarations, unsafe blocks, error code translation |
| `nano-ros-transport-zenoh-sys` | C compilation, zenoh-pico internals |

## Phases

### Phase 13.1: Move FFI to nano-ros-transport-zenoh

**Status**: Not Started

Move all `extern "C"` declarations from examples to `nano-ros-transport-zenoh/src/ffi.rs`.

**Work Items**:

- [ ] **13.1.1** Create `nano-ros-transport-zenoh/src/ffi.rs` module
  - Move FFI declarations from `qemu-rs-talker/src/main.rs`
  - Include all `zenoh_shim_*` functions
  - Include all `smoltcp_*` debug functions

- [ ] **13.1.2** Create safe wrapper functions
  ```rust
  // nano-ros-transport-zenoh/src/ffi.rs
  pub fn init(locator: &[u8]) -> Result<(), ShimError> {
      let ret = unsafe { zenoh_shim_init(locator.as_ptr() as *const i8) };
      if ret == 0 { Ok(()) } else { Err(ShimError::from_code(ret)) }
  }
  ```

- [ ] **13.1.3** Export from `nano-ros-transport-zenoh/src/lib.rs`
  ```rust
  pub mod ffi;
  pub use ffi::{init, open, close, ...};
  ```

- [ ] **13.1.4** Update `qemu-rs-common` to use new FFI module
  - Remove duplicate FFI declarations
  - Import from `nano-ros-transport-zenoh::ffi`

**Acceptance Criteria**:
- No `extern "C"` blocks in example code
- All FFI in `nano-ros-transport-zenoh/src/ffi.rs`
- Examples still compile and run

---

### Phase 13.2: Create nano-ros-baremetal Crate

**Status**: Not Started

Create the high-level API crate.

**Work Items**:

- [ ] **13.2.1** Create crate structure
  ```
  packages/core/nano-ros-baremetal/
  ├── Cargo.toml
  ├── src/
  │   ├── lib.rs
  │   ├── config.rs
  │   ├── node.rs
  │   ├── publisher.rs
  │   ├── subscriber.rs
  │   ├── buffers.rs
  │   └── error.rs
  ```

- [ ] **13.2.2** Define `NodeConfig` struct
  ```rust
  pub struct NodeConfig<'a> {
      pub ip: [u8; 4],
      pub gateway: [u8; 4],
      pub prefix: u8,  // default: 24
      pub zenoh_locator: &'a [u8],  // null-terminated
  }
  ```

- [ ] **13.2.3** Define `EthernetDevice` trait
  ```rust
  pub trait EthernetDevice: smoltcp::phy::Device {
      fn mac_address(&self) -> [u8; 6];
  }
  ```

- [ ] **13.2.4** Implement `BaremetalNode`
  ```rust
  pub struct BaremetalNode<D: EthernetDevice> {
      // Private fields - hidden from user
      eth: D,
      iface: Interface,
      sockets: SocketSet<'static>,
      // ...
  }

  impl<D: EthernetDevice> BaremetalNode<D> {
      pub fn new(eth: D, config: NodeConfig) -> Result<Self, Error>;
      pub fn create_publisher(&mut self, topic: &[u8]) -> Result<Publisher, Error>;
      pub fn create_subscriber<F>(&mut self, topic: &[u8], callback: F) -> Result<Subscriber, Error>
      where F: FnMut(&[u8]) + 'static;
      pub fn spin_once(&mut self, timeout_ms: u32);
      pub fn shutdown(self);
  }
  ```

- [ ] **13.2.5** Implement static buffer management
  ```rust
  // buffers.rs - internal module
  const MAX_SOCKETS: usize = 4;
  const TCP_BUFFER_SIZE: usize = 2048;

  static mut SOCKET_STORAGE: [SocketStorage<'static>; MAX_SOCKETS] = ...;
  static mut TCP_BUFFERS: [[u8; TCP_BUFFER_SIZE]; MAX_SOCKETS * 2] = ...;

  pub(crate) fn allocate_socket_buffers() -> (TcpSocketBuffer, TcpSocketBuffer) { ... }
  ```

- [ ] **13.2.6** Implement poll callback internally
  - Register callback in `BaremetalNode::new()`
  - Call from `spin_once()`
  - No user-visible global state

- [ ] **13.2.7** Add `Cargo.toml` dependencies
  ```toml
  [dependencies]
  smoltcp = { version = "0.12", default-features = false, features = ["medium-ethernet", "proto-ipv4", "socket-tcp"] }
  nano-ros-transport-zenoh = { path = "../nano-ros-transport-zenoh", features = ["smoltcp"] }

  [features]
  default = []
  qemu-mps2 = ["dep:lan9118-smoltcp"]
  stm32f4 = ["dep:stm32-eth", "dep:stm32f4xx-hal"]
  ```

**Acceptance Criteria**:
- Crate compiles with `--features qemu-mps2`
- Public API matches target design
- All smoltcp details are private

---

### Phase 13.3: Create Platform Modules

**Status**: Not Started

Create platform-specific initialization helpers.

**Work Items**:

- [ ] **13.3.1** Create `platform` module structure
  ```rust
  // src/platform/mod.rs
  #[cfg(feature = "qemu-mps2")]
  pub mod qemu_mps2;

  #[cfg(feature = "stm32f4")]
  pub mod stm32f4;
  ```

- [ ] **13.3.2** Implement `platform::qemu_mps2`
  ```rust
  // src/platform/qemu_mps2.rs
  use lan9118_smoltcp::{Config, Lan9118, MPS2_AN385_BASE};

  pub fn create_ethernet(mac: [u8; 6]) -> Result<Lan9118, Error> {
      let config = Config {
          base_addr: MPS2_AN385_BASE,
          mac_addr: mac,
      };
      let mut eth = unsafe { Lan9118::new(config)? };
      eth.init()?;
      Ok(eth)
  }
  ```

- [ ] **13.3.3** Implement `platform::stm32f4`
  ```rust
  // src/platform/stm32f4.rs
  use stm32_eth::{EthernetDMA, EthernetMAC};
  use stm32f4xx_hal::gpio::...;

  pub struct Stm32Ethernet {
      dma: EthernetDMA<'static>,
      // ...
  }

  pub fn create_ethernet(
      mac: [u8; 6],
      // GPIO pins, clocks, etc. passed in
  ) -> Result<Stm32Ethernet, Error> {
      // Initialize STM32 Ethernet peripheral
  }
  ```

- [ ] **13.3.4** Implement `EthernetDevice` trait for each platform
  - `impl EthernetDevice for Lan9118`
  - `impl EthernetDevice for Stm32Ethernet`

**Acceptance Criteria**:
- Each platform has single `create_ethernet()` function
- Platform details hidden from application code
- Feature flags control which platforms are included

---

### Phase 13.4: Simplify Examples

**Status**: Not Started

Rewrite examples to use new API.

**Work Items**:

- [ ] **13.4.1** Rewrite `qemu-rs-talker`
  - Reduce from ~500 lines to ~50 lines
  - Remove all smoltcp imports
  - Remove all FFI declarations
  - Remove static buffer declarations
  - Remove poll callback

- [ ] **13.4.2** Rewrite `qemu-rs-listener`
  - Same simplification as talker
  - Use callback-based subscription

- [ ] **13.4.3** Update `qemu-rs-common`
  - Keep only truly shared code (clock, libc stubs)
  - Remove `SmoltcpZenohBridge` (absorbed into `nano-ros-baremetal`)

- [ ] **13.4.4** Update `stm32f4-rs-rtic`
  - Use `nano-ros-baremetal` with RTIC
  - Platform init in RTIC `#[init]`
  - Polling in RTIC async task

- [ ] **13.4.5** Update `stm32f4-rs-polling`
  - Use `nano-ros-baremetal` with simple loop
  - Minimal main.rs

- [ ] **13.4.6** Update example documentation
  - New README.md for each example
  - Document the simplified API

- [ ] **13.4.7** Update tests
  - Verify `just test-rust-qemu-baremetal` still passes
  - Verify Docker Compose test works

**Acceptance Criteria**:
- Each example is <100 lines
- No smoltcp or zenoh FFI visible in examples
- All examples compile and communicate
- Tests pass

---

### Phase 13.5: Documentation and Cleanup

**Status**: Not Started

Update documentation and clean up.

**Work Items**:

- [ ] **13.5.1** Create `nano-ros-baremetal` README
  - API overview
  - Supported platforms
  - Example usage

- [ ] **13.5.2** Update `docs/reference/embedded-integration.md`
  - Reference new API
  - Update architecture diagrams

- [ ] **13.5.3** Update `CLAUDE.md`
  - Add `nano-ros-baremetal` to workspace structure
  - Update crate descriptions

- [ ] **13.5.4** Run `just quality`
  - Ensure all checks pass
  - Fix any clippy warnings

- [ ] **13.5.5** Update Phase 12 roadmap
  - Mark relevant items complete
  - Reference Phase 13 for API improvements

**Acceptance Criteria**:
- All documentation current
- `just quality` passes
- No dead code warnings

---

## Dependencies

```
Phase 13.1 (Move FFI) ────────────────────────────────────────────────┐
                                                                      │
                                                                      ▼
Phase 13.2 (Create nano-ros-baremetal) ───────────────────────────────┤
                                                                      │
                                                                      ▼
Phase 13.3 (Platform modules) ────────────────────────────────────────┤
                                                                      │
                                                                      ▼
Phase 13.4 (Simplify examples) ───────────────────────────────────────┤
                                                                      │
                                                                      ▼
Phase 13.5 (Documentation) ───────────────────────────────────────────┘
```

## Timeline Estimate

| Phase | Description | Effort | Priority |
|-------|-------------|--------|----------|
| 13.1 | Move FFI to nano-ros-transport-zenoh | 1 day | P0 |
| 13.2 | Create nano-ros-baremetal crate | 2-3 days | P0 |
| 13.3 | Platform modules | 1-2 days | P0 |
| 13.4 | Simplify examples | 1-2 days | P0 |
| 13.5 | Documentation | 1 day | P1 |
| **Total** | | **6-9 days** | |

## Success Criteria

1. **Examples simplified**: Each example <100 lines, focused on application logic
2. **API clean**: No smoltcp or zenoh FFI visible to users
3. **Portable**: Adding new platform requires only `platform::new_platform` module
4. **Tests pass**: `just test-rust-qemu-baremetal` works with simplified examples
5. **Documentation complete**: README, architecture docs updated

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Static lifetime challenges | High | Use `'static` buffers with careful initialization order |
| Platform abstraction too leaky | Medium | Keep platform trait minimal, accept some duplication |
| Breaking existing examples | Medium | Keep old examples until new ones verified working |
| Memory overhead from abstraction | Low | Profile and optimize if needed |

## References

- Current examples: `examples/qemu-rs-{talker,listener}/`
- Phase 12: `docs/roadmap/phase-12-qemu-bare-metal-tests.md`
- Phase 8: `docs/roadmap/phase-8-embedded-networking.md`
- nano-ros-transport-zenoh: `packages/transport/nano-ros-transport-zenoh/`

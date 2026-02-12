# Phase 32: Platform/Transport Architecture Split

## Summary

Refactor the monolithic BSP crates into separate platform crates (system primitives) and transport crates (network protocol implementations). This implements the architecture described in [docs/design/platform-transport-architecture.md](../design/platform-transport-architecture.md).

## Motivation

The current BSP crates (`nano-ros-bsp-qemu`, `nano-ros-bsp-esp32`, etc.) bundle three concerns into one crate:

1. **System primitives** — memory, clock, RNG, sleep, threading stubs
2. **Network transport** — smoltcp bridge, TCP socket management, polling
3. **Application API** — `run_node()`, typed publisher/subscriber wrappers

This coupling means:
- Adding serial transport requires modifying every BSP crate
- Users on custom hardware must clone an entire BSP instead of composing pieces
- The C shim layer (`system.c`, `network.c`) translates between zenoh-pico symbols and custom `smoltcp_*` FFI — an unnecessary indirection
- Transport protocol support is tied to hardware platform (can't use serial on QEMU without full BSP changes)

## Current State

```
zenoh-pico-shim-sys[smoltcp]
├── c/platform_smoltcp/system.c    # C shim: z_malloc → smoltcp_alloc, z_clock_now → smoltcp_clock_now_ms
├── c/platform_smoltcp/network.c   # C shim: _z_open_tcp → smoltcp_socket_open + smoltcp_socket_connect
└── Compiles zenoh-pico with hardcoded zenoh_generic_config.h

nano-ros-bsp-qemu (monolithic)
├── bridge.rs       # SmoltcpBridge + ALL FFI: smoltcp_alloc, smoltcp_socket_open, ...
├── buffers.rs      # TCP socket buffers
├── clock.rs        # smoltcp_clock_now_ms FFI
├── libc_stubs.rs   # strlen, memcpy, strtoul, ...
├── node.rs         # run_node(), smoltcp_network_poll callback
└── timing.rs       # DWT cycle counter

Custom symbols: smoltcp_alloc, smoltcp_realloc, smoltcp_free, smoltcp_clock_now_ms,
smoltcp_random_u32, smoltcp_poll, smoltcp_socket_open, smoltcp_socket_connect,
smoltcp_socket_close, smoltcp_socket_send, smoltcp_socket_recv, etc. (20+ custom symbols)
```

## Target State

```
zenoh-pico-shim-sys[bare-metal]
├── build.rs generates config header from Cargo features (link-tcp, link-serial, etc.)
├── zenoh_bare_metal_platform.h (platform type definitions)
├── zenoh_shim.c (nano-ros's simplified wrapper — unchanged)
├── No C shim files (system.c, network.c removed)
└── Compiles zenoh-pico with feature-gated Z_FEATURE_LINK_* flags

nano-ros-platform-qemu (system primitives only)
├── lib.rs          # z_malloc, z_random_*, z_clock_*, z_sleep_*, z_time_*
│                     _z_task_* stubs, _z_mutex_* stubs, _z_condvar_* stubs
│                     _z_socket_close, _z_socket_wait_event, _z_socket_accept, _z_socket_set_non_blocking
├── libc_stubs.rs   # strlen, memcpy, strtoul, ...
├── config.rs       # Network configuration
├── node.rs         # run_node(), poll callback registration
└── timing.rs       # DWT cycle counter

nano-ros-transport-smoltcp (TCP/UDP via smoltcp IP stack)
├── lib.rs          # _z_create_endpoint_tcp, _z_free_endpoint_tcp
│                     _z_open_tcp, _z_listen_tcp, _z_close_tcp
│                     _z_read_tcp, _z_read_exact_tcp, _z_send_tcp
├── bridge.rs       # SmoltcpBridge: socket table, RX/TX buffers, poll()
└── poll.rs         # Poll callback slot (Rust fn pointer, not FFI)

Custom symbols: none. All FFI symbols are zenoh-pico's standard platform API.
```

## Work Items

### 32.1: Add `link-*` Cargo features to `zenoh-pico-shim-sys` — Complete

**Effort:** 0.5 day
**Dependencies:** None

Add Cargo features that control which `Z_FEATURE_LINK_*` flags are passed to zenoh-pico's CMake build:

```toml
# zenoh-pico-shim-sys/Cargo.toml
[features]
bare-metal = []        # New: replaces smoltcp as the bare-metal platform selector
link-tcp = []          # sets Z_FEATURE_LINK_TCP=1
link-udp-unicast = []  # sets Z_FEATURE_LINK_UDP_UNICAST=1
link-udp-multicast = [] # sets Z_FEATURE_LINK_UDP_MULTICAST=1
link-serial = []       # sets Z_FEATURE_LINK_SERIAL=1
link-raweth = []       # sets Z_FEATURE_RAWETH_TRANSPORT=1
```

Update `build.rs` to generate a config header from these features instead of using the hardcoded `zenoh_generic_config.h`. The `smoltcp` feature is a temporary alias for `bare-metal` + `link-tcp` until 32.8 removes it.

**Deliverables:**
- Updated `Cargo.toml` with new features
- Updated `build.rs` to generate config header from Cargo features
- `smoltcp` feature kept temporarily as alias (removed in 32.8)
- Existing builds continue to work unchanged

### 32.2: Create `nano-ros-transport-smoltcp` crate

**Effort:** 2-3 days
**Dependencies:** 32.1

Extract the TCP implementation from `nano-ros-bsp-qemu` into a standalone transport crate at `packages/transport/nano-ros-transport-smoltcp/`.

**What moves:**
- `bridge.rs` — `SmoltcpBridge` struct, socket table, RX/TX buffer management, `poll()` method
- `buffers.rs` — TCP socket buffer allocation
- TCP symbol implementations — rewrite to implement zenoh-pico symbols directly (`_z_open_tcp`, `_z_send_tcp`, etc.) instead of the custom `smoltcp_*` FFI

**What's new:**
- `poll.rs` — poll callback registration (`set_poll_callback(fn())`)
- Direct `#[unsafe(no_mangle)] extern "C"` implementations of `_z_open_tcp`, `_z_close_tcp`, `_z_send_tcp`, `_z_read_tcp`, `_z_read_exact_tcp`, `_z_listen_tcp`, `_z_create_endpoint_tcp`, `_z_free_endpoint_tcp`
- Uses `extern "C" { fn z_clock_now() -> u64; }` for timeouts (link-time resolution from platform crate)

**What's removed:**
- All `smoltcp_*` custom FFI symbols (`smoltcp_socket_open`, `smoltcp_socket_connect`, etc.)

```toml
# packages/transport/nano-ros-transport-smoltcp/Cargo.toml
[package]
name = "nano-ros-transport-smoltcp"

[dependencies]
zenoh-pico-shim-sys = { path = "../zenoh-pico-shim-sys", features = ["bare-metal", "link-tcp"] }
smoltcp = { version = "0.12", default-features = false, features = ["medium-ethernet", "proto-ipv4", "socket-tcp"] }
```

**Deliverables:**
- New crate at `packages/transport/nano-ros-transport-smoltcp/`
- Implements all TCP symbols from the zenoh-pico platform API
- `SmoltcpBridge` with public `poll()` and `set_poll_callback()` API
- Unit tests for bridge socket management

### 32.3: Create `nano-ros-platform-qemu` crate

**Effort:** 2-3 days
**Dependencies:** 32.2

Extract the system primitives from `nano-ros-bsp-qemu` into a standalone platform crate at `packages/platform/nano-ros-platform-qemu/`.

**What moves:**
- `clock.rs` — rewrite to implement `z_clock_now` directly (not `smoltcp_clock_now_ms`)
- `libc_stubs.rs` — `strlen`, `memcpy`, `memset`, `memcmp`, `strtoul`, etc.
- `config.rs` — network configuration (IP address, gateway, etc.)
- `node.rs` — `run_node()` and poll callback wiring
- `timing.rs` — DWT `CycleCounter`

**What's new:**
- Direct `#[unsafe(no_mangle)] extern "C"` implementations of:
  - Memory: `z_malloc`, `z_realloc`, `z_free`
  - RNG: `z_random_u8`, `z_random_u16`, `z_random_u32`, `z_random_u64`, `z_random_fill`
  - Clock: `z_clock_now`, `z_clock_elapsed_us/ms/s`, `z_clock_advance_us/ms/s`
  - Time: `z_time_now`, `z_time_now_as_str`, `z_time_elapsed_us/ms/s`, `_z_get_time_since_epoch`
  - Sleep: `z_sleep_us`, `z_sleep_ms`, `z_sleep_s`
  - Socket helpers: `_z_socket_close`, `_z_socket_wait_event`, `_z_socket_accept`, `_z_socket_set_non_blocking`
  - Threading stubs: `_z_task_*`, `_z_mutex_*`, `_z_condvar_*` (all no-ops)

**What's removed:**
- All `smoltcp_*` custom FFI symbols from platform side
- Dependency on the C shim `system.c`

```toml
# packages/platform/nano-ros-platform-qemu/Cargo.toml
[package]
name = "nano-ros-platform-qemu"

[dependencies]
zenoh-pico-shim-sys = { path = "../../transport/zenoh-pico-shim-sys", features = ["bare-metal"] }
nano-ros-transport-smoltcp = { path = "../../transport/nano-ros-transport-smoltcp" }
lan9118-smoltcp = { path = "../../drivers/lan9118-smoltcp" }
cortex-m = "0.7"
cortex-m-rt = "0.7"
cortex-m-semihosting = "0.5"
panic-semihosting = "0.6"
nano-ros-core = { path = "../../core/nano-ros-core", default-features = false }
heapless = "0.8"
```

**Deliverables:**
- New crate at `packages/platform/nano-ros-platform-qemu/`
- Implements all system symbols from the zenoh-pico platform API
- Wires `nano-ros-transport-smoltcp::set_poll_callback()` in `z_sleep_ms`
- `run_node()` API preserved (or improved) for application use

### 32.4: Remove C shim layer

**Effort:** 1 day
**Dependencies:** 32.2, 32.3

Remove the C shim files that translated between zenoh-pico symbols and custom `smoltcp_*` FFI. These are no longer needed because Rust crates implement zenoh-pico symbols directly.

**Files to remove:**
- `packages/transport/zenoh-pico-shim-sys/c/platform_smoltcp/system.c`
- `packages/transport/zenoh-pico-shim-sys/c/platform_smoltcp/network.c`

**Files to update:**
- `zenoh-pico-shim-sys/build.rs` — stop compiling `system.c` and `network.c` when `bare-metal` feature is active
- `zenoh-pico-shim-sys/cbindgen.toml` — remove `smoltcp_*` symbols from export list
- `zenoh-pico-shim-sys/c/include/zenoh_shim.h` — remove `smoltcp_*` declarations

**Preserve:**
- `zenoh_bare_metal_platform.h` — still needed for platform type definitions
- `zenoh_generic_platform.h` — redirect header
- `zenoh_generic_config.h` — replaced by generated config header but keep for reference
- `zenoh_shim.c` — nano-ros's own simplified wrapper (not part of the shim layer)

**Deliverables:**
- C shim files removed
- `build.rs` updated to skip them for `bare-metal`
- All `smoltcp_*` symbols eliminated from the codebase
- `cbindgen.toml` export list cleaned up

### 32.5: Migrate `nano-ros-bsp-qemu` to wrapper

**Effort:** 1-2 days
**Dependencies:** 32.3

Convert `nano-ros-bsp-qemu` from a monolithic implementation to a thin wrapper that re-exports from the new platform and transport crates. Examples are updated to use the new crate names directly.

```toml
# packages/bsp/nano-ros-bsp-qemu/Cargo.toml
[dependencies]
nano-ros-platform-qemu = { path = "../../platform/nano-ros-platform-qemu" }
nano-ros-transport-smoltcp = { path = "../../transport/nano-ros-transport-smoltcp" }
# ... other deps unchanged
```

```rust
// packages/bsp/nano-ros-bsp-qemu/src/lib.rs
pub use nano_ros_platform_qemu::*;
pub use nano_ros_transport_smoltcp::SmoltcpBridge;
// Re-export everything examples currently use
```

**Deliverables:**
- `nano-ros-bsp-qemu` is now a thin re-export wrapper (deleted in 32.10 tidy)
- QEMU examples updated to depend on `nano-ros-platform-qemu` + `nano-ros-transport-smoltcp` directly
- `just test-qemu` passes
- `just quality` passes

### 32.6: Migrate ESP32-C3 BSPs

**Effort:** 2-3 days
**Dependencies:** 32.5

Apply the same platform/transport split to the ESP32-C3 BSPs:

- Create `packages/platform/nano-ros-platform-esp32/` from `nano-ros-bsp-esp32`
- Create `packages/platform/nano-ros-platform-esp32-qemu/` from `nano-ros-bsp-esp32-qemu`
- Both reuse `nano-ros-transport-smoltcp` (same TCP transport, different Ethernet drivers)
- Convert original BSP crates to thin re-export wrappers

**Key differences from QEMU ARM:**
- ESP32 uses `esp_hal::time::Instant` instead of DWT for clock
- ESP32-C3 WiFi BSP uses a WiFi driver instead of LAN9118
- ESP32-C3 QEMU BSP uses OpenETH driver instead of LAN9118
- Heap allocation uses ESP32-specific allocator

**Deliverables:**
- Two new platform crates at `packages/platform/`
- Both ESP32 BSP crates converted to thin wrappers
- `just test-qemu-esp32` passes
- ESP32 examples unchanged

### 32.7: Migrate STM32F4 BSP

**Effort:** 1-2 days
**Dependencies:** 32.5

Apply the platform/transport split to the STM32F4 BSP:

- Create `packages/platform/nano-ros-platform-stm32f4/` from `nano-ros-bsp-stm32f4`
- Reuses `nano-ros-transport-smoltcp`
- Convert original BSP crate to thin re-export wrapper

The STM32F4 BSP is simpler (no bridge.rs — it uses a different networking approach via `platform.rs` and `phy.rs`). The platform extraction is more straightforward.

**Deliverables:**
- New platform crate at `packages/platform/nano-ros-platform-stm32f4/`
- STM32F4 BSP crate converted to thin wrapper
- STM32F4 examples unchanged

### 32.8: Update feature flag chain

**Effort:** 1 day
**Dependencies:** 32.5

Replace the `shim-*` feature names with `platform-*` names across all crates:

```
nano-ros (top-level)
├── zenoh             → nano-ros-node/zenoh → nano-ros-transport/zenoh
├── platform-posix    → ... → zenoh-pico-shim-sys/posix
├── platform-zephyr   → ... → zenoh-pico-shim-sys/zephyr
├── platform-bare-metal → ... → zenoh-pico-shim-sys/bare-metal
├── polling           → nano-ros-node/polling
└── rtic              → nano-ros-node/rtic
```

**Changes:**
- Rename `shim-posix` → `platform-posix`, `shim-zephyr` → `platform-zephyr`, `shim-smoltcp` → `platform-bare-metal` in `nano-ros`, `nano-ros-node`, `nano-ros-transport`
- Remove old `shim-*` feature names (no aliases)
- Rename `posix` / `zephyr` / `smoltcp` features in `zenoh-pico-shim-sys` to `posix` / `zephyr` / `bare-metal` (remove `smoltcp` alias)

**Deliverables:**
- New feature names used throughout the crate chain
- Old `shim-*` and `smoltcp` feature names removed
- `CLAUDE.md` updated with new feature names

### 32.9: Update examples and documentation

**Effort:** 1-2 days
**Dependencies:** 32.5, 32.6, 32.7, 32.8

Update all examples, documentation, and CLAUDE.md to use the new architecture:

- Update CLAUDE.md workspace structure to show `packages/platform/` directory
- Update `docs/design/platform-transport-architecture.md` to mark "Current State" sections as complete
- Update example Cargo.toml files to use new crate names (or keep BSP re-exports)
- Add a `packages/platform/README.md` explaining the platform/transport split
- Update `docs/guides/creating-examples.md` if bare-metal example creation instructions change

**Deliverables:**
- All documentation reflects the new architecture
- CLAUDE.md workspace structure updated
- Examples use consistent crate naming

### 32.10: Integration testing and tidy

**Effort:** 1-2 days
**Dependencies:** All above

Full test sweep to ensure nothing is broken:

```bash
just quality          # Format + clippy + unit tests + miri + QEMU examples
just test-integration # Integration tests
just test-qemu        # QEMU bare-metal tests
just test-qemu-esp32  # ESP32-C3 QEMU tests
just test-c           # C API tests
```

**Tidy jobs** (no backwards compat maintained after this phase):
- Remove `smoltcp-platform-rust` feature from `zenoh-pico-shim-sys`
- Remove `smoltcp` alias feature from `zenoh-pico-shim-sys` (done in 32.8)
- Remove all `shim-*` feature names from `nano-ros`, `nano-ros-node`, `nano-ros-transport`
- Delete BSP wrapper crates (`packages/bsp/nano-ros-bsp-{qemu,esp32,esp32-qemu,stm32f4}/`) — examples depend on platform+transport crates directly
- Remove any remaining `smoltcp_*` symbol references
- Remove static `zenoh_generic_config.h` from `c/platform_smoltcp/` (replaced by generated header)
- Delete `c/platform_smoltcp/` directory entirely (C shim removed in 32.4)
- Clean up unused imports and dead code across all touched crates
- Remove `packages/bsp/` directory if empty (only `nano-ros-bsp-zephyr` may remain)
- Audit `.cargo/config.toml` in examples for stale `[patch.crates-io]` entries referencing old BSP crates

**Deliverables:**
- All test suites pass
- No deprecated symbols, features, or aliases remain
- No BSP wrapper crates (direct platform+transport deps only)
- Clean clippy output

## Dependency Graph

```
32.1 (link-* features)
  │
  ├──→ 32.2 (transport-smoltcp) ──→ 32.4 (remove C shim)
  │                                       │
  │     32.3 (platform-qemu) ─────────────┤
  │       │                               │
  │       ├──→ 32.5 (migrate bsp-qemu) ───┤
  │       │         │                      │
  │       │         ├──→ 32.6 (ESP32)      │
  │       │         ├──→ 32.7 (STM32F4)    │
  │       │         └──→ 32.8 (features)   │
  │       │                   │            │
  │       └───────────────────┴────→ 32.9 (docs) ──→ 32.10 (testing)
```

## New Directory Structure

```
packages/
├── platform/                              # NEW: Platform crates (system primitives)
│   ├── nano-ros-platform-qemu/            # QEMU MPS2-AN385 (from bsp-qemu)
│   ├── nano-ros-platform-esp32/           # ESP32-C3 WiFi (from bsp-esp32)
│   ├── nano-ros-platform-esp32-qemu/      # ESP32-C3 QEMU (from bsp-esp32-qemu)
│   └── nano-ros-platform-stm32f4/         # STM32F4 (from bsp-stm32f4)
├── transport/                             # Transport crates
│   ├── nano-ros-transport-smoltcp/        # NEW: TCP/UDP via smoltcp
│   ├── zenoh-pico-shim/                   # Existing: safe Rust API
│   └── zenoh-pico-shim-sys/               # Existing: FFI + zenoh-pico build
├── bsp/                                   # BSP crates (only Zephyr remains)
│   └── nano-ros-bsp-zephyr/               # Unchanged (Zephyr uses zenoh-pico's own backend)
├── drivers/                               # Hardware drivers (unchanged)
│   ├── lan9118-smoltcp/
│   └── openeth-smoltcp/
└── ...
```

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Symbol name conflicts during migration | Incremental approach: create new crates first, then migrate BSPs one at a time |
| Breaking existing examples | BSP crates become thin wrappers — re-export everything, API unchanged |
| Link-time resolution failures | Test each platform immediately after creating its platform/transport crates |
| C shim removal breaks something | Remove C shim only after new Rust implementations are verified working |
| Feature flag complexity | Remove old names outright in 32.8; no aliases maintained |

## Estimated Total Effort

| Phase | Days |
|-------|------|
| 32.1: link-* features | 0.5 |
| 32.2: transport-smoltcp | 2-3 |
| 32.3: platform-qemu | 2-3 |
| 32.4: Remove C shim | 1 |
| 32.5: Migrate bsp-qemu | 1-2 |
| 32.6: Migrate ESP32 BSPs | 2-3 |
| 32.7: Migrate STM32F4 BSP | 1-2 |
| 32.8: Feature flags | 1 |
| 32.9: Docs update | 1-2 |
| 32.10: Testing & cleanup | 1-2 |
| **Total** | **13-21 days** |

## Future Work (Not in This Phase)

These are enabled by the architecture split but out of scope for Phase 32:

- **`nano-ros-transport-serial`** — Serial/UART transport crate (Phase 33 candidate)
- **`nano-ros-transport-raweth`** — Raw Ethernet transport crate
- **UDP support in transport-smoltcp** — Add UDP unicast/multicast to the smoltcp bridge
- **New platform backends** — FreeRTOS, ESP-IDF, RPi Pico (reuse zenoh-pico's own backends)
- **Remove BSP wrapper crates** — Delete the thin re-export wrappers once all examples use platform+transport deps directly

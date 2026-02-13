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
nano-ros-transport-zenoh-sys[smoltcp]
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
packages/transport/
  nano-ros-transport-zenoh-sys[bare-metal]     (renamed from nano-ros-transport-zenoh-sys)
  ├── build.rs generates config header from Cargo features (link-tcp, link-serial, etc.)
  ├── zenoh_bare_metal_platform.h (platform type definitions)
  ├── zenoh_shim.c (nano-ros's simplified wrapper — unchanged)
  ├── No C shim files (system.c, network.c removed)
  └── Compiles zenoh-pico with feature-gated Z_FEATURE_LINK_* flags

  nano-ros-transport-zenoh                     (renamed from nano-ros-transport-zenoh)
  └── Safe Rust API wrapping nano-ros-transport-zenoh-sys

packages/platform/
  nano-ros-platform-qemu (system primitives only)
  ├── lib.rs          # z_malloc, z_random_*, z_clock_*, z_sleep_*, z_time_*
  │                     _z_task_* stubs, _z_mutex_* stubs, _z_condvar_* stubs
  │                     _z_socket_close, _z_socket_wait_event, _z_socket_accept, _z_socket_set_non_blocking
  ├── libc_stubs.rs   # strlen, memcpy, strtoul, ...
  ├── config.rs       # Network configuration
  ├── node.rs         # run_node(), poll callback registration
  └── timing.rs       # DWT cycle counter

packages/link/
  nano-ros-link-smoltcp (TCP/UDP via smoltcp IP stack)
  ├── lib.rs          # _z_create_endpoint_tcp, _z_free_endpoint_tcp
  │                     _z_open_tcp, _z_listen_tcp, _z_close_tcp
  │                     _z_read_tcp, _z_read_exact_tcp, _z_send_tcp
  ├── bridge.rs       # SmoltcpBridge: socket table, RX/TX buffers, poll()
  └── poll.rs         # Poll callback slot (Rust fn pointer, not FFI)

Custom symbols: none. All FFI symbols are zenoh-pico's standard platform API.
```

## Work Items

### 32.1: Add `link-*` Cargo features to `nano-ros-transport-zenoh-sys` — Complete

**Effort:** 0.5 day
**Dependencies:** None

Add Cargo features that control which `Z_FEATURE_LINK_*` flags are passed to zenoh-pico's CMake build:

```toml
# nano-ros-transport-zenoh-sys/Cargo.toml
[features]
bare-metal = []        # New: replaces smoltcp as the bare-metal platform selector
link-tcp = []          # sets Z_FEATURE_LINK_TCP=1
link-udp-unicast = []  # sets Z_FEATURE_LINK_UDP_UNICAST=1
link-udp-multicast = [] # sets Z_FEATURE_LINK_UDP_MULTICAST=1
link-serial = []       # sets Z_FEATURE_LINK_SERIAL=1
link-raweth = []       # sets Z_FEATURE_RAWETH_TRANSPORT=1
```

Update `build.rs` to generate a config header from these features instead of using the hardcoded `zenoh_generic_config.h`. The `smoltcp` feature is a temporary alias for `bare-metal` + `link-tcp` until 32.8 removes it.

**Work items:**
- [x] Add `bare-metal`, `link-tcp`, `link-udp-unicast`, `link-udp-multicast`, `link-serial`, `link-raweth` features to `Cargo.toml`
- [x] Add `smoltcp` alias feature (`bare-metal` + `link-tcp`)
- [x] Update `build.rs` to read `CARGO_FEATURE_LINK_*` env vars
- [x] Generate `zenoh_generic_config.h` in `OUT_DIR` with `Z_FEATURE_LINK_*` values from Cargo features
- [x] Ensure generated config header takes precedence over static one in include path

**Passing criteria:**
- [x] `cargo check -p nano-ros-transport-zenoh-sys --features smoltcp` succeeds
- [x] `cargo check -p nano-ros-transport-zenoh-sys --features bare-metal,link-tcp` succeeds
- [x] `cargo check -p nano-ros-transport-zenoh-sys --features bare-metal,link-tcp,link-serial` succeeds
- [x] QEMU BSP examples build unchanged (`cd examples/qemu/bsp-talker && cargo build --release`)
- [x] `just quality` passes

### 32.2: Create `nano-ros-link-smoltcp` crate — Complete

**Effort:** 2-3 days
**Dependencies:** 32.1

Extract the TCP implementation from `nano-ros-bsp-qemu` into a standalone transport crate at `packages/transport/nano-ros-link-smoltcp/`.

**What moves:**
- `bridge.rs` — `SmoltcpBridge` struct, socket table, RX/TX buffer management, `poll()` method
- `buffers.rs` — TCP socket buffer allocation
- TCP symbol implementations — rewrite to implement zenoh-pico symbols directly (`_z_open_tcp`, `_z_send_tcp`, etc.) instead of the custom `smoltcp_*` FFI

**What's new:**
- Poll callback in `bridge.rs` — `set_poll_callback(fn())` + `smoltcp_poll()` FFI export
- Direct `#[unsafe(no_mangle)] extern "C"` implementations of `_z_open_tcp`, `_z_close_tcp`, `_z_send_tcp`, `_z_read_tcp`, `_z_read_exact_tcp`, `_z_listen_tcp`, `_z_create_endpoint_tcp`, `_z_free_endpoint_tcp` in `tcp.rs`
- Uses `extern "C" { fn smoltcp_clock_now_ms() -> u64; }` for timeouts (link-time resolution from platform crate)
- Convenience functions: `get_socket_storage()`, `get_tcp_buffers()`, `create_and_register_sockets()`
- Re-exports smoltcp types (`Interface`, `SocketSet`, `SocketStorage`, `TcpSocket`, `TcpSocketBuffer`, `Device`)

**What stays in BSPs (until 32.5 migration):**
- Legacy `smoltcp_*` FFI symbols exported from `bridge.rs` for backwards compat
- `c-network-shim` feature in `nano-ros-transport-zenoh-sys` gates `network.c` compilation

```toml
# packages/transport/nano-ros-link-smoltcp/Cargo.toml
[package]
name = "nano-ros-link-smoltcp"

[dependencies]
nano-ros-transport-zenoh-sys = { path = "../nano-ros-transport-zenoh-sys", features = ["bare-metal", "link-tcp"] }
smoltcp = { version = "0.12", default-features = false, features = ["medium-ethernet", "proto-ipv4", "socket-tcp"] }
```

**Work items:**
- [x] Create `Cargo.toml` with `nano-ros-transport-zenoh-sys` (bare-metal + link-tcp) and `smoltcp` deps
- [x] Create `src/lib.rs` — `#![no_std]`, public API, smoltcp re-exports, convenience functions
- [x] Create `src/bridge.rs` — `SmoltcpBridge` with socket table, staging buffers, `poll()`, internal socket ops, legacy FFI exports, poll callback
- [x] Create `src/tcp.rs` — Rust implementations of `_z_open_tcp`, `_z_close_tcp`, `_z_read_tcp`, `_z_read_exact_tcp`, `_z_send_tcp`, `_z_listen_tcp`, `_z_create_endpoint_tcp`, `_z_free_endpoint_tcp`
- [x] Add `c-network-shim` feature to `nano-ros-transport-zenoh-sys/Cargo.toml`
- [x] Update `smoltcp` alias to include `c-network-shim` (backwards compat)
- [x] Gate `network.c` compilation in `build_c_shim()` on `use_c_network_shim`
- [x] Gate `network.c` compilation in `build_zenoh_pico_embedded()` on `use_c_network_shim`
- [x] Add crate to workspace `exclude` list in root `Cargo.toml`

**Passing criteria:**
- [x] `cargo check --target thumbv7m-none-eabi` in transport crate succeeds with zero warnings
- [x] `cargo check -p nano-ros-transport-zenoh-sys --features smoltcp` succeeds (includes `c-network-shim`)
- [x] `cargo check -p nano-ros-transport-zenoh-sys --features bare-metal,link-tcp` succeeds (no `network.c`)
- [x] QEMU BSP examples build unchanged (`cd examples/qemu/bsp-talker && cargo build --release`)
- [x] `just quality` passes (format + clippy + 418 unit tests + miri + QEMU examples)

### 32.3: Create `nano-ros-platform-qemu` crate — Complete

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
nano-ros-transport-zenoh-sys = { path = "../../transport/nano-ros-transport-zenoh-sys", features = ["bare-metal"] }
nano-ros-link-smoltcp = { path = "../../transport/nano-ros-link-smoltcp" }
lan9118-smoltcp = { path = "../../drivers/lan9118-smoltcp" }
cortex-m = "0.7"
cortex-m-rt = "0.7"
cortex-m-semihosting = "0.5"
panic-semihosting = "0.6"
nano-ros-core = { path = "../../core/nano-ros-core", default-features = false }
heapless = "0.8"
```

**Work items:**
- [x] Create `packages/platform/nano-ros-platform-qemu/Cargo.toml` with deps on `nano-ros-transport-zenoh-sys` (bare-metal, no `c-network-shim`), `nano-ros-link-smoltcp`, `lan9118-smoltcp`
- [x] Create `src/lib.rs` — `#![no_std]`, module declarations, public API
- [x] Move `clock.rs` — rewrite `smoltcp_clock_now_ms` to `z_clock_now`, add `z_clock_elapsed_*`, `z_clock_advance_*`
- [x] Move `libc_stubs.rs` — `strlen`, `memcpy`, `memset`, `memcmp`, `strtoul`, etc.
- [x] Create `src/memory.rs` — `z_malloc`, `z_realloc`, `z_free` (bump allocator from BSP `bridge.rs`)
- [x] Create `src/random.rs` — `z_random_u8/u16/u32/u64`, `z_random_fill` (LFSR from BSP `bridge.rs`)
- [x] Create `src/time.rs` — `z_time_now`, `z_time_now_as_str`, `z_time_elapsed_*`, `_z_get_time_since_epoch`
- [x] Create `src/sleep.rs` — `z_sleep_us`, `z_sleep_ms`, `z_sleep_s` (calls `SmoltcpBridge::poll_network()` during busy-wait)
- [x] Create `src/socket.rs` — `_z_socket_close`, `_z_socket_wait_event`, `_z_socket_accept`, `_z_socket_set_non_blocking` (stubs)
- [x] Create `src/threading.rs` — `_z_task_*`, `_z_mutex_*`, `_z_condvar_*` stubs
- [x] Move `config.rs` — network configuration (IP, gateway, MAC)
- [x] Move `node.rs` — `run_node()` wiring (registers poll callback via `nano-ros-link-smoltcp`)
- [x] Move `timing.rs` — DWT `CycleCounter`
- [x] Add crate to workspace `exclude` list

**Passing criteria:**
- [x] `cargo check --target thumbv7m-none-eabi` in platform crate succeeds with zero warnings
- [x] Platform crate depends on `nano-ros-transport-zenoh-sys` with `bare-metal` only (no `c-network-shim`)
- [x] Platform crate does NOT export any `smoltcp_*` symbols
- [x] All zenoh-pico system symbols resolved at link time (no undefined symbol errors when linked with transport crate)
- [x] `just quality` passes

### 32.4: Decouple C shim layer — Complete

**Effort:** 1 day
**Dependencies:** 32.2, 32.3

Decouple the C shim files (`system.c`, `network.c`) from the default bare-metal path. New platform/transport crates use `bare-metal` + `link-tcp` without C shims; old BSPs explicitly opt in to `c-network-shim` + `c-system-shim`. The `smoltcp_*` symbols were removed from the public API header (`zenoh_shim.h`).

**Note:** `system.c` and `network.c` are NOT deleted yet — BSPs still need them until migrated in 32.5-32.7. File deletion is deferred to 32.12 tidy.

**Work items:**
- [x] Add `c-system-shim` feature to `nano-ros-transport-zenoh-sys/Cargo.toml` (gates `system.c`, parallel to `c-network-shim`) *(done in 32.3 as prerequisite)*
- [x] Update `smoltcp` alias to include `c-system-shim` *(done in 32.3 as prerequisite)*
- [x] Gate `system.c` compilation in `build_c_shim()` on `use_c_system_shim` *(done in 32.3 as prerequisite)*
- [x] Gate `system.c` compilation in `build_zenoh_pico_embedded()` on `use_c_system_shim` *(done in 32.3 as prerequisite)*
- [x] Remove `c-network-shim` and `c-system-shim` from `smoltcp` alias (`smoltcp = ["bare-metal", "link-tcp"]`)
- [x] Add `c-network-shim` + `c-system-shim` to `smoltcp-platform-rust` alias (needs C shims for z_* → smoltcp_* translation)
- [x] Add `c-network-shim` + `c-system-shim` explicitly to BSP Cargo.toml files (4 BSPs)
- [x] Remove `smoltcp_*` cbindgen stubs from `ffi.rs` and `PollCallback` type
- [x] Remove `smoltcp_*` entries from `cbindgen.toml` export list, add to exclude list
- [x] Remove `PollCallback` re-export from `nano-ros-transport-zenoh/src/lib.rs`
- [x] Regenerate `zenoh_shim.h` — no `smoltcp_*` declarations
- [x] Add `smoltcp_clock_now_ms` extern declaration to `zenoh_shim.c` (was previously in generated header)
- [x] Gate `build.rs` rerun-if-changed for `system.c`/`network.c` on their respective features
- [ ] ~~Delete `c/platform_smoltcp/network.c`~~ *(deferred to 32.12 — BSPs still need them)*
- [ ] ~~Delete `c/platform_smoltcp/system.c`~~ *(deferred to 32.12 — BSPs still need them)*

**Passing criteria:**
- [x] No `smoltcp_*` symbols in `zenoh_shim.h` (public API header)
- [x] `cargo check -p nano-ros-transport-zenoh-sys --features smoltcp` succeeds (smoltcp alias no longer includes C shims)
- [x] `cargo check -p nano-ros-transport-zenoh-sys --features bare-metal,link-tcp` succeeds (no C shim files)
- [x] QEMU BSP examples still build (BSPs explicitly opt in to C shim features)
- [x] Platform crate builds (`cargo check --target thumbv7m-none-eabi` in `nano-ros-platform-qemu`)
- [x] `just quality` passes

### 32.5: Migrate `nano-ros-bsp-qemu` to wrapper

**Effort:** 1-2 days
**Dependencies:** 32.3

Convert `nano-ros-bsp-qemu` from a monolithic implementation to a thin wrapper, then migrate all QEMU examples to depend on `nano-ros-platform-qemu` directly instead of the BSP wrapper.

```toml
# packages/bsp/nano-ros-bsp-qemu/Cargo.toml
[dependencies]
nano-ros-platform-qemu = { path = "../../platform/nano-ros-platform-qemu" }
```

```rust
// packages/bsp/nano-ros-bsp-qemu/src/lib.rs (11 lines total)
#![no_std]
pub use nano_ros_platform_qemu::*;
```

**Work items:**
- [x] Update `nano-ros-bsp-qemu/Cargo.toml` to depend only on `nano-ros-platform-qemu`
- [x] Replace `nano-ros-bsp-qemu/src/lib.rs` with `pub use nano_ros_platform_qemu::*;` (11 lines)
- [x] Delete all old source files: `bridge.rs`, `buffers.rs`, `clock.rs`, `config.rs`, `error.rs`, `libc_stubs.rs`, `node.rs`, `publisher.rs`, `subscriber.rs`, `timing.rs`
- [x] Keep `build.rs` + `mps2-an385.x` (linker script needed by cortex-m-rt)
- [ ] Migrate QEMU examples to depend on `nano-ros-platform-qemu` directly:
  - [ ] `examples/qemu/bsp-talker` — `Cargo.toml` + `src/main.rs` (`use nano_ros_platform_qemu::`)
  - [ ] `examples/qemu/bsp-listener` — `Cargo.toml` + `src/main.rs`
  - [ ] `examples/qemu/rs-talker` — `Cargo.toml` + `src/main.rs`
  - [ ] `examples/qemu/rs-listener` — `Cargo.toml` + `src/main.rs`
  - [ ] `examples/qemu/rs-wcet-bench` — `src/main.rs` (references BSP crate)
  - [ ] Update `.cargo/config.toml` in each example for new `[patch.crates-io]` entries
  - [ ] Move `build.rs` + `mps2-an385.x` linker script to platform crate (examples no longer pull BSP)

**Passing criteria:**
- [x] `nano-ros-bsp-qemu` `lib.rs` is a thin re-export (11 lines)
- [x] All 4 QEMU BSP examples build: `bsp-talker`, `bsp-listener`, `rs-talker`, `rs-listener`
- [ ] No QEMU examples depend on `nano-ros-bsp-qemu` (all use `nano-ros-platform-qemu` directly)
- [ ] `just test-all` passes

### 32.6: Migrate ESP32-C3 BSPs — Complete

**Effort:** 2-3 days
**Dependencies:** 32.5

Apply the same platform/transport split to the ESP32-C3 BSPs:

- Create `packages/platform/nano-ros-platform-esp32/` from `nano-ros-bsp-esp32`
- Create `packages/platform/nano-ros-platform-esp32-qemu/` from `nano-ros-bsp-esp32-qemu`
- Both reuse `nano-ros-link-smoltcp` (same TCP transport, different Ethernet drivers)
- Convert original BSP crates to thin re-export wrappers

**Key differences from QEMU ARM:**
- ESP32 uses `esp_hal::time::Instant` instead of DWT for clock
- ESP32-C3 WiFi BSP uses a WiFi driver instead of LAN9118
- ESP32-C3 QEMU BSP uses OpenETH driver instead of LAN9118
- Heap allocation uses ESP32-specific allocator (32KB bump allocator for z_malloc + esp_alloc global allocator)
- `as_nanos()` not available on `esp_hal::time::Duration` — `CycleCounter` uses `as_micros()` instead
- WiFi BSP doesn't need snprintf/sprintf stubs (esp-radio provides them)
- WiFi BSP needs additional deps: `esp-rtos`, `esp-radio`, `critical-section`, smoltcp DHCP features

**Work items:**
- [x] Create `packages/platform/nano-ros-platform-esp32/` from `nano-ros-bsp-esp32`
  - [x] Extract system primitives (clock via `esp_hal::time::Instant`, bump allocator, LFSR RNG)
  - [x] Wire `nano-ros-link-smoltcp` poll callback
  - [x] WiFi-specific: `esp-rtos` scheduler, `esp-radio` WiFi init, DHCP polling
  - [x] Config types: `WifiConfig`, `IpMode`, `NodeConfig`
- [x] Create `packages/platform/nano-ros-platform-esp32-qemu/` from `nano-ros-bsp-esp32-qemu`
  - [x] Extract system primitives (clock, 32KB bump allocator, errno shadow)
  - [x] Use OpenETH driver instead of LAN9118
  - [x] Zenoh open retry loop (5 attempts, for TAP networking stability)
- [x] Convert `nano-ros-bsp-esp32` to thin re-export wrapper
- [x] Convert `nano-ros-bsp-esp32-qemu` to thin re-export wrapper
- [x] Migrate ESP32 examples to depend on platform crates directly:
  - [x] `examples/esp32/bsp-talker` — `Cargo.toml` + `src/main.rs` (`use nano_ros_platform_esp32::`)
  - [x] `examples/esp32/bsp-listener` — `Cargo.toml` + `src/main.rs`
  - [x] `examples/esp32/qemu-talker` — `Cargo.toml` + `src/main.rs` (`use nano_ros_platform_esp32_qemu::`)
  - [x] `examples/esp32/qemu-listener` — `Cargo.toml` + `src/main.rs`
  - [x] `.cargo/config.toml` files didn't need changes (no BSP crate name references)
  - [x] No linker scripts needed (ESP32 uses `linkall.x` from esp-hal, not from BSP)
- [x] Add both platform crates to workspace `exclude` list in root `Cargo.toml`

**Passing criteria:**
- [x] Both platform crates compile for `riscv32imc-unknown-none-elf` with zero warnings
- [x] `nano-ros-bsp-esp32` and `nano-ros-bsp-esp32-qemu` are thin re-exports (< 20 lines each)
- [x] No ESP32 examples depend on BSP crates (all use platform crates directly)
- [x] All 4 ESP32 examples compile: `bsp-talker`, `bsp-listener`, `qemu-talker`, `qemu-listener`
- [x] `just quality` passes
- [ ] `just test-all` passes

### 32.7: Migrate STM32F4 BSP

**Effort:** 1-2 days
**Dependencies:** 32.5

Apply the platform/transport split to the STM32F4 BSP:

- Create `packages/platform/nano-ros-platform-stm32f4/` from `nano-ros-bsp-stm32f4`
- Reuses `nano-ros-link-smoltcp`
- Convert original BSP crate to thin re-export wrapper

The STM32F4 BSP is simpler (no bridge.rs — it uses a different networking approach via `platform.rs` and `phy.rs`). The platform extraction is more straightforward.

**Work items:**
- [x] Create `packages/platform/nano-ros-platform-stm32f4/` from `nano-ros-bsp-stm32f4`
  - [x] Extract system primitives (clock, allocator, RNG, libc stubs)
  - [x] Wire `nano-ros-link-smoltcp` for networking
  - [x] DWT-based clock with `update_from_dwt()` for hardware-accurate timing
  - [x] PHY detection and pin configuration modules preserved
  - [x] Chip variant features (stm32f407..stm32f479) forwarded through
- [x] Convert `nano-ros-bsp-stm32f4` to thin re-export wrapper
- [x] Migrate STM32F4 examples to depend on platform crate directly:
  - [x] `examples/stm32f4/bsp-talker` — `Cargo.toml` + `src/main.rs` (`use nano_ros_platform_stm32f4::`)

**Passing criteria:**
- [x] Platform crate compiles for `thumbv7em-none-eabihf` with zero warnings
- [x] `nano-ros-bsp-stm32f4` is a thin re-export (< 20 lines)
- [x] No STM32F4 examples depend on BSP crate (all use platform crate directly)
- [x] `just quality` passes

### 32.8: Update feature flag chain

**Effort:** 1 day
**Dependencies:** 32.5

Replace the `shim-*` feature names with `platform-*` names across all crates:

```
nano-ros (top-level)
├── zenoh             → nano-ros-node/zenoh → nano-ros-transport/zenoh
├── platform-posix    → ... → nano-ros-transport-zenoh-sys/posix
├── platform-zephyr   → ... → nano-ros-transport-zenoh-sys/zephyr
├── platform-bare-metal → ... → nano-ros-transport-zenoh-sys/bare-metal
├── polling           → nano-ros-node/polling
└── rtic              → nano-ros-node/rtic
```

**Changes:**
- Rename `shim-posix` → `platform-posix`, `shim-zephyr` → `platform-zephyr`, `shim-smoltcp` → `platform-bare-metal` in `nano-ros`, `nano-ros-node`, `nano-ros-transport`
- Remove old `shim-*` feature names (no aliases)
- Rename `posix` / `zephyr` / `smoltcp` features in `nano-ros-transport-zenoh-sys` to `posix` / `zephyr` / `bare-metal` (remove `smoltcp` alias)

**Work items:**
- [x] Rename `shim-posix` to `platform-posix` in `nano-ros`, `nano-ros-node`, `nano-ros-transport`
- [x] Rename `shim-zephyr` to `platform-zephyr` in `nano-ros`, `nano-ros-node`, `nano-ros-transport`
- [x] Rename `shim-smoltcp` to `platform-bare-metal` in `nano-ros`, `nano-ros-node`, `nano-ros-transport`
- [x] Remove `smoltcp` alias feature from `nano-ros-transport-zenoh-sys` (keep only `bare-metal`)
- [x] Remove `c-network-shim` and `c-system-shim` features from `nano-ros-transport-zenoh-sys` (C shim deleted in 32.4)
- [x] Remove `smoltcp-platform-rust` feature from `nano-ros-transport-zenoh-sys`
- [x] Update all example `Cargo.toml` files using old feature names
- [x] Update native examples (use `zenoh` alias which maps to `platform-posix`)

**Passing criteria:**
- [x] No `shim-*` feature names remain in any `Cargo.toml`
- [x] No `smoltcp` feature name in `nano-ros-transport-zenoh-sys/Cargo.toml` (only `bare-metal`)
- [x] `cargo check --features zenoh,platform-posix` succeeds for workspace
- [x] All native examples build with new feature name
- [x] `CLAUDE.md` references new feature names
- [x] `just quality` passes

### 32.9: Move link crate to `packages/link/` — Complete

**Effort:** 0.5 day
**Dependencies:** 32.8

Move `nano-ros-transport-smoltcp` out of `packages/transport/` into a new `packages/link/` directory and rename it to `nano-ros-link-smoltcp`. This separates link crates (protocol-specific open/close/read/write) from transport crates (middleware like zenoh). The `link-*` naming matches zenoh-pico's link layer terminology and `nano-ros-transport-zenoh-sys`'s existing `link-tcp`, `link-serial`, `link-raweth` features.

**Work items:**
- [x] Create `packages/link/` directory
- [x] Move `packages/transport/nano-ros-transport-smoltcp/` → `packages/link/nano-ros-link-smoltcp/`
- [x] Update `Cargo.toml` package name to `nano-ros-link-smoltcp`
- [x] Update workspace `exclude` list in root `Cargo.toml`
- [x] Update `nano-ros-transport-zenoh-sys` dep path in the link crate (`../../transport/nano-ros-transport-zenoh-sys`)
- [x] Update platform crate dependencies (4 crates):
  - [x] `nano-ros-platform-qemu/Cargo.toml` — dep name + path (`../../link/nano-ros-link-smoltcp`)
  - [x] `nano-ros-platform-esp32/Cargo.toml` — dep name + path
  - [x] `nano-ros-platform-esp32-qemu/Cargo.toml` — dep name + path
  - [x] `nano-ros-platform-stm32f4/Cargo.toml` — dep name + path
- [x] Update `use nano_ros_transport_smoltcp` → `use nano_ros_link_smoltcp` in platform crate source files
- [x] Update doc comments referencing `nano-ros-transport-smoltcp` in platform crates and shim crates
- [x] Regenerate `Cargo.lock` files in all affected platform crates and examples
- [x] Update CLAUDE.md workspace structure and file locations

**Passing criteria:**
- [x] `packages/link/nano-ros-link-smoltcp/` exists, `packages/transport/nano-ros-transport-smoltcp/` does not
- [x] `grep -r nano-ros-transport-smoltcp packages/` returns zero hits (excluding Cargo.lock)
- [x] `cargo check --target thumbv7m-none-eabi` succeeds in the link crate
- [x] All platform crates compile for their respective targets
- [x] All QEMU examples build
- [x] `just quality` passes

### 32.10: Rename zenoh shim crates — Complete

**Effort:** 1 day
**Dependencies:** 32.9

Rename the zenoh-pico shim crates to follow the `nano-ros-transport-*` naming convention. The `packages/transport/` directory is reserved for transport middleware crates — zenoh is the only transport today.

| Old Name              | New Name                       | Old Path                                  | New Path                                           |
|-----------------------|--------------------------------|-------------------------------------------|----------------------------------------------------|
| `nano-ros-transport-zenoh`     | `nano-ros-transport-zenoh`     | `packages/transport/nano-ros-transport-zenoh/`     | `packages/transport/nano-ros-transport-zenoh/`     |
| `nano-ros-transport-zenoh-sys` | `nano-ros-transport-zenoh-sys` | `packages/transport/nano-ros-transport-zenoh-sys/` | `packages/transport/nano-ros-transport-zenoh-sys/` |

**Work items:**
- [ ] Rename directory `packages/transport/nano-ros-transport-zenoh/` → `packages/transport/nano-ros-transport-zenoh/`
- [ ] Rename directory `packages/transport/nano-ros-transport-zenoh-sys/` → `packages/transport/nano-ros-transport-zenoh-sys/`
- [ ] Update `Cargo.toml` package names in both crates
- [ ] Update workspace members in root `Cargo.toml`
- [ ] Update workspace `exclude` list (link crate, platform crates, BSP crates reference shim-sys)
- [ ] Update dependency references across the workspace:
  - [ ] `nano-ros-transport/Cargo.toml` — `nano-ros-transport-zenoh` dep name + path
  - [ ] `nano-ros-link-smoltcp/Cargo.toml` — `nano-ros-transport-zenoh-sys` dep name + path
  - [ ] All 4 platform crates — `nano-ros-transport-zenoh-sys` dep name + path
  - [ ] All BSP crates still referencing shim-sys — dep name + path
  - [ ] `.cargo/config.toml` patch entries in examples referencing old crate names
- [ ] Update `use nano_ros_transport_zenoh` → `use nano_ros_transport_zenoh` in `nano-ros-transport/src/`
- [ ] Update `use nano_ros_transport_zenoh_sys` → `use nano_ros_transport_zenoh_sys` in source files
- [ ] Update doc comments, CLAUDE.md file locations, and design docs
- [ ] Ensure zenoh-pico git submodule path is updated (currently at `nano-ros-transport-zenoh-sys/zenoh-pico/`)
- [ ] Regenerate `Cargo.lock` files in all affected crates and examples

**Passing criteria:**
- [ ] No `nano-ros-transport-zenoh` or `nano-ros-transport-zenoh-sys` package names in any `Cargo.toml` (paths and dep names all updated)
- [ ] `cargo check --features zenoh` succeeds for workspace
- [ ] `cargo check --target thumbv7m-none-eabi` succeeds for link crate and platform crates
- [ ] All examples build (native + QEMU + ESP32)
- [ ] `just quality` passes

### 32.11: Update examples and documentation

**Effort:** 1-2 days
**Dependencies:** 32.5, 32.6, 32.7, 32.8, 32.9, 32.10

Update all examples, documentation, and CLAUDE.md to use the new architecture:

- Update CLAUDE.md workspace structure to show `packages/platform/` directory
- Update `docs/design/platform-transport-architecture.md` to mark "Current State" sections as complete
- Update example Cargo.toml files to use new crate names (or keep BSP re-exports)
- Add a `packages/platform/README.md` explaining the platform/transport split
- Update `docs/guides/creating-examples.md` if bare-metal example creation instructions change

**Work items:**
- [ ] Update CLAUDE.md workspace structure to show `packages/platform/` directory
- [ ] Update CLAUDE.md "Platform Backends" section with new feature names
- [ ] Update `docs/design/platform-transport-architecture.md` to mark "Current State" as complete
- [ ] Update `docs/guides/creating-examples.md` for bare-metal example creation with platform + transport deps
- [ ] Update example `Cargo.toml` files to use new crate names directly (not BSP wrappers)
- [ ] Add `packages/platform/README.md` explaining the platform/transport split
- [ ] Update `docs/guides/getting-started.md` if BSP references changed

**Passing criteria:**
- [ ] CLAUDE.md workspace tree includes `packages/platform/` with all platform crates
- [ ] No references to old BSP crate names in documentation (except historical notes)
- [ ] `docs/guides/creating-examples.md` shows platform + transport dep pattern
- [ ] `just quality` passes

### 32.12: Integration testing and tidy

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
- Remove `smoltcp-platform-rust` feature from `nano-ros-transport-zenoh-sys`
- Remove `smoltcp` alias feature from `nano-ros-transport-zenoh-sys` (done in 32.8)
- Remove `c-network-shim` and `c-system-shim` features from `nano-ros-transport-zenoh-sys` (done in 32.8)
- Remove all `shim-*` feature names from `nano-ros`, `nano-ros-node`, `nano-ros-transport`
- Delete BSP wrapper crates (`packages/bsp/nano-ros-bsp-{qemu,esp32,esp32-qemu,stm32f4}/`) — examples depend on platform+transport crates directly
- Remove any remaining `smoltcp_*` symbol references
- Remove static `zenoh_generic_config.h` from `c/platform_smoltcp/` (replaced by generated header)
- Delete `c/platform_smoltcp/` directory entirely (C shim removed in 32.4)
- Remove `c_network_shim` / `c_system_shim` cfg gates and `check-cfg` entries from `nano-ros-transport-zenoh-sys`
- Clean up unused imports and dead code across all touched crates
- Remove `packages/bsp/` directory if empty (only `nano-ros-bsp-zephyr` may remain)
- Audit `.cargo/config.toml` in examples for stale `[patch.crates-io]` entries referencing old BSP crates

**Work items:**
- [ ] Run `just quality` — format + clippy + unit tests + miri + QEMU examples
- [ ] Run `just test-integration` — all Rust integration tests
- [ ] Run `just test-qemu` — QEMU bare-metal tests
- [ ] Run `just test-qemu-esp32` — ESP32-C3 QEMU tests
- [ ] Run `just test-c` — C API tests
- [ ] Remove `smoltcp-platform-rust` feature from `nano-ros-transport-zenoh-sys` (if not done in 32.8)
- [ ] Remove `c-network-shim` and `c-system-shim` features from `nano-ros-transport-zenoh-sys` (if not done in 32.8)
- [ ] Remove `c_network_shim` / `c_system_shim` cfg gates and `check-cfg` entries from `nano-ros-transport-zenoh-sys`
- [ ] Delete BSP wrapper crates: `packages/bsp/nano-ros-bsp-{qemu,esp32,esp32-qemu,stm32f4}/`
- [ ] Delete `c/platform_smoltcp/` directory entirely (if not done in 32.4)
- [ ] Remove any remaining `smoltcp_*` symbol references across codebase
- [ ] Clean up unused imports and dead code across all touched crates
- [ ] Remove `packages/bsp/` directory if empty (only `nano-ros-bsp-zephyr` may remain)
- [ ] Audit `.cargo/config.toml` in examples for stale `[patch.crates-io]` entries referencing old BSP crates
- [ ] Update workspace `exclude` list in root `Cargo.toml` (remove deleted BSPs, add platform crates)

**Passing criteria:**
- [ ] `just quality` passes
- [ ] `just test-integration` passes
- [ ] `just test-qemu` passes
- [ ] `just test-qemu-esp32` passes
- [ ] `just test-c` passes
- [ ] `grep -r smoltcp_ packages/` returns zero hits (no `smoltcp_*` symbols remain)
- [ ] `grep -r shim-posix packages/` returns zero hits (no `shim-*` features remain)
- [ ] `grep -r c-network-shim packages/` returns zero hits (transitional features removed)
- [ ] `grep -r c-system-shim packages/` returns zero hits (transitional features removed)
- [ ] No BSP wrapper crates in `packages/bsp/` (except `nano-ros-bsp-zephyr`)
- [ ] No `c/platform_smoltcp/` directory exists
- [ ] Clean clippy output with zero warnings

## Dependency Graph

```
32.1 (link-* features)
  │
  ├──→ 32.2 (link-smoltcp) ──→ 32.4 (remove C shim)
  │                                       │
  │     32.3 (platform-qemu) ─────────────┤
  │       │                               │
  │       ├──→ 32.5 (migrate bsp-qemu) ───┤
  │       │         │                      │
  │       │         ├──→ 32.6 (ESP32)      │
  │       │         ├──→ 32.7 (STM32F4)    │
  │       │         └──→ 32.8 (features)   │
  │       │                   │            │
  │       │                   └──→ 32.9 (move link crate)
  │       │                          │
  │       │                          └──→ 32.10 (rename zenoh crates)
  │       │                                 │
  │       └─────────────────────────────────┴──→ 32.11 (docs) ──→ 32.12 (testing)
```

## New Directory Structure

```
packages/
├── platform/                              # NEW: Platform crates (system primitives)
│   ├── nano-ros-platform-qemu/            # QEMU MPS2-AN385 (from bsp-qemu)
│   ├── nano-ros-platform-esp32/           # ESP32-C3 WiFi (from bsp-esp32)
│   ├── nano-ros-platform-esp32-qemu/      # ESP32-C3 QEMU (from bsp-esp32-qemu)
│   └── nano-ros-platform-stm32f4/         # STM32F4 (from bsp-stm32f4)
├── transport/                             # Transport middleware crates
│   ├── nano-ros-transport-zenoh/          # Safe Rust API (from nano-ros-transport-zenoh)
│   └── nano-ros-transport-zenoh-sys/      # FFI + zenoh-pico build (from nano-ros-transport-zenoh-sys)
├── link/                                  # NEW: Link protocol crates (bare-metal)
│   └── nano-ros-link-smoltcp/             # TCP/UDP via smoltcp
├── bsp/                                   # BSP crates (only Zephyr remains)
│   └── nano-ros-bsp-zephyr/               # Unchanged (Zephyr uses zenoh-pico's own backend)
├── drivers/                               # Hardware drivers (unchanged)
│   ├── lan9118-smoltcp/
│   └── openeth-smoltcp/
└── ...
```

## Risk Assessment

| Risk                                   | Mitigation                                                                     |
|----------------------------------------|--------------------------------------------------------------------------------|
| Symbol name conflicts during migration | Incremental approach: create new crates first, then migrate BSPs one at a time |
| Breaking existing examples             | BSP crates become thin wrappers — re-export everything, API unchanged          |
| Link-time resolution failures          | Test each platform immediately after creating its platform/transport crates    |
| C shim removal breaks something        | Remove C shim only after new Rust implementations are verified working         |
| Feature flag complexity                | Remove old names outright in 32.8; no aliases maintained                       |

## Estimated Total Effort

| Phase                          | Days           |
|--------------------------------|----------------|
| 32.1: link-* features          | 0.5            |
| 32.2: link-smoltcp              | 2-3            |
| 32.3: platform-qemu            | 2-3            |
| 32.4: Remove C shim            | 1              |
| 32.5: Migrate bsp-qemu         | 1-2            |
| 32.6: Migrate ESP32 BSPs       | 2-3            |
| 32.7: Migrate STM32F4 BSP      | 1-2            |
| 32.8: Feature flags            | 1              |
| 32.9: Move link crate          | 0.5            |
| 32.10: Rename zenoh crates     | 1              |
| 32.11: Docs update             | 1-2            |
| 32.12: Testing & cleanup       | 1-2            |
| **Total**                      | **14.5-22.5 days** |

## Future Work (Not in This Phase)

These are enabled by the architecture split but out of scope for Phase 32:

- **`nano-ros-link-serial`** — Serial/UART transport crate (Phase 33 candidate)
- **`nano-ros-link-raweth`** — Raw Ethernet transport crate
- **UDP support in link-smoltcp** — Add UDP unicast/multicast to the smoltcp bridge
- **New platform backends** — FreeRTOS, ESP-IDF, RPi Pico (reuse zenoh-pico's own backends)
- **Remove BSP wrapper crates** — Delete the thin re-export wrappers once all examples use platform+transport deps directly

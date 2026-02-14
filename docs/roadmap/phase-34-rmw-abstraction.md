# Phase 34: RMW Abstraction & XRCE-DDS Integration

**Status: In Progress** (34.1, 34.2, 34.3 complete)

**Prerequisites:** Phase 33 (crate rename + platform split) is complete. Phase 34.1-34.3 (RMW traits + zenoh impl + board refactor) are complete.

**Design docs:**
- `docs/design/rmw-layer-design.md` — Overall architecture and RMW layer design
- `docs/reference/rmw-h-analysis.md` — ROS 2 rmw.h analysis (limitations, what to adopt)
- `docs/reference/xrce-dds-analysis.md` — XRCE-DDS source code analysis (API, build system, platform requirements)

## Goal

1. Add a factory trait (`Rmw`) to `nros-rmw` so middleware backends are compile-time selectable
2. Make `nros-rmw-zenoh` implement the factory trait
3. Refactor platform/board crates to use `nros-rmw` traits instead of calling `zpico-sys` FFI directly
4. Implement XRCE-DDS as the second RMW backend, proving the abstraction works

## Current State (post-Phase 34.3)

**Complete:**
- `nros-rmw` — `Rmw` factory trait, `RmwConfig`, `Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait` traits, `TopicInfo`, `ServiceInfo`, `ActionInfo`, `QosSettings`, `TransportError`
- `nros-rmw-zenoh` — `ZenohRmw` implements `Rmw` trait, `ShimSession`/`ShimPublisher`/`ShimSubscriber` implement session/pub/sub traits
- All 4 board crates (`nros-mps2-an385`, `nros-stm32f4`, `nros-esp32`, `nros-esp32-qemu`) use `nros-rmw` traits exclusively — no direct `zpico-sys` imports

**Remaining (34.4-34.8):**
- XRCE-DDS as second RMW backend, proving the abstraction works
- Source code studied: `external/Micro-XRCE-DDS-Client/` and `external/Micro-CDR/`

## Steps

---

### 34.1: Add `Rmw` factory trait and `RmwConfig` to `nros-rmw`

**Files:** `packages/core/nros-rmw/src/traits.rs`

- [x] Define `Rmw` factory trait with associated `Session` type
- [x] Define `RmwConfig` struct (middleware-agnostic connection parameters)
- [x] Reuse existing `TransportError` (no new error type needed — backends already use it)
- [x] Add unit tests for `RmwConfig` construction and validation
- [x] `just quality` passes

**`Rmw` trait design:**
```rust
/// Factory trait for compile-time middleware selection.
///
/// Embedded crates select a backend via feature flag:
/// ```rust
/// #[cfg(feature = "zenoh")]
/// type DefaultRmw = nros_rmw_zenoh::ZenohRmw;
/// ```
pub trait Rmw {
    type Session: Session;
    type Error: core::fmt::Debug;

    /// Open a new middleware session with the given configuration
    fn open(config: &RmwConfig) -> Result<Self::Session, Self::Error>;
}
```

**`RmwConfig` design:**
```rust
pub struct RmwConfig<'a> {
    /// Middleware-specific connection string
    /// zenoh: "tcp/192.168.1.1:7447"
    /// XRCE-DDS: "udp/192.168.1.1:2019"
    pub locator: &'a str,
    /// Session mode (zenoh: "client"/"peer", XRCE-DDS: ignored)
    pub mode: SessionMode,
    /// ROS 2 domain ID
    pub domain_id: u32,
    /// Node name
    pub node_name: &'a str,
    /// Node namespace
    pub namespace: &'a str,
}
```

**Acceptance criteria:**
- `Rmw` trait compiles on `no_std` without `alloc`
- `RmwConfig` is `#[derive(Debug, Clone)]` and constructible with `const fn`
- Existing `Transport` trait remains for backward compatibility (can be deprecated later)
- No breaking changes to existing `nros-rmw` public API

---

### 34.2: Implement `Rmw` trait in `nros-rmw-zenoh`

**Files:** `packages/zpico/nros-rmw-zenoh/src/lib.rs`, `shim.rs`

- [x] Create `ZenohRmw` unit struct implementing `Rmw` trait
- [x] `ZenohRmw::open()` parses `RmwConfig`, calls existing session open logic
- [x] Verify existing `ShimSession` satisfies `Session` associated type bound
- [x] Ensure `ShimPublisher`, `ShimSubscriber`, etc. satisfy their respective trait bounds
- [x] Add integration test: `ZenohRmw::open()` → create publisher → publish → close
- [x] `just quality` passes

**Key constraint:** `ZenohRmw::open()` must bridge from `RmwConfig` (middleware-agnostic) to the zenoh-specific initialization that currently lives in `shim.rs`. The zenoh locator, session mode, and domain ID must be extracted from `RmwConfig` and passed to `zpico_sys`.

**Acceptance criteria:**
- `ZenohRmw` implements `Rmw` and the existing `Transport` trait
- Existing code that uses `ShimTransport::open()` continues to work
- All existing integration tests pass unchanged
- `nros-rmw-zenoh` re-exports `ZenohRmw` as a public type

---

### 34.3: Refactor platform crates to use `nros-rmw` traits

**Files:** `packages/boards/nros-{qemu,stm32f4,esp32,esp32-qemu}/src/{node,publisher,subscriber}.rs`

> **Note:** Phase 33.3 (platform crate split) is complete. Board crates are at `packages/boards/nros-*`.

**Per board crate:**
- [x] Replace `zpico_sys::zenoh_shim_open_session()` with `ZenohRmw::open(&config)`
- [x] Replace `zpico_sys::zenoh_shim_declare_publisher()` with `session.create_publisher()`
- [x] Replace `zpico_sys::zenoh_shim_put()` with `publisher.publish_raw()`
- [x] Replace `zpico_sys::zenoh_shim_declare_subscriber()` with `session.create_subscriber()`
- [x] Remove all zenoh-specific keyexpr formatting from `node.rs` (now in `nros-rmw-zenoh/keyexpr.rs`)
- [x] Update `Cargo.toml`: add `nros-rmw` + `nros-rmw-zenoh` deps, remove direct `zpico-sys` dep
- [x] Board-specific `Node` type uses concrete `ZenohRmw` (no dynamic dispatch, no generics needed for single-backend boards)

**QEMU board crate (refactored first — most tested):**
- [x] Refactor `nros-mps2-an385/src/node.rs`
- [x] Refactor `nros-mps2-an385/src/publisher.rs`
- [x] Refactor `nros-mps2-an385/src/subscriber.rs`
- [x] All QEMU examples build and pass tests

**STM32F4 board crate:**
- [x] Refactor `nros-stm32f4/src/{node,publisher,subscriber}.rs`
- [x] STM32F4 examples build

**ESP32 board crates:**
- [x] Refactor `nros-esp32/src/{node,publisher,subscriber}.rs`
- [x] Refactor `nros-esp32-qemu/src/{node,publisher,subscriber}.rs`
- [x] ESP32 examples build

**Final verification:**
- [x] `just quality` passes
- [ ] `just test-qemu` passes (if QEMU available)
- [x] No direct `zpico_sys` imports remain in any board crate

**Acceptance criteria:**
- All platform crates go through `nros-rmw` trait methods exclusively
- `zpico-sys` is an implementation detail of `nros-rmw-zenoh`, not visible to platform crates
- Adding a second middleware backend would require only changing a Cargo feature flag and `type DefaultRmw = ...` in each board crate — no FFI call changes

---

### 34.4: Create `xrce-sys` (FFI bindings)

**Directory:** `packages/xrce/xrce-sys/`

**Source study complete:** Micro-XRCE-DDS-Client and Micro-CDR cloned to `external/`. API, build system, and platform requirements analyzed in `docs/reference/xrce-dds-analysis.md`.

**Key design decisions from source study:**
- **No C shim layer** — unlike zpico-sys (`zenoh_shim.c`, 1200+ lines), XRCE-DDS's C API is clean enough to bind directly from Rust FFI via `unsafe extern "C"` blocks
- **cc::Build, not CMake** — compile 28 C source files directly via `cc::Build` in `build.rs`, generate `config.h` from Cargo features (same pattern as zpico-sys)
- **Two git submodules** — `micro-xrce-dds-client/` (XRCE-DDS Client) + `micro-cdr/` (Micro-CDR v2.0.2, the only dependency)
- **Minimal libc** — only `memcpy`, `memset`, `strlen` needed (no heap, no printf/snprintf)

**Submodules:**
- [ ] Add Micro-XRCE-DDS-Client as git submodule at `packages/xrce/xrce-sys/micro-xrce-dds-client/`
- [ ] Add Micro-CDR v2.0.2 as git submodule at `packages/xrce/xrce-sys/micro-cdr/`

**Build system (`build.rs`):**
- [ ] Generate `config.h` in `OUT_DIR` from Cargo features:
  - `UCLIENT_PROFILE_CUSTOM_TRANSPORT` (always enabled for bare-metal)
  - `UXR_CONFIG_CUSTOM_TRANSPORT_MTU` (default 512)
  - `UXR_CONFIG_MAX_*_STREAMS` (default 1 each)
  - No platform defines for bare-metal (falls through to POSIX branch in `time.c`)
- [ ] Generate Micro-CDR `config.h` with `UCDR_MACHINE_ENDIANNESS` (little-endian for ARM/RISC-V)
- [ ] Compile 23 XRCE-DDS core files + 5 Micro-CDR files = 28 total via `cc::Build`
- [ ] Set target-specific flags (ARM: `-mcpu=cortex-m3 -mthumb`, RISC-V: `-march=rv32imc`)
- [ ] Support `posix` / `bare-metal` feature flags (mutually exclusive, like zpico-sys)

**FFI bindings (`src/lib.rs` + `src/ffi.rs`):**
- [ ] `#![no_std]` with optional `std` feature
- [ ] `#[repr(C)]` types: `uxrSession`, `uxrObjectId`, `uxrStreamId`, `uxrQoS_t`, `uxrCustomTransport`, `uxrCommunication`, `uxrDeliveryControl`, `ucdrBuffer`, `SampleIdentity`, `uxrFramingIO`
- [ ] Constants: `UXR_STATUS_*`, `UXR_*_ID` (object types), `UXR_REPLACE`/`UXR_REUSE`, `UXR_DURABILITY_*`, `UXR_RELIABILITY_*`, `UXR_HISTORY_*`
- [ ] Callback types: `uxrOnTopicFunc`, `uxrOnStatusFunc`, `uxrOnRequestFunc`, `uxrOnReplyFunc`, `open_custom_func`, `close_custom_func`, `write_custom_func`, `read_custom_func`
- [ ] Session API: `uxr_init_session`, `uxr_create_session`, `uxr_create_session_retries`, `uxr_delete_session`, `uxr_run_session_time`, `uxr_run_session_until_all_status`, `uxr_run_session_until_data`, `uxr_flash_output_streams`
- [ ] Callback setters: `uxr_set_status_callback`, `uxr_set_topic_callback`, `uxr_set_request_callback`, `uxr_set_reply_callback`
- [ ] Stream API: `uxr_create_output_best_effort_stream`, `uxr_create_output_reliable_stream`, `uxr_create_input_best_effort_stream`, `uxr_create_input_reliable_stream`
- [ ] Entity API (binary format): `uxr_buffer_create_participant_bin`, `uxr_buffer_create_topic_bin`, `uxr_buffer_create_publisher_bin`, `uxr_buffer_create_subscriber_bin`, `uxr_buffer_create_datawriter_bin`, `uxr_buffer_create_datareader_bin`, `uxr_buffer_create_requester_bin`, `uxr_buffer_create_replier_bin`
- [ ] Data API: `uxr_buffer_topic`, `uxr_buffer_request_data`, `uxr_buffer_cancel_data`, `uxr_buffer_request`, `uxr_buffer_reply`
- [ ] Transport API: `uxr_set_custom_transport_callbacks`, `uxr_init_custom_transport`, `uxr_close_custom_transport`
- [ ] Object ID API: `uxr_object_id`, `uxr_stream_id`

**Verification:**
- [ ] `cargo check -p xrce-sys --features bare-metal --target thumbv7m-none-eabi` passes
- [ ] `cargo check -p xrce-sys --features posix` passes (host target)
- [ ] `just quality` passes

**Acceptance criteria:**
- Compiles 28 C source files from submodules without CMake
- All ~30 core XRCE-DDS C functions are bindable from Rust
- Zero heap allocation — fully static memory model
- Feature-gated platform selection (`posix` / `bare-metal`)

---

### 34.5: Create `xrce-smoltcp` (UDP transport via smoltcp)

**Directory:** `packages/xrce/xrce-smoltcp/`

Implements the 4 XRCE-DDS custom transport callbacks using smoltcp UDP sockets. Much simpler than zpico-smoltcp (4 UDP callbacks vs 8+ TCP callbacks).

- [ ] Static UDP socket buffer storage (no heap):
  - `UDP_RX_BUFFER: [u8; 1024]` — receive buffer
  - `UDP_TX_BUFFER: [u8; 1024]` — transmit buffer (metadata)
- [ ] Implement `open_custom_func`: bind smoltcp UDP socket to a local port, store agent endpoint address
- [ ] Implement `close_custom_func`: close UDP socket
- [ ] Implement `write_custom_func`: send UDP datagram to agent via smoltcp, return bytes written
- [ ] Implement `read_custom_func`: poll smoltcp for incoming UDP datagram, respect timeout parameter, return bytes read
- [ ] Global state management: store smoltcp `Interface`/`SocketSet` references for the poll callback (same pattern as zpico-smoltcp's `SmoltcpBridge`)
- [ ] Poll callback: `xrce_smoltcp_poll()` — called from the network poll loop to process smoltcp events
- [ ] Builds for `thumbv7m-none-eabi`

**Dependencies:**
```toml
xrce-sys = { path = "../xrce-sys", features = ["bare-metal"] }
smoltcp = { version = "0.12", default-features = false, features = [
    "medium-ethernet", "proto-ipv4", "socket-udp",
] }
```

**Acceptance criteria:**
- All 4 callbacks compile and link against `xrce-sys`
- UDP transport works on smoltcp 0.12 (same version as zpico-smoltcp)
- No heap allocation
- Socket storage is static (supports 1 UDP socket for agent communication)

---

### 34.6: Create `nros-rmw-xrce` (RMW trait implementation)

**Directory:** `packages/xrce/nros-rmw-xrce/`

Maps XRCE-DDS entities to `nros-rmw` traits. This is the core complexity — orchestrating the DDS entity hierarchy internally while presenting the same simple `Session::create_publisher()` API.

**`XrceRmw` (implements `Rmw` trait):**
- [ ] `XrceRmw::open(config: &RmwConfig)`:
  1. Parse `config.locator` (e.g., `"udp/192.168.1.1:2019"`) to extract agent IP + port
  2. Set up custom transport callbacks (from `xrce-smoltcp`)
  3. `uxr_init_custom_transport` → `uxr_init_session` → `uxr_create_session`
  4. Create output reliable stream + input reliable stream (user-provided static buffers)
  5. Create DDS participant via `uxr_buffer_create_participant_bin` + wait for confirmation
  6. Return `XrceSession`

**`XrceSession` (implements `Session` trait):**
- [ ] Holds `uxrSession`, stream IDs, participant ID, and entity ID counter
- [ ] `create_publisher(topic, qos)`:
  1. Allocate next object IDs for topic, publisher, datawriter
  2. `uxr_buffer_create_topic_bin` with DDS topic name (`"rt/<topic>"`) and type name
  3. `uxr_buffer_create_publisher_bin` + `uxr_buffer_create_datawriter_bin` with QoS
  4. `uxr_run_session_until_all_status` to wait for agent confirmation
  5. Return `XrcePublisher` wrapping the datawriter ID
- [ ] `create_subscriber(topic, qos)`:
  1. Allocate IDs for topic, subscriber, datareader
  2. Create entities via `_bin` calls + wait for confirmation
  3. `uxr_buffer_request_data` with `UXR_MAX_SAMPLES_UNLIMITED`
  4. Return `XrceSubscriber` wrapping the datareader ID + receive buffer
- [ ] `spin_once(timeout)`: `uxr_run_session_time(timeout)` — processes I/O and dispatches callbacks
- [ ] Entity ID management: track next available ID per type to avoid collisions

**`XrcePublisher` (implements `Publisher` trait):**
- [ ] `publish_raw(data)`: `uxr_buffer_topic(session, stream, datawriter_id, data, len)` — passes pre-serialized CDR bytes directly (no double-serialization through Micro-CDR)

**`XrceSubscriber` (implements `Subscriber` trait):**
- [ ] `uxrOnTopicFunc` callback: copies received data from `ucdrBuffer` into static receive buffer, sets atomic flag
- [ ] `try_recv_raw(buf)`: checks flag, copies from receive buffer to caller's buffer, clears flag
- [ ] `has_data()`: checks atomic flag

**`XrceServiceServer` (implements `ServiceServerTrait`):**
- [ ] Uses replier pattern: `uxr_buffer_create_replier_bin`
- [ ] `uxrOnRequestFunc` callback: stores request + `SampleIdentity` correlation ID
- [ ] `send_reply()`: `uxr_buffer_reply(session, stream, replier_id, sample_id, data, len)`

**`XrceServiceClient` (implements `ServiceClientTrait`):**
- [ ] Uses requester pattern: `uxr_buffer_create_requester_bin`
- [ ] `call_raw()`: `uxr_buffer_request` + `uxr_run_session_until_data` for reply via `uxrOnReplyFunc`

**DDS topic naming:**
- [ ] `TopicInfo.name` `/chatter` → DDS `"rt/chatter"` (add `rt/` prefix)
- [ ] `TopicInfo.type_name` `std_msgs::msg::Int32` → DDS `"std_msgs::msg::dds_::Int32_"` (add `dds_::` + trailing `_`)

**Verification:**
- [ ] All types compile on `no_std` without `alloc`
- [ ] Unit tests for entity ID allocation and topic name formatting
- [ ] `just quality` passes

**Acceptance criteria:**
- Implements `Rmw`, `Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait`
- Board crate can swap `ZenohRmw` → `XrceRmw` with only a Cargo feature change
- No heap allocation — all buffers are static or stack-allocated

---

### 34.7: Create `xrce-platform-qemu` (bare-metal platform support)

**Directory:** `packages/xrce/xrce-platform-qemu/`

Provides the **single platform symbol** needed by XRCE-DDS on bare-metal: `clock_gettime()` for `uxr_nanos()` in `time.c`.

This is dramatically simpler than zpico-platform-mps2-an385 (1 symbol vs 55):

| | zpico-platform-mps2-an385 | xrce-platform-qemu |
|---|---|---|
| Memory | z_malloc, z_free, z_realloc | **None** (fully static) |
| Clock | z_clock_now + 6 time functions | `clock_gettime` only |
| Random | z_random_u8/u16/u32/u64/fill | **None** |
| Sleep | z_sleep_us/ms/s | **None** |
| Threading | 23 mutex/condvar/task stubs | **None** |
| Sockets | TCP open/close/read/send stubs | **None** (custom transport) |
| libc | strlen, memcpy, strtoul, snprintf, ... | `memcpy`, `memset`, `strlen` (from compiler builtins or newlib) |
| **Total** | **~55 symbols** | **1 symbol** |

- [ ] Implement `clock_gettime(CLOCK_REALTIME, &ts)` using Cortex-M DWT cycle counter (same timing source as zpico-platform-mps2-an385's `z_clock_now`)
- [ ] Provide `memcpy`, `memset`, `strlen` if not available from the toolchain's newlib/picolibc (ARM `arm-none-eabi-gcc` provides these by default)
- [ ] Builds for `thumbv7m-none-eabi`

**Dependencies:**
```toml
xrce-sys = { path = "../xrce-sys", features = ["bare-metal"] }
cortex-m = "0.7"
```

**Acceptance criteria:**
- XRCE-DDS client links and initializes on QEMU MPS2-AN385
- `uxr_nanos()` returns monotonically increasing nanosecond timestamps
- No heap allocation, no threading, no socket stubs

---

### 34.8: Integration testing

- [ ] Build [Micro-XRCE-DDS Agent](https://github.com/eProsima/Micro-XRCE-DDS-Agent) from source (cloned to `external/Micro-XRCE-DDS-Agent/`)
  - Add `just build-xrce-agent` recipe
  - Output to `build/xrce-agent/MicroXRCEAgent`
- [ ] Create `XrceAgent` fixture in `nros-tests/src/fixtures/`
  - Starts agent on configurable UDP port
  - Auto-discovers DDS domain
  - Cleans up on drop
- [ ] Create `nros-tests/tests/xrce.rs` test suite
- [ ] Test: QEMU XRCE publisher → Agent → verify data arrives (via DDS subscriber or agent log)
- [ ] Test: Agent → QEMU XRCE subscriber → verify data received
- [ ] Test: XRCE service server/client roundtrip
- [ ] Test: Cross-backend interop (zenoh ↔ XRCE via DDS bridge) — if feasible
- [ ] Add `just test-xrce` recipe to justfile
- [ ] Document Agent setup in `tests/README.md`

**Acceptance criteria:**
- At least pub/sub and service roundtrip work end-to-end on QEMU
- Tests are automated (no manual Agent setup during `just test-xrce`)

---

## Platform Requirements Comparison

| Requirement | zenoh-pico (current) | XRCE-DDS |
|---|---|---|
| Client heap | Required (~16KB+) | **None** (fully static) |
| Client RAM | ~16KB+ | ~3KB |
| Client Flash | ~100KB+ | ~75KB |
| C source files to compile | ~100+ | **28** |
| C shim layer | `zenoh_shim.c` (1200+ lines) | **None** (direct FFI binding) |
| Platform symbols | ~55 FFI exports | **1** (`clock_gettime`) |
| Transport impl | 8+ TCP functions | 4 UDP callbacks |
| libc stubs needed | 14 (strlen, snprintf, strtoul, ...) | 3 (`memcpy`, `memset`, `strlen`) |
| Config #defines | 20+ | ~8 |
| Bridge process | zenohd (optional for peer mode) | Agent (**mandatory**) |

## Execution Order

```
34.1 (Rmw trait) ──→ 34.2 (zenoh impl) ──→ 34.3 (board refactor)    ✅ COMPLETE
                                               │
34.4 (xrce-sys) ──→ 34.5 (xrce-smoltcp) ──→ 34.6 (nros-rmw-xrce)
                                                      │
                     34.7 (xrce-platform-qemu) ───────→ 34.8 (integration tests)
```

- **34.1 → 34.2 → 34.3** complete. RMW abstraction validated with zenoh backend.
- **34.4** is the next step — FFI bindings to XRCE-DDS Client + Micro-CDR.
- **34.5 + 34.7** can be done in parallel after 34.4 (transport + platform are independent).
- **34.6** depends on 34.4 + 34.5 (needs FFI + transport).
- **34.8** requires all of 34.4-34.7.

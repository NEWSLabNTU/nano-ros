# Phase 34: RMW Abstraction & XRCE-DDS Integration

**Status: In Progress** (34.1-34.8 complete)

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
- All 4 board crates (`nros-board-mps2-an385`, `nros-board-stm32f4`, `nros-board-esp32`, `nros-board-esp32-qemu`) use `nros-rmw` traits exclusively — no direct `zpico-sys` imports

**Remaining (34.6-34.8):**
- XRCE-DDS as second RMW backend, proving the abstraction works
- `xrce-sys` FFI bindings complete (34.4): `packages/xrce/xrce-sys/`
- `xrce-smoltcp` UDP transport complete (34.5): `packages/xrce/xrce-smoltcp/`

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
- [x] Refactor `nros-board-mps2-an385/src/node.rs`
- [x] Refactor `nros-board-mps2-an385/src/publisher.rs`
- [x] Refactor `nros-board-mps2-an385/src/subscriber.rs`
- [x] All QEMU examples build and pass tests

**STM32F4 board crate:**
- [x] Refactor `nros-board-stm32f4/src/{node,publisher,subscriber}.rs`
- [x] STM32F4 examples build

**ESP32 board crates:**
- [x] Refactor `nros-board-esp32/src/{node,publisher,subscriber}.rs`
- [x] Refactor `nros-board-esp32-qemu/src/{node,publisher,subscriber}.rs`
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
- [x] Add Micro-XRCE-DDS-Client v3.0.1 as git submodule at `packages/xrce/xrce-sys/micro-xrce-dds-client/`
- [x] Add Micro-CDR v2.0.2 as git submodule at `packages/xrce/xrce-sys/micro-cdr/`

**Build system (`build.rs`):**
- [x] Generate `config.h` in `OUT_DIR` from Cargo features:
  - `UCLIENT_PROFILE_CUSTOM_TRANSPORT` (always enabled)
  - `UXR_CONFIG_CUSTOM_TRANSPORT_MTU` (512)
  - `UXR_CONFIG_MAX_*_STREAMS` (1 each)
  - `UCLIENT_PLATFORM_POSIX` only when `posix` feature enabled
- [x] Generate Micro-CDR `config.h` with `UCDR_MACHINE_ENDIANNESS = 1` (little-endian)
- [x] Compile 27 XRCE-DDS core files + 5 Micro-CDR files via `cc::Build` (posix adds `time.c`)
- [x] Compile-time `_Static_assert` size verification for opaque Rust blobs
- [x] Support `posix` / `bare-metal` feature flags (mutually exclusive)

**FFI bindings (`src/lib.rs`):**
- [x] `#![no_std]` with optional `std` feature
- [x] `#[repr(C)]` transparent types: `uxrObjectId`, `uxrStreamId`, `uxrQoS_t`, `uxrDeliveryControl`, `ucdrBuffer`, `SampleIdentity` (+ sub-types `GUID_t`, `GuidPrefix_t`, `EntityId_t`, `SequenceNumber_t`)
- [x] Opaque blob types: `uxrSession` (512 bytes), `uxrCustomTransport` (768 bytes), `uxrCommunication` (pointer-only)
- [x] Constants: `UXR_STATUS_*` (10), `UXR_*_ID` (9 object types), `UXR_REPLACE`/`UXR_REUSE`, `UXR_DURABILITY_*`/`UXR_RELIABILITY_*`/`UXR_HISTORY_*`, delivery limits, stream types/directions
- [x] Callback types: `uxrOnStatusFunc`, `uxrOnTopicFunc`, `uxrOnRequestFunc`, `uxrOnReplyFunc`, `open_custom_func`, `close_custom_func`, `write_custom_func`, `read_custom_func`
- [x] ~40 extern functions: session lifecycle (5), callbacks (4), streams (5), session run (7), entity creation (9), data (6), transport (3), helpers (3)

**Verification:**
- [x] `cargo check -p xrce-sys --features bare-metal --target thumbv7m-none-eabi` passes
- [x] `cargo check -p xrce-sys --features posix` passes (host target)
- [x] `just quality` passes

**Acceptance criteria:**
- Compiles 28 C source files from submodules without CMake
- All ~30 core XRCE-DDS C functions are bindable from Rust
- Zero heap allocation — fully static memory model
- Feature-gated platform selection (`posix` / `bare-metal`)

---

### 34.5: Create `xrce-smoltcp` (UDP transport via smoltcp)

**Directory:** `packages/xrce/xrce-smoltcp/`

Implements the 4 XRCE-DDS custom transport callbacks using smoltcp UDP sockets. Much simpler than zpico-smoltcp (4 UDP callbacks vs 8+ TCP callbacks).

- [x] Static UDP socket buffer storage (no heap):
  - `UDP_RX_BUFFER: [u8; 1024]` — receive buffer
  - `UDP_TX_BUFFER: [u8; 1024]` — transmit buffer (metadata)
- [x] Implement `open_custom_func`: bind smoltcp UDP socket to a local port, store agent endpoint address
- [x] Implement `close_custom_func`: close UDP socket
- [x] Implement `write_custom_func`: send UDP datagram to agent via smoltcp, return bytes written
- [x] Implement `read_custom_func`: poll smoltcp for incoming UDP datagram, respect timeout parameter, return bytes read
- [x] Global state management: store smoltcp `Interface`/`SocketSet` references for the poll callback (same pattern as zpico-smoltcp's `SmoltcpBridge`)
- [x] Poll callback: `XrceSmoltcpTransport::poll()` — called from the network poll loop to process smoltcp events
- [x] Builds for `thumbv7m-none-eabi`

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
- [x] `XrceRmw::open(config: &RmwConfig)`:
  1. Parse `config.locator` (e.g., `"udp/192.168.1.1:2019"`) to extract agent IP + port
  2. Set up custom transport callbacks (from `xrce-smoltcp`)
  3. `uxr_init_custom_transport` → `uxr_init_session` → `uxr_create_session`
  4. Create output reliable stream + input reliable stream (user-provided static buffers)
  5. Create DDS participant via `uxr_buffer_create_participant_bin` + wait for confirmation
  6. Return `XrceSession`

**`XrceSession` (implements `Session` trait):**
- [x] Holds `uxrSession`, stream IDs, participant ID, and entity ID counter
- [x] `create_publisher(topic, qos)`:
  1. Allocate next object IDs for topic, publisher, datawriter
  2. `uxr_buffer_create_topic_bin` with DDS topic name (`"rt/<topic>"`) and type name
  3. `uxr_buffer_create_publisher_bin` + `uxr_buffer_create_datawriter_bin` with QoS
  4. `uxr_run_session_until_all_status` to wait for agent confirmation
  5. Return `XrcePublisher` wrapping the datawriter ID
- [x] `create_subscriber(topic, qos)`:
  1. Allocate IDs for topic, subscriber, datareader
  2. Create entities via `_bin` calls + wait for confirmation
  3. `uxr_buffer_request_data` with `UXR_MAX_SAMPLES_UNLIMITED`
  4. Return `XrceSubscriber` wrapping the datareader ID + receive buffer
- [x] `spin_once(timeout)`: `uxr_run_session_time(timeout)` — processes I/O and dispatches callbacks
- [x] Entity ID management: track next available ID per type to avoid collisions

**`XrcePublisher` (implements `Publisher` trait):**
- [x] `publish_raw(data)`: `uxr_buffer_topic(session, stream, datawriter_id, data, len)` — passes pre-serialized CDR bytes directly (no double-serialization through Micro-CDR)

**`XrceSubscriber` (implements `Subscriber` trait):**
- [x] `uxrOnTopicFunc` callback: copies received data from `ucdrBuffer` into static receive buffer, sets atomic flag
- [x] `try_recv_raw(buf)`: checks flag, copies from receive buffer to caller's buffer, clears flag
- [x] `has_data()`: checks atomic flag

**`XrceServiceServer` (implements `ServiceServerTrait`):**
- [x] Uses replier pattern: `uxr_buffer_create_replier_bin`
- [x] `uxrOnRequestFunc` callback: stores request + `SampleIdentity` correlation ID
- [x] `send_reply()`: `uxr_buffer_reply(session, stream, replier_id, sample_id, data, len)`

**`XrceServiceClient` (implements `ServiceClientTrait`):**
- [x] Uses requester pattern: `uxr_buffer_create_requester_bin`
- [x] `call_raw()`: `uxr_buffer_request` + `uxr_run_session_until_data` for reply via `uxrOnReplyFunc`

**DDS topic naming:**
- [x] `TopicInfo.name` `/chatter` → DDS `"rt/chatter"` (add `rt/` prefix)
- [x] `TopicInfo.type_name` `std_msgs::msg::Int32` → DDS `"std_msgs::msg::dds_::Int32_"` (add `dds_::` + trailing `_`)

**Verification:**
- [x] All types compile on `no_std` without `alloc`
- [x] Unit tests for entity ID allocation and topic name formatting
- [x] `just quality` passes

**Acceptance criteria:**
- Implements `Rmw`, `Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait`
- Board crate can swap `ZenohRmw` → `XrceRmw` with only a Cargo feature change
- No heap allocation — all buffers are static or stack-allocated

---

### 34.7: Create `xrce-platform-mps2-an385` (bare-metal platform support)

**Directory:** `packages/xrce/xrce-platform-mps2-an385/`

Provides the platform symbols needed by XRCE-DDS on bare-metal. Since `time.c` is skipped for bare-metal builds (see `xrce-sys/build.rs`), the platform crate provides `uxr_millis()` and `uxr_nanos()` directly, plus `smoltcp_clock_now_ms()` for the transport layer.

This is dramatically simpler than zpico-platform-mps2-an385 (3 symbols vs 55):

|           | zpico-platform-mps2-an385              | xrce-platform-mps2-an385                                       |
|-----------|----------------------------------------|-----------------------------------------------------------------|
| Memory    | z_malloc, z_free, z_realloc            | **None** (fully static)                                         |
| Clock     | z_clock_now + 6 time functions         | `uxr_millis` + `uxr_nanos` + `smoltcp_clock_now_ms`             |
| Random    | z_random_u8/u16/u32/u64/fill           | **None**                                                        |
| Sleep     | z_sleep_us/ms/s                        | **None**                                                        |
| Threading | 23 mutex/condvar/task stubs            | **None**                                                        |
| Sockets   | TCP open/close/read/send stubs         | **None** (custom transport)                                     |
| libc      | strlen, memcpy, strtoul, snprintf, ... | `memcpy`, `memset`, `strlen` (from compiler builtins or newlib) |
| **Total** | **~55 symbols**                        | **3 symbols**                                                   |

- [x] Implement `uxr_millis()` and `uxr_nanos()` using software millisecond counter (same pattern as zpico-platform-mps2-an385's `clock.rs`)
- [x] Provide `smoltcp_clock_now_ms()` for xrce-smoltcp transport layer
- [x] Provide `set_clock_ms()` and `clock_ms()` for board crate to update and read the clock
- [x] Provide `memcpy`, `memset`, `strlen` if not available from the toolchain's newlib/picolibc (ARM `arm-none-eabi-gcc` provides these by default)
- [x] Builds for `thumbv7m-none-eabi`

**Dependencies:**
```toml
xrce-sys = { path = "../xrce-sys", features = ["bare-metal"] }
```

**Acceptance criteria:**
- XRCE-DDS client links and initializes on QEMU MPS2-AN385
- `uxr_millis()` / `uxr_nanos()` return monotonically increasing timestamps
- No heap allocation, no threading, no socket stubs

---

### 34.8: Integration testing

- [x] Build [Micro-XRCE-DDS Agent](https://github.com/eProsima/Micro-XRCE-DDS-Agent) from source (cloned to `external/Micro-XRCE-DDS-Agent/`)
  - `just build-xrce-agent` recipe: `scripts/xrce-agent/build.sh`
  - Output to `build/xrce-agent/MicroXRCEAgent`
- [x] Create `xrce-native-test` crate (`packages/testing/xrce-native-test/`)
  - Posix UDP custom transport via `std::net::UdpSocket`
  - `xrce-talker` binary: publishes raw CDR Int32 on `/chatter`
  - `xrce-listener` binary: subscribes and prints received Int32 values
- [x] Create `XrceAgent` fixture in `nros-tests/src/fixtures/xrce_agent.rs`
  - Starts agent on configurable UDP port (`XrceAgent::start(port)`)
  - Ephemeral port support (`XrceAgent::start_unique()`)
  - Cleans up on drop
  - `is_xrce_agent_available()` / `require_xrce_agent()` skip helpers
- [x] Create `nros-tests/tests/xrce.rs` test suite
  - `test_xrce_talker_starts`: verifies talker connects to agent and publishes
  - `test_xrce_listener_starts`: verifies listener connects and subscribes
  - `test_xrce_talker_listener_communication`: end-to-end pub/sub roundtrip
- [x] Add `just test-xrce` recipe to justfile
- [x] Add `xrce` test group to `.config/nextest.toml` (max-threads=1)
- [x] Binary builders with OnceCell caching: `build_xrce_talker()` / `build_xrce_listener()`
- [x] Test: XRCE service server/client roundtrip
  - `examples/native/rust/xrce/service-server/` and `service-client/` using `XrceExecutor`/`XrceNode` API
  - `test_xrce_service_server_starts`, `test_xrce_service_client_starts`, `test_xrce_service_request_response` in `xrce.rs`
  - Binary builders with OnceCell caching: `build_xrce_service_server()` / `build_xrce_service_client()`
- [x] Test: XRCE ↔ ROS 2 DDS interop via XRCE Agent
  - `xrce_ros2_interop.rs`: 4 tests (detection, XRCE→ROS2 pub/sub, ROS2→XRCE pub/sub, XRCE service + ROS2 client)
  - Architecture: `nros XRCE node → XRCE Agent (Fast-DDS) ←DDS multicast→ ROS 2 node (rmw_fastrtps_cpp)`
  - `Ros2DdsProcess` helper in `ros2.rs` (parallel to zenoh-based `Ros2Process`)
  - `just test-xrce-ros2` recipe, wired into `test-all`
  - Note: zenoh ↔ XRCE cross-backend bridging is not feasible (incompatible key expression formats between zenoh-bridge-ros2dds and rmw_zenoh). The practical interop path is all-DDS via the XRCE Agent.

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
                     34.7 (xrce-platform-mps2-an385) ───────→ 34.8 (integration tests)
```

- **34.1 → 34.2 → 34.3** complete. RMW abstraction validated with zenoh backend.
- **34.4** complete. FFI bindings to XRCE-DDS Client v3.0.1 + Micro-CDR v2.0.2.
- **34.5** complete. smoltcp UDP transport for XRCE-DDS custom transport.
- **34.6** complete. `nros-rmw-xrce` RMW trait implementation.
- **34.7** complete. `xrce-platform-mps2-an385` platform symbols (uxr_millis, uxr_nanos, smoltcp_clock_now_ms).
- **34.8** complete. Integration testing infrastructure: Agent build script, native test binaries, fixtures, test suite.

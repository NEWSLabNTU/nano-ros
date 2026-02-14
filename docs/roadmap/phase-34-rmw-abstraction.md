# Phase 34: RMW Abstraction & XRCE-DDS Integration

**Status: In Progress** (34.1, 34.2, 34.3 complete)

**Prerequisites:** Phase 33.1 (core rename) and 33.2 (transport split) are complete. Phase 33.3 (platform crate split) is in progress in a separate work tree but is NOT a blocker — Phase 34 work on `nros-rmw` and `nros-rmw-zenoh` is independent of the platform crate restructuring.

**Design docs:**
- `docs/design/rmw-layer-design.md` — Overall architecture and RMW layer design
- `docs/reference/rmw-h-analysis.md` — ROS 2 rmw.h analysis (limitations, what to adopt)
- `docs/reference/xrce-dds-analysis.md` — XRCE-DDS feasibility analysis

## Goal

1. Add a factory trait (`Rmw`) to `nros-rmw` so middleware backends are compile-time selectable
2. Make `nros-rmw-zenoh` implement the factory trait
3. Refactor platform/board crates to use `nros-rmw` traits instead of calling `zpico-sys` FFI directly
4. Implement XRCE-DDS as the second RMW backend, proving the abstraction works

## Current State (post-Phase 33.2)

**What exists:**
- `nros-rmw` (`packages/core/nros-rmw/`) — has `Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait`, `Transport` traits, plus `TopicInfo`, `ServiceInfo`, `ActionInfo`, `QosSettings`, `TransportConfig`, `TransportError`
- `nros-rmw-zenoh` (`packages/zpico/nros-rmw-zenoh/`) — has `shim.rs` (implements `nros-rmw` traits), `zpico.rs` (safe wrapper over `zpico-sys` FFI), `keyexpr.rs` (zenoh key expression formatting)

**What's missing:**
- No `Rmw` factory trait — can't select middleware at compile time
- No `RmwConfig` type — `TransportConfig` is zenoh-specific (locator strings, session modes)
- Platform crates (`nano-ros-platform-{qemu,stm32f4,esp32,esp32-qemu}`) bypass `nros-rmw` entirely and call `zpico_sys::*` FFI directly in `node.rs`, `publisher.rs`, `subscriber.rs`

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
- [x] Refactor `nros-qemu/src/node.rs`
- [x] Refactor `nros-qemu/src/publisher.rs`
- [x] Refactor `nros-qemu/src/subscriber.rs`
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

- [ ] Add [Micro-XRCE-DDS-Client](https://github.com/eProsima/Micro-XRCE-DDS-Client) as git submodule
- [ ] Write `build.rs` to compile client C library with CMake
- [ ] Write `src/lib.rs` with raw FFI bindings (`#[repr(C)]` types, `unsafe extern "C"` functions)
- [ ] Bind session API: `uxr_init_session`, `uxr_create_session`, `uxr_delete_session`, `uxr_run_session_time`
- [ ] Bind stream API: `uxr_create_output_reliable_stream`, `uxr_create_input_reliable_stream`
- [ ] Bind entity API: `uxr_buffer_create_participant_bin`, `_topic_bin`, `_publisher_bin`, `_datawriter_bin`, `_subscriber_bin`, `_datareader_bin`
- [ ] Bind data API: `uxr_prepare_output_stream`, `uxr_buffer_request_data`, `uxr_set_topic_callback`
- [ ] Bind service API: `uxr_buffer_create_requester_bin`, `uxr_buffer_create_replier_bin`
- [ ] Builds for `thumbv7m-none-eabi` (QEMU) and host targets
- [ ] `cargo check -p xrce-sys` passes

**Acceptance criteria:**
- Compiles C library from submodule without external dependencies
- All 15+ core XRCE-DDS C functions are bindable from Rust
- No heap allocation required by the C library itself (static allocation mode)

---

### 34.5: Create `xrce-smoltcp` (UDP transport)

**Directory:** `packages/xrce/xrce-smoltcp/`

- [ ] Implement 4 XRCE-DDS custom transport callbacks using smoltcp UDP sockets:
  - `open_func` → open UDP socket
  - `close_func` → close UDP socket
  - `write_func` → send UDP datagram
  - `read_func` → receive UDP datagram with timeout
- [ ] Static socket storage (no heap)
- [ ] Builds for `thumbv7m-none-eabi`
- [ ] Unit test with mock UDP socket

**Acceptance criteria:**
- All 4 callbacks compile and link against `xrce-sys`
- UDP transport works on smoltcp `0.12` (same version as zpico-smoltcp)
- No heap allocation

---

### 34.6: Create `nros-rmw-xrce` (RMW trait implementation)

**Directory:** `packages/xrce/nros-rmw-xrce/`

- [ ] `XrceRmw` struct implementing `Rmw` trait
- [ ] `XrceSession` implementing `Session` trait
  - Manages DDS participant, wraps `uxr_run_session_time` in `spin_once`
- [ ] `XrcePublisher` implementing `Publisher` trait
  - Orchestrates 4 XRCE calls: participant → topic → publisher → datawriter
  - `publish_raw()` → `uxr_prepare_output_stream`
- [ ] `XrceSubscriber` implementing `Subscriber` trait
  - Manages datareader + static receive buffer
  - `try_recv_raw()` reads from callback buffer
  - Re-requests data after processing (`uxr_buffer_request_data`)
- [ ] `XrceServiceServer` implementing `ServiceServerTrait`
  - Uses replier pattern
- [ ] `XrceServiceClient` implementing `ServiceClientTrait`
  - Uses requester pattern
- [ ] All types compile on `no_std` without `alloc`
- [ ] Unit tests for entity creation and session lifecycle

**Acceptance criteria:**
- Implements all 5 `nros-rmw` traits (`Rmw`, `Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait`)
- Platform crate can swap `ZenohRmw` → `XrceRmw` with only a feature flag change
- No heap allocation

---

### 34.7: Create `xrce-platform-qemu` (platform support)

**Directory:** `packages/xrce/xrce-platform-qemu/`

- [ ] Implement ~6 platform symbols needed by XRCE-DDS client:
  - `clock_gettime` (for session synchronization)
  - Any other platform-required C functions
- [ ] Much smaller than zpico-platform-qemu (6 vs 55 symbols)
- [ ] Builds for `thumbv7m-none-eabi`

**Acceptance criteria:**
- XRCE-DDS client links and initializes on QEMU MPS2-AN385

---

### 34.8: Integration testing

- [ ] Set up [Micro-XRCE-DDS Agent](https://github.com/eProsima/Micro-XRCE-DDS-Agent) build
- [ ] Create `XrceAgent` fixture in `nros-tests/src/fixtures/`
- [ ] Create `nros-tests/tests/xrce.rs` test suite
- [ ] Test: QEMU XRCE publisher → Agent → verify data arrives
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
| Platform symbols | ~55 FFI exports | ~6 callbacks |
| Transport impl complexity | 8+ TCP functions | 4 UDP callbacks |
| Bridge process | zenohd (optional for peer mode) | Agent (**mandatory**) |

## Dependency on Phase 33.3

Phase 33.3 splits each platform crate into `zpico-platform-*` (FFI symbols) + `nros-*` (user API). Phase 34.3 (refactor to use traits) applies to whichever shape the platform crates are in:

| Phase 33.3 status | Phase 34.3 target |
|---|---|
| **Not started** | Refactor existing `nano-ros-platform-*` crates in-place |
| **Complete** | Refactor split `nros-*` board crates (cleaner, less code) |

Either way, the refactoring work is the same: replace `zpico_sys::*` calls with `nros-rmw` trait calls. The split just determines file locations.

## Execution Order

```
34.1 (Rmw trait) ──→ 34.2 (zenoh impl) ──→ 34.3 (platform refactor)
                                               ↓
                     34.4 (xrce-sys) ──→ 34.5 (xrce-smoltcp) ──→ 34.6 (nros-rmw-xrce)
                                                                        ↓
                                         34.7 (xrce-platform-qemu) ──→ 34.8 (integration tests)
```

- **34.1 → 34.2 → 34.3** is the critical path. Validates the abstraction with the existing backend.
- **34.4 → 34.7** can start in parallel once 34.1 stabilizes the trait interface.
- **34.8** requires both paths to converge.
- **34.1 and 34.2 are independent of Phase 33.3** — they modify `nros-rmw` and `nros-rmw-zenoh` which are already split.

## Estimated Effort

| Step | Description | Effort | Parallelizable |
|------|-------------|--------|----------------|
| 34.1 | Add Rmw trait + RmwConfig | 2-3 days | No (foundation) |
| 34.2 | Implement ZenohRmw | 3-5 days | After 34.1 |
| 34.3 | Refactor 4 platform crates | 1 week | After 34.2 |
| 34.4 | xrce-sys FFI bindings | 1 week | After 34.1 |
| 34.5 | xrce-smoltcp UDP transport | 3 days | After 34.4 |
| 34.6 | nros-rmw-xrce implementation | 2 weeks | After 34.1 + 34.4 |
| 34.7 | xrce-platform-qemu | 2 days | After 34.5 |
| 34.8 | Integration testing | 1 week | After 34.3 + 34.6 |
| **Total** | | **~7-8 weeks** | |

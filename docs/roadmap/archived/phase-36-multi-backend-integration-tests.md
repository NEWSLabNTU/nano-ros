# Phase 36: Multi-Backend Integration Tests

**Status: Complete**

**Prerequisites:** Phase 34 (RMW abstraction + XRCE-DDS backend, 34.1-34.8 complete)

## Goal

Create a comprehensive integration test matrix that validates all ROS patterns (pub/sub, services, actions) across all RMW backends (zenoh, XRCE-DDS) and platforms (native, QEMU, Zephyr). Today, each backend has its own isolated test setup, and examples are zenoh-specific. This phase unifies the testing story so that swapping backends is validated end-to-end.

## Current State

### What exists

| Pattern       | Zenoh (native)               | Zenoh (QEMU)              | Zenoh (Zephyr)         | XRCE (native)       |
|---------------|------------------------------|---------------------------|------------------------|---------------------|
| Pub/sub       | `nano2nano.rs` (8 tests)     | `emulator.rs` (10+ tests) | `zephyr.rs` (8+ tests) | `xrce.rs` (4 tests) |
| Services      | `services.rs` (7 tests)      | -                         | `zephyr.rs` (partial)  | `xrce.rs` (3 tests) |
| Actions       | `actions.rs` (5 tests)       | -                         | `zephyr.rs` (partial)  | `xrce.rs` (3 tests) |
| ROS 2 interop | `rmw_interop.rs` (15+ tests) | -                         | -                      | -                   |

### Gaps

1. **XRCE service tests** — `nros-rmw-xrce` implements `ServiceServerTrait`/`ServiceClientTrait` but no integration test exercises them
2. **XRCE action tests** — Actions compose from services + topics at `nros-node` layer; XRCE has the primitives but no test
3. **Native examples are zenoh-only** — `rs-talker`, `rs-service-server`, etc. only have a `zenoh` feature; no way to build them against XRCE
4. **No shared test binary** — Zenoh examples use `nros` + `Context::from_env()` API; XRCE test binaries use raw `nros-rmw` + `nros-rmw-xrce` API. These are different abstraction levels.
5. **Test binaries duplicate code** — Each backend builds its own talker/listener with hand-coded CDR instead of generated message types
6. **Board crates are zenoh-hardcoded** — `nros-mps2-an385` directly depends on `nros-rmw-zenoh`; no feature flag to swap to XRCE

### Architectural observations

- **Native examples** use `nros` (high-level crate) with `Context::from_env()` → executor → node API. The `zenoh` feature on `nros` activates `nros-rmw-zenoh/platform-posix`.
- **XRCE test binaries** (`xrce-native-test`) use raw `nros-rmw` traits + `nros-rmw-xrce` directly, bypassing `nros`/`nros-node`. They hand-code CDR for `Int32`.
- **Board crates** (`nros-mps2-an385`) import `nros-rmw-zenoh` directly in `Cargo.toml` and `node.rs`. Swapping to XRCE requires feature-gating the dependency and transport init.
- The `nros` crate has `zenoh` and `platform-*` features but no `xrce` feature.
- XRCE's `init_transport()` + custom callbacks is a different initialization path from zenoh's session open. Board crate (or test harness) must call it before `XrceRmw::open()`.

## Design Decisions

### Approach: Feature-gated `nros` crate + XRCE-aware test binaries

Rather than renaming existing examples, we add an `xrce` feature to the `nros` crate and create new XRCE-specific native test binaries that use the full `nros` stack. This avoids disrupting existing zenoh examples while enabling the same test patterns for XRCE.

**Why not rename existing examples?**
- The existing native examples (rs-talker, rs-service-server, rs-action-server, etc.) use the `nros` high-level API (`Context`, `Executor`, `Node`), which is zenoh-specific today (the context/executor layer depends on zenoh session management).
- XRCE's single-static-session model and `spin_once()` polling are fundamentally different from zenoh's threaded executor. Forcing both into the same example would require significant abstraction that doesn't yet exist in `nros-node`.
- The right path is: (a) create XRCE test binaries at the same abstraction level as xrce-native-test, (b) add services/actions to those binaries, (c) later unify the `nros-node` layer to support both backends (Phase 37+).

### Test binary structure

```
packages/testing/xrce-native-test/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Transport + CDR helpers (exists)
│   ├── bin/
│   │   ├── xrce-talker.rs        # Pub/sub publisher (exists)
│   │   ├── xrce-listener.rs      # Pub/sub subscriber (exists)
│   │   ├── xrce-service-server.rs  # NEW: Service server
│   │   ├── xrce-service-client.rs  # NEW: Service client
│   │   ├── xrce-action-server.rs   # NEW (Phase 36.5): Action server
│   │   └── xrce-action-client.rs   # NEW (Phase 36.5): Action client
```

## Steps

---

### 36.1: Add generated message types to xrce-native-test

**Files:** `packages/testing/xrce-native-test/Cargo.toml`, `package.xml` (new), `generated/` (regenerated)

Currently xrce-talker/listener hand-code CDR for `Int32`. This is fragile and can't scale to services/actions. Switch to generated types.

- [x] Add `package.xml` to xrce-native-test declaring `std_msgs`, `example_interfaces` deps
- [x] Run `cargo nano-ros generate-rust` to create `generated/` directory (5 packages: std_msgs, builtin_interfaces, example_interfaces, action_msgs, unique_identifier_msgs)
- [x] Add generated crate deps to `Cargo.toml` + `[patch.crates-io]` in `.cargo/config.toml`
- [x] Update `xrce-talker.rs` to use `std_msgs::msg::Int32` + typed `Publisher::publish()` instead of hand-coded `encode_int32_cdr()`
- [x] Update `xrce-listener.rs` to use typed `Subscriber::try_recv::<Int32>()` instead of `decode_int32_cdr()`
- [x] Keep `lib.rs` transport init helpers (still needed)
- [x] Remove hand-coded CDR helpers from `lib.rs` (no longer needed)
- [x] Fix `build_xrce_test_binary()` to use `.dir()` instead of `--manifest-path` (Cargo config discovery)
- [x] Add xrce-native-test to `generate-bindings` and `clean-bindings` justfile recipes
- [x] Verify: `cargo build --release` passes
- [x] Verify: `just test-xrce` — 3/3 tests pass
- [x] Verify: `just quality` passes

---

### 36.2: XRCE service server/client test binaries

**Files:** `packages/testing/xrce-native-test/src/bin/xrce-service-server.rs`, `xrce-service-client.rs`

Create service test binaries using `nros-rmw` `ServiceServerTrait`/`ServiceClientTrait` via XRCE-DDS.

**xrce-service-server:**
- [x] Uses `example_interfaces::srv::AddTwoInts` (from generated bindings)
- [x] Creates XRCE session, creates service server via `session.create_service_server()`
- [x] Polls with `session.spin_once()` in a loop
- [x] Uses `handle_request::<AddTwoInts>()` for typed CDR deserialization + handler callback
- [x] Prints "Received request: a=X b=Y" and "Sent reply: sum=Z" for test pattern matching
- [x] Env vars: `XRCE_AGENT_ADDR`, `XRCE_DOMAIN_ID`, `XRCE_TIMEOUT`

**xrce-service-client:**
- [x] Creates XRCE session, creates service client via `session.create_service_client()`
- [x] Sends N requests using `call::<AddTwoInts>()` (typed wrapper)
- [x] Prints "Sent request: a=X b=Y" and "Received reply: sum=Z"
- [x] Env vars: `XRCE_AGENT_ADDR`, `XRCE_DOMAIN_ID`, `XRCE_REQUEST_COUNT` (default 3)

- [x] Add `[[bin]]` entries to `Cargo.toml`
- [x] Verify: `cargo build --release --bin xrce-service-server --bin xrce-service-client`

---

### 36.3: XRCE service integration tests

**Files:** `packages/testing/nros-tests/tests/xrce.rs`, `src/fixtures/binaries.rs`

Add service tests to the existing `xrce.rs` test suite.

- [x] Add `build_xrce_service_server()` / `build_xrce_service_client()` cached builders to `binaries.rs`
- [x] Add rstest fixtures: `xrce_service_server_binary`, `xrce_service_client_binary`
- [x] Add test: `test_xrce_service_server_starts` — starts server, waits for readiness marker
- [x] Add test: `test_xrce_service_client_starts` — starts client (expects timeout without server)
- [x] Add test: `test_xrce_service_request_response` — starts server, waits for ready, starts client with `XRCE_REQUEST_COUNT=3`, verifies client receives replies
- [x] Pattern: start server first, wait for readiness, start client, wait for "Received reply:" in client output
- [x] Verify: `just test-xrce` — 6/6 tests pass

---

### 36.4: XRCE pub/sub communication assertion

**Files:** `packages/testing/nros-tests/tests/xrce.rs`

The existing `test_xrce_talker_listener_communication` is soft — it prints `[INFO]` instead of failing when no messages are received. Harden it.

- [x] Change the communication test to `assert!(received_count >= 1)` instead of just printing
- [x] Add test: `test_xrce_multiple_messages` — verify at least 3 messages received with `XRCE_MSG_COUNT=5`
- [x] Verify: `just test-xrce` — 7/7 tests pass
- [x] Verify: `just quality` passes

Note: `test_xrce_subscriber_before_publisher` was not added as a separate test because `test_xrce_talker_listener_communication` and `test_xrce_multiple_messages` already start the listener first and assert on received messages. A separate retry-logic test was not needed since the 2s stabilization delay and 15-20s timeouts are sufficient.

---

### 36.5: XRCE action test binaries (stretch goal)

**Files:** `packages/testing/xrce-native-test/src/bin/xrce-action-server.rs`, `xrce-action-client.rs`

Actions compose from 5 channels (3 services + 2 topics). XRCE-DDS supports all required primitives. However, the `nros-rmw` traits don't include action-level abstractions — actions are composed at the `nros-node` layer.

**Approach:** Implement a minimal action protocol using raw service + topic entities:
- `send_goal` service (client → server)
- `get_result` service (client → server)
- `feedback` topic (server → client)
- `cancel_goal` service (client → server, optional)
- `status` topic (server → client, optional)

This is a significant amount of work. Defer if XRCE service tests prove the RMW traits work.

- [x] `xrce-action-server.rs` — Creates 2 service servers (send_goal, get_result) + 1 publisher (feedback)
- [x] `xrce-action-client.rs` — Creates 2 service clients + 1 subscriber (feedback)
- [x] Protocol: Fibonacci action from `example_interfaces`
- [x] Add integration tests to `xrce.rs` (3 tests: server_starts, client_starts, fibonacci E2E)
- [x] Fix: `uxr_buffer_request_data` must be flushed immediately with `uxr_run_session_time` to prevent reliable stream congestion when creating multiple entities (nros-rmw-xrce bug)
- [x] Verify: `just test-xrce` — 10/10 tests pass
- [x] Verify: `just quality` passes

---

### 36.6: XRCE-DDS serial transport (`xrce-serial`)

**Files:** `packages/xrce/xrce-serial/` (new crate), `packages/xrce/xrce-sys/src/lib.rs`, `packages/xrce/xrce-sys/build.rs`

Currently XRCE-DDS only supports UDP transport via `xrce-smoltcp`, which requires an IP-capable network interface (Ethernet or WiFi). Many embedded boards — notably the ROBOTIS OpenCR (STM32F746, TurtleBot3 controller) — lack a network interface but have UART. Adding serial transport enables nano-ros on these boards by communicating with a Micro-XRCE-DDS Agent over UART (the same approach micro-ROS uses).

The upstream Micro-XRCE-DDS-Client library already includes a serial transport with framing protocol (`uxr_init_serial_transport`). The `nros-rmw-xrce` layer is transport-agnostic — it uses custom transport callbacks registered via `init_transport()`, so no changes are needed there.

**Implementation approach: custom transport callbacks** (same pattern as `xrce-smoltcp`):

Create an `xrce-serial` crate that implements the 4 XRCE custom transport callbacks (`open`, `close`, `write`, `read`) over a platform-provided UART. This gives full control over framing, DMA, and ring buffers, and avoids needing to expose the C library's platform-specific `uxrSerialPlatform` struct.

```
packages/xrce/xrce-serial/
├── Cargo.toml
└── src/
    └── lib.rs    # XrceSerialTransport: open/close/write/read callbacks + HDLC framing
```

**Key design:**
- Static TX/RX buffers (no heap, `no_std` compatible)
- Platform crate provides UART open/close/read/write function pointers at init time
- HDLC-like framing for reliable byte-stream transport (flag bytes, CRC, escaping)
- Configurable baud rate, buffer sizes
- Timeout-based read polling (same pattern as `xrce-smoltcp`)

**Implementation approach (revised):** Rather than a separate `xrce-serial` crate, the serial transport is implemented using the existing custom transport callback mechanism with `framing=true`. The XRCE-DDS C library has built-in HDLC framing support — when enabled, the library automatically wraps/unwraps messages in HDLC frames. The transport callbacks just provide raw byte I/O.

**Steps:**
- [x] Enable `UCLIENT_PROFILE_STREAM_FRAMING` in `xrce-sys` config.h and compile `stream_framing_protocol.c`
- [x] Add `framing: bool` parameter to `nros-rmw-xrce::init_transport()` (pass to `uxr_set_custom_transport_callbacks`)
- [x] Update existing callers (`xrce-native-test` UDP transport → `framing: false`)
- [x] Add POSIX serial transport to `xrce-native-test/src/lib.rs` (`init_posix_serial_transport()` with PTY + termios + `framing: true`)
- [x] Create `xrce-serial-talker.rs` and `xrce-serial-listener.rs` test binaries (same as UDP versions but using serial transport)
- [x] Add `XrceSerialAgent` fixture (socat PTY pair + `MicroXRCEAgent pseudoterminal` mode)
- [x] Add `require_socat()` availability check
- [x] Add serial binary builders (`build_xrce_serial_talker/listener`) and rstest fixtures
- [x] Add 3 serial integration tests: startup (talker), startup (listener), E2E communication (two agents for point-to-point serial)
- [x] Verify: `cargo build -p xrce-native-test --release` — all binaries compile
- [x] Verify: `cargo check -p nros-tests --tests` — integration tests compile
- [x] Verify: `just quality` passes

**Use cases unlocked:**
- **OpenCR (STM32F746)**: UART to Raspberry Pi running Micro-XRCE-DDS Agent → ROS 2 network
- **Any MCU with UART**: Boards without Ethernet/WiFi can participate in ROS 2 via a host-side Agent
- **USB CDC**: USB virtual serial ports (same byte-stream interface)

---

### 36.7: Add `xrce` feature to `nros` crate (foundation for future)

**Files:** `packages/core/nros/Cargo.toml`, `packages/core/nros/src/lib.rs`

Lay the groundwork for examples that use the `nros` high-level API with XRCE backend.

- [x] Add `nros-rmw-xrce` as optional dependency
- [x] Add `xrce` feature: `["dep:nros-rmw-xrce", "nros-rmw-xrce/posix"]`
- [x] Add `xrce-bare-metal` feature: `["dep:nros-rmw-xrce", "nros-rmw-xrce/bare-metal"]`
- [x] Re-export `nros_rmw_xrce` under `#[cfg(any(feature = "xrce", feature = "xrce-bare-metal"))]`
- [x] Document that `zenoh` and `xrce` are mutually exclusive (compile-time selection)
- [x] Verify: `cargo check -p nros --no-default-features -F xrce -F std` passes
- [x] Verify: `cargo check -p nros --no-default-features -F xrce-bare-metal` passes
- [x] Verify: `just quality` passes

**Note:** This does not yet make `Context`/`Executor` work with XRCE — that requires `nros-node` changes (Phase 37+). This step only makes the raw `nros-rmw-xrce` types available through the `nros` crate.

---

### 36.8: Decouple RMW/platform/ROS-edition feature axes

**Files:** `packages/core/nros/Cargo.toml`, `packages/core/nros/src/lib.rs`, `packages/core/nros-node/Cargo.toml`, `packages/core/nros-node/src/*.rs`, `packages/xrce/nros-rmw-xrce/Cargo.toml`, `packages/core/nros-c/Cargo.toml`, example `Cargo.toml` files, `CLAUDE.md`

The `nros` crate's feature flags conflate three orthogonal decisions:

1. **RMW backend** — `zenoh` implies `platform-posix`; `platform-*` implies `dep:nros-rmw-zenoh`
2. **Platform** — `platform-zephyr` hard-codes zenoh; no way to express "XRCE on Zephyr"
3. **ROS edition** — already clean (`ros-humble`, `ros-iron`)

This step restructures features into three mandatory, orthogonal axes. Users must specify all three:

```toml
nros = { default-features = false, features = [
    "rmw-zenoh",        # axis 1: RMW backend
    "platform-posix",   # axis 2: platform
    "ros-humble",       # axis 3: ROS edition
    "std",              # memory model
] }
```

Default: `["std", "rmw-zenoh", "platform-posix", "ros-humble"]` (same behavior as today for desktop users).

#### Design: new feature layout

**`nros/Cargo.toml`:**
```toml
[features]
default = ["std", "rmw-zenoh", "platform-posix", "ros-humble"]

# Memory model
std = ["alloc", "nros-core/std", "nros-node/std", "nros-rmw/std",
       "nros-rmw-zenoh?/std", "nros-params/std"]
alloc = ["nros-core/alloc", "nros-node/alloc", "nros-rmw/alloc", "nros-params/alloc"]

# RMW backend (select one)
rmw-zenoh = ["dep:nros-rmw-zenoh", "nros-node/rmw-zenoh"]
rmw-xrce = ["dep:nros-rmw-xrce"]

# Platform (select one; forwarded to whichever RMW backend is active)
platform-posix = ["nros-node/platform-posix",
                  "nros-rmw-zenoh?/platform-posix", "nros-rmw-xrce?/platform-posix"]
platform-zephyr = ["nros-node/platform-zephyr", "nros-rmw-zenoh?/platform-zephyr"]
platform-bare-metal = ["nros-node/platform-bare-metal",
                       "nros-rmw-zenoh?/platform-bare-metal", "nros-rmw-xrce?/platform-bare-metal"]

# ROS edition
ros-humble = ["nros-node/ros-humble"]
ros-iron = ["nros-node/ros-iron"]

# Cross-cutting
safety-e2e = ["nros-node/safety-e2e", "nros-rmw/safety-e2e", "nros-rmw-zenoh?/safety-e2e"]
param-services = ["nros-node/param-services"]
rtic = ["nros-node/rtic"]
polling = ["nros-node/polling"]

# XRCE transport refinements
xrce-udp = ["nros-rmw-xrce?/posix-udp"]
xrce-serial = ["nros-rmw-xrce?/posix-serial"]
```

Key changes from current layout:
- `zenoh` → `rmw-zenoh` (no longer implies `platform-posix`)
- `xrce` / `xrce-bare-metal` → `rmw-xrce` (platform choice is separate)
- `platform-*` features are backend-agnostic (use `?` syntax to forward to active backend)

**`nros-node/Cargo.toml`:**
```toml
# RMW backend
rmw-zenoh = ["dep:nros-rmw-zenoh"]  # replaces both "zenoh" and "shim"

# Platform (forwarded to active backend only)
platform-posix = ["nros-rmw-zenoh?/platform-posix"]
platform-zephyr = ["nros-rmw-zenoh?/platform-zephyr"]
platform-bare-metal = ["nros-rmw-zenoh?/platform-bare-metal"]

# Executor modes
rtic = ["nros-rmw-zenoh?/rtic"]
polling = ["nros-rmw-zenoh?/polling"]

# Safety & params
safety-e2e = ["nros-rmw/safety-e2e", "nros-rmw-zenoh?/safety-e2e"]
param-services = ["dep:nros-rcl-interfaces", "rmw-zenoh", "alloc"]
```

Key changes: `zenoh` and `shim` both become `rmw-zenoh`. The `zenoh` feature no longer implies `platform-posix` or `alloc`.

**`nros-rmw-xrce/Cargo.toml`:**
```toml
# Platform (renamed for consistency with other crates)
platform-posix = ["xrce-sys/posix"]
platform-bare-metal = ["xrce-sys/bare-metal"]
posix-udp = ["platform-posix"]
posix-serial = ["platform-posix", "dep:libc"]
```

Key changes: `posix` → `platform-posix`, `bare-metal` → `platform-bare-metal`.

#### Work items

**Cargo.toml changes (3 files):**
- [x] `packages/core/nros/Cargo.toml` — rewrite features per design above
- [x] `packages/core/nros-node/Cargo.toml` — rename `zenoh`/`shim` → `rmw-zenoh`, decouple platform
- [x] `packages/xrce/nros-rmw-xrce/Cargo.toml` — rename `posix` → `platform-posix`, `bare-metal` → `platform-bare-metal`

**cfg gate updates (6 files, ~120 occurrences):**
- [x] `packages/core/nros/src/lib.rs` — `feature = "zenoh"` → `feature = "rmw-zenoh"`, `feature = "xrce"` / `feature = "xrce-bare-metal"` → `feature = "rmw-xrce"`
- [x] `packages/core/nros-node/src/lib.rs` — `feature = "zenoh"` / `feature = "shim"` → `feature = "rmw-zenoh"`
- [x] `packages/core/nros-node/src/connected.rs` — `feature = "zenoh"` → `feature = "rmw-zenoh"`
- [x] `packages/core/nros-node/src/context.rs` — `feature = "zenoh"` → `feature = "rmw-zenoh"`
- [x] `packages/core/nros-node/src/executor.rs` — `feature = "zenoh"` → `feature = "rmw-zenoh"`
- [x] `packages/core/nros-node/src/lifecycle.rs` — `feature = "zenoh"` → `feature = "rmw-zenoh"`

**Downstream crate updates:**
- [x] `packages/core/nros-c/Cargo.toml` — platform features must explicitly activate `nros/rmw-zenoh`

**Example Cargo.toml updates (~13 files):**
- [x] 7 native zenoh examples — `"nros/zenoh"` → `"nros/rmw-zenoh", "nros/platform-posix"`
- [x] 6 Zephyr examples — `features = ["platform-zephyr"]` → `features = ["rmw-zenoh", "platform-zephyr"]`

**Documentation:**
- [x] Update `CLAUDE.md` feature table and "Platform Backends" section

**Additional changes (discovered during implementation):**
- [x] Extracted `RclrsError` from `context.rs` into `error.rs` (no alloc dependency) so executor/shim work without alloc
- [x] Gated `connected`, `context`, `executor` modules on `all(rmw-zenoh, alloc)` — Connected API needs alloc; Shim API works without
- [x] Fixed `justfile` broken recipes (`nros-rmw --features zenoh,std` → `nros-rmw --features std`)
- [x] Updated `rtic_integration.rs` test cfg gates

**Verification:**
- [x] `cargo check -p nros` — default features (`rmw-zenoh` + `platform-posix` + `std`)
- [x] `cargo check -p nros --no-default-features -F rmw-zenoh,platform-posix,std,ros-humble`
- [x] `cargo check -p nros --no-default-features -F rmw-xrce,platform-posix,std,ros-humble`
- [x] `cargo check -p nros --no-default-features -F rmw-xrce,platform-bare-metal`
- [x] `cargo check -p nros --no-default-features -F rmw-zenoh,platform-bare-metal`
- [ ] `just build` — full build including all examples (requires `just build-zenoh-pico-arm`)
- [x] `just quality` — format + clippy + tests

---

### 36.9: Test matrix documentation

**Files:** `tests/README.md` (update), `docs/roadmap/phase-36-multi-backend-integration-tests.md` (this file)

Document the complete test coverage matrix.

- [x] Update `tests/README.md` with XRCE test section (prerequisites, how to run, what's tested)
- [x] Add test matrix table to this document showing: pattern × backend × platform → test file
- [x] Document which combinations are tested, planned, and not applicable

---

**Note:** 36.8 (feature decoupling) and 36.9 (documentation) can proceed in either order. 36.8 is a prerequisite for Phase 37 (backend-agnostic `nros-node`).

---

## Test Coverage Matrix

Actual test coverage after Phase 36 completion:

### By ROS pattern × RMW backend × platform

| Pattern | Zenoh native | Zenoh QEMU ARM | Zenoh QEMU ESP32 | Zenoh Zephyr | XRCE native (UDP) | XRCE native (serial) | XRCE C API |
|---------|:------------:|:--------------:|:-----------------:|:------------:|:-----------------:|:--------------------:|:----------:|
| Pub/sub | `nano2nano.rs` (7) | `emulator.rs` (12) | `esp32_emulator.rs` (9) | `zephyr.rs` (20) | `xrce.rs` (5) | `xrce.rs` (3) | `c_xrce_api.rs` (5) |
| Services | `services.rs` (8) | - | - | `zephyr.rs` (partial) | `xrce.rs` (3) | - | - |
| Actions | `actions.rs` (4) | - | - | `zephyr.rs` (partial) | `xrce.rs` (3) | - | - |
| Large msgs | - | - | - | - | `xrce.rs` (1) | - | - |
| ROS 2 interop (zenoh) | `rmw_interop.rs` (17) | - | - | - | N/A | N/A | - |
| ROS 2 interop (DDS) | N/A | N/A | N/A | N/A | `xrce_ros2_interop.rs` (4) | N/A | - |
| Custom msgs | `custom_msg.rs` (7) | - | - | - | - | - | - |
| Parameters | `params.rs` (8) | - | - | - | N/A | N/A | - |
| QoS | `qos.rs` (6) | - | - | - | - | - | - |
| Multi-node | `multi_node.rs` (8) | - | - | - | - | - | - |
| Safety E2E | `safety_e2e.rs` (2) | - | - | - | - | - | - |
| Executor | `executor.rs` (7) | - | - | - | - | - | - |
| Error handling | `error_handling.rs` (8) | - | - | - | - | - | - |
| C API (zenoh) | `c_api.rs` (5) | - | - | - | N/A | N/A | N/A |

Numbers in parentheses are test counts. `-` = not tested. `N/A` = not applicable.

### By test command × prerequisites

| Command | Test file(s) | Prerequisites | Test group |
|---------|-------------|---------------|------------|
| `just test-unit` | Unit tests in each crate | None | - |
| `just test-miri` | nros-serdes, nros-core, nros-params | None | - |
| `just test-integration` | nano2nano, services, actions, custom_msg, params, qos, multi_node, safety_e2e, executor, error_handling, rmw, platform | zenohd | default |
| `just test-qemu` | emulator.rs | qemu-system-arm | arm-emulator |
| `just test-qemu-esp32` | esp32_emulator.rs | qemu-system-riscv32, espflash | esp32-emulator |
| `just test-xrce` | xrce.rs | XRCE Agent, socat | xrce |
| `just test-xrce-ros2` | xrce_ros2_interop.rs | XRCE Agent, ROS 2, rmw_fastrtps | xrce_ros2_interop |
| `just test-zephyr` | zephyr.rs | west, TAP network | zephyr |
| `just test-ros2` | rmw_interop.rs | ROS 2, rmw_zenoh_cpp | - |
| `just test-c` | c_api.rs | cmake, zenohd | c_api |
| `just test-c-xrce` | c_xrce_api.rs | cmake, XRCE Agent | c_api |

### Combinations not tested (and why)

| Combination | Reason |
|-------------|--------|
| XRCE + Zephyr | Board crate not yet XRCE-aware (Phase 37+) |
| XRCE + QEMU | Board crate needs feature-gating (Phase 37+) |
| XRCE + ESP32 | ESP32 XRCE platform symbols not implemented |
| Zenoh services on QEMU | QEMU firmware lacks service binaries |
| Zenoh actions on QEMU | QEMU firmware lacks action binaries |
| XRCE serial + services/actions | Serial transport validated via pub/sub; services/actions use same underlying RMW traits |
| XRCE parameters | Parameters are composed at `nros-node` layer which doesn't yet support XRCE backend |

## Execution Order

1. **36.1** (generated messages) — Foundation for all subsequent steps
2. **36.2** (service binaries) — New test binaries
3. **36.3** (service tests) — Integration tests for services
4. **36.4** (harden pub/sub tests) — Improve existing tests
5. **36.5** (action binaries) — Optional stretch goal
6. **36.6** (serial transport) — XRCE-DDS over UART
7. **36.7** (`nros` xrce feature) — Foundation for 36.8
8. **36.8** (feature decoupling) — Decouple RMW/platform/ROS-edition axes
9. **36.9** (documentation) — Final documentation

Steps 36.2-36.4 can proceed in parallel after 36.1. Steps 36.6 and 36.7 are independent of each other and of 36.2-36.5. Step 36.8 depends on 36.7 (rewrites the features 36.7 introduced).

## Future Work (Phase 37+)

- **Backend-agnostic `nros-node`**: Make `Context`/`Executor`/`Node` work with XRCE (requires abstracting session initialization, `spin_once()` integration, transport callback registration). Prerequisite: 36.8 (feature decoupling) completed first.
- **Unified native examples**: Single rs-talker with `--features rmw-zenoh` or `--features rmw-xrce`
- **XRCE QEMU board crate**: `xrce-platform-mps2-an385` is already created (Phase 34.7); need board crate (`nros-mps2-an385-xrce` or feature-gated `nros-mps2-an385`)
- **XRCE-DDS ↔ ROS 2 interop**: XRCE Agent bridges to DDS, enabling ROS 2 interop via different protocol than zenoh
- **OpenCR board crate**: `nros-opencr` using `xrce-serial` over UART to Raspberry Pi running Micro-XRCE-DDS Agent (enables TurtleBot3 with nano-ros)
- **Serial transport on embedded boards**: `UartPlatform` implementations for STM32 HAL, Zephyr UART, ESP32 UART — any board with UART can participate in ROS 2 via host-side Agent
- **CI matrix**: GitHub Actions job matrix with `{backend} × {platform} × {transport}` axes

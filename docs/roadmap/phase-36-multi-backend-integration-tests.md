# Phase 36: Multi-Backend Integration Tests

**Status: Not Started**

**Prerequisites:** Phase 34 (RMW abstraction + XRCE-DDS backend, 34.1-34.8 complete)

## Goal

Create a comprehensive integration test matrix that validates all ROS patterns (pub/sub, services, actions) across all RMW backends (zenoh, XRCE-DDS) and platforms (native, QEMU, Zephyr). Today, each backend has its own isolated test setup, and examples are zenoh-specific. This phase unifies the testing story so that swapping backends is validated end-to-end.

## Current State

### What exists

| Pattern | Zenoh (native) | Zenoh (QEMU) | Zenoh (Zephyr) | XRCE (native) |
|---------|----------------|---------------|----------------|----------------|
| Pub/sub | `nano2nano.rs` (8 tests) | `emulator.rs` (10+ tests) | `zephyr.rs` (8+ tests) | `xrce.rs` (3 tests) |
| Services | `services.rs` (7 tests) | - | `zephyr.rs` (partial) | - |
| Actions | `actions.rs` (5 tests) | - | `zephyr.rs` (partial) | - |
| ROS 2 interop | `rmw_interop.rs` (15+ tests) | - | - | - |

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
- [ ] Uses `example_interfaces::srv::AddTwoInts` (from generated bindings)
- [ ] Creates XRCE session, creates service server via `session.create_service_server()`
- [ ] Polls with `session.spin_once()` in a loop
- [ ] Uses `handle_request::<AddTwoInts>()` for typed CDR deserialization + handler callback
- [ ] Prints "Received request: a=X b=Y" and "Sent reply: sum=Z" for test pattern matching
- [ ] Env vars: `XRCE_AGENT_ADDR`, `XRCE_DOMAIN_ID`

**xrce-service-client:**
- [ ] Creates XRCE session, creates service client via `session.create_service_client()`
- [ ] Sends N requests using `call::<AddTwoInts>()` (typed wrapper)
- [ ] Prints "Sent request: a=X b=Y" and "Received reply: sum=Z"
- [ ] Env vars: `XRCE_AGENT_ADDR`, `XRCE_DOMAIN_ID`, `XRCE_REQUEST_COUNT` (default 3)

- [ ] Add `[[bin]]` entries to `Cargo.toml`
- [ ] Verify: `cargo build --release --bin xrce-service-server --bin xrce-service-client --manifest-path ...`

---

### 36.3: XRCE service integration tests

**Files:** `packages/testing/nros-tests/tests/xrce.rs`, `src/fixtures/binaries.rs`

Add service tests to the existing `xrce.rs` test suite.

- [ ] Add `build_xrce_service_server()` / `build_xrce_service_client()` cached builders to `binaries.rs`
- [ ] Add rstest fixtures: `xrce_service_server_binary`, `xrce_service_client_binary`
- [ ] Add test: `test_xrce_service_server_starts` — starts server, waits for readiness marker
- [ ] Add test: `test_xrce_service_client_starts` — starts client (expects timeout without server)
- [ ] Add test: `test_xrce_service_request_response` — starts server, waits for ready, starts client with `XRCE_REQUEST_COUNT=3`, verifies client receives replies
- [ ] Pattern: start server first, wait for readiness, start client, wait for "Received reply:" in client output
- [ ] Verify: `just test-xrce`

---

### 36.4: XRCE pub/sub communication assertion

**Files:** `packages/testing/nros-tests/tests/xrce.rs`

The existing `test_xrce_talker_listener_communication` is soft — it prints `[INFO]` instead of failing when no messages are received. Harden it.

- [ ] Change the communication test to `assert!(received_count >= 1)` instead of just printing
- [ ] Add retry logic: if first attempt gets 0 messages, retry once with longer timeout (timing-sensitive test)
- [ ] Add test: `test_xrce_multiple_messages` — verify at least 3 messages received with `XRCE_MSG_COUNT=5`
- [ ] Add test: `test_xrce_subscriber_before_publisher` — start listener first (already the pattern), verify receives messages after talker starts

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

- [ ] `xrce-action-server.rs` — Creates 2 service servers (send_goal, get_result) + 1 publisher (feedback)
- [ ] `xrce-action-client.rs` — Creates 2 service clients + 1 subscriber (feedback)
- [ ] Protocol: Fibonacci action from `example_interfaces`
- [ ] Add integration tests to `xrce.rs`

**Decision:** This step is optional for Phase 36 and may be deferred to Phase 37+ when the `nros-node` layer gains backend-agnostic action support.

---

### 36.6: Add `xrce` feature to `nros` crate (foundation for future)

**Files:** `packages/core/nros/Cargo.toml`, `packages/core/nros/src/lib.rs`

Lay the groundwork for examples that use the `nros` high-level API with XRCE backend.

- [ ] Add `nros-rmw-xrce` as optional dependency
- [ ] Add `xrce` feature: `["dep:nros-rmw-xrce", "nros-rmw-xrce/posix"]`
- [ ] Add `xrce-bare-metal` feature: `["dep:nros-rmw-xrce", "nros-rmw-xrce/bare-metal"]`
- [ ] Re-export `nros_rmw_xrce` under `#[cfg(feature = "xrce")]`
- [ ] Document that `zenoh` and `xrce` are mutually exclusive (compile-time selection)
- [ ] Verify: `cargo check -p nros --features xrce --no-default-features --features std`
- [ ] Verify: `just quality`

**Note:** This does not yet make `Context`/`Executor` work with XRCE — that requires `nros-node` changes (Phase 37+). This step only makes the raw `nros-rmw-xrce` types available through the `nros` crate.

---

### 36.7: Test matrix documentation

**Files:** `tests/README.md` (update), `docs/roadmap/phase-36-multi-backend-integration-tests.md` (this file)

Document the complete test coverage matrix.

- [ ] Update `tests/README.md` with XRCE test section (prerequisites, how to run, what's tested)
- [ ] Add test matrix table to this document showing: pattern × backend × platform → test file
- [ ] Document which combinations are tested, planned, and not applicable

---

## Test Coverage Matrix (Target)

After Phase 36 completion:

| Pattern | Zenoh native | Zenoh QEMU | Zenoh Zephyr | XRCE native | XRCE QEMU |
|---------|:------------:|:----------:|:------------:|:-----------:|:----------:|
| Pub/sub | `nano2nano.rs` | `emulator.rs` | `zephyr.rs` | `xrce.rs` | Phase 37+ |
| Services | `services.rs` | - | `zephyr.rs` | `xrce.rs` **NEW** | Phase 37+ |
| Actions | `actions.rs` | - | `zephyr.rs` | 36.5 (stretch) | Phase 37+ |
| ROS 2 interop | `rmw_interop.rs` | - | - | N/A (diff protocol) | - |
| Custom msgs | `custom_msg.rs` | - | - | - | - |
| Parameters | `params.rs` | - | - | N/A (no node layer) | - |
| QoS | `qos.rs` | - | - | - | - |
| Multi-node | `multi_node.rs` | - | - | - | - |

Legend: `-` = not tested, `N/A` = not applicable for this backend

## Execution Order

1. **36.1** (generated messages) — Foundation for all subsequent steps
2. **36.2** (service binaries) — New test binaries
3. **36.3** (service tests) — Integration tests for services
4. **36.4** (harden pub/sub tests) — Improve existing tests
5. **36.5** (action binaries) — Optional stretch goal
6. **36.6** (`nros` xrce feature) — Foundation for future phases
7. **36.7** (documentation) — Final documentation

Steps 36.2-36.4 can proceed in parallel after 36.1. Step 36.6 is independent.

## Future Work (Phase 37+)

- **Backend-agnostic `nros-node`**: Make `Context`/`Executor`/`Node` work with XRCE (requires abstracting session initialization, `spin_once()` integration, transport callback registration)
- **Unified native examples**: Single rs-talker with `--features zenoh` or `--features xrce`
- **XRCE QEMU board crate**: `xrce-platform-mps2-an385` is already created (Phase 34.7); need board crate (`nros-mps2-an385-xrce` or feature-gated `nros-mps2-an385`)
- **XRCE-DDS ↔ ROS 2 interop**: XRCE Agent bridges to DDS, enabling ROS 2 interop via different protocol than zenoh
- **CI matrix**: GitHub Actions job matrix with `{backend} × {platform}` axes

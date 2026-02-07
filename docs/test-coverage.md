# nano-ros Test Coverage Analysis

This document provides a comprehensive overview of test coverage across all platforms and identifies gaps where additional tests are needed.

## Test Infrastructure Summary

| Component         | Location                       | Description                              |
|-------------------|--------------------------------|------------------------------------------|
| Integration Tests | `crates/nano-ros-tests/tests/` | Rust-based integration tests with rstest |
| Unit Tests        | `crates/*/src/*.rs`            | Inline `#[test]` modules in each crate   |
| Shell Scripts     | `tests/*.sh`                   | Legacy/supplementary test scripts        |
| Test Utilities    | `crates/nano-ros-tests/src/`   | Fixtures, process management, helpers    |

**Nextest Tests:** 548 total (553 including 5 skipped) across 27 binaries
**QEMU Semihosting:** 14 tests (9 basic + 5 LAN9118) via `tests/run-test.sh`
**C API:** 5 integration tests via nextest + codegen tests
**Miri:** 143 tests (nano-ros-serdes, nano-ros-core, nano-ros-params)

## Current Test Coverage by Platform

### 1. Native (Linux/macOS) - POSIX Backend

| Test Suite         | File                | Tests | Coverage                               |
|--------------------|---------------------|-------|----------------------------------------|
| Pub/Sub            | `nano2nano.rs`      | 5     | Talker/listener, peer mode, detection  |
| Services           | `services.rs`       | 8     | Service server/client (AddTwoInts)     |
| Actions            | `actions.rs`        | 4     | Action server/client (Fibonacci)       |
| Custom Messages    | `custom_msg.rs`     | 7     | Serialization, pub/sub, error handling |
| ROS 2 Interop      | `rmw_interop.rs`    | 22    | rmw_zenoh_cpp compatibility            |
| Platform Detection | `platform.rs`       | 10    | Tool/environment detection             |
| Parameters         | `params.rs`         | 7     | Parameter server integration           |
| Timer/Executor     | `executor.rs`       | 7     | Timer firing, callback execution       |
| QoS                | `qos.rs`            | 6     | Reliability, history, multi-sub        |
| Error Handling     | `error_handling.rs` | 8     | Timeouts, disconnect, reconnect        |
| Multi-Node         | `multi_node.rs`     | 8     | Scalability, ordering, sustained       |

**Justfile Recipes:**
- `just test-rust-nano2nano` - Native pub/sub tests
- `just test-rust-services` - Native service tests
- `just test-rust-actions` - Action communication tests
- `just test-rust-custom-msg` - Custom message tests
- `just test-rust-rmw-interop` - ROS 2 interoperability
- `just test-rust-platform` - Platform detection
- `just test-rust-params` - Parameter server tests
- `just test-rust-executor` - Timer/executor tests
- `just test-rust-qos` - QoS policy tests
- `just test-rust-errors` - Error handling tests
- `just test-rust-multi-node` - Multi-node scalability tests

**Examples Covered:**
| Example                    | Tested  | Notes                              |
|----------------------------|---------|------------------------------------|
| `native/rs-talker`         | Yes     | nano2nano, rmw_interop, serial E2E |
| `native/rs-listener`       | Yes     | nano2nano, rmw_interop, serial E2E |
| `native/rs-service-server` | Yes     | services.rs                        |
| `native/rs-service-client` | Yes     | services.rs                        |
| `native/rs-action-server`  | Yes     | actions.rs                         |
| `native/rs-action-client`  | Yes     | actions.rs                         |
| `native/rs-custom-msg`     | Yes     | custom_msg.rs                      |
| `native/c-talker`          | Yes     | c_api.rs (build, start, comms)     |
| `native/c-listener`        | Yes     | c_api.rs (build, start, comms)     |

#### Serial Transport (Phase 18.4)

| Test Type       | Status   | Description                                         |
|-----------------|----------|-----------------------------------------------------|
| Build (native)  | **PASS** | Serial compiles for native POSIX (`just quality`)   |
| Build (smoltcp) | **PASS** | smoltcp serial stubs compile for thumbv7m           |
| Unit tests      | **PASS** | 9 tests: `locator_protocol()`, `validate_locator()` |
| Manual E2E      | **PASS** | 494 messages via serial PTY pair, zero loss         |

**E2E Test Setup:** `socat` PTY pair + serial-enabled zenohd (built from `scripts/zenohd/zenoh` submodule via `just build-zenohd`).

**Path:** talker → serial/PTY1 → [socat] → serial/PTY0 → zenohd → tcp → listener

**Requirements:** `socat`, serial-enabled zenohd (`build/zenohd/zenohd` via `just build-zenohd`)

### 2. Zephyr RTOS - native_sim

| Test Suite         | File        | Tests | Coverage                          |
|--------------------|-------------|-------|-----------------------------------|
| Zephyr Integration | `zephyr.rs` | 20    | Build, smoke, E2E, cross-platform |

All Zephyr Rust examples use the high-level nano-ros API (`ShimExecutor`, `create_node()`, `create_publisher()`, etc.).

**Test Breakdown:**
| Test                                   | Type           | Description                   |
|----------------------------------------|----------------|-------------------------------|
| `test_zephyr_availability_checks`      | Detection      | Verify workspace/network      |
| `test_zephyr_talker_build`             | Build          | Build rs-talker               |
| `test_zephyr_listener_build`           | Build          | Build rs-listener             |
| `test_zephyr_talker_smoke`             | Smoke          | Boot without crash            |
| `test_zephyr_listener_smoke`           | Smoke          | Boot without crash            |
| `test_zephyr_talker_to_listener_e2e`   | E2E            | Zephyr ↔ Zephyr               |
| `test_zephyr_to_native_e2e`            | E2E            | Zephyr → Native               |
| `test_native_to_zephyr_e2e`            | E2E            | Native → Zephyr               |
| `test_bidirectional_native_zephyr_e2e` | E2E            | Both directions at once       |
| `test_zephyr_action_server_build`      | Build          | Build action server           |
| `test_zephyr_action_client_build`      | Build          | Build action client           |
| `test_zephyr_action_server_smoke`      | Smoke          | Boot without crash            |
| `test_zephyr_action_client_smoke`      | Smoke          | Boot without crash            |
| `test_zephyr_action_e2e`               | E2E            | Action communication          |
| `test_zephyr_service_server_build`     | Build          | Build service server          |
| `test_zephyr_service_client_build`     | Build          | Build service client          |
| `test_zephyr_service_server_smoke`     | Smoke          | Boot without crash            |
| `test_zephyr_service_client_smoke`     | Smoke          | Boot without crash            |
| `test_native_server_zephyr_client`     | Cross-Platform | Native server + Zephyr client |
| `test_zephyr_server_native_client`     | Cross-Platform | Zephyr server + Native client |

**Justfile Recipes:**
- `just test-rust-zephyr` - All Zephyr tests
- `just test-rust-zephyr-full` - Rebuild + test
- `just test-rust-zephyr-to-native` - Zephyr talker → Native listener
- `just test-rust-native-to-zephyr` - Native talker → Zephyr listener
- `just test-rust-bidirectional-zephyr` - Both directions simultaneously
- `just test-rust-zephyr-actions` - Action tests only
- `just test-rust-zephyr-services` - Service tests only
- `just test-rust-native-server-zephyr-client` - Cross-platform service test
- `just test-rust-zephyr-server-native-client` - Cross-platform service test
- `just test-zephyr-c` - C examples on Zephyr

**Examples Covered:**
| Example                    | Tested | Notes                         |
|----------------------------|--------|-------------------------------|
| `zephyr/rs-talker`         | Yes    | High-level API (ShimExecutor) |
| `zephyr/rs-listener`       | Yes    | High-level API (ShimExecutor) |
| `zephyr/rs-action-server`  | Yes    | High-level API (ShimExecutor) |
| `zephyr/rs-action-client`  | Yes    | High-level API (ShimExecutor) |
| `zephyr/rs-service-server` | Yes    | High-level API (ShimExecutor) |
| `zephyr/rs-service-client` | Yes    | Build, smoke, cross-platform  |
| `zephyr/c-talker`          | Yes    | test-zephyr-c                 |
| `zephyr/c-listener`        | Yes    | test-zephyr-c                 |

### 3. QEMU ARM (Cortex-M3) - Bare Metal

| Test Suite | File          | Tests | Coverage                          |
|------------|---------------|-------|-----------------------------------|
| Emulator   | `emulator.rs` | 15    | CDR, Node API, type metadata, BSP, rs E2E |

**Test Breakdown:**
| Test                            | Description                  |
|---------------------------------|------------------------------|
| `test_qemu_detection`           | Verify QEMU available        |
| `test_arm_toolchain_detection`  | Verify thumbv7m target       |
| `test_qemu_cdr_serialization`   | CDR encode/decode            |
| `test_qemu_node_api`            | Node, publisher, subscriber  |
| `test_qemu_type_metadata`       | Type names                   |
| `test_qemu_all_tests_pass`      | Parse test results           |
| `test_qemu_output_format`       | Verify markers               |
| `test_qemu_bsp_talker_builds`   | BSP talker binary builds     |
| `test_qemu_bsp_listener_builds` | BSP listener binary builds   |
| `test_qemu_bsp_talker_starts`   | BSP talker starts (Docker)   |
| `test_qemu_bsp_listener_starts` | BSP listener starts (Docker) |
| `test_qemu_bsp_both_build`      | Both BSP binaries build      |

**Justfile Recipes:**
- `just test-rust-emulator` - QEMU emulator tests
- `just test-qemu` - Basic + LAN9118 tests
- `just test-qemu-basic` - Semihosting tests
- `just test-qemu-lan9118` - Network tests
- `just test-qemu-bsp` - BSP build tests
- `just test-qemu-zenoh` - Zenoh communication (manual)
- `just test-rust-qemu-baremetal-bsp` - Full BSP Docker test

**Examples Covered:**
| Example             | Tested  | Notes              |
|---------------------|---------|--------------------|
| `qemu/rs-test`      | Yes     | CDR, Node API      |
| `qemu/rs-talker`    | Yes     | Build + Docker E2E |
| `qemu/rs-listener`  | Yes     | Build + Docker E2E |
| `qemu/bsp-talker`   | Yes     | Build + Docker E2E |
| `qemu/bsp-listener` | Yes     | Build + Docker E2E |

### 4. STM32F4 - Physical Hardware

| Test Suite | Tests | Coverage           |
|------------|-------|--------------------|
| None       | 0     | No automated tests |

**Examples (Untested):**
| Example                                | Status      |
|----------------------------------------|-------------|
| `stm32f4/bsp-talker`                  | Manual only |
| `platform-integration/stm32f4-polling` | Manual only |
| `platform-integration/stm32f4-rtic`    | Manual only |
| `platform-integration/stm32f4-embassy` | Manual only |
| `platform-integration/stm32f4-smoltcp` | Manual only |

### 5. C Bindings

| Test Suite    | File                       | Tests | Coverage              |
|---------------|----------------------------|-------|-----------------------|
| C Integration | `c_api.rs`                 | 5     | Build, start, comms   |
| C Codegen     | `tests/c-msg-gen-tests.sh` | 1     | Message generation    |
| Zephyr C      | `tests/zephyr/run-c.sh`   | 1     | Zephyr C pub/sub      |

**Justfile Recipes:**
- `just test-c` - C API integration + codegen
- `just test-zephyr-c` - Zephyr C examples

**Examples Covered:**
| Example                   | Tested | Notes                    |
|---------------------------|--------|--------------------------|
| `native/c-talker`         | Yes    | c_api.rs                 |
| `native/c-listener`       | Yes    | c_api.rs                 |
| `native/c-custom-msg`     | Yes    | c-msg-gen-tests.sh       |
| `native/c-baremetal-demo` | **NO** | No automated tests       |
| `zephyr/c-talker`         | Yes    | tests/zephyr/run-c.sh    |
| `zephyr/c-listener`       | Yes    | tests/zephyr/run-c.sh    |

## Unit Test Coverage by Crate

| Crate                 | Test Count | Coverage Areas                                                                                                   |
|-----------------------|------------|------------------------------------------------------------------------------------------------------------------|
| `nano-ros-core`       | 75         | Time (17), Action (15), Lifecycle (13), Error (13), Logger (7), Clock (6), Service (2), MessageInfo (2)          |
| `nano-ros-serdes`     | 33         | CDR primitives (6), CDR encoder (5), compatibility (22)                                                          |
| `nano-ros-transport`  | 56         | QoS profiles/builder (34), RMW protocol/liveliness/attachment (22)                                               |
| `nano-ros-node`       | 110        | Connected (38), Context (18), Lifecycle (15), Trigger (10), Timer (8), Executor (8), Node (6), Options (6), Shim (1) |
| `nano-ros-params`     | 30         | Typed parameters (14), server (10), types (6)                                                                    |
| `nano-ros-c`          | 64         | Executor (22), Guard condition (18), Lifecycle (15), CDR (5), Platform (4)                                       |
| `zenoh-pico-shim`     | 2          | Error display, error code conversion                                                                             |
| `zenoh-pico-shim-sys` | 1          | Constants validation                                                                                             |

### Integration Test Coverage by Crate

Tests in separate integration test binaries (`tests/` directories within each crate):

| Crate/Binary                            | Test Count | Coverage Areas                                               |
|-----------------------------------------|------------|--------------------------------------------------------------|
| `nano-ros-transport::rtic_integration`  | 7          | RTIC transport config, QoS, session mode, topic/service info |
| `nano-ros-transport::zenoh_integration` | 13 (8 run) | Session open/close, CDR format, topic key generation         |
| `nano-ros-node::rtic_integration`       | 6          | RTIC node/context integration                                |
| `zenoh-pico-shim::integration`          | 13         | Session, pub/sub, liveliness, publisher limits               |

### Test Utility Coverage

Tests in `nano-ros-tests/src/` (library unit tests for test infrastructure):

| Module       | Test Count | Coverage Areas                                          |
|--------------|------------|---------------------------------------------------------|
| `zephyr`     | 5          | Workspace/TAP/west detection, platform board specs      |
| `tests`      | 4          | assert_output_contains, count_pattern, project_root     |
| `ros2`       | 3          | ROS 2/rmw_zenoh detection, env setup                    |
| `fixtures`   | 3          | Project root, ephemeral port allocation, router locator |
| `qemu`       | 2          | Test result parsing                                     |
| `process`    | 1          | zenohd detection                                        |

## Platform × Language Test Matrix

All 31 examples organized by platform and language, with their test coverage status.

### Native (Linux/macOS)

| Example | Lang | Feature | Test Coverage | Status |
|---------|------|---------|---------------|--------|
| `native/rs-talker` | Rust | Pub/Sub | nano2nano, rmw_interop, multi_node, qos, error_handling, executor, params | Complete |
| `native/rs-listener` | Rust | Pub/Sub | nano2nano, rmw_interop, multi_node, qos | Complete |
| `native/rs-service-server` | Rust | Service | services, rmw_interop, zephyr (cross-platform) | Complete |
| `native/rs-service-client` | Rust | Service | services, rmw_interop, zephyr (cross-platform) | Complete |
| `native/rs-action-server` | Rust | Action | actions, rmw_interop | Complete |
| `native/rs-action-client` | Rust | Action | actions, rmw_interop | Complete |
| `native/rs-custom-msg` | Rust | Custom Msg | custom_msg (serialization, pub/sub, structure) | Complete |
| `native/c-talker` | C | Pub/Sub | c_api.rs (build, start, comms) | Complete |
| `native/c-listener` | C | Pub/Sub | c_api.rs (build, start, comms) | Complete |
| `native/c-custom-msg` | C | Custom Msg | c-msg-gen-tests.sh (build, generate, run) | Complete |
| `native/c-baremetal-demo` | C | no_std demo | **NONE** | **Missing** |

### QEMU ARM (Cortex-M3)

| Example | Lang | Feature | Test Coverage | Status |
|---------|------|---------|---------------|--------|
| `qemu/rs-test` | Rust | Unit tests | emulator.rs (CDR, Node API, type metadata) | Complete |
| `qemu/rs-talker` | Rust | Pub/Sub | emulator.rs (build), Docker E2E | Complete |
| `qemu/rs-listener` | Rust | Pub/Sub | emulator.rs (build), Docker E2E | Complete |
| `qemu/bsp-talker` | Rust | BSP Pub/Sub | emulator.rs (build, start), Docker E2E | Complete |
| `qemu/bsp-listener` | Rust | BSP Pub/Sub | emulator.rs (build, start), Docker E2E | Complete |

No C examples exist for QEMU.

### Zephyr RTOS (native_sim)

| Example | Lang | Feature | Test Coverage | Status |
|---------|------|---------|---------------|--------|
| `zephyr/rs-talker` | Rust | Pub/Sub | zephyr.rs (build, smoke, E2E, cross-platform) | Complete |
| `zephyr/rs-listener` | Rust | Pub/Sub | zephyr.rs (build, smoke, E2E, cross-platform) | Complete |
| `zephyr/rs-service-server` | Rust | Service | zephyr.rs (build, smoke, cross-platform) | Complete |
| `zephyr/rs-service-client` | Rust | Service | zephyr.rs (build, smoke, cross-platform) | Complete |
| `zephyr/rs-action-server` | Rust | Action | zephyr.rs (build, smoke, E2E) | Complete |
| `zephyr/rs-action-client` | Rust | Action | zephyr.rs (build, smoke, E2E) | Complete |
| `zephyr/c-talker` | C | Pub/Sub | tests/zephyr/run-c.sh | Complete |
| `zephyr/c-listener` | C | Pub/Sub | tests/zephyr/run-c.sh | Complete |

### STM32F4 (Physical Hardware)

| Example | Lang | Feature | Test Coverage | Status |
|---------|------|---------|---------------|--------|
| `stm32f4/bsp-talker` | Rust | BSP Pub/Sub | **NONE** (requires physical board) | **Not testable** |

### Platform Integration (Reference Implementations)

| Example | Lang | Feature | Test Coverage | Status |
|---------|------|---------|---------------|--------|
| `qemu-lan9118` | Rust | Ethernet driver | emulator.rs (LAN9118 test) | Complete |
| `qemu-smoltcp-bridge` | Rust | Network bridge | Library only (no binary) | N/A |
| `stm32f4-rtic` | Rust | RTIC framework | Build check only | **E2E missing** |
| `stm32f4-embassy` | Rust | Embassy async | Build check only | **E2E missing** |
| `stm32f4-polling` | Rust | Polling network | Build check only | **E2E missing** |
| `stm32f4-smoltcp` | Rust | smoltcp network | Build check only | **E2E missing** |

### Summary by Platform × Language

| Platform | Rust | C | Total |
|----------|------|---|-------|
| Native | 7/7 | 2/3 | 9/10 |
| QEMU | 5/5 | — | 5/5 |
| Zephyr | 6/6 | 2/2 | 8/8 |
| STM32F4 | 0/1 | — | 0/1 |
| Platform-Integration | 2/6 | — | 2/6 |
| **Total** | **20/25** | **4/5** | **24/30** |

(Excludes `qemu-smoltcp-bridge` which is a library, not a runnable example.)

## Remaining Gaps

### Actionable (can be automated)

**1. Native C baremetal demo** — `native/c-baremetal-demo`
- Runs without zenoh (standalone no_std demo)
- Add to `c_api.rs`: build + run, assert exit code 0

### Not automatable

**2. STM32F4 hardware** — `stm32f4/bsp-talker`, `stm32f4-*`
- Requires physical board + debug probe
- Build verification possible; runtime tests require HIL setup

## Feature Coverage Summary

| Area                   | Tests | Status                |
|------------------------|-------|-----------------------|
| **Pub/Sub**            | 5     | Complete              |
| **Services**           | 14    | Complete              |
| **Actions**            | 4     | Complete              |
| **Custom Messages**    | 7     | Complete              |
| **ROS 2 Interop**      | 22    | Complete              |
| **Parameters**         | 7     | Complete              |
| **Timer/Executor**     | 7     | Complete              |
| **QoS**                | 6     | Complete              |
| **Error Handling**     | 8     | Complete              |
| **Multi-Node**         | 8     | Complete              |
| **Zephyr**             | 20    | Complete              |
| **QEMU/Emulator**      | 15    | Complete              |
| **Platform Detection** | 10    | Complete              |
| **Serial Transport**   | 9     | Complete (manual E2E) |
| **C Bindings**         | 6     | Partial (1 untested)  |
| **STM32F4 HIL**        | 0     | Not automatable       |

## Test Execution Quick Reference

```bash
# All tests (nextest + Miri + QEMU semihosting + C)
just test-all

# By platform
just test-rust-nano2nano      # Native
just test-rust-zephyr         # Zephyr
just test-rust-emulator       # QEMU ARM
just test-c                   # C bindings

# By feature
just test-rust-services       # Services
just test-rust-actions        # Actions
just test-rust-params         # Parameters
just test-rust-executor       # Timer/Executor
just test-rust-qos            # QoS policies
just test-rust-custom-msg     # Custom messages
just test-rust-rmw-interop    # ROS 2 interop
just test-rust-multi-node     # Multi-node scalability
just test-rust-errors         # Error handling

# Quality gates
just quality                  # Format + clippy + unit tests + Miri + QEMU examples
```

## Requirements Summary

| Test Suite    | Requirements                                          |
|---------------|-------------------------------------------------------|
| Native        | zenohd (`just build-zenohd`)                          |
| Serial E2E    | socat, serial-enabled zenohd (`just build-zenohd`)    |
| Zephyr        | west workspace, TAP network, zenohd                   |
| QEMU          | qemu-system-arm, thumbv7m-none-eabi                   |
| ROS 2 Interop | ROS 2 Humble, rmw_zenoh_cpp                           |
| C             | cmake, C compiler                                     |
| STM32F4 HIL   | Physical board, debug probe                           |

## Known Issues Found by Tests

### Concurrent Service Clients (FIXED)

**Test:** `test_service_server_multiple_clients`

Previously, when two clients sent requests simultaneously to the same server, one client's
request would time out because the C shim used a single `g_stored_query` slot shared by
all queryables. The second query would overwrite the first before the executor could reply.

**Fix:** The C shim now uses a per-queryable stored query array (`g_stored_queries[]`),
so each queryable independently stores its pending query. `zenoh_shim_query_reply()` takes
a `queryable_handle` parameter to reply to the correct stored query. The `z_get()` reply
handler also uses a stack-allocated context instead of global state, making concurrent
service client calls safe.

### Zephyr spin_once Timeout (FIXED)

**Test:** `test_bidirectional_native_zephyr_e2e`, `test_zephyr_to_native_e2e`

Previously, `zenoh_shim_spin_once()` ignored the timeout parameter, causing Zephyr processes
to block for the socket-level timeout (~10s on native_sim) instead of the requested duration.

**Fix:** `zenoh_shim_poll()` now uses `zsock_poll()` with the caller-supplied timeout before
calling `zp_read()`, so `spin_once(KTimeout::secs(1))` correctly returns after ~1 second.

### Cross-Platform Service Key Expression Mismatch (FIXED)

Zephyr examples now use the high-level nano-ros API with ROS 2-compatible key expressions
(e.g., `node.create_service::<AddTwoInts>("/add_two_ints")`), matching native examples.

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
| Emulator   | `emulator.rs` | 12    | CDR, Node API, type metadata, BSP |

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
| `qemu/rs-talker`    | Partial | Build only         |
| `qemu/rs-listener`  | Partial | Build only         |
| `qemu/bsp-talker`   | Yes     | Build + Docker E2E |
| `qemu/bsp-listener` | Yes     | Build + Docker E2E |

### 4. STM32F4 - Physical Hardware

| Test Suite | Tests | Coverage           |
|------------|-------|--------------------|
| None       | 0     | No automated tests |

**Examples (Untested):**
| Example                                | Status      |
|----------------------------------------|-------------|
| `stm32f4/`                             | Manual only |
| `platform-integration/stm32f4-polling` | Manual only |
| `platform-integration/stm32f4-rtic`    | Manual only |
| `platform-integration/stm32f4-embassy` | Manual only |
| `platform-integration/stm32f4-smoltcp` | Manual only |

### 5. C Bindings

| Test Suite    | File                       | Tests | Coverage           |
|---------------|----------------------------|-------|--------------------|
| C Integration | `c_api.rs`                 | 5     | Build, start, comms|
| C Codegen     | `tests/c-msg-gen-tests.sh` | 1     | Message generation |

**Justfile Recipes:**
- `just test-c` - C API integration
- `just test-c-verbose` - Verbose output
- `just test-c-codegen` - Message generation
- `just test-c-msg-gen` - Shell-based codegen test

**Examples Covered:**
| Example                   | Tested | Notes         |
|---------------------------|--------|---------------|
| `native/c-talker`         | Yes    | c_api.rs      |
| `native/c-listener`       | Yes    | c_api.rs      |
| `native/c-custom-msg`     | **NO** | Missing tests |
| `native/c-baremetal-demo` | **NO** | Missing tests |

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

## Missing Tests (Recommended)

### Completed

All previously identified high and medium priority test suites have been implemented:

- **Services** (14 tests): Native `services.rs` (8) + Zephyr `zephyr.rs` (6)
- **Native ↔ Zephyr E2E** (4 tests): `test_native_to_zephyr_e2e`, `test_bidirectional_native_zephyr_e2e`, cross-platform services
- **Custom Messages** (7 tests): `custom_msg.rs` - serialization, pub/sub, error handling
- **Parameters** (7 tests): `params.rs` - declaration, ROS 2 interop
- **Timer/Executor** (7 tests): `executor.rs` - timer firing, callback order, spin behavior
- **QoS** (6 tests): `qos.rs` - RELIABLE delivery, history ordering, multi-subscriber
- **Error Handling** (8 tests): `error_handling.rs` - timeouts, disconnect, reconnect
- **Multi-Node** (8 tests): `multi_node.rs` - scalability, ordering, sustained communication

### Remaining Gaps

#### Low Priority

**1. STM32F4 Hardware-in-Loop Tests**
```
tests/stm32f4_hil.rs (NEW)
- test_stm32f4_build
- test_stm32f4_flash
- test_stm32f4_communication (requires probe)
```
Physical hardware has no automated testing.

**2. C Example Tests**
```
- native/c-custom-msg      (untested)
- native/c-baremetal-demo   (untested)
```
Only c-talker/c-listener are tested via `c_api.rs`.

**3. QEMU BSP Communication Tests**
```
emulator.rs (ADD)
- test_qemu_bsp_talker_listener_e2e
```
QEMU BSP examples build but aren't tested for E2E communication without Docker.

## Test Coverage Gaps Summary

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
| **QEMU/Emulator**      | 12    | Complete              |
| **Platform Detection** | 10    | Complete              |
| **Serial Transport**   | 9     | Complete (manual E2E) |
| **C Bindings**         | 6     | Partial               |
| **STM32F4 HIL**        | 0     | Not applicable        |

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

# nano-ros Test Coverage Analysis

This document provides a comprehensive overview of test coverage across all platforms and identifies gaps where additional tests are needed.

## Test Infrastructure Summary

| Component         | Location                       | Description                              |
|-------------------|--------------------------------|------------------------------------------|
| Integration Tests | `crates/nano-ros-tests/tests/` | Rust-based integration tests with rstest |
| Unit Tests        | `crates/*/src/*.rs`            | Inline `#[test]` modules in each crate   |
| Shell Scripts     | `tests/*.sh`                   | Legacy/supplementary test scripts        |
| Test Utilities    | `crates/nano-ros-tests/src/`   | Fixtures, process management, helpers    |

**Total Test Functions:** ~364 across all crates

## Current Test Coverage by Platform

### 1. Native (Linux/macOS) - POSIX Backend

| Test Suite         | File             | Tests | Coverage                            |
|--------------------|------------------|-------|-------------------------------------|
| Pub/Sub            | `nano2nano.rs`   | 9     | Basic talker/listener communication |
| Services           | `services.rs`    | 8     | Service server/client (AddTwoInts)  |
| Actions            | `actions.rs`     | 7     | Action server/client (Fibonacci)    |
| ROS 2 Interop      | `rmw_interop.rs` | 19    | rmw_zenoh_cpp compatibility         |
| Platform Detection | `platform.rs`    | 10    | Tool/environment detection          |

**Justfile Recipes:**
- `just test-rust-nano2nano` - Native pub/sub tests
- `just test-rust-services` - Native service tests
- `just test-rust-actions` - Action communication tests
- `just test-rust-rmw-interop` - ROS 2 interoperability
- `just test-rust-platform` - Platform detection

**Examples Covered:**
| Example                    | Tested  | Notes                  |
|----------------------------|---------|------------------------|
| `native/rs-talker`         | Yes     | nano2nano, rmw_interop |
| `native/rs-listener`       | Yes     | nano2nano, rmw_interop |
| `native/rs-service-server` | Yes     | services.rs            |
| `native/rs-service-client` | Yes     | services.rs            |
| `native/rs-action-server`  | Yes     | actions.rs             |
| `native/rs-action-client`  | Yes     | actions.rs             |
| `native/rs-custom-msg`     | **NO**  | Missing tests          |
| `native/c-talker`          | Partial | c-tests.sh only        |
| `native/c-listener`        | Partial | c-tests.sh only        |
| `native/cpp-*`             | **NO**  | Missing tests          |

### 2. Zephyr RTOS - native_sim

| Test Suite         | File        | Tests | Coverage                |
|--------------------|-------------|-------|-------------------------|
| Zephyr Integration | `zephyr.rs` | 12    | Build, smoke, E2E tests |

**Test Breakdown:**
| Test                                 | Type      | Description              |
|--------------------------------------|-----------|--------------------------|
| `test_zephyr_availability_checks`    | Detection | Verify workspace/network |
| `test_zephyr_talker_build`           | Build     | Build rs-talker          |
| `test_zephyr_listener_build`         | Build     | Build rs-listener        |
| `test_zephyr_talker_smoke`           | Smoke     | Boot without crash       |
| `test_zephyr_listener_smoke`         | Smoke     | Boot without crash       |
| `test_zephyr_talker_to_listener_e2e` | E2E       | Zephyr ↔ Zephyr          |
| `test_zephyr_to_native_e2e`          | E2E       | Zephyr → Native          |
| `test_zephyr_action_server_build`    | Build     | Build action server      |
| `test_zephyr_action_client_build`    | Build     | Build action client      |
| `test_zephyr_action_server_smoke`    | Smoke     | Boot without crash       |
| `test_zephyr_action_client_smoke`    | Smoke     | Boot without crash       |
| `test_zephyr_action_e2e`             | E2E       | Action communication     |

**Justfile Recipes:**
- `just test-rust-zephyr` - All Zephyr tests
- `just test-rust-zephyr-full` - Rebuild + test
- `just test-rust-zephyr-to-native` - Specific E2E test
- `just test-rust-zephyr-actions` - Action tests only
- `just test-zephyr-c` - C examples on Zephyr

**Examples Covered:**
| Example                    | Tested | Notes             |
|----------------------------|--------|-------------------|
| `zephyr/rs-talker`         | Yes    | Build, smoke, E2E |
| `zephyr/rs-listener`       | Yes    | Build, smoke, E2E |
| `zephyr/rs-action-server`  | Yes    | Build, smoke, E2E |
| `zephyr/rs-action-client`  | Yes    | Build, smoke, E2E |
| `zephyr/rs-service-server` | **NO** | Missing tests     |
| `zephyr/rs-service-client` | **NO** | Missing tests     |
| `zephyr/c-talker`          | Yes    | test-zephyr-c     |
| `zephyr/c-listener`        | Yes    | test-zephyr-c     |

### 3. QEMU ARM (Cortex-M3) - Bare Metal

| Test Suite | File          | Tests | Coverage                     |
|------------|---------------|-------|------------------------------|
| Emulator   | `emulator.rs` | 12    | CDR, Node API, type metadata |

**Test Breakdown:**
| Test                           | Description                 |
|--------------------------------|-----------------------------|
| `test_qemu_detection`          | Verify QEMU available       |
| `test_arm_toolchain_detection` | Verify thumbv7m target      |
| `test_qemu_cdr_serialization`  | CDR encode/decode           |
| `test_qemu_node_api`           | Node, publisher, subscriber |
| `test_qemu_type_metadata`      | Type names                  |
| `test_qemu_all_tests_pass`     | Parse test results          |
| `test_qemu_output_format`      | Verify markers              |

**Justfile Recipes:**
- `just test-rust-emulator` - QEMU emulator tests
- `just test-qemu` - Basic + LAN9118 tests
- `just test-qemu-basic` - Semihosting tests
- `just test-qemu-lan9118` - Network tests
- `just test-qemu-zenoh` - Zenoh communication (manual)

**Examples Covered:**
| Example             | Tested  | Notes                |
|---------------------|---------|----------------------|
| `qemu/rs-test`      | Yes     | CDR, Node API        |
| `qemu/rs-talker`    | Partial | Build only           |
| `qemu/rs-listener`  | Partial | Build only           |
| `qemu/bsp-talker`   | **NO**  | BSP variant untested |
| `qemu/bsp-listener` | **NO**  | BSP variant untested |

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

### 5. C/C++ Bindings

| Test Suite    | File                       | Tests | Coverage           |
|---------------|----------------------------|-------|--------------------|
| C Integration | `tests/c-tests.sh`         | 2     | Talker/listener    |
| C Codegen     | `tests/c-msg-gen-tests.sh` | 1     | Message generation |

**Justfile Recipes:**
- `just test-c` - C API integration
- `just test-c-verbose` - Verbose output
- `just test-c-codegen` - Message generation
- `just test-c-msg-gen` - Shell-based codegen test

**Examples Covered:**
| Example                   | Tested | Notes         |
|---------------------------|--------|---------------|
| `native/c-talker`         | Yes    | c-tests.sh    |
| `native/c-listener`       | Yes    | c-tests.sh    |
| `native/c-custom-msg`     | **NO** | Missing tests |
| `native/c-baremetal-demo` | **NO** | Missing tests |
| `native/cpp-talker`       | **NO** | Missing tests |
| `native/cpp-listener`     | **NO** | Missing tests |
| `native/cpp-service-*`    | **NO** | Missing tests |
| `native/cpp-custom-msg`   | **NO** | Missing tests |

## Unit Test Coverage by Crate

| Crate                 | Test Count | Coverage Areas                              |
|-----------------------|------------|---------------------------------------------|
| `nano-ros-core`       | ~40        | Time, Clock, Service, Action, Logger, Error |
| `nano-ros-serdes`     | ~30        | CDR primitives, sequences, compatibility    |
| `nano-ros-transport`  | ~10        | Shim, traits, keyexpr                       |
| `nano-ros-node`       | ~20        | Node, Context, Timer, Executor, RTIC        |
| `nano-ros-params`     | ~15        | Server, types, typed parameters             |
| `nano-ros-c`          | ~10        | CDR, guard condition, platform              |
| `zenoh-pico-shim`     | ~5         | Safe wrapper tests                          |
| `zenoh-pico-shim-sys` | ~5         | FFI, smoltcp platform                       |

## Missing Tests (Recommended)

### High Priority

#### 1. Service Tests (Native + Zephyr)

**Native:** Implemented in `tests/services.rs` (8 tests, all passing)
- `test_service_server_builds`
- `test_service_client_builds`
- `test_service_server_starts`
- `test_service_client_starts_without_server`
- `test_service_client_timeout`
- `test_service_request_response`
- `test_service_multiple_sequential_calls`
- `test_service_server_multiple_clients`

**Zephyr:** Not yet implemented - needs `zephyr/rs-service-server` and `zephyr/rs-service-client` tests.

**Run:** `just test-rust-services`

#### 2. Native → Zephyr E2E Test
```
zephyr.rs (ADD)
- test_native_to_zephyr_e2e  # Native talker → Zephyr listener
```

**Why:** Only Zephyr→Native is tested, not the reverse direction.

#### 3. Custom Message Tests
```
tests/custom_msg.rs (NEW)
- test_custom_msg_serialization
- test_custom_msg_pub_sub
- test_nested_msg_types
- test_array_msg_types
```

**Why:** Custom messages are supported but only manually verified.

#### 4. QEMU BSP Communication Tests
```
emulator.rs (ADD)
- test_qemu_bsp_talker_listener_e2e
- test_qemu_lan9118_communication
```

**Why:** QEMU BSP examples exist but aren't tested for actual communication.

### Medium Priority

#### 5. C++ Integration Tests
```
tests/cpp_integration.rs (NEW)
- test_cpp_talker_starts
- test_cpp_listener_starts
- test_cpp_talker_listener_e2e
- test_cpp_service_communication
```

**Why:** C++ bindings exist but have no automated tests.

#### 6. Parameter Server Tests
```
tests/params.rs (NEW)
- test_param_get_set
- test_param_types
- test_param_callbacks
- test_param_persistence
```

**Why:** Parameter server is implemented but only has unit tests.

#### 7. Timer and Executor Tests
```
tests/executor.rs (NEW)
- test_timer_callback
- test_executor_spin_once
- test_executor_spin_some
- test_multi_callback_execution
```

**Why:** Timer/executor are core features without integration tests.

#### 8. QoS Tests
```
tests/qos.rs (NEW)
- test_qos_reliable
- test_qos_best_effort
- test_qos_transient_local
- test_qos_deadline
```

**Why:** QoS is partially implemented but not tested.

### Low Priority

#### 9. Error Handling Tests
```
tests/error_handling.rs (NEW)
- test_connection_timeout
- test_invalid_topic
- test_serialization_error_recovery
- test_network_disconnect_reconnect
```

**Why:** Error paths are implemented but not systematically tested.

#### 10. Multi-Node Tests
```
tests/multi_node.rs (NEW)
- test_multiple_publishers_single_topic
- test_multiple_subscribers_single_topic
- test_many_to_many_communication
- test_node_discovery
```

**Why:** Multi-node scenarios aren't systematically tested.

#### 11. STM32F4 Hardware-in-Loop Tests
```
tests/stm32f4_hil.rs (NEW)
- test_stm32f4_build
- test_stm32f4_flash
- test_stm32f4_communication (requires probe)
```

**Why:** Physical hardware has no automated testing.

#### 12. Platform Integration Tests
```
tests/platform_integration.rs (NEW)
- test_qemu_smoltcp_bridge
- test_embedded_cpp_examples
```

**Why:** Platform integration examples exist but aren't tested.

## Test Coverage Gaps Summary

| Area | Current | Missing | Priority |
|------|---------|---------|----------|
| **Services** | 8 tests (native) | Zephyr, ROS 2 interop | High |
| **Native→Zephyr** | 0 tests | E2E test | High |
| **Custom Messages** | 0 tests | Serialization, pub/sub | High |
| **QEMU Communication** | 0 tests | BSP E2E | High |
| **C++ Bindings** | 0 tests | Full suite | Medium |
| **Parameters** | Unit only | Integration | Medium |
| **Timer/Executor** | Unit only | Integration | Medium |
| **QoS** | 0 tests | All policies | Medium |
| **Error Handling** | Sparse | Systematic | Low |
| **Multi-Node** | Sparse | Comprehensive | Low |
| **STM32F4 HIL** | 0 tests | Full suite | Low |

## Test Execution Quick Reference

```bash
# All tests
just test-rust

# By platform
just test-rust-nano2nano      # Native
just test-rust-zephyr         # Zephyr
just test-rust-emulator       # QEMU ARM
just test-c                   # C bindings

# By feature
just test-rust-services       # Services
just test-rust-actions        # Actions
just test-rust-rmw-interop    # ROS 2 interop

# Quality gates
just quality                  # Format + clippy + unit tests
just ci                       # Full CI pipeline
```

## Requirements Summary

| Test Suite    | Requirements                        |
|---------------|-------------------------------------|
| Native        | zenohd                              |
| Zephyr        | west workspace, TAP network, zenohd |
| QEMU          | qemu-system-arm, thumbv7m-none-eabi |
| ROS 2 Interop | ROS 2 Humble, rmw_zenoh_cpp         |
| C/C++         | cmake, C/C++ compiler               |
| STM32F4 HIL   | Physical board, debug probe         |

## Known Issues Found by Tests

### Concurrent Service Clients

**Test:** `test_service_server_multiple_clients`
**Status:** Partial pass - one client succeeds, one fails

**Description:** When two service clients connect simultaneously to the same server:
- Client 1: ConnectionFailed (0 responses)
- Client 2: All 4 responses received successfully

**Symptoms:**
```
Client 1 responses: 0, Client 2 responses: 4
[PARTIAL] At least one client got responses
```

**Impact:** Sequential client connections work correctly. Concurrent connections may fail for one client.

**Possible Causes:**
1. zenoh-pico limitation in handling multiple simultaneous service subscribers
2. Race condition in service request routing
3. Session/subscriber conflict during concurrent connection

**Workaround:** Ensure service clients connect sequentially rather than simultaneously.

**Tracking:** Investigate in Phase 16 (ROS 2 Interoperability)

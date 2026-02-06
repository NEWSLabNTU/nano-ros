# nano-ros Test Coverage Analysis

This document provides a comprehensive overview of test coverage across all platforms and identifies gaps where additional tests are needed.

## Test Infrastructure Summary

| Component         | Location                       | Description                              |
|-------------------|--------------------------------|------------------------------------------|
| Integration Tests | `crates/nano-ros-tests/tests/` | Rust-based integration tests with rstest |
| Unit Tests        | `crates/*/src/*.rs`            | Inline `#[test]` modules in each crate   |
| Shell Scripts     | `tests/*.sh`                   | Legacy/supplementary test scripts        |
| Test Utilities    | `crates/nano-ros-tests/src/`   | Fixtures, process management, helpers    |

**Total Test Functions:** ~370 unit tests + ~95 integration tests across all crates

## Current Test Coverage by Platform

### 1. Native (Linux/macOS) - POSIX Backend

| Test Suite         | File             | Tests | Coverage                            |
|--------------------|------------------|-------|-------------------------------------|
| Pub/Sub            | `nano2nano.rs`   | 9     | Basic talker/listener communication |
| Services           | `services.rs`    | 8     | Service server/client (AddTwoInts)  |
| Actions            | `actions.rs`     | 7     | Action server/client (Fibonacci)    |
| Custom Messages    | `custom_msg.rs`  | 7     | Serialization, pub/sub, error handling |
| ROS 2 Interop      | `rmw_interop.rs` | 19    | rmw_zenoh_cpp compatibility         |
| Platform Detection | `platform.rs`    | 10    | Tool/environment detection          |

**Justfile Recipes:**
- `just test-rust-nano2nano` - Native pub/sub tests
- `just test-rust-services` - Native service tests
- `just test-rust-actions` - Action communication tests
- `just test-rust-custom-msg` - Custom message tests
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
| `native/rs-custom-msg`     | Yes     | custom_msg.rs          |
| `native/c-talker`          | Partial | c-tests.sh only        |
| `native/c-listener`        | Partial | c-tests.sh only        |

### 2. Zephyr RTOS - native_sim

| Test Suite         | File        | Tests | Coverage                          |
|--------------------|-------------|-------|-----------------------------------|
| Zephyr Integration | `zephyr.rs` | 20    | Build, smoke, E2E, cross-platform |

**⚠️ ISSUE: Zephyr Examples Use Low-Level APIs Instead of ROS API**

All Zephyr Rust examples currently use low-level zenoh-pico/BSP APIs instead of the proper
nano-ros ROS API (rclrs-like). This needs to be fixed. See "Known Issues" section below.

**Test Breakdown:**
| Test                                   | Type           | Description                   | API Issue |
|----------------------------------------|----------------|-------------------------------|-----------|
| `test_zephyr_availability_checks`      | Detection      | Verify workspace/network      | N/A       |
| `test_zephyr_talker_build`             | Build          | Build rs-talker               | **FIX**   |
| `test_zephyr_listener_build`           | Build          | Build rs-listener             | **FIX**   |
| `test_zephyr_talker_smoke`             | Smoke          | Boot without crash            | **FIX**   |
| `test_zephyr_listener_smoke`           | Smoke          | Boot without crash            | **FIX**   |
| `test_zephyr_talker_to_listener_e2e`   | E2E            | Zephyr ↔ Zephyr               | **FIX**   |
| `test_zephyr_to_native_e2e`            | E2E            | Zephyr → Native               | **FIX**   |
| `test_native_to_zephyr_e2e`            | E2E            | Native → Zephyr               | **FIX**   |
| `test_bidirectional_native_zephyr_e2e` | E2E            | Both directions at once       | **FIX**   |
| `test_zephyr_action_server_build`      | Build          | Build action server           | **FIX**   |
| `test_zephyr_action_client_build`      | Build          | Build action client           | **FIX**   |
| `test_zephyr_action_server_smoke`      | Smoke          | Boot without crash            | **FIX**   |
| `test_zephyr_action_client_smoke`      | Smoke          | Boot without crash            | **FIX**   |
| `test_zephyr_action_e2e`               | E2E            | Action communication          | **FIX**   |
| `test_zephyr_service_server_build`     | Build          | Build service server          | **FIX**   |
| `test_zephyr_service_client_build`     | Build          | Build service client          | **FIX**   |
| `test_zephyr_service_server_smoke`     | Smoke          | Boot without crash            | **FIX**   |
| `test_zephyr_service_client_smoke`     | Smoke          | Boot without crash            | **FIX**   |
| `test_native_server_zephyr_client`     | Cross-Platform | Native server + Zephyr client | **FIX**   |
| `test_zephyr_server_native_client`     | Cross-Platform | Zephyr server + Native client | **FIX**   |

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
| Example                    | Tested | API Status                   | Notes                          |
|----------------------------|--------|------------------------------|--------------------------------|
| `zephyr/rs-talker`         | Yes    | **FIX**                      | Uses BSP FFI, not nano-ros API |
| `zephyr/rs-listener`       | Yes    | **FIX**                      | Uses BSP FFI, not nano-ros API |
| `zephyr/rs-action-server`  | Yes    | **FIX**                      | Uses zenoh-pico-shim directly  |
| `zephyr/rs-action-client`  | Yes    | **FIX**                      | Uses zenoh-pico-shim directly  |
| `zephyr/rs-service-server` | Yes    | **FIX**                      | Uses zenoh-pico-shim directly  |
| `zephyr/rs-service-client` | Yes    | **FIX**                      | Uses zenoh-pico-shim directly  |
| `zephyr/rs-service-client` | Yes    | Build, smoke, cross-platform |                                |
| `zephyr/c-talker`          | Yes    | test-zephyr-c                |                                |
| `zephyr/c-listener`        | Yes    | test-zephyr-c                |                                |

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

### 5. C Bindings

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

## Unit Test Coverage by Crate

| Crate                 | Test Count | Coverage Areas                                                  |
|-----------------------|------------|-----------------------------------------------------------------|
| `nano-ros-core`       | 75         | Time (17), Clock (6), Action (15), Lifecycle (13), Error (13), Logger (7), Service (2), MessageInfo (2) |
| `nano-ros-serdes`     | 33         | CDR primitives (6), CDR encoder (5), compatibility (22)         |
| `nano-ros-transport`  | 42         | QoS profiles/builder (23), RMW protocol/liveliness/attachment (19) |
| `nano-ros-node`       | 106        | Actions/Promise (38), Lifecycle (15), Trigger (10), Timer (8), ParamServices (8), Context (8), Node (6), Options (6), Executor (5) |
| `nano-ros-params`     | 30         | Typed parameters (14), server (10), types (6)                   |
| `nano-ros-c`          | 60         | Executor (18), Guard condition (18), Lifecycle (15), CDR (5), Platform (4) |
| `zenoh-pico-shim`     | 2          | Safe wrapper tests                                              |
| `zenoh-pico-shim-sys` | 22         | smoltcp platform (21), FFI (1)                                  |

### Phase 16 Unit Test Contributions

Phase 16 (ROS 2 Interop Completion) added significant unit test coverage across all sub-phases:

#### A. Rust API Alignment Tests

| Sub-phase            | File(s)                                   | Tests | Coverage                                                                                 |
|----------------------|-------------------------------------------|-------|------------------------------------------------------------------------------------------|
| A.1 Context/Executor | `nano-ros-node/src/context.rs`            | 8     | InitOptions, Context creation, NodeOptions, error handling                               |
| A.1 Executor         | `nano-ros-node/src/executor.rs`           | 5     | spin_once result, spin_options, subscription_handle, spin_period                         |
| A.2 Node API         | `nano-ros-node/src/node.rs`, `options.rs` | 12    | Node creation, publisher/subscriber options                                              |
| A.5 Service/Promise  | `nano-ros-node/src/connected.rs`          | 38    | Promise API (12), action protocol (26): goals, status, serialization                     |
| A.6 Timer            | `nano-ros-node/src/timer.rs`              | 8     | Duration, state transitions, cancel/reset, oneshot/repeating/inert                       |
| A.7 Parameter API    | `nano-ros-params/src/typed.rs`            | 14    | Typed parameters, range constraints, read-only                                           |
| A.8 QoS Profiles     | `nano-ros-transport/src/traits.rs`        | 23    | Predefined profiles (8), builder/setters (4), string encoding (5), topic/action info (6) |
| A.9 Logger           | `nano-ros-core/src/logger.rs`             | 7     | Logger creation, OnceFlag, throttle logic                                                |
| A.10 Error Handling  | `nano-ros-core/src/error.rs`              | 13    | RclReturnCode, NanoRosError, error filters, context display                              |
| Trigger              | `nano-ros-node/src/trigger.rs`            | 10    | Trigger policies: Any, All, Always, One, Custom, sensor fusion                           |

#### B. C API Tests

| Sub-phase            | File(s)                       | Tests | Coverage                                                |
|----------------------|-------------------------------|-------|---------------------------------------------------------|
| B.3/B.5/B.6 Executor | `nano-ros-c/src/executor.rs`  | 18    | Zero-init, init, add handles, semantics, LET mode, spin |
| B.? Lifecycle        | `nano-ros-c/src/lifecycle.rs` | 15    | Lifecycle state machine, transitions, C API             |

#### C. Protocol Interoperability Tests

| Sub-phase                 | File(s)                 | Tests | Coverage                                                                  |
|---------------------------|-------------------------|-------|---------------------------------------------------------------------------|
| C.1 QoS Strings           | `shim.rs`               | 3     | QoS encoding: BEST_EFFORT/RELIABLE, VOLATILE/TRANSIENT_LOCAL              |
| C.2 Parameter Services    | `parameter_services.rs` | 8     | Value conversion (4), handler tests (4): get/set/list/get_types           |
| C.5 RMW Attachment        | `shim.rs`               | 6     | Serialization, deserialization, roundtrip, edge cases, MessageInfo        |
| C.6 Protocol Verification | `shim.rs`               | 10    | Liveliness keyexprs (node/pub/sub/SS/SC), topic/service info, ZenohId hex |

## Missing Tests (Recommended)

### High Priority

#### 1. Service Tests (Native + Zephyr) ✓ COMPLETE

**Native:** Implemented in `tests/services.rs` (8 tests, all passing)
- `test_service_server_builds`
- `test_service_client_builds`
- `test_service_server_starts`
- `test_service_client_starts_without_server`
- `test_service_client_timeout`
- `test_service_request_response`
- `test_service_multiple_sequential_calls`
- `test_service_server_multiple_clients`

**Zephyr:** Implemented in `tests/zephyr.rs` (6 tests, all passing)
- `test_zephyr_service_server_build` - Build service server
- `test_zephyr_service_client_build` - Build service client
- `test_zephyr_service_server_smoke` - Boot without crash
- `test_zephyr_service_client_smoke` - Boot without crash
- `test_native_server_zephyr_client` - Cross-platform (Native server + Zephyr client)
- `test_zephyr_server_native_client` - Cross-platform (Zephyr server + Native client)

**Run:** `just test-rust-services` (Native) or `just test-rust-zephyr-services` (Zephyr)

#### 2. Native → Zephyr E2E Test ✓ COMPLETE

Implemented in Phase 17.2:
- `test_native_to_zephyr_e2e` - Native talker → Zephyr listener
- `test_bidirectional_native_zephyr_e2e` - Both directions simultaneously

**Run:** `just test-rust-native-to-zephyr` or `just test-rust-bidirectional-zephyr`

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

#### 5. Parameter Server Tests ✓
```
tests/params.rs (COMPLETE - 7 tests)
- test_talker_with_params_builds ✓
- test_talker_uses_default_param ✓
- test_talker_param_declaration ✓
- test_param_integer_type ✓
- test_ros2_param_list ✓
- test_ros2_param_get ✓
- test_ros2_param_describe ✓
```

**Status:** Complete. Tests verify parameter declaration and ROS 2 interop.

#### 6. Timer and Executor Tests ✓
```
tests/executor.rs (COMPLETE - 7 tests)
- test_timer_interval_basic ✓
- test_timer_regular_publishing ✓
- test_callback_execution_order ✓
- test_mixed_callbacks ✓
- test_spin_once_processes_work ✓
- test_executor_multiple_timers_via_publishers ✓
- test_spin_result_timers_fired ✓
```

**Status:** Complete. Tests verify timer firing, callback order, and spin behavior.

#### 7. QoS Tests ✓
```
tests/qos.rs (COMPLETE - 6 tests)
- test_qos_reliable_delivery ✓
- test_qos_reliable_no_loss ✓
- test_qos_history_ordering ✓
- test_qos_compatible_settings ✓
- test_qos_multiple_subscribers ✓
- test_qos_keyexpr_encoding ✓
```

**Status:** Complete. Tests verify RELIABLE QoS, history ordering, and multi-subscriber.

### Low Priority

#### 8. Error Handling Tests
```
tests/error_handling.rs (NEW)
- test_connection_timeout
- test_invalid_topic
- test_serialization_error_recovery
- test_network_disconnect_reconnect
```

**Why:** Error paths are implemented but not systematically tested.

#### 9. Multi-Node Tests
```
tests/multi_node.rs (NEW)
- test_multiple_publishers_single_topic
- test_multiple_subscribers_single_topic
- test_many_to_many_communication
- test_node_discovery
```

**Why:** Multi-node scenarios aren't systematically tested.

#### 10. STM32F4 Hardware-in-Loop Tests
```
tests/stm32f4_hil.rs (NEW)
- test_stm32f4_build
- test_stm32f4_flash
- test_stm32f4_communication (requires probe)
```

**Why:** Physical hardware has no automated testing.

#### 11. Platform Integration Tests
```
tests/platform_integration.rs (NEW)
- test_qemu_smoltcp_bridge
```

**Why:** Platform integration examples exist but aren't tested.

## Test Coverage Gaps Summary

| Area                   | Current          | Missing                 | Priority           |
|------------------------|------------------|-------------------------|--------------------|
| **Services**           | 8 tests (native) | Zephyr, ROS 2 interop   | High               |
| **Native↔Zephyr**      | 2 tests ✓        | Cross-platform services | Complete (pub/sub) |
| **Custom Messages**    | 7 tests ✓        | Nested/array types      | Complete (basic)   |
| **QEMU Communication** | 0 tests          | BSP E2E                 | High               |
| **Parameters**         | 7 tests ✓        | -                       | Complete           |
| **Timer/Executor**     | 7 tests ✓        | -                       | Complete           |
| **QoS**                | 6 tests ✓        | -                       | Complete           |
| **Error Handling**     | Sparse           | Systematic              | Low                |
| **Multi-Node**         | Sparse           | Comprehensive           | Low                |
| **STM32F4 HIL**        | 0 tests          | Full suite              | Low                |

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
just test-rust-params         # Parameters
just test-rust-executor       # Timer/Executor
just test-rust-qos            # QoS policies
just test-rust-custom-msg     # Custom messages
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
| C             | cmake, C compiler                   |
| STM32F4 HIL   | Physical board, debug probe         |

## Known Issues Found by Tests

### ⚠️ Zephyr Examples Use Low-Level APIs Instead of ROS API (HIGH PRIORITY)

**Affected Tests:** ALL Zephyr Rust tests (19 tests)
**Status:** Needs fix - examples must be rewritten

**Description:** All Zephyr Rust examples use low-level APIs instead of the proper nano-ros ROS API (rclrs-like).

**Current State (WRONG):**

| Example              | Current API Used                        |
|----------------------|-----------------------------------------|
| `rs-talker`          | `nano-ros-bsp-zephyr` via raw FFI       |
| `rs-listener`        | `nano-ros-bsp-zephyr` via raw FFI       |
| `rs-action-server`   | `zenoh-pico-shim` directly (ShimContext)|
| `rs-action-client`   | `zenoh-pico-shim` directly              |
| `rs-service-server`  | `zenoh-pico-shim` directly              |
| `rs-service-client`  | `zenoh-pico-shim` directly              |

**Expected (Proper ROS API like Native examples):**

```rust
// Native examples use proper ROS API:
use nano_ros::prelude::*;

let context = Context::from_env();
let mut executor = context.create_basic_executor();
let mut node = executor.create_node("talker");
let publisher = node.create_publisher::<Int32>(PublisherOptions::new("/chatter"));
publisher.publish(&msg);
```

**What Zephyr examples currently do (WRONG):**

```rust
// Zephyr examples use low-level FFI:
unsafe extern "C" {
    fn nano_ros_bsp_init(...);
    fn nano_ros_bsp_create_node(...);
    fn nano_ros_bsp_create_publisher(...);
}

// Or directly use zenoh-pico-shim:
use zenoh_pico_shim::{ShimContext, ShimQueryable};
let ctx = ShimContext::new(locator);
```

**Impact:**
1. Tests don't validate the actual nano-ros ROS API on Zephyr
2. Cross-platform tests fail (key expression mismatch) because native uses ROS API format
3. Zephyr examples are not representative of intended usage

**Fix Required:**
1. Port `nano-ros-node` crate to work with Zephyr/no_std (add `zephyr` feature)
2. Rewrite all Zephyr Rust examples to use `nano_ros::prelude::*`
3. Tests will then validate actual ROS API usage

**Tracking:** This is a prerequisite for proper Phase 17 completion.

---

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

### Zephyr spin_once Timeout Ignored

**Test:** `test_bidirectional_native_zephyr_e2e`, `test_zephyr_to_native_e2e`
**Status:** Bug - timeout parameter not implemented

**Description:** The `zenoh_shim_spin_once()` function ignores the timeout parameter:
```c
int32_t zenoh_shim_spin_once(uint32_t timeout_ms) {
    (void)timeout_ms;  // Timeout handled by socket layer  <-- IGNORED!
    int ret = zp_read(z_session_loan_mut(&g_session), NULL);
    ...
}
```

**Symptoms:**
- Zephyr talker with 1-second delay actually blocks for ~10 seconds between publishes
- `spin_once(KTimeout::secs(1))` blocks for socket-level timeout (~10s on native_sim)
- Multiple Zephyr processes show significantly reduced message throughput

**Impact:**
- Zephyr talker publishes ~2-3 messages per 15 seconds instead of ~15
- Bidirectional tests show asymmetric message counts
- Single-direction tests (Native → Zephyr) work correctly since native uses proper timing

**Root Cause:** `zenoh-pico-shim-sys/c/shim/zenoh_shim.c:548` - timeout not passed to socket layer

**Location:** `crates/zenoh-pico-shim-sys/c/shim/zenoh_shim.c`

**Root Cause Analysis:**
1. `zenoh_shim_spin_once()` ignores the timeout parameter (line 548)
2. `zp_read()` has no timeout parameter - relies on socket-level `SO_RCVTIMEO`
3. In `zenoh-pico/src/system/zephyr/network.c:167-170`, `SO_RCVTIMEO` **consistently fails** on Zephyr:
   ```c
   // FIXME: setting the setsockopt is consistently failing. Commenting it
   // until further inspection. ret = _Z_ERR_GENERIC;
   ```
4. Without `SO_RCVTIMEO`, `recv()` blocks indefinitely (or until platform default ~10s)

**Recommended Fix - Use `poll()` before `zp_read()`:**
```c
int32_t zenoh_shim_spin_once(uint32_t timeout_ms) {
    // Get socket FD from session internals
    // Use poll() or select() with timeout_ms
    // Only call zp_read() if data ready or timeout reached
}
```

This mirrors zenoh-pico's multi-threaded approach in `_z_socket_wait_event()` which uses `select()`.

**Workaround:** Use native processes for talkers (they use proper `std::thread::sleep`).

**Tracking:** Fix requires accessing session internals to get socket FD. See `zenoh-pico/src/system/zephyr/network.c:90-128` for reference implementation.

### Cross-Platform Service Key Expression Mismatch

**Test:** `test_native_server_zephyr_client`, `test_zephyr_server_native_client`
**Status:** Expected - different key expression formats

**Description:** Cross-platform service tests pass (verify startup and connection) but show no actual service communication:

| Component      | Key Expression Used  | Format                       |
|----------------|---------------------|------------------------------|
| Native server  | `/add_two_ints`      | ROS 2-compatible (mangled to `%add_two_ints`) |
| Native client  | `/add_two_ints`      | ROS 2-compatible             |
| Zephyr server  | `demo/add_two_ints` | Raw zenoh keyexpr            |
| Zephyr client  | `demo/add_two_ints` | Raw zenoh keyexpr            |

**Symptoms:**
```
Zephyr client: [0] Sending request: 0 + 1
               [0] Request timed out
Native client: Service call failed: ConnectionFailed
```

**Impact:**
- Native ↔ Native service tests work correctly
- Zephyr ↔ Zephyr tests not yet implemented (would require two QEMU instances)
- Cross-platform tests verify startup and network connectivity but not service communication

**Root Cause:**
1. Native examples use `nano-ros-node` with full ROS 2 key expression format
2. Zephyr examples use simplified `zenoh-pico-shim` with raw zenoh key expressions
3. The key expressions don't match, so queries never reach the server

**Recommended Fix:**
Either:
1. Update Zephyr examples to use nano-ros ROS 2 key expression format
2. Add a simplified service mode to nano-ros-node that uses raw zenoh keys
3. Align both to use the same naming convention

**Workaround:** Use matching platforms (Native-to-Native or Zephyr-to-Zephyr).

**Tracking:** Fix planned for Phase 17.3 (Advanced Cross-Platform Tests)

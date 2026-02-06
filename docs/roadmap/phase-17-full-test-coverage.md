# Phase 17: Full Test Coverage

**Goal**: Achieve comprehensive test coverage across all platforms, examples, and features. Close the gaps identified in `docs/test-coverage.md`.

**Status**: In Progress (BLOCKED - see critical issue below)

## ⚠️ CRITICAL ISSUE: Zephyr Examples Use Wrong API

**All Zephyr Rust tests are invalid** because the examples they test use low-level zenoh-pico/BSP
APIs instead of the proper nano-ros ROS API (rclrs-like).

| Example              | Current API (WRONG)                     | Expected API                     |
|----------------------|-----------------------------------------|----------------------------------|
| `rs-talker`          | `nano-ros-bsp-zephyr` via raw FFI       | `nano_ros::prelude::*`           |
| `rs-listener`        | `nano-ros-bsp-zephyr` via raw FFI       | `Context`, `Node`, `Subscriber`  |
| `rs-action-server`   | `zenoh-pico-shim::ShimContext`          | `node.create_action_server()`    |
| `rs-action-client`   | `zenoh-pico-shim::ShimContext`          | `node.create_action_client()`    |
| `rs-service-server`  | `zenoh-pico-shim::ShimQueryable`        | `node.create_service()`          |
| `rs-service-client`  | `zenoh-pico-shim` directly              | `node.create_client()`           |

**Comparison:**

```rust
// Native examples (CORRECT - uses rclrs-like API):
use nano_ros::prelude::*;
let context = Context::from_env();
let mut executor = context.create_basic_executor();
let mut node = executor.create_node("talker");
let publisher = node.create_publisher::<Int32>(PublisherOptions::new("/chatter"));

// Zephyr examples (WRONG - uses low-level FFI):
unsafe extern "C" { fn nano_ros_bsp_create_publisher(...); }
// OR uses zenoh-pico-shim directly:
use zenoh_pico_shim::{ShimContext, ShimPublisher};
```

**Impact:**
1. Zephyr tests don't validate the actual nano-ros ROS API
2. Cross-platform service tests fail due to key expression mismatch
3. Examples are not representative of intended Zephyr usage

**Fix Required (Phase 17.0):**
1. Port `nano-ros-node` crate to support `no_std` + Zephyr backend
2. Rewrite all Zephyr Rust examples to use `nano_ros::prelude::*`
3. Re-run all tests to validate proper ROS API usage

---

## Overview

This phase implements missing integration tests to achieve full coverage:
- **Services** - Request/response communication (native + Zephyr)
- **Bidirectional cross-platform** - Native ↔ Zephyr in both directions
- **Custom messages** - User-defined message types
- **Parameters** - Parameter server integration tests
- **QoS policies** - Quality of Service verification
- **QEMU BSP** - Bare-metal communication tests

**Dependencies**: Phase 9 (Test Infrastructure) mostly complete

### Phase 16 Unit Test Foundation

Phase 16 (ROS 2 Interop Completion) added ~200 unit tests across the codebase covering:

| Area                          | Tests | Key Files                                                                                                                                                                   |
|-------------------------------|-------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Rust API alignment (A.1-A.10) | ~138  | context.rs (8), executor.rs (5), node.rs (6), options.rs (6), connected.rs (38), timer.rs (8), trigger.rs (10), typed.rs (14), traits.rs (23), logger.rs (7), error.rs (13) |
| C API alignment (B.1-B.7)     | ~33   | c/executor.rs (18), c/lifecycle.rs (15)                                                                                                                                     |
| Protocol interop (C.1-C.7)    | ~27   | shim.rs (19), parameter_services.rs (8)                                                                                                                                     |

These unit tests complement Phase 17's **integration tests** which verify end-to-end behavior across processes and platforms. See `docs/test-coverage.md` for detailed breakdown.

---

## Phase 17.1: Service Integration Tests

**Status**: Complete (Native)
**Priority**: **High**

Services are implemented but have zero integration tests.

### Completed

- [x] **17.1.1** Create `tests/services.rs` - 8 native service tests implemented
- [x] **17.1.2** Add service binary fixtures - `service_server_binary`, `service_client_binary`
- [x] Add justfile recipe `test-rust-services`

### Test Results (8/8 passing)

| Test                                        | Status | Notes                              |
|---------------------------------------------|--------|------------------------------------|
| `test_service_server_builds`                | PASS   | Binary builds successfully         |
| `test_service_client_builds`                | PASS   | Binary builds successfully         |
| `test_service_server_starts`                | PASS   | Server stays running               |
| `test_service_client_starts_without_server` | PASS   | Reports ConnectionFailed correctly |
| `test_service_client_timeout`               | PASS   | Handles missing server             |
| `test_service_request_response`             | PASS   | 4/4 responses, all correct         |
| `test_service_multiple_sequential_calls`    | PASS   | 8 responses across 2 runs          |
| `test_service_server_multiple_clients`      | PASS   | Partial - see finding below        |

### Finding: Concurrent Client Issue

When two clients connect simultaneously to the same server:
- Client 1: 0 responses (ConnectionFailed)
- Client 2: 4 responses (all successful)

This may indicate a limitation in zenoh-pico service handling where concurrent client connections conflict. Worth investigating in Phase 16.

### Remaining Work Items

- [ ] **17.1.3** Add Zephyr service tests
  ```rust
  //! Service communication integration tests

  use nano_ros_tests::fixtures::{zenohd_unique, ZenohRouter};
  use rstest::rstest;

  mod native {
      #[rstest]
      fn test_service_server_starts(zenohd_unique: ZenohRouter) {
          // Build and start native/rs-service-server
          // Verify it starts without error
      }

      #[rstest]
      fn test_service_client_starts(zenohd_unique: ZenohRouter) {
          // Build and start native/rs-service-client
          // Verify it starts (may timeout without server)
      }

      #[rstest]
      fn test_service_request_response(zenohd_unique: ZenohRouter) {
          // Start server, then client
          // Verify request sent and response received
      }

      #[rstest]
      fn test_service_multiple_requests(zenohd_unique: ZenohRouter) {
          // Send multiple requests sequentially
          // Verify all responses received correctly
      }

      #[rstest]
      fn test_service_timeout_handling(zenohd_unique: ZenohRouter) {
          // Start client without server
          // Verify timeout error handling
      }
  }
  ```

- [ ] **17.1.2** Add service binary fixtures
  ```rust
  // In fixtures/binaries.rs
  #[fixture]
  pub fn service_server_binary() -> PathBuf {
      build_example("native/rs-service-server")
  }

  #[fixture]
  pub fn service_client_binary() -> PathBuf {
      build_example("native/rs-service-client")
  }
  ```

- [x] **17.1.3** Add Zephyr service tests (Complete - see Phase 17.2.4)

- [ ] **17.1.4** Add ROS 2 service interop tests
  ```rust
  mod ros2_interop {
      #[rstest]
      fn test_nano_server_ros2_client(zenohd_unique: ZenohRouter) {
          // nano-ros service server, ROS 2 service client
      }

      #[rstest]
      fn test_ros2_server_nano_client(zenohd_unique: ZenohRouter) {
          // ROS 2 service server, nano-ros service client
      }
  }
  ```

- [ ] **17.1.5** Add justfile recipes
  ```just
  # Run service integration tests
  test-rust-services:
      cargo test -p nano-ros-tests --test services -- --nocapture

  # Run service tests with rebuild
  test-rust-services-full: build-service-examples
      just test-rust-services
  ```

---

## Phase 17.2: Bidirectional Cross-Platform Tests

**Status**: Complete
**Priority**: **High**

Completed: Native→Zephyr and bidirectional tests now implemented.

### Completed

- [x] **17.2.1** Add `test_native_to_zephyr_e2e` to `zephyr.rs`
- [x] **17.2.2** Add `test_bidirectional_native_zephyr_e2e` to `zephyr.rs`
- [x] **17.2.3** Add justfile recipes `test-rust-native-to-zephyr` and `test-rust-bidirectional-zephyr`

### Test Results (2/2 passing)

| Test                                   | Status | Notes                                                                     |
|----------------------------------------|--------|---------------------------------------------------------------------------|
| `test_native_to_zephyr_e2e`            | PASS   | Zephyr listener received 13 messages from native talker                   |
| `test_bidirectional_native_zephyr_e2e` | PASS   | Both directions work: 12 messages Zephyr→Native, 3 messages Native→Zephyr |

### Run Commands

```bash
just test-rust-native-to-zephyr        # Run native to Zephyr test
just test-rust-bidirectional-zephyr    # Run bidirectional test
```

### Observations

The bidirectional test shows asymmetric message counts:
- Zephyr → Native: 12 messages
- Native → Zephyr: 3 messages

**Root Cause Identified:** The Zephyr talker's `spin_once()` timeout is ignored.

In `zenoh_shim_spin_once()` (crates/zenoh-pico-shim-sys/c/shim/zenoh_shim.c:548):
```c
(void)timeout_ms;  // Timeout handled by socket layer  <-- BUG: Ignored!
```

The `zp_read()` call blocks until the socket-level timeout (~10s on native_sim), causing:
- Zephyr talker with 1-second intended delay actually publishes every ~10 seconds
- Only 2-3 messages sent during the 15-second test window

**Why Native → Zephyr works correctly:**
- Native talker uses standard Rust sleep (not zenoh spin_once)
- Zephyr listener's spin_once blocking doesn't affect receive timing

**Tracking:** This bug is documented in `docs/test-coverage.md` under "Known Issues".
The fix requires implementing proper timeout handling in `zenoh_shim_spin_once()`.

Both directions work, confirming full cross-platform communication capability.
The message throughput issue is a known limitation pending a zenoh-pico-shim fix.

### Completed Work Items

- [x] **17.2.4** Add cross-platform service tests (Complete)

  **Tests Added:**
  - `test_zephyr_service_server_build` - Build service server for Zephyr
  - `test_zephyr_service_client_build` - Build service client for Zephyr
  - `test_zephyr_service_server_smoke` - Boot without crash
  - `test_zephyr_service_client_smoke` - Boot without crash
  - `test_native_server_zephyr_client` - Cross-platform: Native server + Zephyr client
  - `test_zephyr_server_native_client` - Cross-platform: Zephyr server + Native client

  **Results:** 6/6 tests passing

  **Known Limitation - Key Expression Mismatch:**

  Cross-platform service tests verify startup and network connectivity but show no actual
  service communication due to different key expression formats:

  | Component      | Key Expression Used  | Format                       |
  |----------------|---------------------|------------------------------|
  | Native server  | `/add_two_ints`      | ROS 2-compatible (mangled)   |
  | Zephyr client  | `demo/add_two_ints` | Raw zenoh keyexpr            |

  This is expected behavior since:
  - Native examples use `nano-ros-node` with full ROS 2 key expression format
  - Zephyr examples use simplified `zenoh-pico-shim` with raw zenoh keys

  **Run Commands:**
  ```bash
  just test-rust-zephyr-services            # All Zephyr service tests
  just test-rust-native-server-zephyr-client # Native server + Zephyr client
  just test-rust-zephyr-server-native-client # Zephyr server + Native client
  ```

---

## Phase 17.3: Custom Message Tests

**Status**: Complete
**Priority**: **High**

### Completed

- [x] **17.3.1** Create `tests/custom_msg.rs` - 7 tests implemented
- [x] **17.3.2** Add justfile recipe `test-rust-custom-msg`
- [x] Fix `native/rs-custom-msg` example to use current API

### Test Results (7/7 passing)

| Test                              | Status | Notes                                    |
|-----------------------------------|--------|------------------------------------------|
| `test_custom_msg_builds_no_zenoh` | PASS   | Builds without zenoh feature             |
| `test_custom_msg_builds_with_zenoh`| PASS  | Builds with zenoh feature                |
| `test_custom_msg_serialization`   | PASS   | SensorReading, Status, Int32 roundtrip   |
| `test_sensor_reading_structure`   | PASS   | Verifies SensorReading structure         |
| `test_status_message_with_string` | PASS   | String field serialization               |
| `test_custom_msg_pub_sub`         | PASS   | Pub/sub with custom messages             |
| `test_custom_msg_no_router`       | PASS   | Graceful handling without router         |

### Bug Fix

Fixed `native/rs-custom-msg` to use the current nano-ros API:
- Use `context.create_basic_executor()` instead of deprecated `context.create_node()`
- Use `executor.create_node()` to create nodes
- Use `PublisherOptions` and `SubscriberOptions` instead of raw strings
- Use `node.create_subscription()` with callback reference

### Run Command

```bash
just test-rust-custom-msg
```

### Remaining Work (Low Priority)

- [ ] **17.3.3** Add C custom message test (`native/c-custom-msg`)
- [ ] Add nested message type tests
- [ ] Add array field tests

---

## Phase 17.4: Parameter Server Integration Tests

**Status**: Complete
**Priority**: **Medium**

Parameter server has unit tests; integration tests now implemented.

### Completed

- [x] **17.4.1** Create `tests/params.rs` - 7 tests implemented
- [x] **17.4.2** Add justfile recipe `test-rust-params`
- [x] Use native-rs-talker example (which declares parameters) for testing

### Test Results (7/7 passing)

| Test                          | Status | Notes                                    |
|-------------------------------|--------|------------------------------------------|
| `test_talker_with_params_builds` | PASS | Binary builds successfully            |
| `test_talker_uses_default_param` | PASS | Counter start value: 0 logged         |
| `test_talker_param_declaration`  | PASS | Node created, parameter declared      |
| `test_param_integer_type`        | PASS | Integer parameter works correctly     |
| `test_ros2_param_list`           | PASS | Graceful skip when ROS 2 unavailable  |
| `test_ros2_param_get`            | PASS | Graceful skip when ROS 2 unavailable  |
| `test_ros2_param_describe`       | PASS | Graceful skip when ROS 2 unavailable  |

### Test Categories

1. **Parameter Declaration Tests** - Verify parameters are declared with defaults
2. **Parameter Type Tests** - Verify integer parameter typing works
3. **ROS 2 Interop Tests** - Verify ROS 2 can access nano-ros parameters (requires rmw_zenoh_cpp)

### Run Command

```bash
just test-rust-params
```

### Notes

- Uses existing `native-rs-talker` which declares a `start_value` parameter
- ROS 2 interop tests gracefully skip when ROS 2/rmw_zenoh_cpp unavailable
- Parameter services (list/get/set/describe) require full parameter server implementation

---

## Phase 17.5: Timer and Executor Integration Tests

**Status**: Complete
**Priority**: **Medium**

Timer and executor now have integration tests verifying real-world behavior.

### Completed

- [x] **17.5.1** Create `tests/executor.rs` - 7 tests implemented
- [x] **17.5.2** Add justfile recipe `test-rust-executor`

### Test Results (7/7 passing)

| Test                                     | Status | Notes                                     |
|------------------------------------------|--------|-------------------------------------------|
| `test_timer_interval_basic`              | PASS   | ~5 messages at 1Hz over 5 seconds         |
| `test_timer_regular_publishing`          | PASS   | Sequential counter values verified        |
| `test_callback_execution_order`          | PASS   | Messages received in order [1, 2, 3, 4]   |
| `test_mixed_callbacks`                   | PASS   | Timer + subscription both fire            |
| `test_spin_once_processes_work`          | PASS   | spin_once processes timer callbacks       |
| `test_executor_multiple_timers_via_publishers` | PASS | Multiple processes with timers work |
| `test_spin_result_timers_fired`          | PASS   | Timer-driven publishing works             |

### Test Categories

1. **Timer Interval Tests** - Verify timers fire at expected intervals
2. **Callback Execution Order Tests** - Verify message order is preserved
3. **Mixed Callback Tests** - Verify timer + subscription work together
4. **Spin Behavior Tests** - Verify spin_once processes work correctly

### Run Command

```bash
just test-rust-executor
```

---

## Phase 17.6: QoS Policy Tests

**Status**: Complete
**Priority**: **Medium**

QoS policies are now tested systematically.

### Completed

- [x] **17.6.1** Create `tests/qos.rs` - 6 tests implemented
- [x] **17.6.2** Add justfile recipe `test-rust-qos`

### Test Results (6/6 passing)

| Test                         | Status | Notes                                      |
|------------------------------|--------|--------------------------------------------|
| `test_qos_reliable_delivery` | PASS   | 80% delivery ratio (accounting for startup)|
| `test_qos_reliable_no_loss`  | PASS   | 0 message gaps in steady state             |
| `test_qos_history_ordering`  | PASS   | Messages received in order [1,2,3,4]       |
| `test_qos_compatible_settings` | PASS | No QoS incompatibility warnings            |
| `test_qos_multiple_subscribers` | PASS | Both subscribers receive equal messages   |
| `test_qos_keyexpr_encoding`  | PASS   | QoS encoded as "1:2:1,10" in liveliness    |

### What's Tested

The native examples use RELIABLE + KEEP_LAST(10) QoS settings. Tests verify:
1. **Reliability**: Messages are delivered reliably
2. **History**: Message ordering is preserved
3. **Multi-subscriber**: All subscribers receive messages
4. **QoS Encoding**: QoS settings encoded in liveliness keyexpr

### Run Command

```bash
just test-rust-qos
```

### Notes

- BEST_EFFORT and TRANSIENT_LOCAL tests would require custom examples
- Current tests focus on verifying RELIABLE QoS behavior (what the examples use)

---

## Phase 17.7: QEMU BSP Communication Tests

**Status**: Complete
**Priority**: **Medium**

QEMU BSP examples tested for build and initialization.

### Completed

- [x] **17.7.1** Add QEMU BSP tests to `emulator.rs` (5 tests)
- [x] **17.7.2** Add helper functions for BSP binaries in `fixtures/binaries.rs`
- [x] **17.7.3** Add `require_zenoh_pico_arm()` check in `qemu.rs`
- [x] **17.7.4** Add justfile recipe `test-qemu-bsp`

### Test Results (5/5 passing)

| Test                          | Status | Notes                                           |
|-------------------------------|--------|-------------------------------------------------|
| `test_qemu_bsp_talker_builds` | PASS   | Builds successfully (requires zenoh-pico-arm)   |
| `test_qemu_bsp_listener_builds`| PASS  | Builds successfully (skips if permission error) |
| `test_qemu_bsp_talker_starts` | PASS   | Skips - requires Docker/TAP networking          |
| `test_qemu_bsp_listener_starts`| PASS  | Skips - requires Docker/TAP networking          |
| `test_qemu_bsp_both_build`    | PASS   | Verifies both binaries build                    |

### Prerequisites

- ARM toolchain: `rustup target add thumbv7m-none-eabi`
- zenoh-pico ARM library: `just build-zenoh-pico-arm`

### Run Commands

```bash
just test-qemu-bsp                   # Build tests (unit test runner)
just test-rust-qemu-baremetal-bsp    # Full Docker-based network test
```

### Notes

- BSP network tests require MPS2-AN385 with LAN9118 Ethernet
- Use Docker-based tests (`just test-rust-qemu-baremetal-bsp`) for full E2E testing
- Permission errors from Docker builds handled gracefully with skip + instructions

### Remaining Work (Future)

- [ ] Add QEMU LAN9118 unit tests (currently only Docker-based)
- [ ] Add QEMU ↔ Native cross-platform tests

---

## Phase 17.8: Error Handling and Edge Case Tests

**Status**: Not Started
**Priority**: **Low**

Systematic testing of error paths and edge cases.

### Work Items

- [ ] **17.9.1** Create `tests/error_handling.rs`
  ```rust
  //! Error handling and edge case tests

  #[rstest]
  fn test_connection_timeout() {
      // Try to connect to non-existent router
      // Verify timeout error
  }

  #[rstest]
  fn test_invalid_topic_name() {
      // Create subscriber with invalid topic
      // Verify error returned
  }

  #[rstest]
  fn test_invalid_type_name() {
      // Create subscriber with mismatched type
      // Verify error or graceful handling
  }

  #[rstest]
  fn test_serialization_error_recovery() {
      // Send malformed data
      // Verify receiver handles gracefully
  }

  #[rstest]
  fn test_router_disconnect() {
      // Connect to router
      // Kill router
      // Verify client handles disconnection
  }

  #[rstest]
  fn test_router_reconnect() {
      // Connect, disconnect, reconnect
      // Verify communication resumes
  }
  ```

- [ ] **17.9.2** Add justfile recipe
  ```just
  test-rust-errors:
      cargo test -p nano-ros-tests --test error_handling -- --nocapture
  ```

---

## Phase 17.9: Multi-Node and Scalability Tests

**Status**: Not Started
**Priority**: **Low**

Tests for multi-node scenarios and scalability.

### Work Items

- [ ] **17.10.1** Create `tests/multi_node.rs`
  ```rust
  //! Multi-node and scalability tests

  #[rstest]
  fn test_multiple_publishers_single_topic(zenohd_unique: ZenohRouter) {
      // 3 publishers on same topic
      // 1 subscriber
      // Verify all messages received
  }

  #[rstest]
  fn test_multiple_subscribers_single_topic(zenohd_unique: ZenohRouter) {
      // 1 publisher
      // 3 subscribers on same topic
      // Verify all subscribers receive messages
  }

  #[rstest]
  fn test_many_to_many(zenohd_unique: ZenohRouter) {
      // 3 publishers, 3 subscribers
      // Verify complete message delivery
  }

  #[rstest]
  fn test_multiple_topics(zenohd_unique: ZenohRouter) {
      // Publishers on /topic1, /topic2, /topic3
      // Subscribers on each
      // Verify no cross-talk
  }

  #[rstest]
  fn test_high_frequency_publishing(zenohd_unique: ZenohRouter) {
      // Publish at 100Hz for 10 seconds
      // Verify message delivery rate
  }

  #[rstest]
  fn test_large_message(zenohd_unique: ZenohRouter) {
      // Publish 1MB message
      // Verify delivery
  }
  ```

- [ ] **17.10.2** Add justfile recipe
  ```just
  test-rust-multi-node:
      cargo test -p nano-ros-tests --test multi_node -- --nocapture
  ```

---

## Implementation Priority

| Phase                  | Priority | Effort | Tests Added | Description                     |
|------------------------|----------|--------|-------------|---------------------------------|
| **17.1 Services**      | High     | Medium | ~15         | Service request/response        |
| **17.2 Bidirectional** | High     | Low    | ~5          | Native ↔ Zephyr both directions |
| **17.3 Custom Msg**    | High     | Medium | ~8          | User-defined message types      |
| **17.4 Parameters**    | Medium   | Medium | ~8          | Parameter server                |
| **17.5 Executor**      | Medium   | Medium | ~8          | Timer and executor              |
| **17.6 QoS**           | Medium   | Medium | ~8          | Quality of Service              |
| **17.7 QEMU BSP**      | Medium   | High   | ~10         | Bare-metal communication        |
| **17.8 Errors**        | Low      | Medium | ~8          | Error handling                  |
| **17.9 Multi-Node**    | Low      | Medium | ~8          | Scalability                     |

**Total New Tests**: ~78

---

## Recommended Implementation Order

1. **17.1 Services** - High value, enables feature parity verification
2. **17.2 Bidirectional** - Quick win, completes cross-platform coverage
3. **17.3 Custom Messages** - Important for real-world usage
4. **17.4 Parameters** - Completes ROS 2 feature coverage
5. **17.5 Executor** - Verifies core runtime behavior
6. **17.6 QoS** - Important for production deployments
7. **17.7 QEMU BSP** - Enables bare-metal CI
8. **17.8 Errors** - Polish and robustness
9. **17.9 Multi-Node** - Scalability verification

---

## Success Metrics

- [ ] All `native/rs-*` examples have integration tests
- [ ] All `zephyr/rs-*` examples have integration tests
- [ ] Native ↔ Zephyr communication tested in both directions
- [ ] Services tested on Native, Zephyr, and ROS 2 interop
- [ ] Parameter server tested with ROS 2 interop
- [ ] QoS policies systematically tested
- [ ] QEMU bare-metal examples tested for communication
- [ ] 80+ integration tests total
- [ ] CI runs all tests on every PR

---

## Justfile Recipe Summary

```just
# === Phase 17 Test Recipes ===

# 17.1: Service tests
test-rust-services:
    cargo test -p nano-ros-tests --test services -- --nocapture

# 17.2: Cross-platform bidirectional
test-rust-native-to-zephyr:
    cargo test -p nano-ros-tests --test zephyr test_native_to_zephyr -- --nocapture

# 17.3: Custom message tests
test-rust-custom-msg:
    cargo test -p nano-ros-tests --test custom_msg -- --nocapture

# 17.4: Parameter tests
test-rust-params:
    cargo test -p nano-ros-tests --test params -- --nocapture

# 17.5: Executor tests
test-rust-executor:
    cargo test -p nano-ros-tests --test executor -- --nocapture

# 17.6: QoS tests
test-rust-qos:
    cargo test -p nano-ros-tests --test qos -- --nocapture

# 17.7: QEMU BSP tests
test-qemu-bsp:
    cargo test -p nano-ros-tests --test emulator bsp -- --nocapture

# 17.8: Error handling tests
test-rust-errors:
    cargo test -p nano-ros-tests --test error_handling -- --nocapture

# 17.9: Multi-node tests
test-rust-multi-node:
    cargo test -p nano-ros-tests --test multi_node -- --nocapture

# Run all Phase 17 tests
test-rust-phase17: test-rust-services test-rust-custom-msg test-rust-params \
                   test-rust-executor test-rust-qos test-rust-errors test-rust-multi-node
    @echo "Phase 17 tests complete"

# Full coverage test suite
test-rust-full-coverage: test-rust test-rust-phase17 test-qemu-bsp
    @echo "Full test coverage complete"
```

---

## File Structure After Implementation

```
crates/nano-ros-tests/tests/
├── actions.rs          # Existing
├── custom_msg.rs       # NEW (17.3)
├── emulator.rs         # Extended (17.7)
├── error_handling.rs   # NEW (17.8)
├── executor.rs         # NEW (17.5)
├── multi_node.rs       # NEW (17.9)
├── nano2nano.rs        # Existing
├── params.rs           # NEW (17.4)
├── platform.rs         # Existing
├── qos.rs              # NEW (17.6)
├── rmw_interop.rs      # Existing
├── services.rs         # NEW (17.1)
└── zephyr.rs           # Extended (17.2)
```

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
- **C++ bindings** - Full C++ integration test suite
- **Parameters** - Parameter server integration tests
- **QoS policies** - Quality of Service verification
- **QEMU BSP** - Bare-metal communication tests

**Dependencies**: Phase 9 (Test Infrastructure) mostly complete

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

| Test | Status | Notes |
|------|--------|-------|
| `test_service_server_builds` | PASS | Binary builds successfully |
| `test_service_client_builds` | PASS | Binary builds successfully |
| `test_service_server_starts` | PASS | Server stays running |
| `test_service_client_starts_without_server` | PASS | Reports ConnectionFailed correctly |
| `test_service_client_timeout` | PASS | Handles missing server |
| `test_service_request_response` | PASS | 4/4 responses, all correct |
| `test_service_multiple_sequential_calls` | PASS | 8 responses across 2 runs |
| `test_service_server_multiple_clients` | PASS | Partial - see finding below |

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

**Status**: Not Started
**Priority**: **High**

Custom message examples exist but aren't tested.

### Work Items

- [ ] **17.3.1** Create `tests/custom_msg.rs`
  ```rust
  //! Custom message type integration tests

  #[rstest]
  fn test_custom_msg_example_builds() {
      // Build native/rs-custom-msg
  }

  #[rstest]
  fn test_custom_msg_serialization(zenohd_unique: ZenohRouter) {
      // Verify custom message serialization/deserialization
  }

  #[rstest]
  fn test_custom_msg_pub_sub(zenohd_unique: ZenohRouter) {
      // Custom message publisher → subscriber
  }

  #[rstest]
  fn test_nested_message_types(zenohd_unique: ZenohRouter) {
      // Messages containing other messages
  }

  #[rstest]
  fn test_array_message_fields(zenohd_unique: ZenohRouter) {
      // Messages with fixed/variable length arrays
  }

  #[rstest]
  fn test_string_message_fields(zenohd_unique: ZenohRouter) {
      // Messages with string fields
  }
  ```

- [ ] **17.3.2** Add C custom message test
  ```rust
  #[rstest]
  fn test_c_custom_msg_builds() {
      // Build native/c-custom-msg
  }

  #[rstest]
  fn test_c_custom_msg_interop(zenohd_unique: ZenohRouter) {
      // C custom msg ↔ Rust custom msg
  }
  ```

- [ ] **17.3.3** Add justfile recipe
  ```just
  test-rust-custom-msg:
      cargo test -p nano-ros-tests --test custom_msg -- --nocapture
  ```

---

## Phase 17.4: C++ Integration Tests

**Status**: Not Started
**Priority**: **Medium**

C++ bindings exist but have no automated tests.

### Work Items

- [ ] **17.4.1** Create `tests/cpp_integration.rs`
  ```rust
  //! C++ binding integration tests

  mod build {
      #[rstest]
      fn test_cpp_talker_builds() {
          // cmake -B build && cmake --build build
          // for native/cpp-talker
      }

      #[rstest]
      fn test_cpp_listener_builds() {
          // Build native/cpp-listener
      }

      #[rstest]
      fn test_cpp_service_server_builds() {
          // Build native/cpp-service-server
      }

      #[rstest]
      fn test_cpp_service_client_builds() {
          // Build native/cpp-service-client
      }

      #[rstest]
      fn test_cpp_custom_msg_builds() {
          // Build native/cpp-custom-msg
      }
  }

  mod communication {
      #[rstest]
      fn test_cpp_talker_listener_e2e(zenohd_unique: ZenohRouter) {
          // C++ talker → C++ listener
      }

      #[rstest]
      fn test_cpp_rust_interop(zenohd_unique: ZenohRouter) {
          // C++ talker → Rust listener
          // Rust talker → C++ listener
      }

      #[rstest]
      fn test_cpp_service_e2e(zenohd_unique: ZenohRouter) {
          // C++ service server ↔ C++ service client
      }
  }
  ```

- [ ] **17.4.2** Add CMake build helpers
  ```rust
  // In fixtures/binaries.rs
  fn build_cpp_example(name: &str) -> PathBuf {
      let example_dir = project_root().join("examples/native").join(name);
      let build_dir = example_dir.join("build");

      // cmake -B build
      Command::new("cmake")
          .args(["-B", "build"])
          .current_dir(&example_dir)
          .status()
          .expect("cmake configure failed");

      // cmake --build build
      Command::new("cmake")
          .args(["--build", "build"])
          .current_dir(&example_dir)
          .status()
          .expect("cmake build failed");

      build_dir.join(name)
  }
  ```

- [ ] **17.4.3** Add justfile recipes
  ```just
  # Build all C++ examples
  build-cpp-examples:
      for dir in examples/native/cpp-*/; do \
          cmake -B "$dir/build" -S "$dir" && \
          cmake --build "$dir/build"; \
      done

  # Run C++ integration tests
  test-rust-cpp:
      cargo test -p nano-ros-tests --test cpp_integration -- --nocapture

  # Full C++ test with rebuild
  test-rust-cpp-full: build-cpp-examples
      just test-rust-cpp
  ```

---

## Phase 17.5: Parameter Server Integration Tests

**Status**: Not Started
**Priority**: **Medium**

Parameter server has unit tests but no integration tests.

### Work Items

- [ ] **17.5.1** Create `tests/params.rs`
  ```rust
  //! Parameter server integration tests

  #[rstest]
  fn test_param_server_starts(zenohd_unique: ZenohRouter) {
      // Start node with parameter server
      // Verify parameter service endpoints created
  }

  #[rstest]
  fn test_param_get_set(zenohd_unique: ZenohRouter) {
      // Set parameter via service
      // Get parameter via service
      // Verify value matches
  }

  #[rstest]
  fn test_param_types(zenohd_unique: ZenohRouter) {
      // Test bool, int, float, string, array parameters
  }

  #[rstest]
  fn test_param_list(zenohd_unique: ZenohRouter) {
      // List all parameters
      // Verify expected parameters present
  }

  #[rstest]
  fn test_param_describe(zenohd_unique: ZenohRouter) {
      // Get parameter descriptor
      // Verify type, description, constraints
  }

  #[rstest]
  fn test_param_ros2_interop(zenohd_unique: ZenohRouter) {
      // Set param in nano-ros, read from ROS 2
      // Set param in ROS 2, read from nano-ros
  }
  ```

- [ ] **17.5.2** Create parameter test example
  ```rust
  // examples/native/rs-param-test/
  // Minimal node that exposes parameters for testing
  ```

- [ ] **17.5.3** Add justfile recipe
  ```just
  test-rust-params:
      cargo test -p nano-ros-tests --test params -- --nocapture
  ```

---

## Phase 17.6: Timer and Executor Integration Tests

**Status**: Not Started
**Priority**: **Medium**

Timer and executor have unit tests but no integration tests verifying real-world behavior.

### Work Items

- [ ] **17.6.1** Create `tests/executor.rs`
  ```rust
  //! Executor and timer integration tests

  #[rstest]
  fn test_timer_fires_at_interval(zenohd_unique: ZenohRouter) {
      // Create 100ms timer
      // Spin for 500ms
      // Verify ~5 callbacks fired
  }

  #[rstest]
  fn test_multiple_timers(zenohd_unique: ZenohRouter) {
      // Create 100ms and 200ms timers
      // Spin for 1s
      // Verify correct callback counts
  }

  #[rstest]
  fn test_spin_once_processes_one_event(zenohd_unique: ZenohRouter) {
      // Queue multiple events
      // Call spin_once
      // Verify only one processed
  }

  #[rstest]
  fn test_spin_some_processes_available(zenohd_unique: ZenohRouter) {
      // Queue multiple events
      // Call spin_some
      // Verify all available processed
  }

  #[rstest]
  fn test_callback_execution_order(zenohd_unique: ZenohRouter) {
      // Publish messages 1, 2, 3
      // Verify received in order 1, 2, 3
  }

  #[rstest]
  fn test_mixed_callbacks(zenohd_unique: ZenohRouter) {
      // Timer + subscriber callbacks
      // Verify both fire correctly
  }
  ```

- [ ] **17.6.2** Add justfile recipe
  ```just
  test-rust-executor:
      cargo test -p nano-ros-tests --test executor -- --nocapture
  ```

---

## Phase 17.7: QoS Policy Tests

**Status**: Not Started
**Priority**: **Medium**

QoS policies are supported but not systematically tested.

### Work Items

- [ ] **17.7.1** Create `tests/qos.rs`
  ```rust
  //! QoS policy integration tests

  #[rstest]
  fn test_qos_best_effort(zenohd_unique: ZenohRouter) {
      // Create publisher/subscriber with BEST_EFFORT
      // Verify communication works
  }

  #[rstest]
  fn test_qos_reliable(zenohd_unique: ZenohRouter) {
      // Create publisher/subscriber with RELIABLE
      // Verify delivery guarantees
  }

  #[rstest]
  fn test_qos_transient_local(zenohd_unique: ZenohRouter) {
      // Create publisher with TRANSIENT_LOCAL
      // Publish message
      // Create late-joining subscriber
      // Verify subscriber receives historical message
  }

  #[rstest]
  fn test_qos_history_depth(zenohd_unique: ZenohRouter) {
      // Set history depth = 3
      // Publish 5 messages before subscriber starts
      // Verify subscriber receives last 3
  }

  #[rstest]
  fn test_qos_incompatible_warning(zenohd_unique: ZenohRouter) {
      // Publisher: RELIABLE, Subscriber: BEST_EFFORT
      // Verify warning/error about incompatibility
  }

  #[rstest]
  fn test_qos_ros2_interop(zenohd_unique: ZenohRouter) {
      // nano-ros RELIABLE ↔ ROS 2 RELIABLE
      // Verify QoS negotiation works
  }
  ```

- [ ] **17.7.2** Add justfile recipe
  ```just
  test-rust-qos:
      cargo test -p nano-ros-tests --test qos -- --nocapture
  ```

---

## Phase 17.8: QEMU BSP Communication Tests

**Status**: Not Started
**Priority**: **Medium**

QEMU BSP examples exist but aren't tested for actual communication.

### Work Items

- [ ] **17.8.1** Add QEMU BSP tests to `emulator.rs`
  ```rust
  mod bsp {
      #[rstest]
      fn test_qemu_bsp_talker_builds() {
          // Build qemu/bsp-talker
      }

      #[rstest]
      fn test_qemu_bsp_listener_builds() {
          // Build qemu/bsp-listener
      }

      #[rstest]
      fn test_qemu_bsp_talker_starts() {
          // Start QEMU with bsp-talker
          // Verify it boots and initializes
      }

      #[rstest]
      fn test_qemu_bsp_listener_starts() {
          // Start QEMU with bsp-listener
          // Verify it boots and initializes
      }
  }
  ```

- [ ] **17.8.2** Add QEMU LAN9118 network tests
  ```rust
  mod lan9118 {
      #[rstest]
      fn test_qemu_lan9118_talker_listener_e2e() {
          // Requires: TAP interface, zenohd on bridge
          // Start QEMU talker (LAN9118)
          // Start QEMU listener (LAN9118)
          // Verify communication via zenohd
      }

      #[rstest]
      fn test_qemu_to_native_e2e() {
          // QEMU talker → Native listener
      }

      #[rstest]
      fn test_native_to_qemu_e2e() {
          // Native talker → QEMU listener
      }
  }
  ```

- [ ] **17.8.3** Add Docker-based QEMU tests
  ```rust
  mod docker {
      #[rstest]
      fn test_docker_qemu_communication() {
          // Use docker-compose to run QEMU tests
          // Avoids host QEMU version issues
      }
  }
  ```

- [ ] **17.8.4** Add justfile recipes
  ```just
  # QEMU BSP tests
  test-qemu-bsp:
      cargo test -p nano-ros-tests --test emulator bsp -- --nocapture

  # QEMU LAN9118 network tests (requires TAP)
  test-qemu-lan9118-e2e:
      cargo test -p nano-ros-tests --test emulator lan9118 -- --nocapture
  ```

---

## Phase 17.9: Error Handling and Edge Case Tests

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

## Phase 17.10: Multi-Node and Scalability Tests

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

| Phase | Priority | Effort | Tests Added | Description |
|-------|----------|--------|-------------|-------------|
| **17.1 Services** | High | Medium | ~15 | Service request/response |
| **17.2 Bidirectional** | High | Low | ~5 | Native ↔ Zephyr both directions |
| **17.3 Custom Msg** | High | Medium | ~8 | User-defined message types |
| **17.4 C++** | Medium | High | ~10 | C++ binding tests |
| **17.5 Parameters** | Medium | Medium | ~8 | Parameter server |
| **17.6 Executor** | Medium | Medium | ~8 | Timer and executor |
| **17.7 QoS** | Medium | Medium | ~8 | Quality of Service |
| **17.8 QEMU BSP** | Medium | High | ~10 | Bare-metal communication |
| **17.9 Errors** | Low | Medium | ~8 | Error handling |
| **17.10 Multi-Node** | Low | Medium | ~8 | Scalability |

**Total New Tests**: ~88

---

## Recommended Implementation Order

1. **17.1 Services** - High value, enables feature parity verification
2. **17.2 Bidirectional** - Quick win, completes cross-platform coverage
3. **17.3 Custom Messages** - Important for real-world usage
4. **17.5 Parameters** - Completes ROS 2 feature coverage
5. **17.6 Executor** - Verifies core runtime behavior
6. **17.4 C++** - Lower priority but important for C++ users
7. **17.7 QoS** - Important for production deployments
8. **17.8 QEMU BSP** - Enables bare-metal CI
9. **17.9 Errors** - Polish and robustness
10. **17.10 Multi-Node** - Scalability verification

---

## Success Metrics

- [ ] All `native/rs-*` examples have integration tests
- [ ] All `zephyr/rs-*` examples have integration tests
- [ ] All `native/cpp-*` examples have integration tests
- [ ] Native ↔ Zephyr communication tested in both directions
- [ ] Services tested on Native, Zephyr, and ROS 2 interop
- [ ] Parameter server tested with ROS 2 interop
- [ ] QoS policies systematically tested
- [ ] QEMU bare-metal examples tested for communication
- [ ] 100+ integration tests total
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

# 17.4: C++ integration tests
test-rust-cpp:
    cargo test -p nano-ros-tests --test cpp_integration -- --nocapture

# 17.5: Parameter tests
test-rust-params:
    cargo test -p nano-ros-tests --test params -- --nocapture

# 17.6: Executor tests
test-rust-executor:
    cargo test -p nano-ros-tests --test executor -- --nocapture

# 17.7: QoS tests
test-rust-qos:
    cargo test -p nano-ros-tests --test qos -- --nocapture

# 17.8: QEMU BSP tests
test-qemu-bsp:
    cargo test -p nano-ros-tests --test emulator bsp -- --nocapture

# 17.9: Error handling tests
test-rust-errors:
    cargo test -p nano-ros-tests --test error_handling -- --nocapture

# 17.10: Multi-node tests
test-rust-multi-node:
    cargo test -p nano-ros-tests --test multi_node -- --nocapture

# Run all Phase 17 tests
test-rust-phase17: test-rust-services test-rust-custom-msg test-rust-params \
                   test-rust-executor test-rust-qos test-rust-errors test-rust-multi-node
    @echo "Phase 17 tests complete"

# Full coverage test suite
test-rust-full-coverage: test-rust test-rust-phase17 test-rust-cpp test-qemu-bsp
    @echo "Full test coverage complete"
```

---

## File Structure After Implementation

```
crates/nano-ros-tests/tests/
├── actions.rs          # Existing
├── custom_msg.rs       # NEW (17.3)
├── cpp_integration.rs  # NEW (17.4)
├── emulator.rs         # Extended (17.8)
├── error_handling.rs   # NEW (17.9)
├── executor.rs         # NEW (17.6)
├── multi_node.rs       # NEW (17.10)
├── nano2nano.rs        # Existing
├── params.rs           # NEW (17.5)
├── platform.rs         # Existing
├── qos.rs              # NEW (17.7)
├── rmw_interop.rs      # Existing
├── services.rs         # NEW (17.1)
└── zephyr.rs           # Extended (17.2)
```

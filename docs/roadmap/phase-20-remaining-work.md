# Phase 20: Remaining Work (TODO Audit)

## Overview

This phase tracks all remaining TODO items, unimplemented stubs, and future work identified through a codebase-wide audit. Items are grouped by theme and prioritized by impact.

**Status**: Planning

## 1. Async Executor Support

**Blocked by**: zenoh-pico types not implementing `Send`

The `spin_async` method on `SpinExecutor` and `BasicExecutor` is commented out because the underlying zenoh-pico shim types are not `Send`. This prevents running the executor on a background thread from async code.

**Files**:
- `crates/nano-ros-node/src/executor.rs:1031` — `SpinExecutor` trait definition
- `crates/nano-ros-node/src/executor.rs:1604` — `BasicExecutor` implementation

**Work required**:
- Audit zenoh-pico-shim types for thread safety
- Either make shim types `Send` (if safe) or use a channel-based design where the executor runs on a dedicated thread and communicates via message passing
- Un-comment and test `spin_async` behind the `async` feature flag
- Requires the `futures` dependency (already optional in Cargo.toml)

**Impact**: Enables idiomatic async Rust usage — important for Embassy integration and desktop applications using tokio/async-std.

## 2. C API `no_std` Backend

**7 stubs** in the C API return `NANO_ROS_RET_ERROR` when compiled without the `std` feature. The Rust `no_std` API works (via shim transport), but the C FFI layer only wires up the `std` (zenoh) path.

**Files**:
- `crates/nano-ros-c/src/publisher.rs:321` — `nano_ros_publisher_create_default()`
- `crates/nano-ros-c/src/subscription.rs:349` — `nano_ros_subscription_create_default()`
- `crates/nano-ros-c/src/service.rs:273` — `nano_ros_service_create_default()`
- `crates/nano-ros-c/src/service.rs:682` — `nano_ros_service_client_create_default()`
- `crates/nano-ros-c/src/action.rs:389` — `nano_ros_action_server_create()`
- `crates/nano-ros-c/src/action.rs:824` — `nano_ros_action_client_create()`

**Work required**:
- In each `#[cfg(not(feature = "std"))]` block, create the resource using the shim transport backend instead of returning an error
- The shim transport session must be accessible from the C API — likely stored in the executor or passed through the node handle
- Add C-level integration tests for the `no_std` path (Zephyr C examples already exist as a reference)

**Impact**: Enables C applications on bare-metal and Zephyr to use the full nano-ros API. Currently only Rust applications can use the shim transport.

## 3. Parameter Array Types (C API)

**5 enum variants** are declared but documented as "not yet supported" in the C parameter API.

**File**: `crates/nano-ros-c/src/parameter.rs:38-47`

Unsupported types:
- `NANO_ROS_PARAMETER_BYTE_ARRAY` (type 5)
- `NANO_ROS_PARAMETER_BOOL_ARRAY` (type 6)
- `NANO_ROS_PARAMETER_INTEGER_ARRAY` (type 7)
- `NANO_ROS_PARAMETER_DOUBLE_ARRAY` (type 8)
- `NANO_ROS_PARAMETER_STRING_ARRAY` (type 9)

**Work required**:
- Extend `nano_ros_parameter_value_t` union to include array pointer + length fields
- Implement conversion between C array representation and the Rust `ParameterValue` array variants
- Add setter/getter functions for each array type
- Memory management: decide whether C callers own the array memory or if nano-ros copies it

**Impact**: Low — array parameters are uncommon in embedded ROS 2 use cases. Scalar types (bool, int, double, string) cover most needs.

## 4. Embassy Integration

**File**: `examples/platform-integration/stm32f4-embassy/src/main.rs:64`

The Embassy example cannot use the full nano-ros executor because zenoh-pico-shim-sys requires a C cross-compilation toolchain visible to `bindgen` at build time.

**Work required**:
- Document the required toolchain setup (arm-none-eabi-gcc in PATH)
- Provide a pre-generated FFI bindings option to avoid runtime bindgen dependency
- Test the full executor integration on STM32F4 with Embassy

**Impact**: Medium — Embassy is increasingly popular for embedded Rust. A working example would demonstrate nano-ros on a major async embedded framework.

## Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | C API `no_std` backend (#2) | Medium | High — unblocks C on bare-metal |
| 2 | Async executor (#1) | Medium | High — unblocks async Rust |
| 3 | Embassy integration (#4) | Low | Medium — documentation + toolchain |
| 4 | Parameter arrays (#3) | Low | Low — rarely used on embedded |

## Verification

After completing each item:
```bash
just quality               # Core checks
just test-c                # C API tests (items 2, 3)
just test-integration      # Full integration (item 1)
```

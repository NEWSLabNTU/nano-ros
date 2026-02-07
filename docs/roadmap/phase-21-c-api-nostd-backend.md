# Phase 21: C API `no_std` Backend

**Status: IN PROGRESS**

## Summary

The C API crate (`nano-ros-c`) has ~30 functions that return `NANO_ROS_RET_ERROR` when compiled without `std`. The `Zenoh*` type aliases (`ZenohPublisher`, `ZenohSession`, etc.) are re-exports of `Shim*` types from `nano-ros-transport`, which already work in `no_std + alloc`. The barriers are:

1. `#[cfg(feature = "std")]` guards on all transport code (should be `alloc`)
2. `std::boxed::Box` usage (needs `alloc::boxed::Box`)
3. Hardcoded `features = ["zenoh"]` in dependencies (forces `shim-posix`, prevents `shim-zephyr`)
4. `Zenoh*` type aliases only available with `zenoh` feature, not generic `shim`

## Progress

| Task | Status | Description |
|------|--------|-------------|
| 21.1 Cargo.toml feature restructuring | Pending | Decouple from hardcoded `zenoh`, add `alloc`/`shim-*` features |
| 21.2 Compile-time guard | Pending | `compile_error!` when `alloc` enabled without a backend |
| 21.3 Type name migration | Pending | `Zenoh*` → `Shim*` across all source files |
| 21.4 cfg guard + Box changes | Pending | `std` → `alloc` guards, `std::boxed::Box` → `alloc::boxed::Box` |
| 21.5 Verification | Pending | `just quality`, `cargo check --no-default-features`, `just test-c` |

## 21.1 Cargo.toml Feature Restructuring

**Status: PENDING**

**File**: `crates/nano-ros-c/Cargo.toml`

- [ ] Add `alloc` feature: `["nano-ros/alloc", "nano-ros-transport/alloc"]`
- [ ] Make `std` imply `alloc`: `["alloc", "nano-ros/std", "nano-ros-transport/std"]`
- [ ] Add `shim-posix` feature: `["nano-ros/shim-posix", "nano-ros-transport/shim-posix"]`
- [ ] Add `shim-zephyr` feature: `["nano-ros/shim-zephyr", "nano-ros-transport/shim-zephyr"]`
- [ ] Set `default = ["std", "shim-posix"]`
- [ ] Remove hardcoded `features = ["zenoh"]` from dependency declarations
- [ ] Use `default-features = false` on all dependencies

Default build (`std + shim-posix`) must be functionally identical to current behavior.

## 21.2 Compile-Time Guard

**Status: PENDING**

**File**: `crates/nano-ros-c/src/lib.rs`

- [ ] Add `compile_error!` for `alloc` without a transport backend

```rust
#[cfg(all(feature = "alloc", not(any(feature = "shim-posix", feature = "shim-zephyr"))))]
compile_error!(
    "nano-ros-c `alloc` requires a transport backend. Enable `shim-posix` or `shim-zephyr`."
);
```

## 21.3 Type Name Migration (`Zenoh*` → `Shim*`)

**Status: PENDING**

`Zenoh*` names require the `zenoh` feature. `Shim*` names are available with any `shim-*` backend. All source files must migrate:

| Old name | New name |
|----------|----------|
| `ZenohSession` | `ShimSession` |
| `ZenohPublisher` | `ShimPublisher` |
| `ZenohSubscriber` | `ShimSubscriber` |
| `ZenohServiceServer` | `ShimServiceServer` |
| `ZenohServiceClient` | `ShimServiceClient` |
| `ZenohTransport` | `ShimTransport` |

**Files to update:**
- [ ] `crates/nano-ros-c/src/support.rs`
- [ ] `crates/nano-ros-c/src/publisher.rs`
- [ ] `crates/nano-ros-c/src/subscription.rs`
- [ ] `crates/nano-ros-c/src/service.rs`
- [ ] `crates/nano-ros-c/src/action.rs`
- [ ] `crates/nano-ros-c/src/executor.rs`

## 21.4 cfg Guard and Box Changes

**Status: PENDING**

All transport-related code blocks need:
1. `#[cfg(feature = "std")]` → `#[cfg(feature = "alloc")]`
2. `#[cfg(not(feature = "std"))]` → `#[cfg(not(feature = "alloc"))]`
3. `std::boxed::Box` → `alloc::boxed::Box`

### support.rs
- [ ] `nano_ros_support_init`: cfg `alloc`, `alloc::boxed::Box`, `ShimSession`
- [ ] `nano_ros_support_fini`: cfg `alloc`, `alloc::boxed::Box::from_raw`, `ShimSession`
- [ ] `get_session()`: cfg `alloc`, `ShimSession`
- [ ] `get_session_mut()`: cfg `alloc`, `ShimSession`

### publisher.rs
- [ ] `nano_ros_publisher_init_with_qos`: cfg `alloc`, Box, `ShimSession`
- [ ] `nano_ros_publish_raw`: cfg `alloc`, `ShimPublisher`
- [ ] `nano_ros_publisher_fini`: cfg `alloc`, Box, `ShimPublisher`

### subscription.rs
- [ ] `nano_ros_subscription_init_with_qos`: cfg `alloc`, Box, `ShimSubscriber`
- [ ] `nano_ros_subscription_fini`: cfg `alloc`, Box, `ShimSubscriber`

### service.rs
- [ ] `nano_ros_service_init`: cfg `alloc`, Box, `ShimServiceServer`
- [ ] `nano_ros_service_take_request`: cfg `alloc`, `ShimServiceServer`
- [ ] `nano_ros_service_send_response`: cfg `alloc`, `ShimServiceServer`
- [ ] `nano_ros_service_fini`: cfg `alloc`, Box, `ShimServiceServer`
- [ ] `nano_ros_service_client_init`: cfg `alloc`, Box, `ShimServiceClient`
- [ ] `nano_ros_service_call`: cfg `alloc`, `ShimServiceClient`
- [ ] `nano_ros_service_client_fini`: cfg `alloc`, Box, `ShimServiceClient`

### action.rs
- [ ] `nano_ros_action_server_init`: cfg `alloc`, Box
- [ ] `nano_ros_action_server_send_goal_response`: cfg `alloc`
- [ ] `nano_ros_action_server_publish_feedback`: cfg `alloc`
- [ ] `nano_ros_action_server_publish_result`: cfg `alloc`
- [ ] `nano_ros_action_server_fini`: cfg `alloc`, Box
- [ ] `nano_ros_action_client_init`: cfg `alloc`, Box
- [ ] `nano_ros_action_client_send_goal_request`: cfg `alloc`
- [ ] `nano_ros_action_client_send_cancel_request`: cfg `alloc`
- [ ] `nano_ros_action_client_get_result_request`: cfg `alloc`
- [ ] `nano_ros_action_client_process_feedback`: cfg `alloc`
- [ ] `nano_ros_action_client_fini`: cfg `alloc`, Box

### executor.rs
- [ ] `process_subscription`: cfg `alloc`, `ShimSubscriber`
- [ ] `process_service_request`: cfg `alloc`, `ShimServiceServer`
- [ ] `sample_subscription_for_let`: cfg `alloc`, `ShimSubscriber`
- [ ] `process_subscription_from_let`: cfg `alloc`
- [ ] `sample_all_handles_for_let`: cfg `alloc`
- [ ] `spin_some` LET blocks: cfg `alloc`
- [ ] `spin_some` subscription dispatch: cfg `alloc`
- [ ] `spin_some` service dispatch: cfg `alloc`

### Note on trait imports
The trait imports (`Publisher`, `Subscriber`, `Session`, `Transport`, `ServiceServerTrait`, `ServiceClientTrait`, `QosSettings`, `TopicInfo`, `ServiceInfo`, `ActionInfo`) from `nano_ros_transport::traits` are always available — no feature gating needed.

## 21.5 Verification

**Status: PENDING**

- [ ] `just quality` — default (`std + shim-posix`) still works, all tests pass
- [ ] `cargo check -p nano-ros-c --no-default-features --features alloc,shim-posix` — `no_std + alloc` type-checks
- [ ] `just test-c` — C integration tests pass

## Files Modified

| File | Change |
|------|--------|
| `crates/nano-ros-c/Cargo.toml` | Remove hardcoded `zenoh`, `std` implies `alloc`, `shim-posix` in default |
| `crates/nano-ros-c/src/lib.rs` | Add `compile_error!` guard |
| `crates/nano-ros-c/src/support.rs` | cfg `alloc`, Box, `ShimSession` |
| `crates/nano-ros-c/src/publisher.rs` | cfg `alloc`, Box, `ShimPublisher` |
| `crates/nano-ros-c/src/subscription.rs` | cfg `alloc`, Box, `ShimSubscriber` |
| `crates/nano-ros-c/src/service.rs` | cfg `alloc`, Box, `ShimServiceServer/Client` |
| `crates/nano-ros-c/src/action.rs` | cfg `alloc`, Box, all action types |
| `crates/nano-ros-c/src/executor.rs` | cfg `alloc`, `ShimSubscriber/ServiceServer` |

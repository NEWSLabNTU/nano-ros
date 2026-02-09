# Phase 21: C API `no_std` Backend

**Status: COMPLETE**

## Summary

The C API crate (`nano-ros-c`) has ~30 functions that return `NANO_ROS_RET_ERROR` when compiled without `std`. The `Zenoh*` type aliases (`ZenohPublisher`, `ZenohSession`, etc.) are re-exports of `Shim*` types from `nano-ros-transport`, which already work in `no_std + alloc`. The barriers are:

1. `#[cfg(feature = "std")]` guards on all transport code (should be `alloc`)
2. `std::boxed::Box` usage (needs `alloc::boxed::Box`)
3. Hardcoded `features = ["zenoh"]` in dependencies (forces `shim-posix`, prevents `shim-zephyr`)
4. `Zenoh*` type aliases only available with `zenoh` feature, not generic `shim`

## Progress

| Task                                  | Status  | Description                                                        |
|---------------------------------------|---------|--------------------------------------------------------------------|
| 21.1 Cargo.toml feature restructuring | Done    | Decouple from hardcoded `zenoh`, add `alloc`/`shim-*` features     |
| 21.2 Compile-time guard               | Done    | `compile_error!` when `alloc` enabled without a backend            |
| 21.3 Type name migration              | Done    | `Zenoh*` → `Shim*` across all source files                         |
| 21.4 cfg guard + Box changes          | Done    | `std` → `alloc` guards, `std::boxed::Box` → `alloc::boxed::Box`    |
| 21.5 Verification                     | Done    | `just quality` ✓, `cargo check no_std` ✓, `just test-c` ✓         |

## 21.1 Cargo.toml Feature Restructuring

**Status: DONE**

**File**: `packages/core/nano-ros-c/Cargo.toml`

- [x] Add `alloc` feature: `["nano-ros/alloc", "nano-ros-transport/alloc"]`
- [x] Make `std` imply `alloc`: `["alloc", "nano-ros/std", "nano-ros-transport/std"]`
- [x] Add `shim-posix` feature: `["nano-ros/shim-posix", "nano-ros-transport/shim-posix"]`
- [x] Add `shim-zephyr` feature: `["nano-ros/shim-zephyr", "nano-ros-transport/shim-zephyr"]`
- [x] Set `default = ["std", "shim-posix"]`
- [x] Remove hardcoded `features = ["zenoh"]` from dependency declarations
- [x] Use `default-features = false` on all dependencies

Note: `alloc` and `shim-*` features already existed; the key changes were adding `shim-posix` to `default`, making `std` imply `alloc`, and removing `features = ["zenoh"]` from dependencies.

## 21.2 Compile-Time Guard

**Status: DONE**

**File**: `packages/core/nano-ros-c/src/lib.rs`

- [x] Add `compile_error!` for `alloc` without a transport backend

```rust
#[cfg(all(feature = "alloc", not(any(feature = "shim-posix", feature = "shim-zephyr"))))]
compile_error!(
    "nano-ros-c `alloc` requires a transport backend. Enable `shim-posix` or `shim-zephyr`."
);
```

## 21.3 Type Name Migration (`Zenoh*` → `Shim*`)

**Status: DONE**

`Zenoh*` names require the `zenoh` feature. `Shim*` names are available with any `shim-*` backend. All source files must migrate:

| Old name             | New name            |
|----------------------|---------------------|
| `ZenohSession`       | `ShimSession`       |
| `ZenohPublisher`     | `ShimPublisher`     |
| `ZenohSubscriber`    | `ShimSubscriber`    |
| `ZenohServiceServer` | `ShimServiceServer` |
| `ZenohServiceClient` | `ShimServiceClient` |
| `ZenohTransport`     | `ShimTransport`     |

**Files to update:**
- [x] `packages/core/nano-ros-c/src/support.rs`
- [x] `packages/core/nano-ros-c/src/publisher.rs`
- [x] `packages/core/nano-ros-c/src/subscription.rs`
- [x] `packages/core/nano-ros-c/src/service.rs`
- [x] `packages/core/nano-ros-c/src/action.rs`
- [x] `packages/core/nano-ros-c/src/executor.rs`

## 21.4 cfg Guard and Box Changes

**Status: DONE**

All transport-related code blocks need:
1. `#[cfg(feature = "std")]` → `#[cfg(feature = "alloc")]`
2. `#[cfg(not(feature = "std"))]` → `#[cfg(not(feature = "alloc"))]`
3. `std::boxed::Box` → `alloc::boxed::Box`

### support.rs
- [x] `nano_ros_support_init`: cfg `alloc`, `alloc::boxed::Box`, `ShimSession`
- [x] `nano_ros_support_fini`: cfg `alloc`, `alloc::boxed::Box::from_raw`, `ShimSession`
- [x] `get_session()`: cfg `alloc`, `ShimSession`
- [x] `get_session_mut()`: cfg `alloc`, `ShimSession`

### publisher.rs
- [x] `nano_ros_publisher_init_with_qos`: cfg `alloc`, Box, `ShimSession`
- [x] `nano_ros_publish_raw`: cfg `alloc`, `ShimPublisher`
- [x] `nano_ros_publisher_fini`: cfg `alloc`, Box, `ShimPublisher`

### subscription.rs
- [x] `nano_ros_subscription_init_with_qos`: cfg `alloc`, Box, `ShimSubscriber`
- [x] `nano_ros_subscription_fini`: cfg `alloc`, Box, `ShimSubscriber`

### service.rs
- [x] `nano_ros_service_init`: cfg `alloc`, Box, `ShimServiceServer`
- [x] `nano_ros_service_take_request`: cfg `alloc`, `ShimServiceServer`
- [x] `nano_ros_service_send_response`: cfg `alloc`, `ShimServiceServer`
- [x] `nano_ros_service_fini`: cfg `alloc`, Box, `ShimServiceServer`
- [x] `nano_ros_service_client_init`: cfg `alloc`, Box, `ShimServiceClient`
- [x] `nano_ros_service_call`: cfg `alloc`, `ShimServiceClient`
- [x] `nano_ros_service_client_fini`: cfg `alloc`, Box, `ShimServiceClient`

### action.rs
- [x] `nano_ros_action_server_init`: cfg `alloc`, Box
- [x] `nano_ros_action_server_send_goal_response`: cfg `alloc`
- [x] `nano_ros_action_server_publish_feedback`: cfg `alloc`
- [x] `nano_ros_action_server_publish_result`: cfg `alloc`
- [x] `nano_ros_action_server_fini`: cfg `alloc`, Box
- [x] `nano_ros_action_client_init`: cfg `alloc`, Box
- [x] `nano_ros_action_client_send_goal_request`: cfg `alloc`
- [x] `nano_ros_action_client_send_cancel_request`: cfg `alloc`
- [x] `nano_ros_action_client_get_result_request`: cfg `alloc`
- [x] `nano_ros_action_client_process_feedback`: cfg `alloc`
- [x] `nano_ros_action_client_fini`: cfg `alloc`, Box

### executor.rs
- [x] `process_subscription`: cfg `alloc`, `ShimSubscriber`
- [x] `process_service_request`: cfg `alloc`, `ShimServiceServer`
- [x] `sample_subscription_for_let`: cfg `alloc`, `ShimSubscriber`
- [x] `process_subscription_from_let`: cfg `alloc`
- [x] `sample_all_handles_for_let`: cfg `alloc`
- [x] `spin_some` LET blocks: cfg `alloc`
- [x] `spin_some` subscription dispatch: cfg `alloc`
- [x] `spin_some` service dispatch: cfg `alloc`

### Note on action.rs UUID generation
`nano_ros_goal_uuid_generate()` retains `#[cfg(feature = "std")]` / `#[cfg(not(feature = "std"))]` guards because it genuinely requires `std::time::SystemTime` and `std::sync::atomic::AtomicU64`. In `no_std` mode it returns zeroed UUIDs (a valid fallback).

### Note on trait imports
The trait imports (`Publisher`, `Subscriber`, `Session`, `Transport`, `ServiceServerTrait`, `ServiceClientTrait`, `QosSettings`, `TopicInfo`, `ServiceInfo`, `ActionInfo`) from `nano_ros_transport::traits` are always available — no feature gating needed.

## 21.5 Verification

**Status: DONE**

- [x] `just quality` — default (`std + shim-posix`) still works, all tests pass
- [x] `cargo check -p nano-ros-c --no-default-features --features alloc,shim-posix` — `no_std + alloc` type-checks (only expected `no_std` boilerplate errors: `global_allocator`, `panic_handler`)
- [x] `just test-c` — C integration tests pass (c-integration, c-codegen, c-msg-gen)

## Files Modified

| File                                    | Change                                                                   |
|-----------------------------------------|--------------------------------------------------------------------------|
| `packages/core/nano-ros-c/Cargo.toml`          | Remove hardcoded `zenoh`, `std` implies `alloc`, `shim-posix` in default |
| `packages/core/nano-ros-c/src/lib.rs`          | Add `compile_error!` guard                                               |
| `packages/core/nano-ros-c/src/support.rs`      | cfg `alloc`, Box, `ShimSession`                                          |
| `packages/core/nano-ros-c/src/publisher.rs`    | cfg `alloc`, Box, `ShimPublisher`                                        |
| `packages/core/nano-ros-c/src/subscription.rs` | cfg `alloc`, Box, `ShimSubscriber`                                       |
| `packages/core/nano-ros-c/src/service.rs`      | cfg `alloc`, Box, `ShimServiceServer/Client`                             |
| `packages/core/nano-ros-c/src/action.rs`       | cfg `alloc`, Box, all action types                                       |
| `packages/core/nano-ros-c/src/executor.rs`     | cfg `alloc`, `ShimSubscriber/ServiceServer`                              |

# Phase 77: Async Action Client — Eliminate Blocking zpico_get

**Goal**: Replace the blocking `zpico_get` (condvar wait) in C/C++ action client calls with a non-blocking, executor-polled pattern aligned with rclc and rclcpp conventions. This fixes FreeRTOS QEMU hangs where the condvar is never signaled.

**Status**: In Progress (77.1–77.2 done)
**Priority**: High
**Depends on**: Phase 68 (Alloc-free C/C++ bindings), Phase 69 (C/C++ action examples)

## Overview

### Problem

The C and C++ action client APIs use `send_goal_blocking` and `get_result_blocking`, which call `zpico_get` — a blocking function that waits on a condvar for the zenoh dropper to signal completion. On FreeRTOS QEMU, the condvar wait blocks indefinitely because QEMU's single-threaded emulation can starve the lease task that fires the dropper.

The Rust API avoids this entirely — it uses non-blocking `send_goal_raw` (via `zpico_get_start`) and returns a `Promise` that is polled during `spin_once`.

### Root Cause

`zpico_get` (multi-threaded path in `zpico.c`):
```c
_z_mutex_lock(&ctx.mutex);
while (!ctx.done) {
    _z_condvar_wait(&ctx.cond, &ctx.mutex);  // blocks indefinitely
}
_z_mutex_unlock(&ctx.mutex);
```

The dropper (which sets `ctx.done = true` and signals the condvar) is fired by zenoh-pico's lease task via `_z_pending_query_process_timeout`. On FreeRTOS QEMU, the lease task may not get scheduled in time, causing the condvar wait to block forever.

### Solution

Replace blocking `zpico_get` calls in action client paths with the non-blocking `zpico_get_start` + `zpico_get_check` pattern, polled by the executor's `spin_once`.

This aligns with:
- **rclc**: fire-and-forget send + callback-driven response (executor polls)
- **rclcpp**: `async_send_goal` returns future + callbacks via `SendGoalOptions`

## Reference APIs

### rclc (C) Action Client

```c
// Send goal — non-blocking, returns immediately
rcl_ret_t rclc_action_send_goal_request(
    rclc_action_client_t *client,
    void *ros_request,
    rclc_action_goal_handle_t **goal_handle);

// Callbacks registered via executor:
rclc_executor_add_action_client(
    &executor, &action_client, max_goals,
    &result_response, &feedback,
    goal_callback,      // called on acceptance/rejection
    feedback_callback,  // called on feedback
    result_callback,    // called on result
    cancel_callback,    // called on cancel response
    context);

// Executor spin drives all callbacks
rclc_executor_spin_some(&executor, timeout_ns);
```

Key pattern: `send_goal_request` fires immediately, executor's `spin_some` takes responses and invokes callbacks.

### rclcpp (C++) Action Client

```cpp
// Send goal — returns future, callbacks via options
auto future = client->async_send_goal(goal, SendGoalOptions{
    .goal_response_callback = [](auto handle) { ... },
    .feedback_callback = [](auto handle, auto fb) { ... },
    .result_callback = [](auto wrapped_result) { ... },
});

// Executor spin drives callbacks
executor.spin_some();
```

Key pattern: `async_send_goal` returns a future, callbacks are optional. Result is retrieved via `async_get_result` or via the `result_callback` in options.

## Design

### C API (nros-c)

Add non-blocking action client functions alongside the existing blocking ones:

```c
// --- Non-blocking (new) ---

// Send goal — non-blocking, goal_uuid filled on success
// Goal acceptance arrives via goal_callback during nros_executor_spin_some()
nros_ret_t nros_action_send_goal_async(
    nros_action_client_t *client,
    const uint8_t *goal_buf, size_t goal_len,
    nros_goal_uuid_t *goal_uuid);

// Get result — non-blocking, result arrives via result_callback
nros_ret_t nros_action_get_result_async(
    nros_action_client_t *client,
    const nros_goal_uuid_t *goal_uuid);

// Cancel — non-blocking
nros_ret_t nros_action_cancel_goal_async(
    nros_action_client_t *client,
    const nros_goal_uuid_t *goal_uuid);

// Register callbacks (called during init or separately)
nros_ret_t nros_action_client_set_goal_callback(
    nros_action_client_t *client,
    nros_action_goal_response_callback_t callback, void *context);

nros_ret_t nros_action_client_set_result_callback(
    nros_action_client_t *client,
    nros_action_result_callback_t callback, void *context);

// --- Blocking (existing, kept as convenience) ---
nros_ret_t nros_action_send_goal(...);     // blocks until accepted/rejected
nros_ret_t nros_action_get_result(...);    // blocks until result received
```

Callback signatures:
```c
typedef void (*nros_action_goal_response_callback_t)(
    const nros_goal_uuid_t *goal_uuid,
    bool accepted,
    void *context);

typedef void (*nros_action_result_callback_t)(
    const nros_goal_uuid_t *goal_uuid,
    nros_goal_status_t status,
    const uint8_t *result, size_t result_len,
    void *context);
```

### C++ API (nros-cpp)

Add `SendGoalOptions` with callback pointers (freestanding C++14, no `std::function`):

```cpp
template <typename A>
class ActionClient {
public:
    struct SendGoalOptions {
        void (*goal_response)(bool accepted, const uint8_t goal_id[16], void *ctx) = nullptr;
        void (*feedback)(const uint8_t goal_id[16],
                         const typename A::Feedback&, void *ctx) = nullptr;
        void (*result)(const uint8_t goal_id[16], GoalStatus status,
                       const typename A::Result&, void *ctx) = nullptr;
        void *context = nullptr;
    };

    // Non-blocking — callbacks invoked during spin_once()
    Result send_goal(const typename A::Goal& goal, uint8_t goal_id[16],
                     const SendGoalOptions& options = {});

    // Existing polling API remains:
    bool try_recv_feedback(typename A::Feedback& feedback);
    Result get_result(const uint8_t goal_id[16], typename A::Result& result);  // blocking
};
```

### Executor Integration

The executor's `spin_once` already processes action server requests via `action_server_raw_try_process`. For the client side, add:

1. **Action client entry in executor**: Register the action client's pending `zpico_get_start` handles with the executor
2. **Poll during spin**: `action_client_try_process` checks `zpico_get_check` for each pending request
3. **Invoke callbacks**: When a reply arrives, deserialize and invoke the registered callback

This follows the same pattern as the existing subscription and service callbacks — the executor drives all I/O.

### Internal Changes

#### ActionClientCore (nros-node)

Already has the non-blocking primitives:
- `send_goal_raw()` → uses `send_request_raw` (zpico_get_start)
- `try_recv_send_goal_reply()` → uses `try_recv_reply_raw` (zpico_get_check)
- `send_get_result_request()` → uses `send_request_raw`
- `try_recv_get_result_reply()` → uses `try_recv_reply_raw`

Add executor arena entry for action client that polls these during `spin_once`.

#### zpico_get Blocking Path

Keep `zpico_get` for backward compatibility (used by blocking service calls and the Rust `#[cfg(not(feature = "ffi-sync"))]` path). The blocking path continues to work on POSIX and platforms where the lease task runs reliably.

The action client async path bypasses `zpico_get` entirely — it uses `zpico_get_start`/`zpico_get_check` which never block.

## Work Items

- [x] 77.1 — Add `ActionClientCore` executor entry (nros-node)
- [x] 77.2 — Add C async action client API (nros-c)
- [ ] 77.3 — Add C++ async action client API (nros-cpp)
- [ ] 77.4 — Update C action examples to use async API
- [ ] 77.5 — Update C++ action examples to use async API
- [ ] 77.6 — Re-enable `test_freertos_cpp_action_e2e`
- [ ] 77.7 — Update documentation

### 77.1 — Add ActionClientCore executor entry (nros-node)

Add `action_client_try_process` function that:
1. Polls `try_recv_send_goal_reply()` — if reply arrived, invoke goal response callback
2. Polls `try_recv_get_result_reply()` — if reply arrived, invoke result callback
3. Polls `try_recv_feedback_raw()` — if feedback arrived, invoke feedback callback

Register this as a new `EntryKind::ActionClient` in the executor arena with `has_data: always_ready` and `InvocationMode::Always`.

**Files**:
- `packages/core/nros-node/src/executor/arena.rs` — add `action_client_try_process`
- `packages/core/nros-node/src/executor/action.rs` — add `add_action_client_raw`
- `packages/core/nros-node/src/executor/types.rs` — add `EntryKind::ActionClient`

### 77.2 — Add C async action client API (nros-c)

Implement `nros_action_send_goal_async`, `nros_action_get_result_async`, `nros_action_cancel_goal_async` in nros-c. These use the non-blocking `ActionClientCore` methods and register callbacks that are invoked during `nros_executor_spin_some`.

Add `nros_executor_add_action_client` to register the action client with the executor (similar to `nros_executor_add_action_server`).

**Files**:
- `packages/core/nros-c/src/action/client.rs` — async FFI functions
- `packages/core/nros-c/src/executor.rs` — `nros_executor_add_action_client`
- `packages/core/nros-c/include/nano_ros/action.h` — C header declarations

### 77.3 — Add C++ async action client API (nros-cpp)

Implement `SendGoalOptions` and update `ActionClient::send_goal` to use the non-blocking path. Callbacks are stored in the `CppActionClient` internal struct and invoked during `spin_once`.

**Files**:
- `packages/core/nros-cpp/src/action.rs` — async FFI functions
- `packages/core/nros-cpp/include/nros/action_client.hpp` — SendGoalOptions, updated send_goal

### 77.4 — Update C action examples to use async API

Update `examples/qemu-arm-freertos/c/zenoh/action-client/` to use `nros_action_send_goal_async` + callbacks instead of `nros_action_send_goal` (blocking).

**Files**:
- `examples/qemu-arm-freertos/c/zenoh/action-client/src/main.c`
- Other platform C action client examples

### 77.5 — Update C++ action examples to use async API

Update `examples/qemu-arm-freertos/cpp/zenoh/action-client/` to use `SendGoalOptions` with callbacks.

**Files**:
- `examples/qemu-arm-freertos/cpp/zenoh/action-client/src/main.cpp`
- Other platform C++ action client examples

### 77.6 — Re-enable test_freertos_cpp_action_e2e

Remove the `#[ignore]` from `test_freertos_cpp_action_e2e` once the async path eliminates the blocking issue.

Also investigate and fix the C++ action server's `create_action_server` hang (may be a separate zenoh-pico mutex issue when declaring 5 entities).

**Files**:
- `packages/testing/nros-tests/tests/freertos_qemu.rs`

### 77.7 — Update documentation

Update C API reference, C++ API guide, and example guides to document the async action client pattern.

**Files**:
- `docs/reference/c-api-cmake.md`
- `docs/guides/cpp-api.md`
- `book/src/reference/c-api.md`

## Acceptance Criteria

- [ ] C action client examples use non-blocking `send_goal_async` + callbacks
- [ ] C++ action client examples use `SendGoalOptions` with callbacks
- [ ] No `zpico_get` (blocking condvar) in the action client path
- [ ] `test_freertos_c_action_e2e` passes reliably (no flakiness)
- [ ] `test_freertos_cpp_action_e2e` passes (currently `#[ignore]`)
- [ ] Blocking variants (`nros_action_send_goal`, `client.get_result`) remain for convenience
- [ ] `just quality` passes
- [ ] Existing Rust action API unchanged

## Notes

- The Rust API already uses the non-blocking pattern (Promise-based). This phase brings C/C++ to parity.
- The blocking `zpico_get` is still used by service client `call_raw` — those calls are shorter-lived and less prone to the FreeRTOS scheduling issue. A future phase could migrate services to the async pattern too.
- The `zpico_get_start`/`zpico_get_check` pair is already implemented and tested — this phase wires it into the action client FFI and executor.
- The C++ action server hang (`create_action_server` declaring 5 entities) may be a separate zenoh-pico issue. If it persists after the client-side fix, it should be tracked independently.

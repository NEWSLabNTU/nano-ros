# Phase 77: Async Action Client — Eliminate Blocking zpico_get

**Goal**: Replace the blocking `zpico_get` (condvar wait) in C/C++ action client calls with a non-blocking, executor-polled pattern aligned with rclc and rclcpp conventions. This fixes FreeRTOS QEMU hangs where the condvar is never signaled.

**Status**: In Progress (77.1–77.5 done)
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

## Design Principle: Only `spin*()` Drives the Runtime

The Rust `Promise::wait()` already follows this pattern:
```rust
pub fn wait(&mut self, executor: &mut Executor, timeout_ms: u64) -> Result<T, NodeError> {
    for _ in 0..max_spins {
        executor.spin_once(spin_interval_ms);  // drives all I/O
        if let Some(result) = self.try_recv()? { return Ok(result); }
    }
    Err(NodeError::Timeout)
}
```

It never calls `zpico_get` (blocking condvar). The executor's `spin_once` drives all I/O, and `try_recv` checks if the reply arrived. The C/C++ API must follow the same principle: **no direct zpico_get calls — only spin_once + non-blocking poll**.

### Unified Architecture

A single `ActionClientCore` is owned by the executor's arena entry:

1. `nros_action_client_init()` — stores metadata + callbacks only (no transport handles)
2. `nros_executor_add_action_client()` — creates `ActionClientCore` in the arena (3 service clients + 1 subscriber)
3. `nros_action_send_goal_async()` — routes through the arena entry's core → `send_goal_raw()` → `zpico_get_start()`
4. `nros_executor_spin_some()` → `action_client_raw_try_process()` → polls `try_recv_send_goal_reply()`, `try_recv_feedback_raw()`, `try_recv_get_result_reply()` → invokes callbacks
5. Blocking convenience wrappers spin the executor internally (like Rust's `Promise::wait`)

No separate `nros_action_client_poll()` needed. No `zpico_get` in any path.

### C API (nros-c)

```c
// --- Core async API (executor-driven) ---

// Send goal — non-blocking, response via goal_response_callback during spin
nros_ret_t nros_action_send_goal_async(
    nros_action_client_t *client,
    const uint8_t *goal_buf, size_t goal_len,
    nros_goal_uuid_t *goal_uuid);

// Request result — non-blocking, result via result_callback during spin
nros_ret_t nros_action_get_result_async(
    nros_action_client_t *client,
    const nros_goal_uuid_t *goal_uuid);

// Cancel — non-blocking
nros_ret_t nros_action_cancel_goal_async(
    nros_action_client_t *client,
    const nros_goal_uuid_t *goal_uuid);

// --- Blocking convenience (spins executor internally) ---
// These NEVER call zpico_get — they loop spin_once + check reply.

nros_ret_t nros_action_send_goal(
    nros_action_client_t *client,
    nros_executor_t *executor,          // NEW: executor parameter
    const uint8_t *goal_buf, size_t goal_len,
    nros_goal_uuid_t *goal_uuid);

nros_ret_t nros_action_get_result(
    nros_action_client_t *client,
    nros_executor_t *executor,          // NEW: executor parameter
    const nros_goal_uuid_t *goal_uuid,
    nros_goal_status_t *status,
    uint8_t *result, size_t capacity, size_t *result_len);
```

The blocking wrappers take an `executor` parameter (like Rust's `Promise::wait` takes `&mut Executor`) and spin it internally:
```c
nros_ret_t nros_action_send_goal(..., executor, ...) {
    nros_action_send_goal_async(client, goal, goal_len, goal_uuid);
    for (int i = 0; i < timeout / 10; i++) {
        nros_executor_spin_some(executor, 10ms);
        if (accepted) return NROS_RET_OK;
        if (rejected) return NROS_RET_ERROR;
    }
    return NROS_RET_TIMEOUT;
}
```

### C++ API (nros-cpp)

```cpp
template <typename A>
class ActionClient {
public:
    struct SendGoalOptions {
        void (*goal_response)(bool accepted, const uint8_t goal_id[16], void *ctx);
        void (*feedback)(const uint8_t goal_id[16], const uint8_t* data, size_t len, void *ctx);
        void (*result)(const uint8_t goal_id[16], int status, const uint8_t* data, size_t len, void *ctx);
        void *context;
    };

    // Async — callbacks invoked during spin_once()
    Result send_goal_async(const GoalType& goal, uint8_t goal_id[16]);
    Result get_result_async(const uint8_t goal_id[16]);
    void set_callbacks(const SendGoalOptions& options);

    // Blocking convenience (spins executor internally, like Rust Promise::wait)
    Result send_goal(const GoalType& goal, uint8_t goal_id[16]);
    Result get_result(const uint8_t goal_id[16], ResultType& result);
};
```

### Executor Integration

The executor's `spin_once` dispatches `action_client_raw_try_process` for each registered action client — same as subscriptions and action servers. The `ActionClientCore` lives in the arena. No user-side polling needed.

### Waker Integration

On POSIX/Zephyr, `zpico_spin_once` blocks on `g_spin_cv` condvar — it wakes when `_zpico_notify_spin()` fires (triggered by `pending_get_reply_handler`). On FreeRTOS+lwIP, `zpico_spin_once` uses `vTaskDelay`. Both are efficient — not busy-polling.

The Rust `AtomicWaker` per pending_get slot enables `Promise` to implement `Future`. For C/C++, the executor-level `g_spin_cv` / `vTaskDelay` provides equivalent efficiency — `spin_once` blocks until data arrives, then dispatches.

## Work Items

### Done (initial async infrastructure)

- [x] 77.1 — Executor action client arena entry (nros-node): `EntryKind::ActionClient`, `ActionClientRawArenaEntry`, `action_client_raw_try_process`, callback types, `add_action_client_raw` / `add_action_client_core`, `action_client_core_mut`
- [x] 77.2 — C async action client FFI (nros-c): `nros_action_send_goal_async`, `nros_action_get_result_async`, `nros_action_client_poll`, `nros_executor_add_action_client`, trampolines
- [x] 77.3 — C++ async action client FFI (nros-cpp): `nros_cpp_action_client_send_goal_async`, `nros_cpp_action_client_get_result_async`, `nros_cpp_action_client_poll`, `set_callbacks`, `SendGoalOptions`
- [x] 77.4 — C FreeRTOS action client example uses async API
- [x] 77.5 — C++ FreeRTOS action client example uses async API

### Remaining (unified design — single arena-owned core)

- [x] 77.6 — Unify C ActionClientCore ownership: single core in arena
    - `nros_action_client_init` stores metadata only (no transport handles)
    - `nros_executor_add_action_client` creates the ONLY `ActionClientCore` in the arena
    - `nros_action_send_goal_async` routes through the arena entry's core
    - Remove `nros_action_client_poll` — `spin_once` handles everything
    - **Files**: `nros-c/src/action/client.rs`, `nros-c/src/executor.rs`
- [x] 77.7 — Rewrite C blocking API to spin executor internally (C++ pending)
    - `nros_action_send_goal(client, executor, ...)` → `send_goal_async` + `spin_once` loop
    - `nros_action_get_result(client, executor, ...)` → `get_result_async` + `spin_once` loop
    - Remove all `zpico_get` / `call_raw` / `send_goal_blocking` calls from C/C++ action client
    - **Files**: `nros-c/src/action/client.rs`, `nros-cpp/src/action.rs`
- [x] 77.8 — Unify C++ ActionClient to use arena core
    - `nros_cpp_action_client_create` stores metadata only
    - Executor registration creates `ActionClientCore` in arena
    - `send_goal_async`/`get_result_async` route through arena
    - Remove `CppActionClient.core` — only arena core exists
    - Remove `nros_cpp_action_client_poll` — `spin_once` handles everything
    - **Files**: `nros-cpp/src/action.rs`, `nros-cpp/include/nros/action_client.hpp`
- [x] 77.9 — Fix C++ action server deferred init (same pattern)
    - `nros_cpp_action_server_create` stores metadata only
    - Transport handles created during executor registration
    - Fixes the FreeRTOS QEMU deadlock (5 entity declarations)
    - **Files**: `nros-cpp/src/action.rs`
- [x] 77.10 — Update C header (`action.h`) with new API signatures
    - Add `executor` parameter to `nros_action_send_goal` and `nros_action_get_result`
    - Declare `nros_executor_add_action_client`
    - Declare `nros_action_send_goal_async`, `nros_action_get_result_async`
    - Declare `nros_action_client_set_goal_response_callback`
    - Declare `nros_goal_response_callback_t` typedef
    - Remove `nros_action_client_poll` declaration (if present)
    - Update `nros_action_client_t` struct: replace `_internal` opaque size (now 16 bytes)
    - **Files**: `nros-c/include/nros/action.h`, `nros-c/include/nros/executor.h`
- [x] 77.11 — Update C action client examples for new API
    - FreeRTOS example uses `nros_executor_add_action_client` + blocking `nros_action_send_goal(client, executor, ...)`
    - NuttX, ThreadX, Zephyr C action client examples: same pattern
    - Native POSIX C action client example: same pattern
    - **Files**: `examples/*/c/zenoh/action-client/src/main.c`
- [x] 77.12 — Update C++ action client/server examples for new API
    - FreeRTOS C++ action client: remove `client.poll()`, callbacks fire via `spin_once`
    - FreeRTOS C++ action server: verify deferred init works
    - Other platform C++ examples: same pattern
    - **Files**: `examples/*/cpp/zenoh/action-client/src/main.cpp`, `examples/*/cpp/zenoh/action-server/src/main.cpp`
- [x] 77.13 — Fix action client/server bugs and re-enable tests
    - **Root cause 1**: Arena trampoline callbacks only registered when C callbacks were non-None at `nros_executor_add_action_client` time. Blocking wrappers install temporary callbacks AFTER registration, so the arena consumed replies without invoking the trampoline → flag never set → timeout.
    - **Fix**: Always register trampolines in the arena (they check the C struct's callback at runtime).
    - **Root cause 2**: Native and NuttX C action server examples missing `nros_executor_add_action_server()` call (deferred init pattern). Server transport handles never created → goals never received.
    - **Fix**: Add `nros_executor_add_action_server()` to native and NuttX examples.
    - **Root cause 3**: Native C action client example had no warm-up spin before sending goal → zenoh discovery not completed.
    - **Fix**: Add 3s warm-up spin loop.
    - Native POSIX C + C++ action tests pass with strict assertions.
    - FreeRTOS QEMU tests: `#[ignore]` pending SDK availability for verification.
    - **Files**: `nros-c/src/executor.rs`, `examples/native/c/zenoh/action-{server,client}/src/main.c`, `examples/qemu-arm-nuttx/c/zenoh/action-server/src/main.c`, `nros-tests/tests/c_api.rs`
- [ ] 77.14 — Update documentation
    - C API reference: document new `executor` parameter on blocking functions
    - C++ API guide: document `SendGoalOptions`, `set_callbacks`, arena-based architecture
    - **Files**: `docs/reference/c-api-cmake.md`, `docs/guides/cpp-api.md`, `book/src/reference/c-api.md`
- [ ] 77.15 — Extend unified design to service client (`Client<S>`)
    - Service client blocking `call()` currently uses `zpico_get` directly
    - Refactor to `call_async` + executor spin (same pattern as action client)
    - This eliminates the last `zpico_get` usage in C/C++ client paths

## Acceptance Criteria

- [ ] Single `ActionClientCore` per action client, owned by the executor arena
- [ ] No `zpico_get` (blocking condvar) in any C/C++ action client path
- [ ] Blocking APIs spin the executor internally (like Rust `Promise::wait`)
- [ ] No user-side `poll()` calls needed — `spin_once` dispatches everything
- [ ] C header declarations match Rust FFI signatures
- [ ] `test_freertos_c_action_e2e` passes reliably
- [ ] `test_freertos_cpp_action_e2e` passes
- [ ] `just quality` passes
- [ ] Existing Rust action API unchanged

## Notes

- The Rust API already follows this pattern: `Promise::wait` spins the executor, `Promise::try_recv` is non-blocking poll, `Promise` implements `Future` with `AtomicWaker`.
- The blocking `zpico_get` should eventually be removed from ALL C/C++ client paths (service + action). 77.15 tracks extending the pattern to service clients.
- On POSIX/Zephyr, `spin_once` blocks efficiently on `g_spin_cv` condvar — woken by `_zpico_notify_spin`. On FreeRTOS+lwIP, `spin_once` uses `vTaskDelay`. Neither is busy-polling.
- The C++ action server deferred init (77.9) splits `nros_cpp_action_server_create` (metadata) from `nros_cpp_action_server_register` (transport handles). `Node::create_action_server` calls both sequentially.

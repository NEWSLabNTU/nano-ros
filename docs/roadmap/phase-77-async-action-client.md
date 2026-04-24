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
- [x] 77.9 — C++ action server deferred init (same pattern)
    - `nros_cpp_action_server_create` stores metadata only
    - `nros_cpp_action_server_register` creates transport handles via `add_action_server_raw`
    - Does NOT fix FreeRTOS QEMU deadlock — see 77.13 notes
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
    - **Bug 1 — Arena trampoline not registered**: Trampolines only registered when C callbacks were non-None at `nros_executor_add_action_client` time. Blocking wrappers install temporary callbacks AFTER registration → arena consumed replies without invoking trampoline → silent timeout.
    - **Fix**: Always register trampolines (they check the C struct at invocation time).
    - **Bug 2 — Missing `nros_executor_add_action_server()`**: Native and NuttX C action server examples lacked the call required after deferred init → server transport handles never created.
    - **Fix**: Add the call to both examples.
    - **Bug 3 — No warm-up spin**: Native C action client sent goal before zenoh discovery completed.
    - **Fix**: Add 3s warm-up spin loop.
    - **Bug 4 — CppActionServer storage overflow**: `build.rs` computed 4520-byte storage using host (x86_64) `usize` = 8, but ARM struct is 4996 bytes. `ptr::write` overflowed → HardFault.
    - **Fix**: Target-aware size calculation using `CARGO_CFG_TARGET_POINTER_WIDTH`.
    - **Test status**:
        - Native POSIX C + C++ action tests: **pass** with strict assertions
        - FreeRTOS C action E2E (`test_freertos_c_action_e2e`): **enabled**, passes (mildly flaky due to QEMU timing)
        - FreeRTOS C++ action E2E (`test_freertos_cpp_action_e2e`): **`#[ignore]`** — C++ action server hangs on 4th zenoh entity declaration (feedback publisher). C++ client works (confirmed with C server). Root cause: zenoh-pico mutex contention between app task declaring entities and background read/lease tasks on FreeRTOS QEMU.
    - **Files**: `nros-c/src/executor.rs`, `nros-cpp/build.rs`, `examples/native/c/zenoh/action-{server,client}/src/main.c`, `examples/qemu-arm-nuttx/c/zenoh/action-server/src/main.c`, `nros-tests/tests/c_api.rs`, `nros-tests/tests/freertos_qemu.rs`
- [ ] 77.14 — Update documentation
    - C API reference: document new `executor` parameter on blocking functions
    - C++ API guide: document `SendGoalOptions`, `set_callbacks`, arena-based architecture
    - **Files**: `docs/reference/c-api-cmake.md`, `docs/guides/cpp-api.md`, `book/src/reference/c-api.md`
- [x] 77.15 — Extend unified design to service client (`Client<S>`)
    - [x] C API `nros_client_call()` already uses async + executor spin (no `zpico_get`)
    - [x] C++ `Client<S>::call()` rewritten to use `send_request()` + `Future::wait(executor_, timeout, resp)`
    - [x] Added `executor_` field to `Client<S>`, set by `Node::create_client()`
    - [x] Removed deprecated `nros_cpp_service_client_call_raw` FFI declaration from C++ header
    - No `zpico_get` in any C/C++ client path (service or action)
- [x] 77.16 — FreeRTOS: replace `vTaskDelay` in `zpico_spin_once` with event-driven wake
    - Landed in commit `d2880d9a` (2026-04-21)
    - `zpico_spin_once` on FreeRTOS now waits on `xSemaphoreTake(g_spin_sem, pdMS_TO_TICKS(timeout_ms))`; the binary semaphore is signaled by `_zpico_notify_spin()` from the read task when application data arrives (subscriptions, query replies). Wake-up latency dropped from up to 10 ms/poll to near-zero.
    - Note: `zpico_poll` (distinct entry point) still uses `vTaskDelay` — out of scope for 77.16 since the executor drives everything through `spin_once` now.
    - **Files**: `packages/zpico/zpico-sys/c/zpico/zpico.c` (FreeRTOS block, lines 203–213 / 1170–1178)
- [x] 77.17 — NuttX: replace `usleep` in `zpico_spin_once` with condvar or semaphore wake
    - Use POSIX `sem_t` + `sem_timedwait` (analogous to the FreeRTOS binary-semaphore path from 77.16). The NuttX kernel's watchdog-backed pthread timed-wait is the bug source; POSIX `sem_timedwait` does not share that code path.
    - Implementation: new `g_spin_sem_posix` / `g_spin_sem_initialized` pair in `zpico.c`, init in `zpico_open` via `sem_init(&, 0, 0)`, destroy in `zpico_close` via `sem_destroy`, signal from the read task via `_zpico_notify_spin` → `sem_post`, and wait in `zpico_spin_once` with an absolute `CLOCK_REALTIME` deadline computed from `timeout_ms`. `EINTR` is retried; `ETIMEDOUT` is accepted. A residual `usleep` fallback handles the (shouldn't-happen) case where `sem_init` failed at session open.
    - Validated by `just nuttx test` + NuttX Rust/Cpp rtos_e2e pubsub/service/action tests (6/6 pass).
    - **Files**: `packages/zpico/zpico-sys/c/zpico/zpico.c` (NuttX notify helper + init/destroy + spin_once)
- [ ] 77.18 — Bare-metal: explore interrupt-driven network polling
    - Current: bare-metal (smoltcp/serial) `zpico_spin_once` busy-polls the network stack in a loop
    - Options to reduce CPU usage:
      - (a) WFI (wait-for-interrupt) between polls — CPU sleeps until NIC interrupt fires
      - (b) DMA completion callback — smoltcp poll triggered by NIC DMA transfer complete interrupt
      - (c) Hardware timer interrupt — poll at fixed interval (e.g., 1ms) instead of tight loop
    - Trade-off: interrupt-driven reduces power consumption but adds latency jitter; tight-loop gives lowest latency for real-time
    - Board crates would implement a platform-specific `wait_for_event(timeout_ms)` hook
    - **Files**: `packages/zpico/zpico-sys/c/zpico/zpico.c` (smoltcp/serial blocks), board crates
- [x] 77.19 — `nros-rmw-zenoh` ffi-sync poll loop: replace busy-loop with blocking `spin_once`
    - Was: `loop { ffi_guard(|| zpico_spin_once(0)); elapsed_ms?; }`,
      tight-looping a non-blocking 0-timeout spin until deadline or data.
    - Now: `loop { ffi_guard(|| zpico_spin_once(remaining_ms)); … }`.
      On POSIX/Zephyr/FreeRTOS/NuttX the event-driven wake from 77.16 /
      77.17 returns the inner call immediately on data arrival; on
      polled-smoltcp / polled-serial bare-metal the inner call's own
      tight loop handles the timeout (no second layer of busy-waiting).
    - Note: the critical section is now held for up to `remaining_ms`
      per iteration. The only active `ffi-sync` consumer today is
      bare-metal RTIC mixed-priority, and those examples always call
      `spin_once(0)` (single pass, loop body never fires), so this is
      benign. A future consumer calling with `timeout_ms > 0` on a
      cortex-m RTIC target would hold IRQs off for the duration and
      should split the wait from the zpico-state touch.
    - **Files**: `packages/zpico/nros-rmw-zenoh/src/zpico.rs`
- [x] 77.20 — Deprecate / retire `zpico_poll()` FreeRTOS fixed-delay path
    - Option (a): deleted outright. Audit showed the only callers were
      `nros-rmw-zenoh`'s internal `Context::poll` / `Session::poll`
      wrappers and the `zpico-sys/ffi.rs` stub — nothing external relied
      on the "read-only, no keep-alive" semantics. Deletion sites:
      - `packages/zpico/zpico-sys/c/zpico/zpico.c` — dropped the
        `zpico_poll()` function body (the one with `vTaskDelay(timeout_ms)`
        on FreeRTOS+lwIP and friends).
      - `packages/zpico/zpico-sys/src/{lib.rs,ffi.rs}` — removed the
        extern decl and the no-platform stub.
      - `packages/zpico/zpico-sys/cbindgen.toml` — removed from the export
        allow-list (regenerated `zpico.h` no longer declares it).
      - `packages/zpico/nros-rmw-zenoh/src/zpico.rs` — deleted
        `Context::poll` and dropped the `zpico_poll` import.
      - `packages/zpico/nros-rmw-zenoh/src/shim/session.rs` — deleted
        `Session::poll`.
    - Callers use `zpico_spin_once` directly via `RmwSession::drive_io`.
- [x] 77.21 — ThreadX task "join": replace 1 ms polling loop with `tx_event_flags`
    - `packages/zpico/zpico-sys/c/platform/threadx/platform.h` — added a
      `TX_EVENT_FLAGS_GROUP done_flags` field to `_z_task_t`.
    - `packages/zpico/zpico-sys/c/platform/threadx/task.c`:
      - `_z_task_init` calls `tx_event_flags_create(&task->done_flags,
        "zdone")` before `tx_thread_create`; cleanup on failure via
        `tx_event_flags_delete`.
      - `_z_task_trampoline` signals `_Z_TASK_DONE_FLAG` (bit 0) via
        `tx_event_flags_set(..., TX_OR)` after the user `_fun` returns.
      - `_z_task_join` waits with `tx_event_flags_get(..., TX_OR_CLEAR,
        ..., TX_WAIT_FOREVER)` — one system call, true event-driven
        wake, no polling.
      - `_z_task_free` deletes the flags group before `z_free`.
    - Tested against all 6 ThreadX-RV64 rtos_e2e cases
      (Rust/C × pubsub/service/action).
- [ ] 77.22 — Introduce `nros_platform::Yield` trait to unify per-platform yield
      fallbacks used inside `socket_wait_event`
    - Current: three near-identical hand-written 1-tick / 1 ms yields in
      the platform shims, each with different units and no common home:
      - `packages/core/nros-platform-posix/src/net.rs:487–495` — `libc::usleep(1000)`
      - `packages/core/nros-platform-freertos/src/net.rs:538–550` — `vTaskDelay(1)`
      - `packages/core/nros-platform-threadx/src/net.rs:430` — `tx_thread_sleep(1)` (+ `src/ffi.rs:37–38`)
      - (NuttX & Zephyr are close cousins — `select(.., 1 ms)` and
        `k_usleep(1 ms)` respectively; roll them in at the same time)
      Callers are not waiting for I/O readability — the background read
      task handles that — they're just "let the scheduler run so the
      real waiter can make progress". The intent is a *yield*, not a
      timed sleep.
    - Target: add a `PlatformYield` capability alongside `PlatformSleep`
      in `packages/core/nros-platform/src/traits.rs`. Backend mapping
      (survey result — all 7 platforms have a primitive):

      | Backend             | Primitive                       | Header / API                |
      |---------------------|---------------------------------|-----------------------------|
      | POSIX               | `sched_yield()`                 | `<sched.h>` → `c_int`       |
      | NuttX               | `sched_yield()`                 | POSIX-compliant             |
      | Zephyr              | `k_yield()`                     | `<zephyr/kernel.h>`         |
      | FreeRTOS            | `vPortYield()` (C shim for macro `taskYIELD()`) | `task.h` |
      | ThreadX             | `tx_thread_relinquish()`        | `tx_api.h`                  |
      | Bare-metal default  | `core::hint::spin_loop()`       | Rust intrinsic (no FFI)     |
      | Bare-metal opt-in   | `cortex_m::asm::wfi()`          | via `BoardIdle` trait, board-crate opt-in |

    - **Key subtlety — bare-metal has no real yield**: there's nothing
      to yield *to*. `core::hint::spin_loop()` is a CPU hint (emits
      `YIELD`/`PAUSE`/`WFE` per arch) — safe everywhere. `wfi` is deep
      idle and requires a live IRQ source to wake the CPU; enabling it
      on a board that hasn't armed an ethernet/timer IRQ deadlocks.
      Therefore `PlatformYield` default is `spin_loop()`, and boards
      that know their IRQ story override via a separate `BoardIdle`
      hook. Precedent already exists:
      - WFI usage (board-layer opt-in):
        `packages/boards/nros-stm32f4/src/node.rs:96,167,173,190`,
        `packages/boards/nros-mps2-an385/src/lib.rs:64,73`,
        `packages/boards/nros-mps2-an385-freertos/build.rs:401,410,671`
      - `spin_loop()` usage (safe default, proven on ESP32):
        `packages/boards/nros-esp32/src/node.rs:126,136,151,157,163,170,252,370`,
        `packages/boards/nros-esp32-qemu/src/node.rs:255`
    - **Safety**: none of the RTOS yields are ISR-safe
      (`tx_thread_relinquish`, `k_yield`, `taskYIELD` all panic /
      error from ISR). Document the constraint on the trait; add a
      debug-only `debug_assert!(!in_isr())` where backends can detect
      ISR context.
    - **Benefits**: single tuning point; removes three unexplained
      magic numbers; aligns with existing `PlatformSleep` shape; gives
      77.18 a natural home (board crate implements `BoardIdle::wait`
      with `wfi` + smoltcp `poll_delay()`).
    - **Files**: new `PlatformYield` trait in
      `packages/core/nros-platform/src/traits.rs` (model after
      `PlatformSleep` at line 50) + per-backend impls in each
      `nros-platform-*` crate; call sites in `net.rs` under each
      platform.

- [x] 77.23 — C++ action/service: `send_goal` returns -1 before the
      server callback runs — root cause was 0-byte opaque storage
      (surfaced by Phase 89.3 triage)
    - **Surface**: `native_api::test_cpp_action_communication` was the
      lead repro; the C++ `lang_3` variants of
      `rtos_e2e::test_rtos_{action,service}_e2e` and
      `zephyr::test_zephyr_cpp_action_server_to_client_e2e` are
      likely aliases that will need re-measurement.
    - **Root cause**: `NROS_CPP_ACTION_SERVER_STORAGE_SIZE` and
      `NROS_CPP_ACTION_CLIENT_STORAGE_SIZE` in
      `nros_cpp_config_generated.h` were both **0**. The C++
      `ActionServer<A>` / `ActionClient<A>` classes define
      `alignas(8) uint8_t storage_[NROS_CPP_ACTION_*_STORAGE_SIZE]`,
      so `storage_` became a zero-size array and the next field
      (`executor_` / `user_goal_fn_ctx_`) aliased where Rust wrote
      the `CppActionServer` / `CppActionClient` struct. When
      `install_callbacks()` later called
      `nros_cpp_action_server_set_callbacks(storage_, &goal_trampoline, …)`,
      `goal_trampoline` got written into the same byte range as
      `user_goal_fn_ctx_`, so the trampoline recursed on itself
      instead of calling the user's `on_goal` — the arena saw
      `accepted=false` and the client's blocking wrapper returned -1.
      (Debug printed `user_fn_ctx=0x55555555f37a`, which nm resolved
      to `goal_trampoline` at `0xb37a + base_0x555555554000`.)
    - **Fix**: two-part patch (one cosmetic, one substantive):
      - `packages/core/nros-sizes-build/src/lib.rs` — the probe's
        `cargo_target_dir()` was ignoring Corrosion's
        `--target-dir`, so it looked under `target/...` while the
        rlib actually lived under
        `build/cmake-zenoh/cargo/nano-ros_*/...`. Fixed by deriving
        the target dir from `OUT_DIR.ancestors().nth(5)`
        (`<target>/<triple>/<profile>/build/<pkg-hash>/out`) when
        `CARGO_TARGET_DIR` is unset and falling back to
        `cargo metadata`. This makes the probe reach the correct
        rlib, but its `extract_sizes` still returns nothing because
        the workspace's fat-LTO profile emits **bitcode-only**
        rlibs (`*.rcgu.o` is LLVM IR, not ELF; `object::parse`
        errors out). Cargo rejects per-package `lto` overrides, so
        the probe still can't extract byte sizes at build time —
        captured as 77.24 below.
      - `packages/core/nros-cpp/build.rs` — corrected the
        hand-math fallbacks (previously `ptr_bytes * 4` for the
        server, which collapses `Option<ActionServerRawHandle>` to
        a single pointer; the real struct is 6-ptr
        `ActionServerRawHandle` + 3-ptr tail = 9*ptr = 72 bytes on
        64-bit). The client fallback was already correct
        (`5*ptr + 8` = 48 on 64-bit).
    - **Result**: `test_cpp_action_communication` now passes 3/3
      consecutive runs at ~4.5 s (was failing at 7.7 s with
      Rust `cpp_arena_core_mut` returning `None` because
      `arena_entry_index` and `executor_ptr` were being read from
      the aliased `/fibonacci` topic-name buffer).
    - **Files**:
      - `packages/core/nros-sizes-build/src/lib.rs`
      - `packages/core/nros-cpp/build.rs`
- [x] 77.24 — Guard against the silent-zero-probe landmine
      (stopgap; true LTO-resilient probe still pending)
    - **Context**: the release profile (`lto = true`,
      `codegen-units = 1`) makes rustc emit only LLVM bitcode for
      rlib member objects. `object::parse` can't read bitcode, so
      `extract_sizes` silently returns an empty map for every
      downstream consumer (`nros-cpp`, `nros-c`,
      `zpico-platform-shim`) while the rlib itself is found. Hand-
      math fallbacks cover this in `nros-cpp/build.rs` today
      (77.23), but **every `NROS_*_SIZE` macro in
      `nros_config_generated.h` (nros-c) is 0** — the consequence
      is that `_Alignas(8) uint8_t _opaque[NROS_PUBLISHER_SIZE]` in
      `publisher.h` (and friends) is a zero-size flexible array,
      which would corrupt adjacent memory if C code ever depended
      on that storage. Today no C pubsub test trips it because the
      RMW publisher is small enough that nothing downstream reads
      past the end of the `nros_publisher_t` struct, but this is
      a landmine.
    - **Options** (pick one):
      - (a) Make probe failure a hard build error in every
        consumer's `build.rs`. That mirrors what 87.10 did for
        `zpico-platform-shim` and forces the issue to surface at
        build time rather than runtime. Downside: CI (and `just
        test-all`) breaks everywhere until a probe fix lands.
      - (b) In `nros-sizes-build`, add a bitcode-aware extraction
        path — either invoke `rustc --print sysroot`'s bundled
        `llvm-nm` (which can parse the bitcode; confirmed) and
        parse its textual output, or use `llvm-sys` to walk the
        module. Symbol *sizes* aren't encoded in bitcode at
        `llvm-nm` granularity, so we'd need a different channel
        (see (c)).
      - (c) Encode the size in the *symbol name* rather than
        symbol storage: e.g. add an auxiliary
        `__NROS_SIZE_PUBLISHER_AT_<size>_MARKER` static alongside
        the existing one, and have the probe parse `<size>` out of
        the name. Symbol names survive LTO, so this works even
        with the current bitcode rlib.
      - (d) Invoke `cargo rustc -p nros -- -C lto=off
        --emit=obj=<path>` from consumer build scripts to get a
        real ELF object for size extraction. Correct but doubles
        the `nros` compile for every build.
    - **Recommended order**: (a) first so the latent bug surfaces
      in CI, then (c) (lowest-impact workaround, doesn't touch the
      workspace LTO setting).
    - **Landed (stopgap)**: `nros-c/build.rs` and `nros-cpp/build.rs`
      now gate the `include/nros/*_config_generated.h` write on
      `probe_executor != 0`. When the probe silently returns 0 for
      every entry (the LTO-bitcode failure mode) the committed
      header is preserved and a `cargo:warning=` is emitted
      explaining the situation; when no committed header exists the
      build panics with a message directing the user to a non-LTO
      profile. This closes the landmine (zeros never land in
      `_Alignas(8) uint8_t _opaque[NROS_*_SIZE]`) without forcing an
      immediate LTO-policy change. A real fix — option (c)
      (symbol-name-encoded sizes) or option (d) (dedicated non-LTO
      probe build) — is still open and should be filed as 77.25 once
      someone has the bandwidth.
    - **Files**: `packages/core/nros-c/build.rs`,
      `packages/core/nros-cpp/build.rs`.

- [x] 77.25 — LTO-resilient size probe via v0 mangling + build-dep
      ordering
    - **Context**: 77.24 only protects the committed header from
      being clobbered — the probe itself still returns 0 under
      `lto = true`. That means every CI build emits
      `cargo:warning=` spam and any size drift between
      the live Rust types and the committed macros goes
      unnoticed until a runtime crash (the 77.23 failure mode).
    - **Approach** (option (c) from 77.24, stable-Rust only):
      - Switch `export_size!` in `nros/src/sizes.rs` from a static
        `[u8; N]` (size lives only in the symbol's object-file size,
        which LTO bitcode drops) to a monomorphized function
        reference: `pub fn marker<const N: usize>() {}` + a
        `#[used] static FOO: fn() = marker::<{ size_of::<T>() }>`.
        With v0 symbol mangling the instantiated function's mangled
        name contains `Kj<hex>_` for the const-generic value — that
        survives LTO because the linker needs the symbol name.
      - Add a workspace-wide `.cargo/config.toml` setting
        `rustflags = ["-C", "symbol-mangling-version=v0"]`. v0 is
        the forward-looking default (stable since 1.60); the only
        user-visible change is prettier demangled output in
        backtraces.
      - In `nros-sizes-build::extract_sizes`, shell out to rustc's
        bundled `llvm-nm --demangle` (at
        `$(rustc --print sysroot)/lib/rustlib/$TRIPLE/bin/llvm-nm`)
        against the rlib. The demangled line looks like
        `nros::sizes::marker::<48>` — a single regex captures the
        const value. Fall back to the current `object::parse`
        path first so non-LTO builds stay on the fast path.
    - **Non-goals**: no change to the workspace LTO setting
      itself; no new build-time sub-compilation of `nros`.
    - **Files** (expected):
      - `.cargo/config.toml` (new)
      - `packages/core/nros/src/sizes.rs` (macro rewrite)
      - `packages/core/nros-sizes-build/src/lib.rs`
        (bitcode-aware extraction path)
    - **Landed**:
      - `.cargo/config.toml` — `rustflags = ["-C",
        "symbol-mangling-version=v0"]` workspace-wide.
      - `packages/core/nros/src/sizes.rs` — `export_size!` now
        also emits a `fn __nros_size_NAME<const N: usize>()` +
        `#[used] static __NROS_SIZE_FN_NAME: fn() = ...::<{$name}>`
        pair alongside the legacy `[u8; N]` static. The
        monomorphisation's v0-mangled symbol name contains both
        the NAME and the const-generic SIZE value.
      - `packages/core/nros-sizes-build/src/lib.rs` — when
        `object::parse` can't read an rlib member and the member
        starts with bitcode magic (`BC\xC0\xDE`), fall back to
        shelling out to rustc's bundled `llvm-nm --demangle` and
        regex-parse `NAME::<SIZE>` out of the demangled output.
        Also: cross-compile safety — only search the host deps
        dir when `TARGET == HOST`, so an embedded ARM build
        doesn't accidentally consume host pointer widths.
      - `packages/core/nros-cpp/Cargo.toml` and
        `packages/core/nros-c/Cargo.toml` — add `nros` to
        `[build-dependencies]` so its rlib exists when the probe
        runs (regular deps compile in parallel with build scripts,
        which left the rlib missing on clean builds).
    - **Verified**: clean Corrosion build of `cargo-build_nros_cpp`
      now prints no probe warnings, and `NROS_EXECUTOR_SIZE=16784`
      /  `NROS_PUBLISHER_SIZE=48` (etc.) land correctly.
      `test_cpp_action_communication` still passes (~4.5 s).

## Acceptance Criteria

- [x] Single `ActionClientCore` per action client, owned by the executor arena
- [x] No `zpico_get` (blocking condvar) in any C/C++ action client path
- [x] Blocking APIs spin the executor internally (like Rust `Promise::wait`)
- [x] No user-side `poll()` calls needed — `spin_once` dispatches everything
- [x] C header declarations match Rust FFI signatures
- [x] `test_freertos_c_action_e2e` passes
- [ ] `test_freertos_cpp_action_e2e` passes — blocked on C++ server entity declaration deadlock
- [x] `just ci` passes
- [x] Existing Rust action API unchanged

## Known Issue: C++ Action Server Deadlock on FreeRTOS QEMU

`nros_cpp_action_server_register` → `add_action_server_raw` declares 5 zenoh entities in sequence (3 queryables + 2 publishers). On FreeRTOS QEMU, the 4th declaration (`create_publisher` for feedback) deadlocks.

**Observed behavior** (C++ action server, zenoh-pico debug output):
```
queryable[0] send_goal ... OK
queryable[1] cancel_goal ... OK
queryable[2] get_result ... OK
declare_pub feedback (slot 0) ... OK
declare_pub status ... <hangs>          ← 5th entity deadlocks
```

**Trigger**: QEMU `-icount shift=auto`. Without this flag, all 5 entities declare successfully. With it, the 5th declaration (`z_declare_publisher` for status) deadlocks.

**Root cause**: `-icount shift=auto` synchronizes the virtual CPU clock with wall time, changing the FreeRTOS scheduling behavior. The background read/lease tasks (started by `zpico_open`) hold the zenoh-pico session mutex for longer periods (because `vTaskDelay` actually waits at wall-clock speed). By the time the 5th `z_declare_publisher` call tries to acquire the session mutex, the background tasks are holding it — and the app task waits indefinitely because the FreeRTOS scheduler on QEMU's single-threaded emulation doesn't preempt them.

**Why C server usually works**: Same `add_action_server_raw` function, same 5 entities. The C binary is slightly smaller/faster, which shifts the timing window. The deadlock is timing-dependent — C sometimes hits it too (flaky).

**Why Rust server always works**: The Rust binary goes through the same `add_action_server_raw_sized` call. It may have different binary layout or different stack/memory access patterns that change the timing just enough. Confirmed: Rust action server with `-icount shift=auto` declares all 5 entities reliably.

**Key insight**: The `-icount shift=auto` flag is required for correct slirp networking timing in E2E tests. Without it, lwIP timers run too fast and network I/O breaks.

### Detailed Investigation

**Mutex architecture** in zenoh-pico unicast transport:

| Mutex | Scope | Held by |
|-------|-------|---------|
| `_mutex_tx` | TX buffer + wire send | App task (entity declarations via `_z_send_n_msg(BLOCK)`), Lease task (keep-alives via `_z_transport_tx_send_t_msg`) |
| `_mutex_rx` | RX buffer | Read task (held for entire loop: line 330→373 of `read.c`) |
| `_mutex_inner` | Session resource table | Any task registering resources |

**Each `z_declare_publisher` sends TWO messages** (a declaration + a write filter interest). Each message acquires `_mutex_tx` with `Z_CONGESTION_CONTROL_BLOCK`. So declaring 5 entities = 10 TX mutex acquisitions.

**FreeRTOS mutex implementation**: `xSemaphoreCreateRecursiveMutex()` — includes priority inheritance.

**Task priorities** (CMake C++ binary — the failing one):

| Task | Priority | Notes |
|------|----------|-------|
| lwIP tcpip_thread | 4 | Set in `lwipopts.h` |
| Zenoh read task | 4 | Default: `configMAX_PRIORITIES/2` = 4 (CMake never calls `zpico_set_task_config`) |
| Zenoh lease task | 4 | Same default |
| Network poll task | 4 | Set in `startup.c` |
| **App task** | **3** | **Lower than all background tasks** |

**Priority inheritance has no effect** because the mutex holder (lease task, priority 4) is already higher than the waiter (app task, priority 3).

**Rust binary priorities** (for comparison — different mapping via `to_freertos_priority()`):
- App: 2, Zenoh read/lease: 3, Poll: 3. Same relative order — app is lower. But the Rust binary works because the entity declarations complete before the first keep-alive timer fires.

**Socket timeout**: `SO_RCVTIMEO = 100ms` on the TCP socket. The read task blocks in `lwip_recv()` for up to 100ms, yielding CPU. Not a busy-wait.

**Network poll**: 1ms interval (`POLL_INTERVAL_MS = 1`), reads from LAN9118 NIC.

**Lease keep-alive interval**: `Z_TRANSPORT_LEASE / Z_TRANSPORT_LEASE_EXPIRE_FACTOR` = `10000 / 3` ≈ 3333ms.

**GDB finding**: at the point of deadlock, the CPU is in `lwip_recv_tcp()` → `netconn_recv_data_tcp()` (read task blocked in recv). Other task states could not be determined from single-core GDB.

**Hypothesized deadlock sequence**:
1. Session opens, read/lease tasks start (~5s wall time with `-icount`)
2. App task (priority 3) begins declaring entities
3. After ~3.3s (keep-alive interval), lease task wakes and acquires `_mutex_tx` for keep-alive
4. lwIP `send()` blocks waiting for TCP window (posts to tcpip_thread, waits on semaphore)
5. App task blocks on `_mutex_tx` (waiting for lease task)
6. **Issue**: the TCP ACK for the keep-alive arrives via LAN9118 → poll task → tcpip_thread → unblocks lease → releases `_mutex_tx`. This chain SHOULD work but may be disrupted by `-icount shift=auto` timing distortion.

**Key unanswered question**: Why does the chain in step 6 fail with `-icount`? Possible causes:
- The poll task's 1ms delay interacts badly with `-icount`'s instruction-count-based time
- The tcpip_thread's internal timers (TCP retransmit, delayed ACK) fire at wrong intervals
- QEMU slirp processes network I/O in its main loop, which with `-icount` may not align with the guest's TCP expectations

### Experiments Performed

1. **Priority fix attempted**: Set zenoh read/lease tasks to priority 2 (below app task at 3) via `zpico_set_task_config`. **Still deadlocks.** Rules out priority inversion — the issue is not which task runs first.

2. **GDB backtrace**: CPU is in `lwip_recv_tcp()` → `netconn_recv_data_tcp()` (read task blocked in recv). This is expected — the read task yields when no data.

3. **Batching analysis**: `Z_FEATURE_BATCHING=1` but `_batch_state` starts as `IDLE` (no one calls `zp_batch_start`). Messages flush immediately via `_z_transport_tx_flush_buffer` → `_z_link_send_wbuf` → `lwip_send()`.

**The actual blocking call**: `lwip_send()` inside `_z_transport_tx_flush_buffer`, while the app task holds `_mutex_tx`. The TCP send blocks waiting for the tcpip_thread to process the segment. This SHOULD complete in ~10ms. The question is why it doesn't.

### Confirmed Root Cause

**Experiment**: disabled `zp_start_lease_task` in zpico.c. Result: all 5 entities declare reliably with `-icount shift=auto`. **The lease task causes the deadlock.**

The mechanism: the lease task's keep-alive send (`_zp_unicast_send_keep_alive` → `_z_transport_tx_send_t_msg` → `_z_mutex_lock(&_mutex_tx)`) contends with the entity declaration send on the same `_mutex_tx`. With `-icount shift=auto`, the virtual clock runs at wall-clock speed, making the keep-alive timer fire during the entity declaration sequence. The lease task acquires `_mutex_tx` for its keep-alive, and `lwip_send()` blocks for a wall-clock TCP round-trip. Meanwhile the app task waits for `_mutex_tx`. The combined effect: each entity declaration takes longer, which pushes subsequent declarations past more keep-alive intervals, creating a cascading delay that eventually leads to a complete stall.

This is NOT a classical deadlock (two mutexes in opposite order). It is a **livelock/starvation** scenario: the lease task periodically holds `_mutex_tx` for wall-clock TCP round-trip durations, and the app task can never complete all 5 declarations between keep-alive intervals.

### Fix Applied: Non-blocking keep-alive (zenoh-pico)

Changed the lease task's keep-alive send to use `_z_transport_tx_try_send_t_msg` (non-blocking try-lock on `_mutex_tx`). If the TX mutex is held by the app task during declarations, the keep-alive is skipped — declaration sends prove liveness.

**Result**: Eliminates the keep-alive contention but the C++ server still hangs.

### True Root Cause: Read Task Holds TX Mutex During `lwip_send()`

**Diagnostic output** (10 TCP sends traced):
```
send(0, 103 bytes)... → returned 103    ← 9th send (publisher declaration) OK
send(0, 103 bytes)... → <hangs>         ← 10th send (interest message) never returns
```

The 10th send hangs inside `_z_transport_tx_mutex_lock(BLOCK)` — the TX mutex is already held.

**Who holds `_mutex_tx`?** The **read task** (priority 4). The read task processes incoming responses from the router. When it receives a request (e.g., interest reply with `_Z_REQUEST_PUT`), `session/rx.c:123` sends a `response_final` via `_z_send_n_msg(BLOCK)`, which acquires `_mutex_tx`. The read task's `lwip_send()` then blocks waiting for TCP completion.

**The complete lock chain:**
1. Read task holds `_mutex_rx` (held for entire task lifetime, line 330 of `read.c`)
2. Read task processes incoming router message → sends response_final → acquires `_mutex_tx`
3. Read task's `lwip_send()` blocks (TCP send through slirp takes wall-clock time with `-icount`)
4. App task (priority 3) tries to send interest message → `_z_transport_tx_mutex_lock(BLOCK)` → **blocked by read task**

This is NOT a classical deadlock (no circular dependency). It's a **priority inversion with I/O dependency**: the read task (higher priority) holds `_mutex_tx` while blocked in `lwip_send()`, starving the app task. The `lwip_send()` blocks because the tcpip_thread doesn't process the write request in time under `-icount shift=auto`.

**Why the Rust binary works**: the Rust binary completes all declarations before the router has time to send responses that trigger `response_final` sends. The C++ binary is slower (larger code), giving the router enough time to send responses during the declaration sequence.

### Deeper: Why `lwip_send()` Blocks Indefinitely

**lwIP diagnostic trace** (printf in `tcpip_send_msg_wait_sem` and `tcpip_thread_handle_msg`):

```
tick=2865: mbox_post fn=WRITE          ← 10th send posted to tcpip_mbox
tick=2869: tcpip API fn=WRITE          ← tcpip_thread picks up the WRITE
tick=2878: tcpip API done fn=WRITE     ← lwip_netconn_do_write returns
                                          (NO sem_done — semaphore NOT signaled)
tick=2892: mbox_post fn=RECV           ← read task posts recv
tick=2911: sem_wait fn=WRITE           ← write caller blocks forever
tick=5333: next recv (2.4s later)      ← system continues for recv
```

**Finding**: `lwip_netconn_do_write` called `do_writemore` → `tcp_write()` returned `ERR_MEM`. Despite `SO_SNDTIMEO=100ms`, the timeout check in `do_writemore` (line 1667) only fires on RE-INVOCATION by `sent_tcp` callback — not on the initial call. The function returns without signaling the semaphore, waiting for `sent_tcp` to retry.

`sent_tcp` fires when a TCP ACK arrives (freeing send buffer space). The ACK delivery chain: QEMU slirp → LAN9118 NIC → poll task (every 1ms) → `tcpip_input` → tcpip_thread. With `-icount shift=auto`, this chain doesn't complete because QEMU's main loop (which processes slirp I/O) runs during WFI, and the relationship between virtual ticks and main loop I/O is imprecise.

### True Root Cause: Shared `conn->op_completed` Semaphore

Further investigation revealed that `tcp_write` actually returns `ERR_OK` (not `ERR_MEM`) and `write_finished=1` on every send. The semaphore IS signaled by `do_writemore`. **The problem is that the semaphore signal is consumed by the WRONG task.**

lwIP's `netconn` uses a single `conn->op_completed` semaphore per connection. When `LWIP_NETCONN_SEM_PER_THREAD=0` (default), ALL tasks sharing the same TCP socket use the SAME semaphore:
- The read task calls `lwip_recv()` → waits on `conn->op_completed`
- The app task calls `lwip_send()` → waits on `conn->op_completed`

When the tcpip_thread signals `conn->op_completed` for the app task's write completion, the read task's pending `lwip_recv()` may consume the signal instead (or vice versa). This causes **lost wakeups** — the app task's semaphore signal is stolen by the read task.

Without `-icount shift=auto`, the timing is fast enough that both tasks complete before the race becomes visible. With `-icount`, the wall-clock timing makes the race deterministic.

### Fix: `LWIP_NETCONN_SEM_PER_THREAD=1`

Enable per-thread semaphores in lwIP so each FreeRTOS task gets its own semaphore for netconn operations. This eliminates the shared semaphore race.

**Requirements**:
1. `lwipopts.h`: `#define LWIP_NETCONN_SEM_PER_THREAD 1`
2. `FreeRTOSConfig.h`: `#define configNUM_THREAD_LOCAL_STORAGE_POINTERS 1`
3. Each task that uses sockets must call `lwip_socket_thread_init()` before first use
4. The lwIP FreeRTOS port's `sys_arch_netconn_sem_alloc()` uses `mem_malloc` (lwIP heap) which may fail if the heap is exhausted. Patch to use `pvPortMalloc` (FreeRTOS heap) instead.
5. `MEM_SIZE` or `configTOTAL_HEAP_SIZE` may need increasing to accommodate the per-thread semaphores.

**Status**: Fixed. The `sem != NULL` assertion was from `netifapi_netif_add()` in the network init — called BEFORE any zenoh-pico code, so the per-thread sem wasn't initialized yet. Fix: call `lwip_socket_thread_init()` at the start of `nros_freertos_init_network()`, before `tcpip_init()`. Also patched the lwIP FreeRTOS port to use `pvPortMalloc` instead of `mem_malloc` (lwIP heap was full).

**Verified**: C++ action server declares all 5 entities reliably with `-icount shift=auto` (3/3 trials).

## Notes

- The Rust API already follows this pattern: `Promise::wait` spins the executor, `Promise::try_recv` is non-blocking poll, `Promise` implements `Future` with `AtomicWaker`.
- The blocking `zpico_get` should eventually be removed from ALL C/C++ client paths (service + action). 77.15 tracks extending the pattern to service clients.
- On POSIX/Zephyr, `spin_once` blocks efficiently on `g_spin_cv` condvar — woken by `_zpico_notify_spin`. On FreeRTOS+lwIP, `spin_once` uses `vTaskDelay`. Neither is busy-polling.
- The C++ action server deferred init (77.9) splits `nros_cpp_action_server_create` (metadata) from `nros_cpp_action_server_register` (transport handles). `Node::create_action_server` calls both sequentially.
- `CppActionServer` storage size is now target-aware via `CARGO_CFG_TARGET_POINTER_WIDTH` in `build.rs`. Compile-time assertions validate correctness on every build.

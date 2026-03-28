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
- [ ] 77.15 — Extend unified design to service client (`Client<S>`)
    - Service client blocking `call()` currently uses `zpico_get` directly
    - Refactor to `call_async` + executor spin (same pattern as action client)
    - This eliminates the last `zpico_get` usage in C/C++ client paths

## Acceptance Criteria

- [x] Single `ActionClientCore` per action client, owned by the executor arena
- [x] No `zpico_get` (blocking condvar) in any C/C++ action client path
- [x] Blocking APIs spin the executor internally (like Rust `Promise::wait`)
- [x] No user-side `poll()` calls needed — `spin_once` dispatches everything
- [x] C header declarations match Rust FFI signatures
- [x] `test_freertos_c_action_e2e` passes
- [ ] `test_freertos_cpp_action_e2e` passes — blocked on C++ server entity declaration deadlock
- [x] `just quality` passes
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

### Fix Options

1. **Suppress keep-alives during entity declaration** — set `_transmitted = true` before the declaration batch. The lease task skips keep-alives when `_transmitted` is true (line 107 in lease.c). After declarations complete, reset it. This is the minimal targeted fix.
2. **Stop/restart the lease task around declarations** — `zp_stop_lease_task` before, `zp_start_lease_task` after. More disruptive but guaranteed.
3. **Increase lease timeout for FreeRTOS** — increase `Z_TRANSPORT_LEASE` from 10s to 30s. Gives more headroom but doesn't fix the root cause.

## Notes

- The Rust API already follows this pattern: `Promise::wait` spins the executor, `Promise::try_recv` is non-blocking poll, `Promise` implements `Future` with `AtomicWaker`.
- The blocking `zpico_get` should eventually be removed from ALL C/C++ client paths (service + action). 77.15 tracks extending the pattern to service clients.
- On POSIX/Zephyr, `spin_once` blocks efficiently on `g_spin_cv` condvar — woken by `_zpico_notify_spin`. On FreeRTOS+lwIP, `spin_once` uses `vTaskDelay`. Neither is busy-polling.
- The C++ action server deferred init (77.9) splits `nros_cpp_action_server_create` (metadata) from `nros_cpp_action_server_register` (transport handles). `Node::create_action_server` calls both sequentially.
- `CppActionServer` storage size is now target-aware via `CARGO_CFG_TARGET_POINTER_WIDTH` in `build.rs`. Compile-time assertions validate correctness on every build.

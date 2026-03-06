# Phase 48 — Async Service API

## Status: Complete

### Progress

| Work Item | Description                                                 | Status   |
|-----------|-------------------------------------------------------------|----------|
| 48.1      | Non-blocking RMW trait methods                              | Done     |
| 48.2      | XRCE-DDS non-blocking service client                        | Done     |
| 48.3      | Zenoh non-blocking service client (C shim)                  | Done     |
| 48.4      | Zenoh non-blocking service client (Rust)                    | Done     |
| 48.5      | Promise type                                                | Done     |
| 48.6      | `call()` → Promise API                                      | Done     |
| 48.7      | Async spin (`spin_async`)                                   | Done     |
| 48.8      | Typed RMW wrappers (`send_request<S>`, `try_recv_reply<S>`) | Done     |
| 48.9      | Action client Promise support                               | Done     |
| 48.10     | Tests (unit + integration)                                  | Done     |
| 48.11     | Migrate examples to Promise pattern                         | Done     |
| 48.12     | New async example                                           | Done     |
| 48.13     | Remove blocking methods                                     | Done     |
| 48.14     | Documentation updates                                       | Done     |
| 48.15     | Remove `block_on` from nros API                             | Done     |
| 48.16     | Revise native async example (tokio background spin)         | Done     |
| 48.17     | New Zephyr async example (Embassy background spin)          | Done     |
| 48.18     | Documentation updates (remove `block_on` references)        | Done     |

### Completed

- **48.1**: Added `send_request_raw()` and `try_recv_reply_raw()` as required methods on `ServiceClientTrait` in `nros-rmw/src/traits.rs`
- **48.2**: Implemented non-blocking methods for `XrceServiceClient`, refactored `call_raw()` to use them
- **48.3**: Added `zenoh_shim_get_start()`/`zenoh_shim_get_check()` C shim with static pending-get slot array, FFI bindings in `zpico-sys`. Fixed `z_owned_bytes_t` lifetime bug (use-after-free when payload_bytes went out of scope before `z_get()` consumed it)
- **48.4**: Wrapped C shim in `ShimContext::get_start()`/`get_check()`, implemented `send_request_raw`/`try_recv_reply_raw` for `ShimServiceClient` with `pending_handle` field
- **48.5+48.6**: Added `Promise<'a, T, Cli>` with `try_recv()` and `Future` impl; renamed `call()` → `call_blocking()`, new `call()` returns `Promise`
- **48.7**: Added `spin_async()` method on `Executor` using `core::future::poll_fn` (no external deps)
- **48.8**: Added `send_request<S>()` and `try_recv_reply<S>()` default typed methods on `ServiceClientTrait`
- **48.9**: Added non-blocking `send_goal()`, `cancel_goal()`, `get_result()` returning Promises on `EmbeddedActionClient`; renamed old methods to `_blocking` suffix; arena-based `ActionClientHandle` stays blocking
- **48.10**: Unit tests (2 Promise tests + 296 total pass), integration tests (154 pass including service and action tests exercising Promise pattern)
- **48.11**: All 7 example files migrated from `_blocking` calls to Promise+`spin_once()`+`try_recv()` pattern (3 service clients, 3 action clients, fairness-bench keeps `call_blocking` in subprocesses)
- **CFFI fallback**: `CffiServiceClient` stores request in buffer, falls back to blocking `call_raw()` for `try_recv_reply_raw()`
- **Re-exports**: `Promise` re-exported from `nros-node` and `nros`
- ~~**48.12 — New async example**~~ Done: `examples/native/rust/zenoh/async-service-client/` — to be revised in 48.16
- ~~**48.13 — Remove blocking methods**~~ Done: removed `call_blocking()` from `EmbeddedServiceClient`; downgraded `send_goal_blocking`/`cancel_goal_blocking`/`get_result_blocking` to `pub(crate)` on `EmbeddedActionClient` (arena code still needs them internally); migrated fairness-bench's remaining `call_blocking` usages to Promise pattern
- ~~**48.14 — Documentation updates**~~ Done: updated `docs/reference/std-alloc-requirements.md` with Promise, `spin_async()` entries; added "Service Calls with Promise API" section to `docs/guides/getting-started.md`
- ~~**48.15 — Remove `block_on` from nros API**~~ Done: deleted `block_on` function from `spin.rs`; removed re-exports from `nros-node/executor/mod.rs`, `nros-node/lib.rs`, `nros/lib.rs`
- ~~**48.16 — Revise native async example (tokio)**~~ Done: replaced `embassy-futures` + `nros::block_on` with tokio `current_thread` + `spawn_local` background spin pattern in `examples/native/rust/zenoh/async-service-client/`
- ~~**48.17 — Zephyr async example (Embassy)**~~ Done: new `examples/zephyr/rust/zenoh/async-service-client/` using `zephyr::embassy::Executor` (`executor-zephyr` feature, `k_sem`-backed kernel waking); background spin via `#[embassy_executor::task]`
- ~~**48.18 — Documentation updates**~~ Done: removed all `block_on` references from `std-alloc-requirements.md` and `getting-started.md`; updated async patterns to show tokio (desktop) and Embassy (Zephyr) background spin

### Pending — Runtime Externalization (48.15–48.18)

The `block_on()` function was initially added as a convenience for std targets but
doesn't belong in nano-ros — async runtime functions should come from external crates
(tokio for desktop, Embassy for embedded). The following work items remove `block_on`
and revise examples to use the **background spin pattern** with proper external runtimes.

Background spin is single-threaded concurrent: spawn `spin_async()` as a task managed
by the async runtime, then `.await` Promises directly from the main task. This works
because `EmbeddedServiceClient` is an owned type (no lifetime tied to Node/Executor) —
after creating the client, the executor can be moved to a background spin task while the
client operates independently via shared global transport state.

**48.15 — Remove `block_on` from nros API:**
- Delete `block_on` function from `nros-node/src/executor/spin.rs`
- Remove re-exports from `nros-node/src/executor/mod.rs`, `nros-node/src/lib.rs`, `nros/src/lib.rs`

**48.16 — Revise native async example (tokio):**
- Replace `embassy-futures` + `nros::block_on` with `tokio` (`current_thread` + `spawn_local`)
- Pattern: `tokio::task::spawn_local(async move { executor.spin_async().await })` for
  background spin, then `client.call(&req).unwrap().await` for service calls
- Shows the background spin pattern on desktop with the standard async runtime

**48.17 — New Zephyr async example (Embassy):**
- `examples/zephyr/rust/zenoh/async-service-client/` with `zephyr::embassy::Executor` (`executor-zephyr` feature)
- Kernel-backed waking via `k_sem_take`/`k_sem_give` (no busy-loop, proper power efficiency)
- Uses `nros::RmwSession` type alias to name concrete executor type in Embassy task signatures
- Pattern: `#[embassy_executor::task] async fn spin_task(mut exec: NrosExecutor) -> !`
- First Embassy usage in Zephyr examples

**48.18 — Documentation updates:**
- Remove all `block_on` references from `std-alloc-requirements.md` and `getting-started.md`
- Update async patterns: sync polling (unchanged), tokio background spin (native), Embassy background spin (Zephyr)
- Update Phase 48 doc examples and status

**Example matrix:**

| | Native (POSIX) | Zephyr |
|---|---|---|
| **Sync** | `service-client` (existing, spin_once + try_recv) | `service-client` (existing, spin_once + try_recv) |
| **Async** | `async-service-client` (revised: tokio background spin) | `async-service-client` (new: Embassy background spin) |

## Background

On single-threaded embedded systems, the current synchronous `client.call()`
blocks while waiting for a service reply. The call internally drives I/O
(XRCE: `uxr_run_session_time` retry loop; zenoh: `zenoh_shim_get` poll loop),
but while it blocks, no subscription callbacks, timers, or other service
handlers can fire. This is the fundamental tension:

```
// Today: call() blocks, preventing spin_once() from running
let reply = client.call(&request)?;  // blocks for N retries × timeout
executor.spin_once(10);              // never reached until call completes
```

The solution is cooperative async concurrency: one logical task drives I/O
while another awaits a service reply. They yield to each other on a
single thread.

### Goals

1. **Promise-based service call API** — `client.call(&request)` returns
   `Promise<Reply>` immediately (non-blocking), following the rclrs pattern.
   The promise can be `.await`ed or polled with `try_recv()`.
2. **Runtime-agnostic** — uses only `core::future::Future`, no dependency
   on Embassy, RTIC, tokio, or any specific async runtime
3. **no_std, no_alloc** — the promise is allocation-free (borrows the
   client's internal reply slot, unlike rclrs which uses a heap-allocated
   oneshot channel)
4. **Non-blocking service client primitives** — split `call_raw()` into
   `send_request_raw()` + `try_recv_reply_raw()` in the RMW trait
5. **Both RMW backends supported** — zenoh and XRCE-DDS
6. **Backward-compatible** — old blocking `call()` renamed to
   `call_blocking()`, new `call()` returns `Promise`

### Design Principles

1. **Only `spin*()` drives the runtime.** Consistent with rclcpp, rclc, and
   rclpy, the I/O event loop is only pumped by explicit spin calls
   (`spin_once`, `spin_blocking`, `spin_async`). Service calls, action calls,
   and other operations do NOT internally drive I/O — they yield and
   expect the application to run a concurrent spin. This avoids surprise
   I/O side effects from non-spin methods.

2. **`call()` returns a `Promise`, following rclrs.** In rclrs (ROS 2 Rust
   client), `client.call(&request)` returns `Promise<Response>` immediately.
   The promise can be `.await`ed in an async context or polled manually
   with `try_recv()` in a sync spin loop. We adopt this exact pattern.
   Unlike rclrs which uses `futures::channel::oneshot::Receiver` (requires
   alloc), our `Promise` borrows the client's reply slot (no_alloc).

3. **nros does not provide async runtimes or combinators.** The async
   runtime comes from the application's chosen crate — tokio for desktop,
   Embassy for embedded. nros only provides `core::future`-based primitives
   (`Promise`, `spin_async`).

4. **Two concurrency patterns are supported:**
   - **Background spin task (recommended)** — spawn `spin_async()` as a
     task in the async runtime (tokio `spawn_local`, Embassy `Spawner::spawn`),
     then `.await` promises directly from the main task. This works because
     `EmbeddedServiceClient` is an owned type with no lifetime tied to the
     executor — after creating the client, the executor can be moved to a
     background task. Single-threaded, no multi-threading required.
   - **Manual polling** — call `promise.try_recv()` in a `spin_once()` loop
     for sync code that doesn't use an async runtime.

### Non-Goals

- Bundling or depending on a specific async runtime (Embassy, RTIC, etc.)
- Providing `block_on` or any future executor — use tokio, Embassy, etc.
- Providing async combinators (`select`, `join`, etc.) — use `embassy-futures`
- Async publish/subscribe (pub/sub is already non-blocking via `try_recv`)
- Multi-threaded async (single-threaded cooperative only)
- Multiple concurrent outstanding requests per client (single reply slot)

## Architecture

### Layer diagram

```
┌─────────────────────────────────────────────────────────┐
│  Application                                             │
│  (picks runtime: Embassy / RTIC v2 / custom executor)    │
│  (picks combinators: embassy-futures select/join/…)      │
├─────────────────────────────────────────────────────────┤
│  nros-node Promise + async API ← NEW (Phase 48)         │
│  core::future::Future only, runtime-agnostic             │
│  - call() → Promise<Reply>                               │
│  - Promise::try_recv() / .await                          │
│  - spin_async()                                          │
├─────────────────────────────────────────────────────────┤
│  nros-node sync API            ← EXISTING, unchanged    │
│  - spin_once(timeout_ms)                                 │
│  - Session::drive_io()                                   │
├─────────────────────────────────────────────────────────┤
│  nros-rmw traits               ← EXTENDED (Phase 48)    │
│  - ServiceClientTrait::send_request_raw()     NEW        │
│  - ServiceClientTrait::try_recv_reply_raw()   NEW        │
│  - ServiceClientTrait::call_raw()             unchanged  │
├─────────────────────────────────────────────────────────┤
│  RMW backends                  ← EXTENDED (Phase 48)    │
│  rmw-zenoh: new non-blocking get shim                    │
│  rmw-xrce:  expose existing non-blocking primitives      │
├─────────────────────────────────────────────────────────┤
│  Platform transport            ← UNCHANGED               │
│  posix / zephyr / bare-metal                             │
└─────────────────────────────────────────────────────────┘
```

The async layer is **orthogonal** to the three existing feature axes
(RMW backend, platform, ROS edition). It uses only `core::future` and
`core::task` — available in all `no_std` environments.

### Compatible async runtimes

| Runtime | Platforms                            | no_std | no_alloc | Zephyr                | Notes                             |
|---------|--------------------------------------|--------|----------|-----------------------|-----------------------------------|
| Embassy | Cortex-M/A/R, RISC-V, AVR, WASM, std | Yes    | Yes      | Yes (`executor-zephyr` — `k_sem`-backed) | Dominant embedded runtime         |
| RTIC v2 | Cortex-M only                        | Yes    | Yes      | No                    | Hardware interrupt priorities     |
| Custom  | Any                                  | Yes    | Yes      | —                     | Application implements `RawWaker` |

nano-ros does not depend on any of these. Applications pick one and run
nano-ros futures on it.

### Application usage patterns

**Pattern 1: Background spin task (recommended for async)**

Spawn `spin_async()` as a concurrent task, then `.await` promises
directly. Works because `EmbeddedServiceClient` is an owned type —
after creating the client, the executor can be moved to a background task.

*Native with tokio:*
```rust
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut executor = Executor::open(&config).unwrap();
    let mut client = {
        let mut node = executor.create_node("client").unwrap();
        node.create_client::<AddTwoInts>("/add").unwrap()
    }; // node dropped, executor free to move

    let local = tokio::task::LocalSet::new();
    local.run_until(async move {
        tokio::task::spawn_local(async move {
            executor.spin_async().await;
        });
        let reply = client.call(&req).unwrap().await.unwrap();
    }).await;
}
```

*Embedded with Embassy:*
```rust
type NrosExecutor = nros::Executor<nros::RmwSession, 0, 0>;

#[embassy_executor::task]
async fn spin_task(mut exec: NrosExecutor) -> ! {
    exec.spin_async().await
}

#[embassy_executor::task]
async fn app_main(spawner: embassy_executor::Spawner) {
    let mut nros_exec = nros::Executor::<_, 0, 0>::open(&config).unwrap();
    let mut client = {
        let mut node = nros_exec.create_node("client").unwrap();
        node.create_client::<AddTwoInts>("/add").unwrap()
    };
    spawner.spawn(spin_task(nros_exec)).unwrap();
    let reply = client.call(&req).unwrap().await.unwrap();
}
```

**Pattern 2: Sync manual polling — no async runtime**

```rust
fn main_loop(executor: &mut Executor<...>, client: &mut Client<...>) {
    let mut promise = client.call(&request).unwrap();
    loop {
        executor.spin_once(10);
        if let Ok(Some(reply)) = promise.try_recv() {
            println!("reply: {:?}", reply);
            break;
        }
    }
}
```

All patterns work with any runtime (Embassy, RTIC v2, custom) or no
runtime at all. The `call()` → `Promise` API is the same everywhere.
Async combinators come from `embassy-futures` (no_std, no_alloc,
runtime-agnostic despite the name).

## How blocking works today

### XRCE-DDS `call_raw()`

```
uxr_buffer_request()              ← non-blocking, queues request
loop (SERVICE_REPLY_RETRIES):
    uxr_run_session_time(timeout)  ← blocks: sends request + polls for reply
    if slot.has_reply → return reply
return Err(Timeout)
```

The non-blocking primitives already exist in the XRCE C library:
- `uxr_buffer_request()` — queues without blocking
- `slot.has_reply.load()` — non-blocking flag check

The blocking comes from the retry loop calling `uxr_run_session_time()`
with a long timeout. For async, we split: buffer the request once, then
let `drive_io()` pump I/O in short bursts between yield points.

### Zenoh `call_raw()`

```
zenoh_shim_get()                   ← blocks in C until reply or timeout
  → z_get() sends query
  → Multi-threaded: condvar wait (background thread delivers)
  → Single-threaded: busy-poll zp_read() loop
  → Reply callback fires → copies to buffer
  → Returns length
```

Zenoh bundles everything into one blocking C call. For async, we need to
split this into:
- `zenoh_shim_get_start()` — sends the query, returns immediately
- `zenoh_shim_get_check()` — checks if reply callback has fired

## Per-backend × platform behavior

| Platform   | RMW   | `drive_io()`                     | Async `call` approach                                                 |
|------------|-------|----------------------------------|-----------------------------------------------------------------------|
| POSIX      | zenoh | `select()` on socket fd          | New non-blocking C shim for `z_get`                                   |
| POSIX      | XRCE  | `uxr_run_session_time()`         | Split: buffer request + short `uxr_run_session_time` + check slot     |
| Zephyr     | zenoh | `select()` on socket fd          | Same as POSIX zenoh; `executor-zephyr` sleeps via `k_sem_take` |
| Zephyr     | XRCE  | `uxr_run_session_time()`         | Same as POSIX XRCE; `executor-zephyr` sleeps via `k_sem_take`  |
| bare-metal | zenoh | smoltcp poll loop                | Non-blocking C shim + smoltcp pump in `drive_io`                      |
| bare-metal | XRCE  | smoltcp + `uxr_run_session_time` | Same split as POSIX XRCE                                              |

In all cases, `drive_io()` with a short timeout provides the I/O pump.
The async layer alternates between driving I/O and checking for the reply.

## Work Items

### 48.1 — Non-blocking service client trait methods

**Crate:** `nros-rmw` (`packages/core/nros-rmw/src/traits.rs`)

Add two methods to `ServiceClientTrait` with default implementations
that fall back to `call_raw()`:

```rust
pub trait ServiceClientTrait {
    type Error;

    // Existing (unchanged)
    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, Self::Error>;

    // NEW: Send a service request without waiting for reply.
    // Returns Err if the request could not be buffered.
    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error>;

    // NEW: Check for a pending reply without blocking.
    // Returns Ok(Some(len)) if a reply is available, Ok(None) if not yet.
    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;

    // Existing typed wrappers (unchanged)
    fn call<S: RosService>(...) -> Result<S::Reply, Self::Error> { ... }
}
```

Default implementations can delegate to `call_raw()` for backends that
don't implement the split (the async layer won't work optimally but
won't break). Backends that implement the split get true non-blocking
async.

### 48.2 — XRCE-DDS non-blocking service client

**Crate:** `nros-rmw-xrce` (`packages/xrce/nros-rmw-xrce/src/lib.rs`)

Implement `send_request_raw()` and `try_recv_reply_raw()` for
`XrceServiceClient`. The primitives already exist:

```rust
impl ServiceClientTrait for XrceServiceClient {
    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), TransportError> {
        // Clear stale reply
        slot.has_reply.store(false, Ordering::Release);
        // Buffer the request (non-blocking)
        let req = uxr_buffer_request(..., request.as_ptr(), request.len());
        if req == UXR_INVALID_REQUEST_ID {
            return Err(TransportError::ServiceRequestFailed);
        }
        Ok(())
    }

    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        if !slot.has_reply.load(Ordering::Acquire) {
            return Ok(None);
        }
        // Check overflow, copy reply, return length
        // (same logic as current call_raw post-check)
        ...
        Ok(Some(len))
    }
}
```

No C code changes needed — `uxr_buffer_request` and the reply slot
atomics are already non-blocking.

### 48.3 — Zenoh non-blocking service client (C shim)

**Crate:** `zpico-sys` (`packages/zpico/zpico-sys/c/shim/zenoh_shim.c`)

Add two new C shim functions:

```c
// Start a z_get query without blocking for the reply.
// Returns a handle (slot index) for checking the reply later.
int32_t zenoh_shim_get_start(const char *keyexpr,
                              const uint8_t *payload, size_t payload_len,
                              uint32_t timeout_ms);

// Check if the reply for a previous get_start has arrived.
// Returns: >0 = reply length, 0 = not yet, <0 = error/timeout.
int32_t zenoh_shim_get_check(int32_t handle,
                              uint8_t *reply_buf, size_t reply_buf_size);
```

Implementation approach:
- Reuse the existing `get_reply_ctx_t` with a static slot array
  (similar to subscriber/service slots)
- `get_start()` calls `z_get()` with the reply/dropper callbacks pointing
  to the slot, returns slot index
- `get_check()` checks `slot.done` and `slot.received` flags, copies data
  if available
- Multi-threaded mode: callbacks fire from background thread, check is
  lock-free via atomics
- Single-threaded mode: caller must call `drive_io()` / `zp_read()`
  between checks to process incoming data

### 48.4 — Zenoh non-blocking service client (Rust)

**Crate:** `nros-rmw-zenoh` (`packages/zpico/nros-rmw-zenoh/src/shim.rs`)

Wrap the new C shim functions and implement `send_request_raw()` /
`try_recv_reply_raw()` for `ShimServiceClient`.

### 48.5 — Promise type

**Crate:** `nros-node` (`packages/core/nros-node/src/executor/`)

Add the `Promise` type (see 48.6 for full API) and an internal
`yield_now()` helper for `spin_async()`. All use only `core::future`
and `core::task` — no feature gate needed.

nros does **not** provide async combinators (`select`, `join`, etc.).
Users add `embassy-futures` (no_std, no_alloc, runtime-agnostic) for
those. `spin_async()` uses `embassy_futures::yield_now()` internally
or an equivalent `pub(crate)` helper.

### 48.6 — Promise type and `call()` API

**Crate:** `nros-node` (`packages/core/nros-node/src/executor/handles.rs`)

Replace the blocking `call()` with a non-blocking version that returns
`Promise<Reply>`, following the rclrs pattern. Rename the old blocking
implementation to `call_blocking()`.

**`Promise` type** — allocation-free, borrows the client's reply slot:

```rust
/// A promise for a service reply. Created by `ServiceClient::call()`.
///
/// Following the rclrs pattern, the promise can be consumed in two ways:
/// - `.await` in an async context (requires concurrent `spin_async()`)
/// - `try_recv()` in a sync spin loop
///
/// Unlike rclrs which uses `futures::channel::oneshot::Receiver` (alloc),
/// this Promise borrows the client's internal reply slot — no_alloc.
pub struct Promise<'a, Reply, Cli: ServiceClientTrait> {
    handle: &'a mut Cli,
    reply_buffer: &'a mut [u8],
    _phantom: PhantomData<Reply>,
}

impl<Reply: Deserialize, Cli: ServiceClientTrait> Promise<'_, Reply, Cli> {
    /// Check if the reply has arrived (non-blocking).
    ///
    /// Returns `Ok(Some(reply))` if available, `Ok(None)` if not yet.
    pub fn try_recv(&mut self) -> Result<Option<Reply>, NodeError> {
        match self.handle.try_recv_reply_raw(self.reply_buffer)? {
            Some(len) => {
                let reply = Reply::deserialize(&self.reply_buffer[..len])?;
                Ok(Some(reply))
            }
            None => Ok(None),
        }
    }
}

impl<Reply: Deserialize, Cli: ServiceClientTrait> Future for Promise<'_, Reply, Cli> {
    type Output = Result<Reply, NodeError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.try_recv() {
            Ok(Some(reply)) => Poll::Ready(Ok(reply)),
            Ok(None) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}
```

**Updated `ServiceClient` methods:**

```rust
impl<Svc, Cli, ...> ServiceClient<Svc, Cli, REQ_BUF, REPLY_BUF>
where
    Cli: ServiceClientTrait,
{
    /// Send a service request and return a promise for the reply.
    ///
    /// Non-blocking — the request is serialized and buffered immediately.
    /// The returned promise can be:
    /// - `.await`ed in an async context (requires concurrent `spin_async()`)
    /// - Polled with `try_recv()` in a manual `spin_once()` loop
    ///
    /// Follows the rclrs `client.call()` → `Promise<Response>` pattern.
    pub fn call(
        &mut self,
        request: &Svc::Request,
    ) -> Result<Promise<'_, Svc::Reply, Cli>, NodeError> {
        // Serialize and send request (non-blocking)
        self.handle.send_request_raw(&self.serialize(request)?)?;
        Ok(Promise {
            handle: &mut self.handle,
            reply_buffer: &mut self.reply_buffer,
            _phantom: PhantomData,
        })
    }

    /// Blocking service call (legacy API).
    ///
    /// Renamed from the old `call()`. Drives I/O internally until the
    /// reply arrives. Prefer `call()` + `spin_once()` loop or async for
    /// new code.
    pub fn call_blocking(
        &mut self,
        request: &Svc::Request,
    ) -> Result<Svc::Reply, NodeError> {
        // Existing blocking implementation (unchanged)
        ...
    }
}
```

### 48.7 — Async spin

**Crate:** `nros-node` (`packages/core/nros-node/src/executor/spin.rs`)

Add `spin_async()` to the executor:

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize> Executor<S, MAX_CBS, CB_ARENA> {
    /// Drive I/O and dispatch callbacks asynchronously. This is the
    /// **only** method that drives the runtime — consistent with the
    /// ROS 2 convention (rclcpp, rclc, rclpy).
    ///
    /// Runs forever, yielding between poll cycles. Usage patterns:
    ///
    /// ```ignore
    /// use embassy_futures::select::{select, Either};
    ///
    /// // Async with select (embassy-futures):
    /// let promise = client.call(&req)?;
    /// let Either::Second(reply) = select(executor.spin_async(), promise).await
    ///     else { unreachable!() };
    ///
    /// // Async with background spin:
    /// spawner.spawn(spin_task(executor)).unwrap();
    /// let reply = client.call(&req)?.await;
    ///
    /// // Sync manual polling (no spin_async needed):
    /// let mut promise = client.call(&req)?;
    /// loop {
    ///     executor.spin_once(10);
    ///     if let Ok(Some(r)) = promise.try_recv() { break r; }
    /// }
    /// ```
    pub async fn spin_async(&mut self) -> ! {
        loop {
            self.spin_once(1);   // short non-blocking poll
            yield_now().await;   // yield to other async tasks
        }
    }
}
```

### 48.8 — Typed wrappers for non-blocking send/recv

**Crate:** `nros-rmw` (`packages/core/nros-rmw/src/traits.rs`)

Add typed convenience methods on `ServiceClientTrait` (default
implementations using CDR ser/de, mirroring existing `call()`):

```rust
pub trait ServiceClientTrait {
    /// Typed send: serialize request and buffer it (non-blocking).
    fn send_request<S: RosService>(
        &mut self,
        request: &S::Request,
        req_buf: &mut [u8],
    ) -> Result<(), Self::Error>
    where Self::Error: From<TransportError>;

    /// Typed receive: check for reply, deserialize if available.
    fn try_recv_reply<S: RosService>(
        &mut self,
        reply_buf: &mut [u8],
    ) -> Result<Option<S::Reply>, Self::Error>
    where Self::Error: From<TransportError>;
}
```

### 48.9 — Action client Promise support

**Crate:** `nros-node` (`packages/core/nros-node/src/executor/action.rs`)

Actions use services internally (send_goal, cancel_goal, get_result).
Apply the same `call()` → `Promise` pattern to action client methods:

```rust
impl ActionClientHandle<...> {
    /// Send goal. Returns Promise that resolves to GoalResponse.
    pub fn send_goal(&mut self, goal: &A::Goal) -> Result<Promise<'_, GoalResponse, ...>, NodeError>;

    /// Get result. Returns Promise that resolves to (GoalStatus, A::Result).
    pub fn get_result(&mut self, goal_id: &GoalId) -> Result<Promise<'_, (GoalStatus, A::Result), ...>, NodeError>;

    /// Cancel goal. Returns Promise that resolves to CancelResponse.
    pub fn cancel_goal(&mut self, goal_id: &GoalId) -> Result<Promise<'_, CancelResponse, ...>, NodeError>;

    /// Blocking variants (legacy API, renamed from old methods).
    pub fn send_goal_blocking(&mut self, ...) -> Result<GoalResponse, NodeError>;
    pub fn get_result_blocking(&mut self, ...) -> Result<(GoalStatus, A::Result), NodeError>;
    pub fn cancel_goal_blocking(&mut self, ...) -> Result<CancelResponse, NodeError>;
}
```

Feedback and status are already non-blocking (topic subscriptions with
`try_recv`), so they don't need promises.

### 48.10 — Integration tests

**Crate:** `nros-tests` (`packages/testing/nros-tests/`)

Test the Promise API with both backends:

- **Unit tests**: Mock session with `send_request_raw` / `try_recv_reply_raw`
  that simulate delayed replies; verify `Promise::try_recv()` returns
  `None` then `Some`, and `Promise.await` resolves correctly
- **Sync polling test**: `call()` + `spin_once()` + `try_recv()` loop
- **Async test**: `call()` + `embassy_futures::select(spin_async(), promise)`
  using a minimal custom executor (poll loop) — no Embassy/RTIC runtime
- **XRCE integration**: Promise-based service call against XRCE Agent
- **Zenoh integration**: Promise-based service call through zenohd

### 48.11 — Migrate existing examples to Promise API

Update all existing service and action client examples to use the new
`call()` → `Promise` pattern with `spin_once()` + `try_recv()`. This
replaces the old blocking calls with explicit spin loops, making I/O
driving visible.

**Service client examples (4 files):**

| Example                     | File                  | Current pattern                | New pattern                                  |
|-----------------------------|-----------------------|--------------------------------|----------------------------------------------|
| native/zenoh/service-client | `src/main.rs:51`      | `client.call(&request)` blocks | `client.call(&req)?.try_recv()` in spin loop |
| zephyr/zenoh/service-client | `src/lib.rs:47`       | `client.call(&req)` blocks     | `client.call(&req)?.try_recv()` in spin loop |
| native/xrce/service-client  | `src/main.rs:57`      | `client.call(&request)` blocks | `client.call(&req)?.try_recv()` in spin loop |
| native/zenoh/fairness-bench | `src/main.rs:151,197` | `client.call(&request)` blocks | `client.call(&req)?.try_recv()` in spin loop |

**Action client examples (3 files):**

| Example                    | File                 | Current pattern                  | New pattern                                        |
|----------------------------|----------------------|----------------------------------|----------------------------------------------------|
| native/zenoh/action-client | `src/main.rs:50`     | `client.send_goal(&goal)` blocks | `client.send_goal(&goal)?.try_recv()` in spin loop |
| zephyr/zenoh/action-client | `src/lib.rs:44,109`  | `send_goal` + `get_result` block | Promise + `try_recv()` in spin loop                |
| native/xrce/action-client  | `src/main.rs:51,103` | `send_goal` + `get_result` block | Promise + `try_recv()` in spin loop                |

The migration pattern for each is:
```rust
// Before:
let reply = client.call(&request)?;

// After:
let mut promise = client.call(&request)?;
let reply = loop {
    executor.spin_once(10);
    if let Ok(Some(reply)) = promise.try_recv() {
        break reply;
    }
};
```

C API examples (`native/c/zenoh/service-client`, `native/c/zenoh/action-client`)
are unaffected — they use raw buffer APIs, not the Rust Promise type.

### 48.12 — New async example

Add a new async example demonstrating the async Promise pattern:

- `examples/native/rust/zenoh/async-service-client/` — async service client
  (originally used `embassy_futures::select` + `nros::block_on`;
  revised in 48.16 to use tokio background spin pattern)

### 48.13 — Remove `call_blocking()` and action `*_blocking()` methods

After all examples and tests have migrated to the Promise API (48.11),
remove the legacy blocking methods in a follow-up:

- `ServiceClient::call_blocking()` — delete
- `ActionClientHandle::send_goal_blocking()` — delete
- `ActionClientHandle::get_result_blocking()` — delete
- `ActionClientHandle::cancel_goal_blocking()` — delete

These methods drive I/O internally, violating the "only spin drives the
runtime" principle. Keeping them temporarily eases migration, but they
should not persist long-term. The sync polling pattern
(`call()` + `spin_once()` + `try_recv()`) is the correct non-async
replacement.

Also remove any internal `call_raw()` usage that bypasses the
`send_request_raw()` + `try_recv_reply_raw()` split, once all callers
have migrated.

### 48.14 — Documentation

- Update `docs/reference/std-alloc-requirements.md` to document Promise
  and async API availability (no_std, no_alloc)
- Add Promise usage section to `docs/guides/getting-started.md`
- Document three usage patterns: sync `try_recv()`, async `select`, background spin

## Dependencies

- **Core nros crates**: No new external dependencies. `Promise` and
  `spin_async()` use only `core::future` and `core::task`.
- **Application code**: Add `embassy-futures` for async combinators
  (`select`, `join`, etc.). This is no_std, no_alloc, and
  runtime-agnostic — works with Embassy, RTIC, custom executors, or
  even tokio.
- `core::future::Future`, `core::task::{Poll, Context, Waker}` — in
  `core` since Rust 1.36, no feature flags needed

## std / alloc Impact

| Component                                     | std | alloc | Notes                                              |
|-----------------------------------------------|-----|-------|----------------------------------------------------|
| `Promise<Reply>`                              | No  | No    | Borrows client's reply slot (no channel allocation) |
| `Promise::try_recv()`                         | No  | No    | Non-blocking poll of RMW reply slot                |
| `Promise: Future` impl                        | No  | No    | `core::future` only, yields via `wake_by_ref`      |
| `send_request_raw()` / `try_recv_reply_raw()` | No  | No    | Buffer operations on existing static slots         |
| `call()` → `Promise`                          | No  | No    | Serialize + send, return borrowed promise          |
| `spin_async()`                                | No  | No    | Async wrapper over `spin_once()`                   |
| Action client promise methods                 | No  | No    | Delegate to service promise primitives             |
| Zenoh C shim additions                        | No  | No    | Static slot array, no malloc                       |
| XRCE implementation                           | No  | No    | Exposes existing atomics                           |

The entire async API is available with `--no-default-features`.

## Risk Assessment

**Zenoh C shim complexity (48.3):** The non-blocking `z_get` split requires
managing query lifetime across `get_start` / `get_check` calls. The reply
callback and dropper must reference a persistent slot. Static slot array
(similar to existing subscriber slots) keeps this allocation-free but limits
concurrent outstanding queries.

**Waker semantics:** `yield_now()` uses `wake_by_ref()` to ensure the
executor re-polls immediately. This is correct for cooperative single-threaded
executors (Embassy, RTIC) but would busy-loop on a multi-threaded executor
that doesn't coalesce wakeups. Since the target is single-threaded embedded,
this is acceptable.

**Borrow conflicts:** `spin_async()` borrows `&mut Executor` and the
`Promise` borrows `&mut ServiceClient`. Since the service client is a
separate owned object (not borrowing the executor), this works in
`select!` — the borrow checker sees two disjoint `&mut` borrows. The
underlying RMW backends use global statics (XRCE: `SESSION`, zenoh:
`g_session`) so concurrent access is safe in single-threaded cooperative
scheduling (only one future polls at a time). No `RefCell` or `Mutex`
needed.

**Promise lifetime:** The `Promise<'a, Reply, Cli>` borrows `&'a mut`
the service client. While a promise is alive, no new `call()` can be
made on the same client. This is correct — our RMW backends support
only one outstanding request per client (single reply slot). The borrow
checker enforces this at compile time, unlike rclrs where it's a
runtime invariant.

## User Examples

These examples show how applications use the async nano-ros API with
different runtimes and platforms. The nano-ros API is identical across
all combinations — only the runtime boilerplate differs.

### Example 1: Zephyr + Embassy — Async service client

The current blocking Zephyr service client (shown for comparison):

```rust
// TODAY: call_blocking() blocks — spin/timers/subscriptions cannot run
fn run() -> Result<(), NodeError> {
    let config = ExecutorConfig::new("tcp/192.0.2.2:7447");
    let mut executor = Executor::<_, 0, 0>::open(&config)?;
    let mut node = executor.create_node("client")?;
    let mut client = node.create_client::<AddTwoInts>("/add")?;

    loop {
        let reply = client.call_blocking(&AddTwoIntsRequest { a: 1, b: 2 })?;
        info!("sum = {}", reply.sum);
        zephyr::time::sleep(zephyr::time::Duration::secs(2));
    }
}
```

The async version with Embassy on Zephyr:

```rust
#![no_std]

use embassy_futures::select::{select, Either};
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe { zephyr::set_logger().ok() };

    // executor-zephyr: k_sem-backed sleeping (no busy-loop)
    let zephyr_executor = zephyr::embassy::Executor::new();
    zephyr_executor.run(|spawner| {
        spawner.spawn(ros_main()).unwrap();
    });
}

#[embassy_executor::task]
async fn ros_main() {
    if let Err(e) = run_async().await {
        error!("Error: {:?}", e);
    }
}

async fn run_async() -> Result<(), NodeError> {
    let config = ExecutorConfig::new("tcp/192.0.2.2:7447");
    let mut executor = Executor::<_, 0, 0>::open(&config)?;
    let mut node = executor.create_node("client")?;
    let mut client = node.create_client::<AddTwoInts>("/add")?;

    info!("Service client ready");

    loop {
        // call() sends request immediately and returns a Promise.
        // select runs spin_async (I/O driver) alongside the promise.
        // When the promise resolves, select returns and spin is dropped.
        let promise = client.call(&AddTwoIntsRequest { a: 1, b: 2 })?;
        let Either::Second(reply) = select(executor.spin_async(), promise).await
            else { unreachable!() };

        match reply {
            Ok(resp) => info!("sum = {}", resp.sum),
            Err(e) => error!("call failed: {:?}", e),
        }

        embassy_time::Timer::after_secs(2).await;
    }
}
```

Key differences from blocking version:
- `rust_main` sets up the Embassy executor (Zephyr backend), then spawns a task
- `call()` returns a `Promise` immediately (non-blocking), following rclrs
- `embassy_futures::select` runs `spin_async` and the promise concurrently
- While awaiting the reply, timers and subscriptions registered on the
  executor continue to fire via `spin_async`

### Example 2: Zephyr + Embassy — Subscription + service client

A more realistic example: publish sensor data on a timer, subscribe to
commands, and call a calibration service — all concurrently.

```rust
use embassy_futures::select::{select, Either};

async fn run_async() -> Result<(), NodeError> {
    let config = ExecutorConfig::new("tcp/192.0.2.2:7447");
    let mut executor = Executor::open(&config)?;
    let mut node = executor.create_node("sensor_node")?;

    // Publisher (used inside timer callback)
    let publisher = node.create_publisher::<SensorData>("/sensor/data")?;

    // Subscription callback — fires during spin_async
    executor.add_subscription::<Command, _>("/sensor/command", |cmd| {
        info!("Command received: mode={}", cmd.mode);
    })?;

    // Timer callback — fires during spin_async
    let mut seq: u32 = 0;
    executor.add_timer(TimerDuration::from_millis(100), move || {
        let _ = publisher.publish(&SensorData { seq, value: read_sensor() });
        seq += 1;
    })?;

    // Service client for calibration (non-blocking)
    let mut calibrate = node.create_client::<Calibrate>("/sensor/calibrate")?;

    info!("Sensor node ready");

    // Periodic calibration loop
    loop {
        // Run spin for 10 seconds (timer publishes, subscription handles commands)
        select(executor.spin_async(), embassy_time::Timer::after_secs(10)).await;

        // Call calibration service (spin_async keeps callbacks alive)
        info!("Calibrating...");
        let promise = calibrate.call(&CalibrateRequest { samples: 100 })?;
        let Either::Second(result) = select(executor.spin_async(), promise).await
            else { unreachable!() };
        match result {
            Ok(resp) => info!("Calibrated: offset={}", resp.offset),
            Err(e) => error!("Calibration failed: {:?}", e),
        }
    }
}
```

Notice that the subscription callback and timer callback keep firing
while the promise is pending because `spin_async` is running concurrently
via `select`. This is impossible with the blocking `call_blocking()` API.

### Example 3: Background spin task — sequential calls without `select`

When making many sequential service calls, wrapping each in
`select(spin_async(), ...)` is repetitive. The alternative is
to spawn `spin_async()` as a long-lived background task and `.await`
promises directly:

```rust
use embassy_executor::Spawner;
use nros::{Executor, ExecutorConfig, NodeError};
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};

// Background spin task — drives I/O for the entire session lifetime
#[embassy_executor::task]
async fn spin_task(mut executor: Executor<RmwSession, 4, 4096>) -> ! {
    executor.spin_async().await
}

#[embassy_executor::task]
async fn app_task() {
    if let Err(e) = run_app().await {
        error!("Error: {:?}", e);
    }
}

async fn run_app() -> Result<(), NodeError> {
    let config = ExecutorConfig::new("tcp/192.0.2.2:7447");
    let mut executor = Executor::open(&config)?;
    let mut node = executor.create_node("multi_client")?;

    let mut add = node.create_client::<AddTwoInts>("/add")?;
    let mut calibrate = node.create_client::<Calibrate>("/calibrate")?;
    let mut configure = node.create_client::<Configure>("/configure")?;

    // Move executor to background spin task
    spawner.spawn(spin_task(executor)).unwrap();

    // Sequential calls — just call() and await the promise.
    // The background spin task drives I/O concurrently.
    let config_resp = configure.call(&ConfigureRequest { mode: 1 })?.await?;
    info!("Configured: {:?}", config_resp);

    let cal_resp = calibrate.call(&CalibrateRequest { samples: 100 })?.await?;
    info!("Calibrated: offset={}", cal_resp.offset);

    loop {
        let reply = add.call(&AddTwoIntsRequest { a: 1, b: 2 })?.await?;
        info!("sum = {}", reply.sum);
        embassy_time::Timer::after_secs(1).await;
    }
}
```

This is the embedded equivalent of the tokio pattern
`tokio::spawn(async { executor.spin_blocking() })`. The background
spin task cooperatively yields via `spin_async`'s internal
`yield_now()`, allowing the main task to poll its promises on the
same thread.

**Trade-off:** The `select` pattern keeps the executor local (no
ownership transfer). The background task pattern requires moving the
executor into the spawned task, meaning the main task cannot access
it directly. Choose `select` for simple cases and background task
for complex sequential workflows.

### Example 4: Zephyr + Embassy — Action client with promises

The current blocking action client requires a manual poll loop
(see `examples/zephyr/rust/zenoh/action-client/`). The promise version:

```rust
use embassy_futures::select::{select, Either};

async fn run_async() -> Result<(), NodeError> {
    let config = ExecutorConfig::new("tcp/192.0.2.2:7447");
    let mut executor = Executor::<_, 0, 0>::open(&config)?;
    let mut node = executor.create_node("fibonacci_client")?;
    let mut action = node.create_action_client::<Fibonacci>("/fibonacci")?;

    info!("Action client ready");
    embassy_time::Timer::after_secs(3).await; // Wait for server

    let goal = FibonacciGoal { order: 10 };
    info!("Sending goal: order={}", goal.order);

    // Send goal — returns a Promise, select drives I/O
    let promise = action.send_goal(&goal)?;
    let Either::Second(goal_id) = select(executor.spin_async(), promise).await
        else { unreachable!() };
    let goal_id = goal_id?;
    info!("Goal accepted: {:02x}{:02x}...", goal_id.uuid[0], goal_id.uuid[1]);

    // Receive feedback while waiting for result
    loop {
        // Drive I/O for a bit
        select(executor.spin_async(), embassy_time::Timer::after_millis(100)).await;

        // Check for feedback (non-blocking, no promise needed)
        while let Ok(Some((fid, feedback))) = action.try_recv_feedback() {
            if fid.uuid == goal_id.uuid {
                info!("Feedback: {:?}", feedback.sequence.as_slice());
                if feedback.sequence.len() as i32 > goal.order {
                    break;
                }
            }
        }
    }

    // Get result — same pattern
    let promise = action.get_result(&goal_id)?;
    let Either::Second(result) = select(executor.spin_async(), promise).await
        else { unreachable!() };
    let (status, result) = result?;
    info!("Result: status={:?}, sequence={:?}", status, result.sequence.as_slice());

    Ok(())
}
```

### Example 5: Bare-metal Cortex-M + Embassy

For bare-metal targets without Zephyr. Uses Embassy's built-in
Cortex-M executor with hardware WFE/SEV for idle sleep.

```rust
#![no_std]
#![no_main]

use embassy_executor::Spawner;
use nros::{Executor, ExecutorConfig, NodeError};
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use defmt_rtt as _;
use panic_halt as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    defmt::info!("nros Embassy bare-metal example");

    // Initialize board (HAL-specific)
    let _p = embassy_stm32::init(Default::default());

    // nros setup — identical to Zephyr version
    let config = ExecutorConfig::new("tcp/192.168.1.1:7447");
    let mut executor = Executor::<_, 0, 0>::open(&config)
        .expect("Failed to open session");
    let mut node = executor.create_node("client").unwrap();
    let mut client = node.create_client::<AddTwoInts>("/add").unwrap();

    defmt::info!("Service client ready");

    // call() returns a Promise — same API as Zephyr
    use embassy_futures::select::{select, Either};
    loop {
        let promise = client.call(&AddTwoIntsRequest { a: 1, b: 2 }).unwrap();
        let Either::Second(reply) = select(executor.spin_async(), promise).await
            else { unreachable!() };
        match reply {
            Ok(resp) => defmt::info!("sum = {}", resp.sum),
            Err(_) => defmt::error!("call failed"),
        }
        embassy_time::Timer::after_secs(2).await;
    }
}
```

The nano-ros API is identical to the Zephyr version. Only the entry
point (`#[embassy_executor::main]` vs `zephyr::embassy::Executor`) and
HAL initialization differ.

### Example 6: Bare-metal Cortex-M + RTIC v2

RTIC uses hardware interrupt priorities for preemption. Async support
was added in RTIC v2.

```rust
#![no_std]
#![no_main]

use rtic::app;
use nros::{Executor, ExecutorConfig, NodeError};
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use defmt_rtt as _;
use panic_halt as _;

#[app(device = stm32f4xx_hal::pac, dispatchers = [SPI1])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {}

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        defmt::info!("nros RTIC example");
        ros_task::spawn().unwrap();
        (Shared {}, Local {})
    }

    /// Async task — uses the same nros API as Embassy
    #[task(priority = 1)]
    async fn ros_task(_cx: ros_task::Context) {
        let config = ExecutorConfig::new("tcp/192.168.1.1:7447");
        let mut executor = Executor::<_, 0, 0>::open(&config).unwrap();
        let mut node = executor.create_node("client").unwrap();
        let mut client = node.create_client::<AddTwoInts>("/add").unwrap();

        defmt::info!("Service client ready");

        use embassy_futures::select::{select, Either};
        loop {
            // Identical nano-ros Promise API
            let promise = client.call(&AddTwoIntsRequest { a: 1, b: 2 }).unwrap();
            let Either::Second(reply) = select(executor.spin_async(), promise).await
                else { unreachable!() };
            match reply {
                Ok(resp) => defmt::info!("sum = {}", resp.sum),
                Err(_) => defmt::error!("call failed"),
            }

            // RTIC doesn't have embassy_time — use rtic_monotonics
            rtic_monotonics::systick::Systick::delay(2000.millis()).await;
        }
    }
}
```

The nano-ros code inside `ros_task` is identical to the Embassy
version. Only the RTIC app structure and timing primitives differ.

### Example 7: Native POSIX (desktop testing)

For development and testing on Linux/macOS. Two patterns: sync polling
(no runtime needed) and async with tokio background spin.

```rust
// Sync polling — no async runtime needed
use nros::{Executor, ExecutorConfig};
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};

fn main() {
    let config = ExecutorConfig::from_env().node_name("client");
    let mut executor = Executor::<_, 0, 0>::open(&config).unwrap();
    let mut node = executor.create_node("client").unwrap();
    let mut client = node.create_client::<AddTwoInts>("/add").unwrap();

    let mut promise = client.call(&AddTwoIntsRequest { a: 1, b: 2 }).unwrap();
    let reply = loop {
        executor.spin_once(10);
        if let Ok(Some(reply)) = promise.try_recv() { break reply; }
    };
    println!("sum = {}", reply.sum);
}
```

```rust
// Async with tokio background spin
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut executor = Executor::open(&config).unwrap();
    let mut client = {
        let mut node = executor.create_node("client").unwrap();
        node.create_client::<AddTwoInts>("/add").unwrap()
    };

    let local = tokio::task::LocalSet::new();
    local.run_until(async move {
        tokio::task::spawn_local(async move { executor.spin_async().await });
        let reply = client.call(&AddTwoIntsRequest { a: 1, b: 2 }).unwrap().await.unwrap();
        println!("sum = {}", reply.sum);
    }).await;
}
```

### Async combinators — `embassy-futures`

nros does **not** provide async combinators. Use `embassy-futures`
(no_std, no_alloc, runtime-agnostic despite the name):

```toml
[dependencies]
embassy-futures = "0.1"
```

Key combinators:

| Function              | Description                              | Use case                                        |
|-----------------------|------------------------------------------|-------------------------------------------------|
| `select(a, b)`        | First to complete wins, returns `Either` | `spin_async()` + one promise                    |
| `select3(a, b, c)`    | Three futures                            | `spin_async()` + two concurrent promises        |
| `select4(a, b, c, d)` | Four futures                             | spin + three concurrent operations              |
| `select_array([...])` | N homogeneous futures                    | spin + N promises of same type                  |
| `join(a, b)`          | Wait for both                            | Two independent promises (with background spin) |
| `yield_now()`         | Yield one poll cycle                     | Used internally by `spin_async()`               |

Example — spin + two concurrent service calls:
```rust
use embassy_futures::select::{select3, Either3};

let p1 = client_a.call(&req1)?;
let p2 = client_b.call(&req2)?;
match select3(executor.spin_async(), p1, p2).await {
    Either3::First(_) => unreachable!(),  // spin never returns
    Either3::Second(r1) => handle(r1?),   // client_a replied first
    Either3::Third(r2) => handle(r2?),    // client_b replied first
}
```

The `Either` return type requires a match, but since `spin_async()`
never returns, the first arm is always `unreachable!()`. This is a
minor ergonomic cost that avoids nros reinventing combinator code.

### Async runtime — external (not provided by nros)

nros does **not** provide `block_on` or any async executor. The async
runtime comes from the application's chosen crate:

| Platform | Runtime | Dependency |
|----------|---------|------------|
| Desktop/POSIX | tokio (`current_thread`) | `tokio = { version = "1", features = ["rt", "macros"] }` |
| Zephyr | Embassy (`executor-zephyr`) | `zephyr = { version = "0.1.0", features = ["executor-zephyr"] }` + `embassy-executor = "0.7"` (no `arch-*` features) |
| Bare-metal | Embassy | `embassy-executor` with arch-specific feature (`arch-cortex-m`, `arch-riscv32`, etc.) |
| RTIC v2 | RTIC | (built-in async support) |

### Summary: what differs vs what stays the same

| Aspect         | Varies by platform/runtime                                                                        | Same everywhere                    |
|----------------|---------------------------------------------------------------------------------------------------|------------------------------------|
| Entry point    | `rust_main` (Zephyr), `#[embassy_executor::main]` (bare-metal), `fn main` (POSIX), `#[rtic::app]` | —                                  |
| Executor setup | `zephyr::embassy::Executor` (Zephyr, `k_sem`-backed), Embassy Cortex-M (WFE), RTIC dispatchers, tokio `current_thread` | —                                  |
| Timing         | `embassy_time::Timer`, `rtic_monotonics`, `std::thread::sleep`                                    | —                                  |
| Logging        | `log` (Zephyr), `defmt` (bare-metal), `println` (POSIX)                                           | —                                  |
| nros config    | —                                                                                                 | `ExecutorConfig::new(locator)`     |
| nros session   | —                                                                                                 | `Executor::open(&config)`          |
| Create node    | —                                                                                                 | `executor.create_node(name)`       |
| Create client  | —                                                                                                 | `node.create_client::<Svc>(topic)` |
| Async spin     | —                                                                                                 | `executor.spin_async()`              |
| Service call   | —                                                                                                 | `client.call(&req)` → `Promise`     |
| Poll reply     | —                                                                                                 | `promise.try_recv()` or `.await`     |
| Concurrent I/O | —                                                                                                 | `select(spin, promise)` (embassy-futures) or background spin task |

### Comparison with rclrs

The API follows the rclrs (ROS 2 Rust client) 0.7.0 pattern while
adapting it for no_std/no_alloc embedded:

| Aspect                 | rclrs                                                                   | nros                                                |
|------------------------|-------------------------------------------------------------------------|-----------------------------------------------------|
| `call()` return type   | `Result<Promise<Out>, RclrsError>`                                      | `Result<Promise<'_, Reply, Cli>, NodeError>`        |
| `Promise` backing      | `futures::channel::oneshot::Receiver` (alloc)                           | Borrows `&mut Client` reply slot (no_alloc)         |
| `.await`               | Yes (`Future` impl from `futures` crate)                                | Yes (`core::future::Future` impl)                   |
| `try_recv()`           | Yes (`Receiver::try_recv()`)                                            | Yes (`Promise::try_recv()`)                         |
| Response metadata      | Generic: `Response`, `(Response, RequestId)`, `(Response, ServiceInfo)` | `Response` only (no metadata variants)              |
| Concurrent requests    | Multiple per client (HashMap of sequence numbers)                       | One per client (single reply slot, borrow-enforced) |
| I/O driving            | Executor spin loop fulfills promise internally                          | Same — `spin_once`/`spin_async` fulfills promise    |
| `call_then()` callback | Yes (`FnOnce` callback on response)                                     | Not planned (use `.await` or `try_recv()`)          |

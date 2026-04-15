# Phase 82: Blocking Service Client Must Take an Executor

**Goal**: Bring the service client into line with the design principle
[established in Phase 77](phase-77-async-action-client.md): *if a public API
blocks, the caller must pass an executor and the API must spin that executor*.
The action client already follows this; the service client does not, and the
inconsistency is observable across all three language bindings.

**Status**: Not Started
**Priority**: Medium — soundness fix, no functional regression on currently
passing tests, but blocks future "service-call from a callback" use cases.
**Depends on**: Phase 77 (executor-spin pattern for blocking helpers)

## Overview

### The rule

> Any blocking helper exposed to user code must take an executor handle and
> drive that executor while it waits.

The motivation is the same one that drove Phase 77:

1. **Single source of I/O**: only `spin_*()` may call into the transport's
   read path. A blocking call that bypasses the executor either deadlocks on
   single-threaded transports (no read task to deliver the reply) or starves
   timers/subscriptions/parameter services on multi-threaded ones.
2. **Reentrancy safety**: when a blocking helper drives the executor, the
   user can at least *see* (via the executor argument) that calling it from
   inside another callback is reentrant. A blocking helper that takes no
   executor looks innocent and silently breaks.
3. **Timeout semantics**: Phase 77 already proved that condvar timed waits
   are unreliable across our platform matrix (NuttX kernel `nxsem_clockwait`
   hang, FreeRTOS QEMU lease-task starvation, icount virtual-time skew).
   Spinning the executor + checking a non-blocking poll is the only timeout
   mechanism that works uniformly.

### Action client (Phase 77, done)

```c
// C — blocking, takes executor, spins it internally
nros_ret_t nros_action_send_goal(
    nros_action_client_t *client,
    nros_executor_t      *executor,    // ← required
    const uint8_t *goal, size_t goal_len,
    nros_goal_uuid_t *goal_uuid);

nros_ret_t nros_action_get_result(
    nros_action_client_t *client,
    nros_executor_t      *executor,    // ← required
    const nros_goal_uuid_t *goal_uuid,
    nros_goal_status_t *status,
    uint8_t *result, size_t cap, size_t *result_len);
```

```rust
// Rust — Promise::wait takes &mut Executor
let mut promise = client.send_goal(&goal)?;
let accepted    = promise.wait(&mut executor, 5000)?;
```

Both implementations are `send_async + spin_executor + check_pending_get`.
There is no `zpico_get` (blocking condvar) anywhere in the action path.

### Service client (the inconsistency)

The service client still ships three blocking entry points that never see an
executor and instead block at the transport layer:

1. **Rust**: `ServiceClientTrait::call_raw(&mut self, request, reply_buf)` —
   defined in `packages/core/nros-rmw/src/traits.rs:965`. The
   `nros-rmw-zenoh` impl forwards to `Context::get` → `zpico_get` → condvar
   wait on the background read task.

2. **C**: `nros_client_call(client, request, request_len, response, ...)` in
   `packages/core/nros-c/src/service.rs:602`. Forwards directly to the trait
   method above. Note: this is the *only* blocking helper in `nros-c` that
   does not take a `nros_executor_t*`. Every action helper does.

3. **C++**: `nros::Client<S>::call(req, resp)` in
   `packages/core/nros-cpp/include/nros/client.hpp:50`. Forwards to
   `nros_cpp_service_client_call_raw` which goes through the same Rust trait
   method.

All three share a single transport implementation: `zpico_get`'s
multi-threaded path takes the form

```c
// packages/zpico/zpico-sys/c/zpico/zpico.c
_z_mutex_lock(&ctx.mutex);
while (!ctx.done) {
    _z_condvar_wait(&ctx.cond, &ctx.mutex);   // unbounded wait
}
_z_mutex_unlock(&ctx.mutex);
```

This is exactly the antipattern Phase 77 removed from the action path.

### Concrete failure modes

- **Single-threaded transports** (`Z_FEATURE_MULTI_THREAD == 0`, used by
  bare-metal MPS2-AN385, ESP32-C3-QEMU, smoltcp): there is no background read
  task to deliver the reply or fire the dropper, so the condvar is never
  signaled. `call_raw` deadlocks until the watchdog kills the binary.
- **Multi-threaded with TX-mutex contention** (FreeRTOS QEMU): the lease
  task is starved while the calling thread holds the condvar wait, which can
  in turn starve the dropper that should signal it. Same hang root-cause as
  Phase 77.
- **Reentrancy from a callback**: if a subscription callback calls
  `nros_client_call`, it re-enters the executor's dispatch loop without the
  executor knowing. The executor is already mid-`spin_once`; the second
  entry will sit in a condvar wait that only the first entry's read task
  could unblock.
- **Surprise timeout policy**: `call_raw` has no `timeout_ms` argument. It
  relies on `SERVICE_DEFAULT_TIMEOUT_MS` baked in at zpico-sys build time
  (10 s default, env-overridable). Users have no per-call control.

### What the rule-following shape looks like

The async pair already exists on `ServiceClientTrait`:

```rust
fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error>;
fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8])
    -> Result<Option<usize>, Self::Error>;
```

And `nros-node` already wraps them as `Promise<T>`:

```rust
let mut promise = client.call(&request)?;            // sends, returns Promise
let response    = promise.wait(&mut executor, 5000)?; // spins executor
```

So Rust users who follow the *current* `Client::call → Promise::wait`
pattern are already compliant. The violation is the lower-level
`ServiceClientTrait::call_raw` method, plus the C and C++ wrappers built on
top of it.

## Design

### Rust

Rust users who follow the existing user-facing path are already compliant —
`Client::call(&request)` returns a `Promise<Response>`, and the blocking
helper `Promise::wait(&mut executor, timeout_ms)` already takes the
executor and spins it (`packages/core/nros-node/src/executor/handles.rs`).
This is the canonical "blocking convenience over async + spin" shape and
mirrors `Promise::wait` for the action client.

The only violation is the lower-level `ServiceClientTrait::call_raw` method
on the RMW trait, which blocks at the transport layer (`Context::get` →
`zpico_get` → condvar wait) without ever taking an executor. It exists
because the C and C++ FFI wrappers call it, and once those move to the
spin-driven pattern (see "C" below) it has no more callers.

**Action**: remove `call_raw` from the public trait surface and from every
backend impl. `Promise::wait` stays as the user-facing blocking convenience.

```rust
// Removed
trait ServiceClientTrait {
    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8])
        -> Result<usize, Self::Error>;   // ← gone
    fn send_request_raw(&mut self, request: &[u8])
        -> Result<(), Self::Error>;       // ← stays
    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8])
        -> Result<Option<usize>, Self::Error>; // ← stays
}
```

Two phases for the removal:

1. Mark `call_raw` `#[deprecated(note = "use Promise::wait via Client::call")]`.
   The default body forwards to `send_request_raw` + a short busy loop on
   `try_recv_reply_raw`, returning `TransportError::Timeout` after a small
   build-time-configurable budget. Still keeps single-threaded transports
   alive for one release without the condvar wait.
2. Delete it. Verify no in-tree caller routes through `Context::get`. Then
   delete `Context::get` and `zpico_get`/`zpico.c::zpico_get` (the
   blocking transport entry point) — only `zpico_get_start` /
   `zpico_get_check` remain.

The Rust user-facing surface (`nros::Client`, `Promise`) doesn't change at
all.

### C (`nros-c`)

The minimal-friction fix is to give the service client the same lifecycle
the action client and service server already have: **deferred transport
creation + an explicit registration step that stashes the executor**. Once
the executor is stashed, the existing `nros_client_call(client, ...)`
signature can stay exactly as it is today and just become spin-driven
internally.

#### Current asymmetry

| Entity         | Init creates entity? | Registration fn                       | Stashes executor? |
|---------------|----------------------|---------------------------------------|--------------------|
| Service server | No (defers)          | `nros_executor_add_service`           | handle_id only     |
| Action client  | No (defers)          | `nros_executor_add_action_client`     | yes (`_internal`)  |
| Action server  | No (defers)          | `nros_executor_add_action_server`     | yes (`_internal`)  |
| **Service client** | **Yes** (`session.create_service_client(...)` at init) | **none — never registered with executor** | **no `_internal` at all** |

The service client is the only entity that creates its transport handle at
`init` time and never gets registered with the executor. That's why
`call_raw` was the only blocking path: there was no executor for the
wrapper to spin.

#### Target lifecycle

```c
// 1. Init = metadata only (matches nros_service_init)
nros_client_init(&client, &node, "/add_two_ints", &type);

// 2. Register with executor — REQUIRED before any send/call.
//    Creates the RmwServiceClient inside the arena, stashes
//    executor_ptr + handle_id into a new ClientInternal.
nros_executor_add_client(&executor, &client);

// 3a. Async path (mirrors nros_action_send_goal_async + callback)
nros_client_set_response_callback(&client, on_response, ctx);
nros_client_send_request_async(&client, req, req_len);
// user spins executor; on_response fires when reply arrives

// 3b. Pull-based async (mirrors try_recv on subscriptions)
nros_client_send_request_async(&client, req, req_len);
nros_client_try_recv_response(&client, resp, cap, &resp_len);

// 3c. Blocking sugar — UNCHANGED SIGNATURE
//     Internally reads executor_ptr from client._internal and spins it.
nros_client_call(&client, req, req_len, resp, cap, &resp_len);

// Optional client-wide timeout knob (defaults to NROS_DEFAULT_SERVICE_TIMEOUT_MS = 5000)
nros_client_set_timeout(&client, 10000);
```

#### Signatures

```c
// NEW: registration step (parallels nros_executor_add_service)
nros_ret_t nros_executor_add_client(
    nros_executor_t *executor,
    nros_client_t   *client);

// NEW: client-wide timeout setter
nros_ret_t nros_client_set_timeout(
    nros_client_t *client,
    uint32_t       timeout_ms);

// NEW: async pair (parallels nros_action_send_goal_async +
//                  nros_action_client_set_goal_response_callback)
nros_ret_t nros_client_send_request_async(
    nros_client_t *client,
    const uint8_t *request, size_t request_len);

nros_ret_t nros_client_try_recv_response(
    nros_client_t *client,
    uint8_t *response, size_t response_capacity, size_t *response_len);

nros_ret_t nros_client_set_response_callback(
    nros_client_t          *client,
    nros_response_callback_t callback,
    void                   *context);

// UNCHANGED: blocking convenience, same signature as today
nros_ret_t nros_client_call(
    nros_client_t *client,
    const uint8_t *request, size_t request_len,
    uint8_t *response, size_t response_capacity, size_t *response_len);
```

#### Implementation sketch

```rust
// packages/core/nros-c/src/service.rs

#[repr(C)]
pub(crate) struct ClientInternal {
    pub(crate) handle: Option<nros_node::ServiceClientRawHandle>,
    pub(crate) executor_ptr: *mut c_void,           // stashed at add_client
    pub(crate) timeout_ms: u32,                     // 5000 default
    pub(crate) response_callback: nros_response_callback_t,
    pub(crate) context: *mut c_void,
}

pub unsafe extern "C" fn nros_client_init(client, node, type_info, name) {
    // metadata only — DO NOT create RmwServiceClient here
    // (current init code at line 495-538 moves to nros_executor_add_client)
    init_metadata(client, node, type_info, name);
    init_internal(client);  // ClientInternal { handle: None, executor_ptr: null, ... }
    client.state = INITIALIZED;
    NROS_RET_OK
}

pub unsafe extern "C" fn nros_executor_add_client(executor, client) {
    let rust_exec = get_executor_from_ptr(executor);
    let handle = rust_exec.add_service_client_raw(name, type, hash, /*cb trampoline*/)?;
    let internal = client._internal.as_mut::<ClientInternal>();
    internal.handle = Some(handle);
    internal.executor_ptr = executor._opaque.as_mut_ptr() as *mut c_void;
    NROS_RET_OK
}

pub unsafe extern "C" fn nros_client_call(client, req, req_len, resp, cap, resp_len) {
    let internal = client._internal.as_mut::<ClientInternal>();
    if internal.executor_ptr.is_null() { return NROS_RET_NOT_INIT; }
    let executor = internal.executor_ptr;
    let timeout = internal.timeout_ms;

    // Stash existing callback, install one-shot
    static mut BLK_DONE: i32 = -1;
    static mut BLK_BUF: [u8; 4096] = [0u8; 4096];
    static mut BLK_LEN: usize = 0;
    BLK_DONE = -1;

    let orig_cb = internal.response_callback;
    let orig_ctx = internal.context;
    internal.response_callback = Some(blk_response_cb);

    nros_client_send_request_async(client, req, req_len);

    let max_spins = (timeout / 10).max(1);
    for _ in 0..max_spins {
        nros_executor_spin_some(executor, 10_000_000);
        if BLK_DONE >= 0 {
            internal.response_callback = orig_cb;
            internal.context = orig_ctx;
            // copy BLK_BUF[..BLK_LEN] to resp
            *resp_len = BLK_LEN;
            return NROS_RET_OK;
        }
    }
    internal.response_callback = orig_cb;
    internal.context = orig_ctx;
    NROS_RET_TIMEOUT
}
```

This is the same shape as `nros_action_send_goal` in
`packages/core/nros-c/src/action/client.rs:254`. **No `zpico_get` is
reachable from any path here.**

#### Reentrancy guard

The wrapper checks an `in_dispatch` flag on `nros_executor_t` (set by
`nros_executor_spin_some` for the duration of the dispatch loop). If a
user calls `nros_client_call` from inside a subscription/service callback,
the wrapper returns `NROS_RET_REENTRANT` immediately instead of starting
a nested spin. Same guard applies to `nros_action_send_goal` and friends —
add it once in 82.x.

#### Migration

This is **NOT** an ABI break for `nros_client_call`. Existing C user code
needs exactly one new line after init:

```diff
  nros_client_init(&client, &node, "/srv", &type);
+ nros_executor_add_client(&executor, &client);
  nros_client_call(&client, req, req_len, resp, cap, &resp_len);
```

The repository's own examples and tests (FreeRTOS, NuttX, ThreadX, MPS2,
ESP32, native) need this one-line addition. No header surgery, no symbol
renaming, no soft-deprecation period.

The behaviour change for users who don't add the new line is a clean
`NROS_RET_NOT_INIT` from `nros_client_call`, not silent breakage. Phase
82 documents this in the migration notes; existing users will hit it
immediately the first time they run.

### Service server symmetry

Service servers already follow the metadata-then-register lifecycle via
`nros_service_init` + `nros_executor_add_service`, but they currently only
stash a `handle_id` on the server struct — not an `_internal` with
`executor_ptr`. Service servers don't have any blocking helpers (servers
react to incoming requests via callbacks fired from `spin_once`, never
block on outgoing operations), so they don't strictly *need* the executor
pointer.

However, for symmetry with the new client design and with action server,
Phase 82 will introduce a `ServerInternal { handle, executor_ptr }`
mirror so that:

1. The lifecycle shape is identical across all four entity kinds (sub,
   service-{server,client}, action-{server,client}).
2. Future server-side blocking helpers (e.g. `nros_service_wait_for_first_request`
   if we ever want it) have an executor to spin without changing signatures.
3. Diagnostic helpers like `nros_service_get_executor(server)` work the
   same way for server and client.

The service server's `nros_service_init` already defers transport creation
to `nros_executor_add_service`, so this is a small structural cleanup, not
a lifecycle change. Existing user code is unaffected.

### C++ (`nros-cpp`)

C++ uses the `Future<T>` pattern as the single way to express any
operation that has a deferred response. Every send-shaped method on every
client returns a `Future<T>`; there are no blocking convenience overloads,
no stashed executors, no `_async` prefix. The user always writes the
two-step "start the operation, then wait on the future" form. This is the
same shape Rust uses with `Promise<T>`, just spelled in C++14.

#### Design decisions

1. **Single function per operation.** No overloading to expose
   blocking/non-blocking/async variants. `Client<S>::send_request(req)`
   returns a `Future<Response>` and that's the only way to send a request.
   Users who want blocking call `fut.wait(executor, timeout, resp)`; users
   who want non-blocking call `fut.is_ready()` / `fut.try_take(resp)`;
   users who want to await (in a future C++20 build) call `co_await fut`.
   The `Future` is the dispatch point, not the function name.

2. **No stashed state on `Client`.** `Client<S>` does not store an
   `Executor*`. Every blocking operation takes the executor as an explicit
   argument. This matches Rust's `Promise::wait(&mut executor, timeout)`
   exactly, and it makes the executor dependency visible in the type
   signature so reentrancy is easier to reason about.

3. **Consistent across every C++ entity.** The same single-function +
   future-returning convention applies to `ActionClient<A>` and any future
   request/response-shaped API. There is one C++ idiom for "operation
   with a deferred response", end of story. Subscriptions stay as
   streams (different shape) and service servers stay callback-driven
   (no outgoing operations).

#### `Future<T>`

```cpp
namespace nros {

template <typename T>
class Future {
public:
    enum class Status { Ready, NotReady, Failed };

    // Non-blocking primitives
    bool   is_ready() const noexcept;
    Result try_take(T& out);    // Ready → fills out + consumes; NotReady → leaves intact

    // Blocking — explicit executor + timeout, mirrors Rust's Promise::wait.
    // Returns Result::ok() on Ready (out filled), Result(Timeout) on Timeout,
    // Result(propagated error) on Failed.
    Result wait(Executor& executor,
                std::chrono::milliseconds timeout,
                T& out);

    // Move-only single-shot, like std::future
    Future(Future&&) noexcept;
    Future& operator=(Future&&) noexcept;
    Future(const Future&) = delete;
    Future& operator=(const Future&) = delete;
    ~Future();   // calls cancel() if still pending

    void cancel();   // idempotent; releases the slot

    // C++20 coroutine adapter — forward-compatibility, gated behind
    // __cpp_impl_coroutine. Lets users co_await a future without changing
    // the C++14 surface.
    #if defined(__cpp_impl_coroutine)
    bool        await_ready() const noexcept { return is_ready(); }
    void        await_suspend(std::coroutine_handle<> h) noexcept;
    T           await_resume();
    #endif

private:
    template <typename S> friend class Client;
    template <typename A> friend class ActionClient;

    void*  client_storage_ = nullptr;   // back-pointer to the parent
    int    slot_index_     = -1;        // -1 == consumed/empty
    using  Parser          = bool (*)(const uint8_t*, size_t, void*);
    Parser parser_         = nullptr;   // type-erased CDR → T deserializer
};

} // namespace nros
```

The future owns no transport state — it carries a pointer back to the
parent client, an index into that client's pending-slot pool, and a
function pointer that knows how to parse the CDR bytes into `T`. Same
shape as Rust's `Promise<'a, T>` — just three fields glued together.

`wait` spins the passed-in executor:

```cpp
template <typename T>
Result Future<T>::wait(Executor& executor,
                       std::chrono::milliseconds timeout, T& out)
{
    if (slot_index_ < 0) return Result(ErrorCode::AlreadyConsumed);
    auto deadline = std::chrono::steady_clock::now() + timeout;
    while (std::chrono::steady_clock::now() < deadline) {
        executor.spin_some(std::chrono::milliseconds(10));
        if (is_ready()) return try_take(out);
    }
    return Result(ErrorCode::Timeout);
}
```

This is the C++ analogue of:

```rust
pub fn wait(&mut self, executor: &mut Executor, timeout_ms: u64) -> Result<T, NodeError> {
    for _ in 0..max_spins {
        executor.spin_once(spin_interval_ms);
        if let Some(result) = self.try_recv()? { return Ok(result); }
    }
    Err(NodeError::Timeout)
}
```

#### `Client<S>`

```cpp
template <typename S>
class Client {
public:
    using Request  = typename S::Request;
    using Response = typename S::Response;

    // The only way to send. Returns a Future<Response>.
    // Returns a Future already in the Failed state (slot_index_ = -1) if
    // the client has an outstanding request or serialization fails.
    Future<Response> send_request(const Request& req);

    // No call() overload. No async_send_request alias. send_request is it.

private:
    void* storage_ = nullptr;   // opaque C ABI handle from node.create_client
    // No stashed Executor* — wait() takes one explicitly.
};
```

User code:

```cpp
nros::Client<AddTwoInts> client;
NROS_TRY(node.create_client(client, "/add_two_ints"));

ReqType req{1, 2};
auto   fut = client.send_request(req);

// Blocking — mirror of Rust's promise.wait(&mut exec, 500)?
RespType resp;
NROS_TRY(fut.wait(executor, 500ms, resp));
```

Or non-blocking:

```cpp
auto fut = client.send_request(req);
while (!fut.is_ready()) {
    do_other_work();
    executor.spin_some(10ms);
}
RespType resp;
NROS_TRY(fut.try_take(resp));
```

Or co_await (C++20 only, when available):

```cpp
RespType resp = co_await client.send_request(req);
```

The same `client.send_request(req)` call serves all three patterns.

#### `ActionClient<A>` — same pattern, end-to-end

For consistency, every operation on the action client also returns a
`Future<T>`:

```cpp
template <typename A>
class ActionClient {
public:
    using Goal     = typename A::Goal;
    using Feedback = typename A::Feedback;
    using Result   = typename A::Result;

    struct GoalAccept {
        GoalUuid uuid;
        bool     accepted;
    };

    struct ResultStatus {
        GoalStatus status;
        Result     result;
    };

    // Send a goal. Future resolves when the server accepts/rejects.
    Future<GoalAccept> send_goal(const Goal& goal);

    // Request the result for an accepted goal. Future resolves when the
    // server publishes the final result.
    Future<ResultStatus> get_result(const GoalUuid& uuid);

    // Cancel an in-flight goal. Future resolves with the cancel ack.
    Future<CancelResponse> cancel_goal(const GoalUuid& uuid);

    // Feedback is a stream, not a future — see "Streams vs Futures" below.
    FeedbackStream<Feedback> feedback_for(const GoalUuid& uuid);

private:
    void* storage_ = nullptr;
    // No stashed Executor*.
};
```

User code becomes a sequence of `wait` calls, each with an explicit
executor:

```cpp
nros::ActionClient<Fibonacci> client;
NROS_TRY(node.create_action_client(client, "/fibonacci"));

GoalType goal{10};

// 1. Send goal, wait for accept
auto goal_fut = client.send_goal(goal);
GoalAccept accept;
NROS_TRY(goal_fut.wait(executor, 5s, accept));
if (!accept.accepted) { return; }

// 2. Stream feedback while polling for result
auto feedback_stream = client.feedback_for(accept.uuid);
auto result_fut      = client.get_result(accept.uuid);
ResultStatus result;
while (true) {
    Feedback fb;
    if (feedback_stream.try_take(fb) == Status::Ready) {
        std::cout << "feedback: " << fb << "\n";
    }
    if (result_fut.try_take(result) == Result::ok()) break;
    executor.spin_some(10ms);
}

// Or just block on the result and ignore intermediate feedback
NROS_TRY(result_fut.wait(executor, 30s, result));
```

This mirrors how the Rust action client already works
(`client.send_goal(&goal)? → Promise<GoalAccept>` + `feedback_stream_for`).

#### Streams vs futures

Feedback messages and subscription samples are *streams* — multiple
values over time — not single-shot futures. They get a different shape:

```cpp
template <typename T>
class Stream {
public:
    // Non-blocking
    Status try_take(T& out);

    // Blocking — explicit executor, mirrors Rust's wait_next
    Result wait_next(Executor& executor,
                     std::chrono::milliseconds timeout,
                     T& out);

    // C++20 async iteration adapter
    #if defined(__cpp_impl_coroutine)
    AsyncIterator<T> begin() noexcept;
    AsyncIterator<T> end()   noexcept;
    #endif
};
```

`Subscription<T>` exposes a `Stream<T>` directly; the action client's
`feedback_for(uuid)` returns one. Same explicit-executor convention as
`Future<T>::wait`.

#### Service server, action server: unchanged

Service and action servers do not have outgoing blocking operations.
Their request/cancel/result handling is event-driven via callbacks (or
via `try_accept_goal` for the action server). They were never the
problem and don't gain or lose any API in Phase 82. The
`accepted_callback` post-accept hook from the recent C action E2E fix
is preserved as-is.

#### Migration

This is a **hard break** for C++ users — the existing one-liner
`client.call(req, resp)` no longer compiles. Migration is mechanical:

```diff
- ResponseType resp;
- NROS_TRY(client.call(req, resp));
+ ResponseType resp;
+ auto fut = client.send_request(req);
+ NROS_TRY(fut.wait(executor, 5s, resp));
```

Every C++ service-client example in the repo gets the same two-line
substitution in the same PR that lands the new API. C++ has no ABI
contract here so the break is contained: users recompile, the compiler
points at every call site, they update each one. No deprecation shim.

The same migration applies to the action client — every
`async_send_goal` / `async_get_result` style call becomes
`send_goal(...).wait(executor, ...)` / `get_result(...).wait(executor, ...)`.

#### Single-slot vs multi-slot

Phase 82 ships with **one outstanding request per `Client<S>` instance**.
A second `send_request` call before the first future has been consumed
returns a `Future` already in the `Failed` state with
`ErrorCode::Busy`. Users who need parallelism instantiate multiple
`Client<S>` objects today, or wait for the future enhancement that adds
a `MAX_PENDING` template parameter.

This is the same constraint Rust enforces via the borrow checker
(`Promise<'a, T>` borrows `&'a mut Client`). Different mechanism, same
end state.

#### Reentrancy guard

Same as the C side: `nros_executor_t` carries an `in_dispatch` flag set
by `spin_some` for the duration of a dispatch pass. `Future<T>::wait`
checks the flag and returns `Result(ErrorCode::Reentrant)` immediately
if set, without nesting `spin_some`. The test suite includes a
regression test that calls `send_request().wait(...)` from inside a
subscription callback and asserts `Reentrant`.

## Work Items

- [ ] 82.1 — Audit & document the rule
  - **Files**: new `docs/design/blocking-api-rules.md`,
    `book/src/concepts/api-conventions.md` (new section)
  - **Goal**: One canonical statement: "every blocking helper that the
    user can call must take or own an executor, and must spin that
    executor while it waits". Cross-reference Phase 77 (action client)
    and Phase 82 (service client). Confirm via grep that no other public
    API violates the rule.

- [ ] 82.2 — C: defer `nros_client_init` transport creation
  - **Files**: `packages/core/nros-c/src/service.rs` (lines 424–548),
    `packages/core/nros-c/src/types.rs` (or wherever `nros_client_t` is
    defined — add `_internal: [u64; N]` opaque storage like
    `nros_action_client_t`)
  - **Goal**: `nros_client_init` only copies metadata
    (service_name/type_name/type_hash/node_ptr) and zeroes a new
    `ClientInternal` blob. The current `session.create_service_client(...)`
    call moves to `nros_executor_add_client`. Mirrors `nros_service_init`'s
    existing deferral pattern.

- [ ] 82.3 — C: add `nros_executor_add_client`
  - **Files**: `packages/core/nros-c/src/executor.rs`,
    `packages/core/nros-c/include/nros/executor.h`
  - **Goal**: Mirror `nros_executor_add_service`. Calls
    `Executor::add_service_client_raw_sized(...)` (new — see 82.4) on the
    Rust side, captures the returned handle into `ClientInternal.handle`,
    and stashes `executor._opaque.as_mut_ptr()` into
    `ClientInternal.executor_ptr`. Rejects double-registration. Increments
    `executor.handle_count`.

- [ ] 82.4 — nros-node: add `Executor::add_service_client_raw_sized`
  - **Files**: `packages/core/nros-node/src/executor/handles.rs`,
    `packages/core/nros-node/src/executor/arena.rs`
  - **Goal**: Service clients are not currently arena entries. Add a
    `ServiceClientRawArenaEntry` with `try_process =
    service_client_raw_try_process` that polls the pending get slot and
    fires the C trampoline when a reply arrives. Same shape as
    `ActionClientRawArenaEntry::action_client_raw_try_process`. Returns a
    `ServiceClientRawHandle { entry_index }`.

- [ ] 82.5 — C: add the async pair + setters
  - **Files**: `packages/core/nros-c/src/service.rs`,
    `packages/core/nros-c/include/nros/client.h`
  - **Goal**: Public API additions:
    - `nros_client_set_timeout(client, timeout_ms)`
    - `nros_client_set_response_callback(client, cb, ctx)`
    - `nros_client_send_request_async(client, req, req_len)` — calls
      `zpico_get_start` via the arena entry's pending get slot
    - `nros_client_try_recv_response(client, resp, cap, &resp_len)` —
      calls `zpico_get_check`. Returns `NROS_RET_NOT_READY` if pending,
      `NROS_RET_OK` with payload on success.

- [ ] 82.6 — C: rewrite `nros_client_call` on the spin-loop pattern
  - **Files**: `packages/core/nros-c/src/service.rs:602` (the existing
    `nros_client_call`)
  - **Goal**: Same external signature. Internally:
    1. Read `executor_ptr` and `timeout_ms` from `ClientInternal`. Return
       `NROS_RET_NOT_INIT` if `executor_ptr` is null (caller forgot
       `nros_executor_add_client`).
    2. Stash existing `response_callback` + `context`, install a one-shot
       `blk_response_cb` that copies into static buffers.
    3. Call `nros_client_send_request_async`.
    4. Spin: `for _ in 0..(timeout_ms / 10) { nros_executor_spin_some(executor, 10ms); if BLK_DONE { break; } }`.
    5. Restore original callback. Return the result.

- [ ] 82.7 — Service server symmetry
  - **Files**: `packages/core/nros-c/src/service.rs` (server side, lines
    141–226), `packages/core/nros-c/src/types.rs` (add `_internal` to
    `nros_service_t`), `packages/core/nros-c/src/executor.rs`
    (`nros_executor_add_service`)
  - **Goal**: Add a `ServerInternal { handle_id, executor_ptr }` mirror so
    every entity has the same lifecycle shape. `nros_executor_add_service`
    fills both fields. No new public API; no behaviour change for existing
    user code. This is the lifecycle-symmetry cleanup mentioned in the
    "Service server symmetry" section above.

- [ ] 82.8 — Reentrancy guard on `nros_executor_t`
  - **Files**: `packages/core/nros-c/src/executor.rs`,
    `packages/core/nros-c/src/service.rs`,
    `packages/core/nros-c/src/action/client.rs`
  - **Goal**: Add a `bool in_dispatch` field to `nros_executor_t`. Set it
    inside `nros_executor_spin_some` for the duration of the dispatch
    pass, clear it on return. `nros_client_call`,
    `nros_action_send_goal`, and `nros_action_get_result` check the flag
    and return `NROS_RET_REENTRANT` immediately if set.

- [ ] 82.9 — Rust: deprecate `ServiceClientTrait::call_raw`
  - **Files**: `packages/core/nros-rmw/src/traits.rs:965`,
    `packages/zpico/nros-rmw-zenoh/src/shim/service.rs:459`,
    `packages/xrce/nros-rmw-xrce/src/lib.rs`
  - **Goal**: Mark `#[deprecated(note = "use Promise::wait via Client::call")]`.
    Provide a default body that loops `send_request_raw` +
    `try_recv_reply_raw` for one release. Verify no in-tree caller routes
    through it (`grep call_raw`).

- [ ] 82.10 — Update C and C++ examples + tests
  - **Files**: `examples/*/c/zenoh/service-client/src/main.c`,
    `examples/*/cpp/zenoh/service-client/src/main.cpp`,
    `packages/testing/nros-tests/tests/services.rs`,
    `packages/testing/nros-tests/tests/c_api.rs`,
    `packages/testing/nros-tests/tests/nuttx_qemu.rs`,
    `packages/testing/nros-tests/tests/freertos_qemu.rs`
  - **Goal**: Add `nros_executor_add_client(&executor, &client);` after
    `nros_client_init` in every example. Same one-line addition for C++.
    Verify all service-client integration tests still pass on every
    platform.

- [ ] 82.11 — Test coverage: reentrancy + nested spin
  - **Files**: `packages/testing/nros-tests/tests/services.rs`
  - **Goal**: Regression test that calls `nros_client_call` from inside a
    subscription callback and asserts it returns `NROS_RET_REENTRANT`.
    Second test that calls it from outside a callback and asserts a
    successful response (proves the blocking wrapper drives the
    executor).

- [ ] 82.12 — Strip `zpico_get` from the call path entirely
  - **Files**: `packages/zpico/nros-rmw-zenoh/src/shim/service.rs`,
    `packages/zpico/nros-rmw-zenoh/src/zpico.rs`,
    `packages/zpico/zpico-sys/c/zpico/zpico.c`
  - **Goal**: Once 82.2–82.10 land and no caller routes through
    `Context::get`, delete `Context::get` (the Rust wrapper around
    `zpico_get`) and `zpico_get` itself from `zpico.c`. Add a
    `#error "zpico_get is removed; use zpico_get_start + zpico_get_check"`
    if anything still references it. The non-blocking
    `zpico_get_start` / `zpico_get_check` pair stays.

- [ ] 82.13 — Update Phase 64 / parameter services
  - **Files**: `packages/core/nros-node/src/parameter_services.rs`
  - **Goal**: ROS 2 param queries are services. Confirm the C-side
    helpers for requesting them (if any blocking ones exist) route
    through the new executor-driven path. If a "blocking get parameter"
    helper exists, it follows the same `executor_ptr` stash pattern.

- [ ] 82.14 — C++: introduce `Future<T>` + `Stream<T>`
  - **Files**: new `packages/core/nros-cpp/include/nros/future.hpp`,
    new `packages/core/nros-cpp/include/nros/stream.hpp`,
    `packages/core/nros-cpp/src/future.rs` (FFI for slot management)
  - **Goal**: Implement the move-only `Future<T>` template with
    `is_ready()`, `try_take(out)`, `wait(executor, timeout, out)`, and
    `cancel()`. Single-slot per parent client. Type-erased CDR parser
    function pointer. C++20 coroutine adapter gated behind
    `__cpp_impl_coroutine`. Same shape for `Stream<T>` with
    `wait_next(executor, timeout, out)`.

- [ ] 82.15 — C++: rewrite `Client<S>` on `Future<Response>`
  - **Files**: `packages/core/nros-cpp/include/nros/client.hpp`,
    `packages/core/nros-cpp/src/client.rs` (FFI),
    `examples/*/cpp/zenoh/service-client/src/main.cpp`
  - **Goal**: Delete `Client::call`. The only public method is
    `send_request(req) -> Future<Response>`. No stashed executor on
    `Client`. No overloads. Update every C++ service-client example to
    the new two-step form. Hard break, single PR.

- [ ] 82.16 — C++: rewrite `ActionClient<A>` on `Future<T>` + `Stream<T>`
  - **Files**: `packages/core/nros-cpp/include/nros/action_client.hpp`,
    `packages/core/nros-cpp/src/action.rs` (FFI),
    `examples/*/cpp/zenoh/action-client/src/main.cpp`
  - **Goal**: `send_goal(goal)` returns `Future<GoalAccept>`,
    `get_result(uuid)` returns `Future<ResultStatus>`, `cancel_goal(uuid)`
    returns `Future<CancelResponse>`, `feedback_for(uuid)` returns
    `Stream<Feedback>`. Delete every existing async/blocking method
    variant. Update every C++ action-client example. The same hard break
    as 82.15, in the same PR.

- [ ] 82.17 — C++: `Subscription<T>` exposes a `Stream<T>`
  - **Files**: `packages/core/nros-cpp/include/nros/subscription.hpp`
  - **Goal**: Subscriptions become a thin handle whose only access
    method returns/borrows a `Stream<T>`. `subscription.stream()` →
    `Stream<T>&`. Existing callback-style subscription registration
    stays as a separate alternative for users who prefer it. Update C++
    subscriber examples to use the stream form where it reads cleaner.

- [ ] 82.18 — C++: reentrancy guard
  - **Files**: `packages/core/nros-cpp/include/nros/future.hpp`,
    `packages/core/nros-cpp/include/nros/stream.hpp`
  - **Goal**: `Future::wait` and `Stream::wait_next` check the C ABI's
    `in_dispatch` flag (set by `spin_some`) and return
    `ErrorCode::Reentrant` immediately if a callback re-enters. Add a
    regression test alongside the C reentrancy test (work item 82.11).

## Acceptance Criteria

- [ ] **Rust**: no public API blocks on a transport-level primitive
      without taking `&mut Executor`. `ServiceClientTrait::call_raw` is
      removed (or fully deprecated with a non-condvar default body).
- [ ] **C**: every blocking helper (`nros_client_call`,
      `nros_action_send_goal`, `nros_action_get_result`) reads its
      executor from the registered client's stashed `executor_ptr` and
      drives that executor via `nros_executor_spin_some`. None of them
      call `zpico_get`. None of them take new arguments.
- [ ] **C**: every entity (sub, service-{server,client},
      action-{server,client}) follows the same lifecycle:
      metadata-only `init` → `nros_executor_add_*` → use. The asymmetry
      from `nros_client_init` creating its transport handle eagerly is
      eliminated.
- [ ] **C++**: every operation that has a deferred response is exposed
      via exactly one method that returns `Future<T>`. No `call`
      overload, no `async_*` aliases, no stashed executor on any client
      type. Every C++ blocking call goes through `Future::wait(executor,
      timeout, out)` or `Stream::wait_next(executor, timeout, out)`.
- [ ] `grep -rn 'zpico_get\b' packages/core/ packages/zpico/nros-rmw-zenoh/`
      returns zero results outside of `zpico_get_start` / `zpico_get_check`.
      `zpico_get` itself is deleted from `zpico.c`.
- [ ] Reentrancy: a subscription callback that calls a blocking helper
      (C `nros_client_call` / C++ `Future::wait`) returns the
      `Reentrant` error code immediately rather than corrupting executor
      state. Covered by integration tests on at least native POSIX and
      NuttX QEMU.
- [ ] All existing service-client and action-client tests still pass on
      every platform (POSIX, NuttX QEMU, FreeRTOS QEMU, ThreadX,
      ESP32-QEMU, MPS2-AN385).

## Notes & Caveats

- **Reentrancy is still the user's responsibility.** Even with the executor
  argument, calling `nros_client_call(executor, ...)` from inside *that
  executor's* `spin_once` is a reentrant `spin_once`. The wrapper should
  detect this (by checking an "in dispatch" flag on `nros_executor_t`) and
  return `NROS_RET_REENTRANT` rather than corrupting executor state. This
  is a Phase 82 deliverable, not Phase 77.
- **Single-threaded transports already work for the async pair.** On
  bare-metal / smoltcp, `zpico_get_start` queues the query and
  `zpico_get_check` polls the pending-get slot. `spin_once` calls
  `zp_read` between iterations to drain the socket. This is exactly the
  pattern Phase 77 proved out for actions; the service client just needs to
  use it.
- **Timeout granularity.** Like the action helpers, the spin uses a fixed
  10 ms inner step. On icount QEMU virtual time advances faster than wall
  clock during TCP sends, so the user's `timeout_ms` is approximate. This
  is the same caveat documented for `nros_action_send_goal` and is
  acceptable: the alternative (clock-based deadline check inside the spin
  loop) was already proven brittle by the NuttX `z_clock_t` size-mismatch
  bug.
- **rmw_zenoh interop.** The reply key expression and CDR framing don't
  change — only the *poll mechanism* does. ROS 2 interop tests should not
  be affected.
- **No relation to Phase 77 unfinished items.** Phase 77 closed out the
  action-client path; this phase does the equivalent for the service
  client. They share the design principle but are otherwise independent.

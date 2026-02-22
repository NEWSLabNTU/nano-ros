# Phase 50 — Action API Redesign

## Status: Complete

### Progress

| Work Item | Description                                                       | Status      |
|-----------|-------------------------------------------------------------------|-------------|
| 50.1      | Rename `EmbeddedAction{Server,Client}` → `Action{Server,Client}` | Complete    |
| 50.2      | Add `Promise::wait(executor, timeout_ms)`                         | Complete    |
| 50.3      | Update goal callback signature to include `GoalId`                | Complete    |
| 50.4      | Add `active_goals()` / `active_goal_count()` to handle            | Complete    |
| 50.5      | Remove executor-integrated `ActionClientHandle`                   | Complete    |
| 50.6      | Activate status topic publisher                                   | Complete    |
| 50.7      | Update CLAUDE.md + tests                                          | Complete    |
| 50.8      | Remove `Embedded*` backward-compat aliases                        | Complete    |
| 50.9      | Extract `ActionServerCore` (raw-bytes inner type)                 | Complete    |
| 50.10     | Wrap typed `ActionServer<A>` around `ActionServerCore`            | Complete    |
| 50.11     | Extract `ActionClientCore` (raw-bytes inner type)                 | Complete    |
| 50.12     | Wrap typed `ActionClient<A>` around `ActionClientCore`            | Complete    |
| 50.13     | Add `add_action_server_raw()` + `ActionServerRawHandle`           | Complete    |
| 50.14     | Tests and verification                                            | Complete    |

## Background

The current action API has two parallel paths with different semantics:

- **Node API** (`create_action_server/client`): Returns `EmbeddedActionServer`
  / `EmbeddedActionClient` that the user polls manually. The action client
  already uses the `Promise` pattern from Phase 48 — `send_goal()`,
  `cancel_goal()`, and `get_result()` all return `Promise<T>` that can be
  polled with `try_recv()` or `.await`ed.

- **Executor API** (`add_action_server/client`): Returns handles for
  callback-based dispatch. The server handle (`ActionServerHandle`) works
  well, but the client handle (`ActionClientHandle`) only exposes blocking
  methods (`send_goal_blocking`, `cancel_goal_blocking`,
  `get_result_blocking`) that spin the RMW layer internally — violating the
  "only `spin*()` drives the runtime" principle established in Phase 48.

### Problems

1. **Blocking client handle**: `ActionClientHandle::send_goal_blocking()` etc.
   call `ServiceClientTrait::call_raw()` internally, blocking until the reply
   arrives. This prevents timers, subscriptions, and other callbacks from
   firing during the wait.

2. **Can't fix with Promise**: The handle can't return `Promise<T>` because
   Promise borrows `&'a mut ServiceClient`, which lives inside the executor's
   arena. Returning it would conflict with `executor.spin_once()` (both
   borrow the executor). The standalone `ActionClient` doesn't have this
   problem — the client is a separate object from the executor.

3. **Naming**: `EmbeddedActionServer`/`EmbeddedActionClient` are verbose.
   Following the Phase 43 pattern (which renamed `EmbeddedExecutor` →
   `Executor`), these should be `ActionServer`/`ActionClient`.

4. **Goal callback lacks GoalId**: The executor-integrated server's goal
   callback signature is `FnMut(&A::Goal) -> GoalResponse` — the user can't
   see which `GoalId` was assigned to a newly accepted goal.

5. **Unused status publisher**: `EmbeddedActionServer` creates a status topic
   publisher (`_status_publisher`) but never uses it. The ROS 2 action
   protocol requires publishing `GoalStatusArray` on status changes.

6. **No goal query on handle**: `ActionServerHandle` has `publish_feedback()`,
   `complete_goal()`, and `set_goal_status()`, but no way to query active
   goals from the arena.

### Design Principles

Following Phase 48's async service API design:

1. **Only `spin*()` drives the runtime.** Service calls, action calls, and
   other operations do NOT internally drive I/O — they return immediately
   and expect the application to run `spin_once()` or `spin_async()`.

2. **`Promise<T>` for request-response operations.** Following the rclrs
   pattern: `send_goal()` → `Promise<bool>`, `cancel_goal()` →
   `Promise<CancelResponse>`, `get_result()` → `Promise<(GoalStatus, Result)>`.
   Promise supports three consumption patterns:
   - `.try_recv()` — non-blocking poll
   - `.wait(executor, timeout_ms)` — blocking convenience (new)
   - `.await` — async (via `core::future::Future` impl)

3. **Standalone client is the sole client API.** The `ActionClient` (created
   via `Node::create_action_client()`) is separate from the executor, so
   Promise borrows work correctly alongside `executor.spin_once()`.

4. **Executor-integrated server stays.** Callback-based dispatch is right for
   the server side — the executor auto-handles goal acceptance, cancel
   requests, and result serving during `spin_once()`.

## API Design

### Comparison with rclrs 0.7.0

| rclrs feature                                          | nros equivalent                              | Notes                                     |
|--------------------------------------------------------|----------------------------------------------|-------------------------------------------|
| `request_goal()` → `RequestedGoalClient` (Future)      | `send_goal()` → `Promise<bool>`              | Promise is `no_alloc`, borrows client     |
| `receive_feedback()` → `FeedbackClient` (Receiver)     | `try_recv_feedback()` → `Option`             | Non-blocking poll                         |
| `receive_result()` → `ResultClient` (Shared Future)    | `get_result()` → `Promise<(Status, Result)>` | Single consumer (embedded)                |
| `cancel_goal()` → `CancelResponseClient` (Future)      | `cancel_goal()` → `Promise<CancelResponse>`  |                                           |
| `receive_status()` → `StatusClient` (Receiver)         | Status topic auto-publish (server)           | Client status subscription deferred       |
| Typestate server: `RequestedGoal → AcceptedGoal → ...` | `GoalId` + handle methods                    | Goals in fixed-size arena, not user-owned |
| `FeedbackPublisher` (detachable, cloneable)            | `publish_feedback()` on handle               | No `Arc`/`Send+Sync` on embedded          |
| `GoalClient::stream()` (multiplexed events)            | Poll each channel separately                 | No `Stream` trait on `no_std`             |

### ActionServer (standalone — Node API)

Rename `EmbeddedActionServer` → `ActionServer`. Unchanged behavior.

```rust
let mut server = node.create_action_server::<Fibonacci>("/fibonacci")?;

loop {
    executor.spin_once(10);

    if let Some(goal_id) = server.try_accept_goal(|goal| {
        GoalResponse::AcceptAndExecute
    })? {
        server.set_goal_status(&goal_id, GoalStatus::Executing);
    }

    server.try_handle_cancel(|goal_id, status| CancelResponse::Ok)?;
    server.try_handle_get_result()?;

    server.publish_feedback(&goal_id, &feedback)?;
    server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
}
```

### ActionServer (executor-integrated)

Goal callback gains `&GoalId` parameter. Handle gains goal query methods.

```rust
let server_handle = executor.add_action_server::<Fibonacci>(
    "/fibonacci",
    |goal_id: &GoalId, goal: &FibonacciGoal| -> GoalResponse {
        GoalResponse::AcceptAndExecute
    },
    |goal_id: &GoalId, status: GoalStatus| -> CancelResponse {
        CancelResponse::Ok
    },
)?;

loop {
    executor.spin_once(10);

    let count = server_handle.active_goal_count(&executor);
    server_handle.publish_feedback(&mut executor, &goal_id, &feedback)?;
    server_handle.complete_goal(&mut executor, &goal_id, GoalStatus::Succeeded, result);
}
```

### ActionClient (standalone — sole client API)

Rename `EmbeddedActionClient` → `ActionClient`. Promise-based, all
non-blocking. Three consumption patterns per Phase 48.

```rust
let mut client = node.create_action_client::<Fibonacci>("/fibonacci")?;

// send_goal → Promise<bool>
let (goal_id, mut acceptance) = client.send_goal(&goal)?;

// Pattern 1: Sync polling
loop {
    executor.spin_once(10);
    if let Some(accepted) = acceptance.try_recv()? { break; }
}

// Pattern 2: Blocking convenience (new in Phase 50)
let accepted = acceptance.wait(&mut executor, 5000)?;

// Pattern 3: Async .await (via Future impl from Phase 48)
let accepted = acceptance.await?;

// Feedback — non-blocking poll (subscription, not request-response)
if let Some((fid, fb)) = client.try_recv_feedback()? { ... }

// cancel_goal → Promise<CancelResponse>
let mut cancel = client.cancel_goal(&goal_id)?;
let response = cancel.wait(&mut executor, 5000)?;

// get_result → Promise<(GoalStatus, A::Result)>
let mut result = client.get_result(&goal_id)?;
let (status, result) = result.wait(&mut executor, 5000)?;
```

### Promise::wait() — new blocking convenience

```rust
impl<T, Cli: ServiceClientTrait> Promise<'_, T, Cli> {
    /// Block until reply arrives, spinning the executor.
    ///
    /// Internally calls `executor.spin_once()` in a loop until
    /// the reply arrives or `timeout_ms` is exhausted.
    ///
    /// This is equivalent to the manual spin+poll loop pattern
    /// but more ergonomic for simple use cases.
    pub fn wait<S: Session, const M: usize, const C: usize>(
        &mut self,
        executor: &mut Executor<S, M, C>,
        timeout_ms: u64,
    ) -> Result<T, NodeError> { ... }
}
```

No borrow conflict: `executor` and `self` (which borrows the standalone
client) are disjoint objects. The standalone `ActionClient`/`ServiceClient`
is not inside the executor.

## Work Items

### 50.1 — Rename types

**Files:** `handles.rs`, `action.rs`, `node.rs`, re-exports

- `EmbeddedActionServer` → `ActionServer`
- `EmbeddedActionClient` → `ActionClient`
- `EmbeddedActiveGoal` → `ActiveGoal`
- `EmbeddedCompletedGoal` → `CompletedGoal`
- Add backward-compat type aliases with `#[deprecated]`
- Update all examples and tests

### 50.2 — `Promise::wait()`

**File:** `handles.rs`

Add `Promise::wait(executor, timeout_ms)`:

```rust
pub fn wait<S: Session, const M: usize, const C: usize>(
    &mut self,
    executor: &mut Executor<S, M, C>,
    timeout_ms: u64,
) -> Result<T, NodeError> {
    let max_spins = timeout_ms / 10;
    for _ in 0..max_spins.max(1) {
        executor.spin_once(10);
        if let Some(result) = self.try_recv()? {
            return Ok(result);
        }
    }
    Err(NodeError::Timeout)
}
```

Add `NodeError::Timeout` variant to `types.rs`.

### 50.3 — Goal callback signature

**Files:** `action.rs`, `arena.rs`

Change `GoalF` bound from `FnMut(&A::Goal) -> GoalResponse` to
`FnMut(&GoalId, &A::Goal) -> GoalResponse`.

In `action_server_try_process()`, pass the `GoalId` to the callback after
the server has assigned it.

### 50.4 — Goal query methods on `ActionServerHandle`

**Files:** `action.rs`, `arena.rs`

Add function pointers and methods:

```rust
impl ActionServerHandle<A> {
    pub fn active_goal_count<S, M, C>(&self, executor: &Executor<S, M, C>) -> usize;
    pub fn active_goals<S, M, C>(&self, executor: &Executor<S, M, C>) -> &[ActiveGoal<A>];
}
```

Requires new arena accessor function pointers (similar to existing
`as_publish_feedback`).

### 50.5 — Remove `ActionClientHandle`

**Files:** `action.rs`, `arena.rs`

- Delete `ActionClientHandle` struct
- Delete `Executor::add_action_client()` and `add_action_client_sized()`
- Delete `ActionClientArenaEntry`
- Delete `action_client_try_process()`, `ac_send_goal()`, `ac_cancel_goal()`,
  `ac_get_result()`
- Delete `EntryKind::ActionClient`
- Remove `send_goal_blocking`, `cancel_goal_blocking`, `get_result_blocking`
  from `ActionClient` (they were `pub(crate)`, only used by arena code)

### 50.6 — Activate status topic publisher

**Files:** `handles.rs`

- Rename `_status_publisher` → `status_publisher`
- In `set_goal_status()`: publish `GoalStatusArray` containing all active
  goals' statuses
- In `complete_goal()`: publish updated `GoalStatusArray`
- CDR format: sequence of `GoalStatusStamped` (already defined in nros-core)

### 50.7 — Tests and documentation

- Run `just quality`
- Run `just test-c` (C API unaffected but verify no regressions)
- Verify action examples compile and work
- Update `CLAUDE.md` phase table

### 50.8 — Remove `Embedded*` backward-compat aliases

**Files:** `nros-node/src/lib.rs`, `nros/src/lib.rs`

Remove all `Embedded*` type aliases added during Phase 43.13 renames:

- `EmbeddedExecutor` → deleted (use `Executor`)
- `EmbeddedNode` → deleted (use `Node`)
- `EmbeddedNodeError` → deleted (use `NodeError`)
- `EmbeddedConfig` → deleted (use `ExecutorConfig`)
- `EmbeddedSubscription` → deleted (use `Subscription`)
- `EmbeddedActionServer` → deleted (use `ActionServer`)
- `EmbeddedActionClient` → deleted (use `ActionClient`)
- `EmbeddedActiveGoal` → deleted (use `ActiveGoal`)
- `EmbeddedCompletedGoal` → deleted (use `CompletedGoal`)

Also remove from `nros::prelude`:
- `EmbeddedConfig`, `EmbeddedExecutor`, `EmbeddedNode`, `EmbeddedNodeError`,
  `EmbeddedSubscription`

Update all internal uses and examples that still reference `Embedded*` names.

### 50.9 — Extract `ActionServerCore` (raw-bytes inner type)

**Files:** `handles.rs` (new type), possibly `core_action.rs` (new file)

Split the type-agnostic protocol logic out of `ActionServer<A>` into a new
`ActionServerCore` that works entirely with raw `&[u8]` bytes:

```rust
pub struct ActionServerCore<
    Srv,
    Pub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
    const MAX_GOALS: usize = 4,
> {
    send_goal_server: Srv,
    cancel_goal_server: Srv,
    get_result_server: Srv,
    feedback_publisher: Pub,
    status_publisher: Pub,
    active_goals: heapless::Vec<RawActiveGoal, MAX_GOALS>,
    completed_results: heapless::Vec<RawCompletedGoal, MAX_GOALS>,
    goal_buffer: [u8; GOAL_BUF],
    result_buffer: [u8; RESULT_BUF],
    feedback_buffer: [u8; FEEDBACK_BUF],
    cancel_buffer: [u8; 256],
}
```

**Supporting types** (no `A: RosAction` parameter):

```rust
/// Goal tracked by the core — only GoalId + status, no typed goal data.
pub struct RawActiveGoal {
    pub goal_id: GoalId,
    pub status: GoalStatus,
}

/// Completed goal — stores raw CDR result bytes.
pub struct RawCompletedGoal {
    pub goal_id: GoalId,
    pub status: GoalStatus,
    result_offset: usize,  // offset into shared result slab
    result_len: usize,
}
```

**Methods on `ActionServerCore`:**

- `try_accept_goal_raw(&mut self) -> Result<Option<(GoalId, &[u8])>, NodeError>`
  — receives raw CDR from `send_goal_server`, extracts `GoalId` (protocol
  framing), returns `(goal_id, goal_cdr_bytes)` without deserializing the
  goal payload. Does NOT send the reply yet — caller decides accept/reject.

- `accept_goal(&mut self, goal_id: &GoalId, seq: u32) -> Result<(), NodeError>`
  — sends the acceptance reply and adds goal to `active_goals`.

- `reject_goal(&mut self, seq: u32) -> Result<(), NodeError>`
  — sends the rejection reply.

- `publish_feedback_raw(&mut self, goal_id: &GoalId, feedback_cdr: &[u8]) -> Result<(), NodeError>`
  — frames GoalId + raw feedback bytes, publishes via `feedback_publisher`.

- `complete_goal_raw(&mut self, goal_id: &GoalId, status: GoalStatus, result_cdr: &[u8])`
  — moves goal from active to completed, stores raw result bytes.

- `set_goal_status(&mut self, goal_id: &GoalId, status: GoalStatus)`
  — updates status, publishes `GoalStatusArray`. (Unchanged from typed API.)

- `try_handle_cancel(&mut self, handler: impl FnOnce(&GoalId, GoalStatus) -> CancelResponse) -> Result<Option<(GoalId, CancelResponse)>, NodeError>`
  — type-agnostic, identical to current implementation.

- `try_handle_get_result_raw(&mut self) -> Result<Option<GoalId>, NodeError>`
  — looks up completed goal, sends stored raw result bytes as reply.

- `active_goal_count(&self) -> usize`
- `for_each_active_goal(&self, f: impl FnMut(&RawActiveGoal))`

**Key design choice — result storage:** Completed goal results are stored as
raw CDR bytes in a fixed slab (part of `result_buffer`). The typed wrapper
serializes `A::Result` before calling `complete_goal_raw()`. This avoids
the `A::Result: Clone` bound on the core and lets the C API pass raw bytes
directly.

### 50.10 — Wrap typed `ActionServer<A>` around `ActionServerCore`

**Files:** `handles.rs`

Refactor `ActionServer<A>` to contain an `ActionServerCore` and add the
typed layer on top:

```rust
pub struct ActionServer<
    A: RosAction,
    Srv,
    Pub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
    const MAX_GOALS: usize = 4,
> {
    core: ActionServerCore<Srv, Pub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
    /// Typed active goals (parallel to core.active_goals — same indices).
    typed_goals: heapless::Vec<A::Goal, MAX_GOALS>,
    _phantom: PhantomData<A>,
}
```

**Typed methods delegate to core:**

- `try_accept_goal(handler: FnOnce(&GoalId, &A::Goal) -> GoalResponse)`:
  1. Calls `core.try_accept_goal_raw()` → `(goal_id, goal_cdr)`
  2. Deserializes `A::Goal` from `goal_cdr`
  3. Calls `handler(&goal_id, &goal)` → response
  4. If accepted: calls `core.accept_goal()`, pushes typed goal
  5. If rejected: calls `core.reject_goal()`

- `publish_feedback(goal_id, &A::Feedback)`:
  1. Serializes `A::Feedback` into feedback buffer
  2. Calls `core.publish_feedback_raw(goal_id, &serialized)`

- `complete_goal(goal_id, status, A::Result)`:
  1. Serializes `A::Result` into result buffer
  2. Calls `core.complete_goal_raw(goal_id, status, &serialized)`
  3. Removes typed goal from `typed_goals`

- `set_goal_status`, `try_handle_cancel`, `try_handle_get_result`:
  Direct delegation to core.

- `get_goal(goal_id) -> Option<&ActiveGoal<A>>`:
  Looks up in parallel typed_goals + core.active_goals by index.

- `active_goals() -> impl Iterator<Item = ActiveGoal<A>>`:
  Zips core goals (GoalId + status) with typed_goals (A::Goal).

**Behavioral equivalence:** The refactored typed `ActionServer<A>` must
produce identical CDR bytes and identical observable behavior as the
current implementation. All existing tests and examples must pass unchanged.

### 50.11 — Extract `ActionClientCore` (raw-bytes inner type)

**Files:** `handles.rs`

Same inner/outer split for the client side:

```rust
pub struct ActionClientCore<
    Cli,
    Sub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
> {
    send_goal_client: Cli,
    cancel_goal_client: Cli,
    get_result_client: Cli,
    feedback_subscriber: Sub,
    goal_buffer: [u8; GOAL_BUF],
    result_buffer: [u8; RESULT_BUF],
    feedback_buffer: [u8; FEEDBACK_BUF],
}
```

**Methods on `ActionClientCore`:**

- `send_goal_raw(&mut self, goal_cdr: &[u8]) -> Result<(GoalId, Promise<'_, bool, Cli>), NodeError>`
  — generates GoalId, frames GoalId + raw goal bytes, sends via service client,
  returns promise for acceptance.

- `try_recv_feedback_raw(&mut self) -> Result<Option<(GoalId, &[u8])>, NodeError>`
  — receives raw feedback, extracts GoalId, returns `(goal_id, feedback_cdr)`.

- `cancel_goal_raw(&mut self, goal_id: &GoalId) -> Result<Promise<'_, CancelResponse, Cli>, NodeError>`
  — serializes GoalId, sends cancel request, returns promise.

- `get_result_raw(&mut self, goal_id: &GoalId) -> Result<Promise<'_, (GoalStatus, &[u8]), NodeError>`
  — serializes GoalId, sends get_result request, returns promise for
  `(status, result_cdr)`.

**Note on Promise return types:** `Promise<'_, (GoalStatus, &[u8]), Cli>`
may need a different approach since `&[u8]` has a lifetime tied to the
buffer. Options:
- Promise stores the raw bytes in an internal buffer and returns a reference
- Promise returns `(GoalStatus, usize)` where `usize` is the result length,
  and a separate `result_bytes(&self) -> &[u8]` accessor reads from the buffer
- Use a `RawPromise` type that owns the buffer position

Choose the simplest approach that avoids copying.

### 50.12 — Wrap typed `ActionClient<A>` around `ActionClientCore`

**Files:** `handles.rs`

```rust
pub struct ActionClient<A: RosAction, Cli, Sub, ...> {
    core: ActionClientCore<Cli, Sub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
    _phantom: PhantomData<A>,
}
```

**Typed methods delegate to core:**

- `send_goal(&A::Goal) -> (GoalId, Promise<bool>)`:
  Serializes goal, calls `core.send_goal_raw(&serialized)`.

- `try_recv_feedback() -> Option<(GoalId, A::Feedback)>`:
  Calls `core.try_recv_feedback_raw()`, deserializes `A::Feedback`.

- `cancel_goal(goal_id) -> Promise<CancelResponse>`:
  Direct delegation (type-agnostic).

- `get_result(goal_id) -> Promise<(GoalStatus, A::Result)>`:
  Calls `core.get_result_raw()`, deserializes `A::Result` from raw bytes.

### 50.13 — Add `add_action_server_raw()` + `ActionServerRawHandle`

**Files:** `action.rs`, `arena.rs`, `spin.rs`

Add executor registration for raw-bytes action servers, enabling the C API
thin wrapper (Phase 49.4).

**Callback types:**

```rust
/// Raw goal callback: receives GoalId + raw CDR goal bytes.
pub type RawGoalCallback = unsafe extern "C" fn(
    goal_id: *const GoalId,
    goal_data: *const u8,
    goal_len: usize,
    context: *mut core::ffi::c_void,
) -> GoalResponse;

/// Raw cancel callback: receives GoalId + current status.
pub type RawCancelCallback = unsafe extern "C" fn(
    goal_id: *const GoalId,
    status: GoalStatus,
    context: *mut core::ffi::c_void,
) -> CancelResponse;
```

**Executor method:**

```rust
pub fn add_action_server_raw(
    &mut self,
    action_name: &str,
    type_name: &str,
    type_hash: &str,
    goal_callback: RawGoalCallback,
    cancel_callback: RawCancelCallback,
    context: *mut c_void,
) -> Result<ActionServerRawHandle, NodeError>

pub fn add_action_server_raw_sized<
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
>(...) -> Result<ActionServerRawHandle, NodeError>
```

**Arena entry:**

```rust
struct ActionServerRawArenaEntry<const GB, const RB, const FB, const MG> {
    core: ActionServerCore<S::ServiceServerHandle, S::PublisherHandle, GB, RB, FB, MG>,
    goal_callback: RawGoalCallback,
    cancel_callback: RawCancelCallback,
    context: *mut c_void,
}
```

**Dispatch function:** `action_server_raw_try_process()` — calls
`core.try_accept_goal_raw()`, invokes C goal callback with raw bytes,
calls `core.accept_goal()` or `core.reject_goal()` based on response.
Calls `core.try_handle_cancel()` wrapping the C cancel callback.
Calls `core.try_handle_get_result_raw()`.

**Handle:**

```rust
#[derive(Clone, Copy)]
pub struct ActionServerRawHandle {
    entry_index: usize,
    publish_feedback_raw_fn: unsafe fn(*mut u8, &GoalId, &[u8]) -> Result<(), NodeError>,
    complete_goal_raw_fn: unsafe fn(*mut u8, &GoalId, GoalStatus, &[u8]),
    set_goal_status_fn: unsafe fn(*mut u8, &GoalId, GoalStatus),
    active_goal_count_fn: unsafe fn(*const u8) -> usize,
}

impl ActionServerRawHandle {
    pub fn publish_feedback_raw<S, M, C>(&self, executor: &mut Executor<S, M, C>, goal_id: &GoalId, feedback: &[u8]) -> Result<(), NodeError>;
    pub fn complete_goal_raw<S, M, C>(&self, executor: &mut Executor<S, M, C>, goal_id: &GoalId, status: GoalStatus, result: &[u8]);
    pub fn set_goal_status<S, M, C>(&self, executor: &mut Executor<S, M, C>, goal_id: &GoalId, status: GoalStatus);
    pub fn active_goal_count<S, M, C>(&self, executor: &Executor<S, M, C>) -> usize;
}
```

**`EntryKind`:** Add `ActionServerRaw` variant.

### 50.14 — Tests and verification

- Verify all existing action tests pass unchanged (typed API behavioral
  equivalence after inner/outer split)
- Add unit tests for `ActionServerCore` raw-bytes methods
- Add unit tests for `ActionClientCore` raw-bytes methods
- Add unit test for `add_action_server_raw()` executor registration
- Run `just quality`
- Update `CLAUDE.md` phase table

## Dependencies

- Phase 48 (async service API) — complete, provides `Promise` type
- Phase 47 (executor trigger conditions) — complete, provides `HandleId`

## Risk Assessment

**Low risk** for 50.1–50.8. Most changes are renames and removals. The core
`Promise` mechanism is already proven from Phase 48.

**Medium risk** for 50.9–50.14. The inner/outer split is a significant
refactor of `ActionServer` and `ActionClient` internals:

- **Result storage in `ActionServerCore`**: Storing raw CDR result bytes
  requires a slab or ring buffer inside the core. Must handle the case where
  `MAX_GOALS` completed results accumulate before `get_result` requests
  arrive. The typed API currently stores `A::Result` directly — the raw
  core must store serialized bytes instead, which uses more buffer space.

- **Promise lifetime for raw client**: `get_result_raw()` returns raw bytes
  that live in the core's buffer. The `Promise` must ensure the buffer isn't
  overwritten before the caller reads the result. May need a `RawPromise`
  variant or a two-phase read pattern.

- **Behavioral equivalence**: The refactored typed wrapper must produce
  identical CDR output. Goal ID extraction, status array serialization,
  and cancel response framing must be byte-identical.

- **`Promise::wait()`**: Simple loop wrapper around existing `spin_once()`
  + `try_recv()`. Timeout is approximate (loop iteration based, not
  clock-based) — acceptable for embedded.

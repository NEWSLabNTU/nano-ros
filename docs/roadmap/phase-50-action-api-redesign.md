# Phase 50 — Action API Redesign

## Status: Complete

### Progress

| Work Item | Description                                                      | Status   |
|-----------|------------------------------------------------------------------|----------|
| 50.1      | Rename `EmbeddedAction{Server,Client}` → `Action{Server,Client}` | Complete |
| 50.2      | Add `Promise::wait(executor, timeout_ms)`                        | Complete |
| 50.3      | Update goal callback signature to include `GoalId`               | Complete |
| 50.4      | Add `active_goals()` / `active_goal_count()` to handle           | Complete |
| 50.5      | Remove executor-integrated `ActionClientHandle`                  | Complete |
| 50.6      | Activate status topic publisher                                  | Complete |
| 50.7      | Update CLAUDE.md + tests                                         | Complete |
| 50.8      | Remove `Embedded*` backward-compat aliases                       | Complete |

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

## Dependencies

- Phase 48 (async service API) — complete, provides `Promise` type
- Phase 47 (executor trigger conditions) — complete, provides `HandleId`

## Risk Assessment

**Low risk.** Most changes are renames and removals. The core `Promise`
mechanism is already proven from Phase 48.

- **Status publisher**: New behavior, but straightforward (serialize
  `GoalStatusArray` and publish). No protocol change — matches ROS 2 spec.
- **Removing `ActionClientHandle`**: Breaking change, but the executor-
  integrated client was only used in arena dispatch code (not in any
  examples). All examples already use the standalone `ActionClient` with
  `Promise`.
- **`Promise::wait()`**: Simple loop wrapper around existing `spin_once()`
  + `try_recv()`. Timeout is approximate (loop iteration based, not
  clock-based) — acceptable for embedded.

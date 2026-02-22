# Phase 52 — Action Feedback Stream API

**Status:** Complete

## Background

Action clients currently poll for feedback using a manual `spin_once()` +
`try_recv_feedback()` loop. This is verbose and doesn't compose well with
async runtimes or iterator-style processing. Meanwhile, `Promise` already
implements `core::future::Future` for single-value replies (service calls,
goal acceptance, get_result). Feedback is the remaining gap — it produces
*multiple* values over time, which maps naturally to `futures::Stream`.

## Goals

- Model action feedback as `futures_core::Stream` for async composability
- Provide `StreamExt` combinator support (e.g., `take_while`, `for_each`)
- Keep the `no_std` / embedded story intact (optional dependency)
- Offer sync-mode equivalent (`wait_next`) matching `Promise::wait()` pattern
- Simplify action client examples

## Non-Goals

- Auto-terminating streams (feedback doesn't carry "done" semantics)
- Combined feedback+result stream (user calls `get_result()` separately)
- Multi-threaded waker strategies (current `wake_by_ref` pattern is fine)

## Design

### `FeedbackStream` type

```rust
pub struct FeedbackStream<'a, A, Cli, Sub, const GB: usize, const RB: usize, const FB: usize> {
    client: &'a mut ActionClient<A, Cli, Sub, GB, RB, FB>,
}
```

Borrows `&mut ActionClient` exclusively. Fits the action lifecycle — the
promise from `send_goal()` is consumed before `feedback_stream()` is called,
and `get_result()` is called after the stream is dropped.

### Three access modes

| Mode | Method | Feature gate | Use case |
|------|--------|-------------|----------|
| Async (`Stream` trait) | `poll_next()` via `futures_core::Stream` | `stream` feature | `StreamExt` combinators |
| Async (no deps) | `async fn recv()` via `core::future::poll_fn` | Always available | `while let` loops |
| Sync (blocking) | `wait_next(&mut executor, timeout_ms)` | Always available | Non-async code |

### Goal-filtered variant

`feedback_stream_for(&goal_id)` returns a `GoalFeedbackStream` that
pre-filters by goal ID and yields `Result<A::Feedback, NodeError>` instead
of `Result<(GoalId, A::Feedback), NodeError>`.

### Feature wiring

```
nros (stream) → nros-node (stream) → futures-core 0.3 (optional, no_std)
```

### Async usage (StreamExt)

```rust
use futures::StreamExt;

tokio::task::spawn_local(async move { executor.spin_async().await });

let (goal_id, promise) = client.send_goal(&goal)?;
let accepted = promise.await?;

// StreamExt::next() drives the stream one item at a time.
// The inherent method is named recv() to avoid shadowing StreamExt::next().
{
    let mut stream = client.feedback_stream_for(goal_id);
    while let Some(result) = stream.next().await {
        let feedback = result?;
        println!("Feedback: {:?}", feedback.sequence);
        if feedback.sequence.len() as i32 > goal.order { break; }
    }
} // stream dropped — releases &mut client

let (status, result) = client.get_result(&goal_id)?.await?;
```

### Sync usage (wait_next)

```rust
let mut stream = client.feedback_stream();
while let Some((id, fb)) = stream.wait_next(&mut executor, 1000)? {
    if fb.sequence.len() as i32 > goal.order { break; }
}
```

## Work Items

- [x] 52.1 — Add `futures-core` optional dep + `stream` feature to `nros-node` and `nros`
- [x] 52.2 — Implement `FeedbackStream` (async fn next, wait_next, Stream impl)
- [x] 52.3 — Implement `GoalFeedbackStream` (goal-filtered variant)
- [x] 52.4 — Add `feedback_stream()` and `feedback_stream_for()` to `ActionClient`
- [x] 52.5 — Re-export from `nros` crate
- [x] 52.6 — Update action client examples to use `FeedbackStream`
- [x] 52.7 — Create async action client example using `StreamExt`
- [x] 52.8 — Add `stream` feature to clippy check matrix

## Files Modified

| File | Change |
|------|--------|
| `packages/core/nros-node/Cargo.toml` | Add `futures-core` optional dep, `stream` feature |
| `packages/core/nros/Cargo.toml` | Add `stream` feature forwarding |
| `packages/core/nros-node/src/executor/handles.rs` | `FeedbackStream`, `GoalFeedbackStream`, ActionClient methods |
| `packages/core/nros/src/lib.rs` | Re-export stream types |
| `examples/native/rust/zenoh/action-client/src/main.rs` | Use `wait_next` |
| `examples/native/rust/xrce/action-client/src/main.rs` | Use `wait_next` |
| `examples/zephyr/rust/zenoh/action-client/src/lib.rs` | Use `wait_next` |
| `examples/native/rust/zenoh/async-action/` | New example with `StreamExt` |
| `justfile` | Add `stream` feature to check matrix (if needed) |

## Acceptance Criteria

- `just quality` passes (default features — no `stream`)
- `cargo clippy -p nros-node --features stream` passes
- `cargo clippy -p nros --features "std,rmw-zenoh,platform-posix,ros-humble,stream"` passes
- Action client examples compile and work with `wait_next`
- Async action example compiles with `stream` feature

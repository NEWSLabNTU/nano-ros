# Rust API

This chapter provides a concise overview of the nano-ros Rust API. For full
details, see the [rustdoc documentation](https://docs.rs/nros/) or generate
it locally with `just doc`.

## Prelude

Import everything you need with a single statement:

```rust
use nros::prelude::*;
```

## Executor

The executor owns the transport session and manages all callbacks. Its static
memory layout is controlled via environment variables at build time:

- **`NROS_EXECUTOR_MAX_CBS`** (default 4) -- maximum registered callbacks
  (subscriptions + timers + services)
- **`NROS_EXECUTOR_ARENA_SIZE`** (default 4096) -- byte budget for storing
  callback closures inline

Construct an `ExecutorConfig` and pass it to `Executor::open`:

```rust
let config = ExecutorConfig::from_env().node_name("my_node");
let mut executor = Executor::open(&config)?;
```

On embedded targets without `std`, build the config manually:

```rust
let config = ExecutorConfig::new("tcp/192.168.1.50:7447")
    .node_name("talker")
    .domain_id(0);
let mut executor = Executor::open(&config)?;
```

### Spin Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `spin_once(timeout_ms)` | `fn(&mut self, i32) -> SpinOnceResult` | Drive I/O for up to `timeout_ms` ms, then dispatch ready callbacks once. Returns immediately after processing. |
| `spin(timeout_ms)` | `fn(&mut self, i32) -> !` | Infinite loop of `spin_once(timeout_ms)`. Never returns. |
| `spin_blocking(opts)` | `fn(&mut self, SpinOptions) -> Result<(), NodeError>` | Block until halt signaled. Requires `std`. |
| `spin_period(period)` | `fn(&mut self, Duration) -> Result<(), NodeError>` | Spin at a fixed rate, sleeping between iterations. Requires `std`. |
| `spin_async()` | `async fn(&mut self)` | Future that drives the executor. Use with an external async runtime. |

**`spin_once` vs `spin_blocking`**: `spin_once(timeout_ms)` is the low-level
primitive — one poll cycle, then returns control. `spin_blocking` /
`spin_period` are `std`-only convenience wrappers. On `no_std` embedded
targets, call `spin_once()` in your own loop or use `spin(timeout_ms)` when
you never need to return.

> **Note** (Phase 84.D7): every wait API currently takes `i32` / `u64`
> milliseconds. These will migrate to `core::time::Duration` and drop the
> "negative means indefinite" sentinel — it is not exercised by any in-repo
> example and has a latent bug with timer delta. See [Phase 84 roadmap][1].

[1]: https://github.com/.../docs/roadmap/phase-84-api-ergonomics-and-consistency.md

## Node

Create a node from the executor to get a handle for declaring communication
primitives:

```rust
let mut node = executor.create_node("my_node")?;
```

## Publisher

```rust
let publisher = node.create_publisher::<Int32>("/my_topic")?;
publisher.publish(&Int32 { data: 42 })?;
```

Use `create_publisher_with_qos()` to specify QoS settings.

## Subscription

### Callback-based (via executor)

The callback signature is `FnMut(&M) + 'static`:

```rust
executor.add_subscription::<Int32, _>("/topic", |msg: &Int32| {
    // handle message
})?;
```

Variants:

- `add_subscription_with_info()` — callback receives `(&M, &MessageInfo)`
- `add_subscription_with_safety()` — includes E2E integrity status

Use the `_sized` variants (e.g., `add_subscription_sized::<M, _, 2048>()`)
for messages larger than the default 1024-byte receive buffer, or
`add_subscription_buffered` for deeper QoS depth.

### Manual poll (via node)

```rust
let mut sub = node.create_subscription::<Int32>("/topic")?;
executor.spin_once(10);
if let Some(msg) = sub.try_recv()? {
    // handle message
}
```

## Service

### Server

Callback-based via executor:

```rust
executor.add_service::<AddTwoInts, _>("/add", |req: &AddTwoIntsRequest| {
    AddTwoIntsResponse { sum: req.a + req.b }
})?;
```

Or create a standalone server via the node and call its poll methods from a
spin loop.

### Client

```rust
let mut client = node.create_client::<AddTwoInts>("/add")?;
```

Non-blocking — `call()` returns a `Promise` that resolves on subsequent
`spin_once()` calls:

```rust
let mut promise = client.call(&request)?;              // send, returns immediately

executor.spin_once(10);
if let Some(reply) = promise.try_recv()? {
    println!("Result: {}", reply.sum);
}

// Or wait with timeout (internally spins the executor):
let reply = promise.wait(&mut executor, 5000)?;
```

`Promise<T>` also implements `core::future::Future<Output = Result<T,
NodeError>>`, so you can `.await` it from an async runtime.

## Action

### Server (callback-based via executor)

Goal and cancel callbacks are registered together. The goal callback
signature is `FnMut(&GoalId, &A::Goal) -> GoalResponse`; cancel is
`FnMut(&GoalId, GoalStatus) -> CancelResponse`:

```rust
use nros::prelude::*;
use nros_core::{GoalId, GoalResponse, GoalStatus, CancelResponse};

executor.add_action_server::<Fibonacci, _, _>(
    "/fibonacci",
    |_id: &GoalId, goal: &FibonacciGoal| {
        if goal.order > 46 { GoalResponse::Reject }
        else { GoalResponse::AcceptAndExecute }
    },
    |_id: &GoalId, _status: GoalStatus| CancelResponse::Accept,
)?;
```

The executor dispatches goal / cancel / accept events during `spin_once()`.
Use the returned `ActionServerHandle<A>` to publish feedback and complete
goals:

```rust
let handle = executor.add_action_server::<Fibonacci, _, _>(...)?;
handle.publish_feedback(&goal_id, &FibonacciFeedback { partial_sequence: seq })?;
handle.complete_goal(&goal_id, &FibonacciResult { sequence: seq }, GoalStatus::Succeeded)?;
```

Use `add_action_server_sized::<A, _, _, GB, RB, FB, MAX_GOALS>()` to tune
buffer sizes and the concurrent-goal cap.

### Server (manual-poll via node)

```rust
let mut server = node.create_action_server::<Fibonacci>("/fibonacci")?;
```

`create_action_server()` is **not** arena-registered — `spin_once()` does
not drain the server's three channels automatically. Call `poll()` on
every loop iteration to accept new goals, handle cancel requests, and
drain result queries:

```rust
loop {
    executor.spin_once(10);
    server.poll(
        |_id, goal| {
            if goal.order > 46 { GoalResponse::Reject }
            else              { GoalResponse::AcceptAndExecute }
        },
        |_id, _status| CancelResponse::Accept,
    )?;
}
```

If you need finer control, the three draining methods are also exposed
individually: `try_accept_goal()`, `try_handle_cancel()`,
`try_handle_get_result()`. `poll()` calls them in the same order.

### Client

```rust
let mut client = node.create_action_client::<Fibonacci>("/fibonacci")?;

// send_goal returns (GoalId, Promise<'_, bool>)
let (goal_id, mut accepted) = client.send_goal(&FibonacciGoal { order: 10 })?;
let was_accepted = accepted.wait(&mut executor, 5000)?;

if was_accepted {
    let mut result_promise = client.get_result(&goal_id)?;
    let (status, result) = result_promise.wait(&mut executor, 10_000)?;
}

// Feedback is a stream, not a one-shot promise:
let mut fb_stream = client.feedback_stream();
if let Some((id, fb)) = fb_stream.try_next()? {
    // handle feedback
}

// Cancellation returns a Promise<CancelResponse>:
let mut cancel = client.cancel_goal(&goal_id)?;
let response = cancel.wait(&mut executor, 5000)?;
```

`GoalStatus` values: `Unknown(0)`, `Accepted(1)`, `Executing(2)`,
`Canceling(3)`, `Succeeded(4)`, `Canceled(5)`, `Aborted(6)`.

## Timer

Both variants take a `FnMut() + 'static` closure (no argument). They return
a `HandleId`:

```rust
let timer_id = executor.add_timer(TimerDuration::from_millis(1000), || {
    // called every second
})?;

executor.add_timer_oneshot(TimerDuration::from_millis(5000), || {
    // called once after 5 seconds
})?;
```

The repeating / one-shot distinction is in the function name, not a mode
enum. The executor computes the delta internally from the `spin_once`
timeout argument (another thing Phase 84.D7 will clean up).

## Parameters

`ParameterServer::declare` returns `bool` (success / name-already-taken):

```rust
use nros::prelude::*;
use nros::{ParameterServer, ParameterValue};

let mut params = ParameterServer::new();
if !params.declare("max_speed", ParameterValue::Double(1.5)) {
    // name already declared
}
```

For typed, constrained parameters use the `ParameterBuilder`. Note that
`new()` takes the server first, and the terminal methods are
`.mandatory()` / `.optional()` / `.read_only()` — **there is no `.build()`**:

```rust
use nros_params::ParameterBuilder;

let mut speed = ParameterBuilder::<f64>::new(&mut params, "max_speed")
    .default(1.5)
    .description("Maximum speed in m/s")
    .range(0.0..=10.0)?
    .mandatory()?;   // returns MandatoryParameter<'a, f64>

let v = speed.get();           // typed read
speed.set(2.0)?;               // typed write
```

Enable the `param-services` feature on `nros-node` to expose the standard
ROS 2 parameter services (`~/get_parameters`, `~/set_parameters`, etc.).

## Error Types

The primary user-facing error type is `NodeError`, returned by every
executor and node operation. Transport-level failures are wrapped as
`NodeError::Transport(TransportError)`.

> **Note** (Phase 84.D1): `NodeError` and the richer `NanoRosError` will
> merge into a single public error at the `nros` crate boundary; per-crate
> errors will become internal. See the Phase 84 roadmap.

## Transport Backends

The transport backend is selected at compile time via feature flags:

- `rmw-zenoh` → zenoh-pico transport
- `rmw-xrce` → XRCE-DDS transport

Exactly one RMW feature must be enabled. The concrete session type is
resolved automatically; advanced users can access it via
`nros::internals::RmwSession`.

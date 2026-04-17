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

```rust
let config = ExecutorConfig::from_env().node_name("my_node");
let mut executor = Executor::open(&config)?;
```

### Spin Methods

| Method | Description |
|--------|-------------|
| `spin_once(timeout_ms)` | Poll once, process ready callbacks. Returns immediately after processing. |
| `spin(count)` | Call `spin_once()` in a loop `count` times (convenience for examples). |
| `spin_blocking(opts)` | Block forever processing callbacks. Requires `std`. Uses `SpinOptions` for configuration. |
| `spin_period(duration)` | Spin at a fixed rate, sleeping between iterations. Requires `std`. Returns `SpinPeriodResult`. |
| `spin_async()` | Returns a future that drives the executor. Use with an external async runtime (tokio, Embassy). Does not call `block_on` — the caller's runtime provides the event loop. |

**`spin_once` vs `spin_blocking`:** `spin_once(timeout_ms)` is the low-level primitive — one poll cycle, returns control immediately. `spin_blocking` and `spin_period` are convenience wrappers for common patterns. On `no_std` embedded targets, use `spin_once()` in a loop or `spin(count)`.

## Node

Create a node from the executor to get a typed handle for creating
communication primitives:

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

```rust
executor.add_subscription::<Int32, _>("/topic", |msg: &Int32| {
    // handle message
})?;
```

Variants: `add_subscription_with_info()` (includes `MessageInfo`),
`add_subscription_with_safety()` (includes E2E integrity status).

Use the `_sized` variants (e.g., `add_subscription_sized::<M, F, 2048>()`)
for messages larger than the default 1024-byte receive buffer.

### Manual poll (via node)

```rust
let sub = node.create_subscription::<Int32>("/topic")?;
if let Some(msg) = sub.try_recv()? {
    // handle message
}
```

## Service

### Server

```rust
let service = node.create_service::<AddTwoInts>("/add")?;
```

Register a handler callback via the executor, or poll manually.

### Client

```rust
let mut client = node.create_client::<AddTwoInts>("/add")?;
```

**Non-blocking (recommended):**

```rust
let promise = client.call(&request)?;  // returns Promise<Response>

// Promise resolves on subsequent spin_once() calls
executor.spin_once(100);
if let Some(reply) = promise.try_recv()? {
    println!("Result: {}", reply.sum);
}

// Or wait with timeout
let reply = promise.wait(&mut executor, 5000)?;
```

**Blocking (legacy):**

```rust
let reply = client.call_blocking(&request, 5000)?;  // blocks up to 5s
```

`call_blocking()` internally calls `spin_once()` in a loop. Prefer `call()` + `Promise` for better control over timeouts and cancellation.

## Action

### Server

```rust
let mut action_server = node.create_action_server::<Fibonacci>("/fibonacci")?;
```

**Callback-based (via executor):**

```rust
executor.add_action_server::<Fibonacci, _>("/fibonacci", |goal| {
    // Process goal, publish feedback, return result
    let mut feedback = FibonacciFeedback { sequence: heapless::Vec::new() };
    // ... compute ...
    FibonacciResult { sequence: feedback.sequence }
})?;
```

**Manual-poll:**

```rust
let mut server = node.create_action_server::<Fibonacci>("/fibonacci")?;

// create_action_server() is NOT arena-registered — spin_once() does NOT
// process get_result queries automatically. After completing a goal, you
// must explicitly call:
server.try_handle_get_result()?;
```

### Client

```rust
let mut action_client = node.create_action_client::<Fibonacci>("/fibonacci")?;

// send_goal() returns (GoalId, Promise<bool>)
let (goal_id, accepted) = action_client.send_goal(&goal)?;
let was_accepted = accepted.wait(&mut executor, 5000)?;

// get_result() returns Promise<(GoalStatus, Result)>
let result_promise = action_client.get_result(&goal_id)?;
let (status, result) = result_promise.wait(&mut executor, 10000)?;
```

**GoalStatus** values: `Unknown(0)`, `Accepted(1)`, `Executing(2)`, `Canceling(3)`, `Succeeded(4)`, `Canceled(5)`, `Aborted(6)`.

## Timer

```rust
executor.add_timer(TimerDuration::from_millis(1000), TimerMode::Repeating, |_| {
    // called every second
})?;

executor.add_timer_oneshot(TimerDuration::from_millis(5000), |_| {
    // called once after 5 seconds
})?;
```

Callbacks receive a `&TimerState` argument.

## Parameters

```rust
use nros::prelude::*;
use nros::{ParameterServer, ParameterValue};

let mut params = ParameterServer::new();
params.declare("max_speed", ParameterValue::Double(1.5))?;
```

Use `ParameterBuilder` for typed parameter declaration with constraints:

```rust
use nros_params::ParameterBuilder;

ParameterBuilder::<f64>::new("max_speed")
    .default(1.5)
    .description("Maximum speed in m/s")
    .build(&mut params)?;
```

Enable the `param-services` feature on `nros-node` to expose standard ROS 2
parameter services (`~/get_parameters`, `~/set_parameters`, etc.).

## Error Types

The primary error type is `NodeError`, returned by executor and node
operations. Transport-level errors use `TransportError`.

## Transport Backends

The transport backend is selected at compile time via feature flags:

- `rmw-zenoh` --> zenoh-pico transport
- `rmw-xrce` --> XRCE-DDS transport

The concrete session type is resolved automatically. Advanced users can access
it via `nros::internals::RmwSession`.

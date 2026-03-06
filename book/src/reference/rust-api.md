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

The executor owns the transport session and manages all callbacks. Its const
generics control static memory layout:

- **`MAX_CBS`** -- maximum registered callbacks (subscriptions + timers + services)
- **`CB_ARENA`** -- byte budget for storing callback closures inline (4096 is
  generous for most use cases)

```rust
let config = ExecutorConfig::from_env().node_name("my_node");
let mut executor: Executor<_> = Executor::open(&config)?;
```

### Spin Methods

| Method | Description |
|--------|-------------|
| `spin_once(timeout_ms)` | Poll once, process ready callbacks |
| `spin_blocking(opts)` | Block forever processing callbacks (requires `std`) |
| `spin_period(duration)` | Spin at a fixed rate (requires `std`) |
| `spin_async().await` | Async spin loop (never returns) |

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
let client = node.create_client::<AddTwoInts>("/add")?;
let promise: Promise<AddTwoInts> = client.call(&request)?;
```

`Promise` resolves on a subsequent `spin_once()` call.

Use the `_sized` variants to control request/reply buffer sizes.

## Action

### Server

```rust
let action_server = node.create_action_server::<Fibonacci>("/fibonacci")?;
```

### Client

```rust
let action_client = node.create_action_client::<Fibonacci>("/fibonacci")?;
```

Goal feedback is delivered via `FeedbackStream`. See the `action-server` and
`action-client` examples for complete usage.

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

Types are generic over `S: Session`. You do not need to name `S` explicitly --
the compiler infers it from your enabled feature flag:

- `rmw-zenoh` --> `ZenohSession`
- `rmw-xrce` --> `XrceSession`

Advanced users can access the concrete session type via
`nros::internals::RmwSession`.

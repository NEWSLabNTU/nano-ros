# Adding a New RMW Backend

This guide explains how to implement a new RMW (ROS Middleware) backend for
nano-ros. An RMW backend is a crate that implements the transport traits
defined in `nros-rmw`, enabling nano-ros to send and receive ROS 2 messages
over a new middleware protocol.

## Overview

nano-ros uses a trait-based abstraction layer (`nros-rmw`) that decouples
application code from the underlying transport. Two backends ship today:

- **`nros-rmw-zenoh`** — zenoh-pico (peer-to-peer, TCP/UDP/TLS)
- **`nros-rmw-xrce`** — Micro-XRCE-DDS-Client (agent-based, UDP/serial)

Adding a third backend (e.g., DDS, MQTT, custom protocol) means implementing
the same trait hierarchy and wiring it into the feature flag system.

## Trait Hierarchy

All traits are defined in `packages/core/nros-rmw/src/traits.rs`. Your
backend must implement:

```
Rmw              — Factory: creates Sessions from RmwConfig
└─ Session       — Connection lifecycle, creates handles
   ├─ Publisher   — Send serialized messages
   ├─ Subscriber  — Receive messages (non-blocking poll)
   ├─ ServiceServerTrait — Request/reply (server side)
   └─ ServiceClientTrait — Request/reply (client side)
```

### Rmw (factory)

The entry point. Creates a session from middleware-agnostic configuration.

```rust
pub trait Rmw {
    type Session: Session;
    type Error;

    fn open(config: &RmwConfig) -> Result<Self::Session, Self::Error>;
}
```

`RmwConfig` provides:

| Field       | Type          | Description                                    |
|-------------|---------------|------------------------------------------------|
| `locator`   | `&str`        | Connection string (e.g., `tcp/127.0.0.1:7447`) |
| `mode`      | `SessionMode` | `Client` or `Peer`                             |
| `domain_id` | `u32`         | ROS 2 domain ID                                |
| `node_name` | `&str`        | Node name for discovery                        |
| `namespace` | `&str`        | Node namespace                                 |

Your `open()` maps these to backend-specific connection parameters and
establishes the transport session.

### Session

Manages the connection lifecycle and creates communication handles.

```rust
pub trait Session {
    type Error;
    type PublisherHandle;
    type SubscriberHandle;
    type ServiceServerHandle;
    type ServiceClientHandle;

    fn create_publisher(
        &mut self, topic: &TopicInfo, qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error>;

    fn create_subscriber(
        &mut self, topic: &TopicInfo, qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error>;

    fn create_service_server(
        &mut self, service: &ServiceInfo,
    ) -> Result<Self::ServiceServerHandle, Self::Error>;

    fn create_service_client(
        &mut self, service: &ServiceInfo,
    ) -> Result<Self::ServiceClientHandle, Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error>;

    // Poll-based I/O — call periodically to process incoming data
    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error> {
        let _ = timeout_ms;
        Ok(())
    }
}
```

**Key types passed to handle creation:**

- `TopicInfo` — topic name, ROS type name, type hash, domain ID, node
  name/namespace (for liveliness)
- `ServiceInfo` — same fields for services
- `QosSettings` — history, reliability, durability, depth

### Publisher

Send CDR-encoded messages. Only `publish_raw` and the two error helpers
need implementation — `publish` has a default that serializes and delegates.

```rust
pub trait Publisher {
    type Error;

    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error>;

    // Default provided — serializes M then calls publish_raw
    fn publish<M: RosMessage>(&self, msg: &M, buf: &mut [u8]) -> Result<(), Self::Error>;

    fn buffer_error(&self) -> Self::Error;
    fn serialization_error(&self) -> Self::Error;
}
```

### Subscriber

Non-blocking message receive. Only `try_recv_raw` and `deserialization_error`
are required — all other methods have defaults.

```rust
pub trait Subscriber {
    type Error;

    // Required
    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;
    fn deserialization_error(&self) -> Self::Error;

    // Optional overrides
    fn has_data(&self) -> bool { true }
    fn try_recv<M: RosMessage>(&mut self, buf: &mut [u8]) -> Result<Option<M>, Self::Error>;
    fn process_raw_in_place(&mut self, f: impl FnOnce(&[u8])) -> Result<bool, Self::Error>;
    fn try_recv_raw_with_info(&mut self, buf: &mut [u8])
        -> Result<Option<(usize, Option<MessageInfo>)>, Self::Error>;
    fn register_waker(&self, waker: &core::task::Waker) {}

    // Only when safety-e2e feature is enabled
    #[cfg(feature = "safety-e2e")]
    fn try_recv_validated(&mut self, buf: &mut [u8])
        -> Result<Option<(usize, IntegrityStatus)>, Self::Error>;
}
```

**`has_data()`** is used by the executor to skip subscribers with no
pending data. Override it if your backend can report availability cheaply.

**`process_raw_in_place()`** enables zero-copy receive. Override if your
backend stores received data in a static buffer that can be borrowed
directly.

### ServiceServerTrait

Handle incoming service requests.

```rust
pub trait ServiceServerTrait {
    type Error;

    fn has_request(&self) -> bool { true }

    fn try_recv_request<'a>(
        &mut self, buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error>;

    fn send_reply(
        &mut self, sequence_number: i64, data: &[u8],
    ) -> Result<(), Self::Error>;

    // Default provided — deserializes request, calls handler, serializes reply
    fn handle_request<S: RosService>(
        &mut self, req_buf: &mut [u8], reply_buf: &mut [u8],
        handler: impl FnOnce(&S::Request) -> S::Reply,
    ) -> Result<bool, Self::Error>;
}
```

`ServiceRequest` carries raw CDR bytes and a sequence number for
request/response matching.

### ServiceClientTrait

Send requests and receive replies.

```rust
pub trait ServiceClientTrait {
    type Error;

    // Blocking call
    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8])
        -> Result<usize, Self::Error>;

    // Non-blocking: send then poll
    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error>;
    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8])
        -> Result<Option<usize>, Self::Error>;

    fn register_waker(&self, waker: &core::task::Waker) {}

    // Defaults provided for typed wrappers
    fn call<S: RosService>(...) -> Result<S::Reply, Self::Error>;
    fn send_request<S: RosService>(...) -> Result<(), Self::Error>;
    fn try_recv_reply<S: RosService>(...) -> Result<Option<S::Reply>, Self::Error>;
}
```

## Step-by-Step Implementation

### 1. Create the backend crate

```
packages/<backend>/nros-rmw-<name>/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── session.rs       # Rmw + Session impl
    ├── publisher.rs     # Publisher impl
    ├── subscriber.rs    # Subscriber impl
    └── service.rs       # ServiceServer + ServiceClient impl
```

`Cargo.toml` dependencies:

```toml
[dependencies]
nros-rmw = { path = "../../core/nros-rmw", default-features = false }
nros-core = { path = "../../core/nros-core", default-features = false }
```

### 2. Define your session type

```rust
pub struct MySession {
    // Backend-specific connection state
}

pub struct MyRmw;

impl Rmw for MyRmw {
    type Session = MySession;
    type Error = TransportError;

    fn open(config: &RmwConfig) -> Result<MySession, TransportError> {
        // Parse config.locator, establish connection
        // Map config.domain_id to backend-specific namespace
        todo!()
    }
}
```

### 3. Implement handle types

Each `Session::create_*` method returns a handle. Handles are typically
indices into a static array (for `no_std`) or heap-allocated state:

```rust
pub struct MyPublisher {
    // Backend-specific publisher state
}

impl Publisher for MyPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
        // Send data over the transport
        todo!()
    }

    fn buffer_error(&self) -> TransportError {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> TransportError {
        TransportError::SerializationError
    }
}
```

### 4. Wire message buffering

Subscribers need to store incoming messages until the application polls
them. Two patterns exist:

**Static buffers** (preferred for `no_std`): Pre-allocate a fixed array
of per-subscriber buffers. Each subscriber gets an index at creation time.
See `nros-rmw-zenoh/src/shim/subscriber.rs` for the pattern.

**Heap queues** (when `alloc` is available): Use `VecDeque<Vec<u8>>` or
similar. Simpler but requires heap.

### 5. Implement `drive_io`

Most embedded transports are pull-based — the application must call
`drive_io()` periodically to read from the network and dispatch messages
to subscriber buffers.

```rust
impl Session for MySession {
    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), TransportError> {
        // 1. Read from network socket (non-blocking if timeout_ms == 0)
        // 2. Parse incoming frames
        // 3. Route messages to subscriber/service buffers
        // 4. Send keepalive if needed
        Ok(())
    }
}
```

The executor calls `drive_io(0)` on every spin iteration.

### 6. Add the feature flag

In the `nros` facade crate:

```toml
# Cargo.toml
[features]
rmw-<name> = ["nros-node/rmw-<name>"]
```

In `nros-node`:

```rust
#[cfg(feature = "rmw-<name>")]
pub type DefaultSession = nros_rmw_<name>::MySession;
```

Add mutual exclusivity enforcement:

```rust
#[cfg(all(feature = "rmw-<name>", feature = "rmw-zenoh"))]
compile_error!("Only one RMW backend can be enabled");
```

### 7. Map QoS settings

ROS 2 QoS profiles must be mapped to your transport's equivalent:

| QoS Field | ROS 2 Meaning | Example Mapping |
|-----------|---------------|-----------------|
| `reliability: Reliable` | Retransmit on loss | TCP or reliable stream |
| `reliability: BestEffort` | No retransmit | UDP or unreliable channel |
| `durability: TransientLocal` | Late-joining subscribers get last value | Retain last N messages |
| `durability: Volatile` | No history for late joiners | Don't retain |
| `history: KeepLast(N)` | Buffer last N messages | Ring buffer of depth N |

### 8. Implement ROS 2 discovery (optional)

For ROS 2 interop, your backend should participate in graph discovery.
The zenoh backend uses liveliness tokens; DDS uses participant discovery.
If your transport doesn't support discovery, nodes will still work for
pub/sub but won't appear in `ros2 node list`.

## Key Expression Format

If your backend needs ROS 2 wire compatibility, messages must use the
standard key expression format:

```
<domain_id>/<topic_name>/<type_name>/<type_hash>
```

For example:
```
0/chatter/std_msgs::msg::dds_::String_/RIHS01_...
```

See [RMW Zenoh Protocol](../reference/rmw-zenoh-protocol.md) for the full
format specification.

## Testing

1. **Unit tests** — test serialization/deserialization round-trips
2. **Integration tests** — pub/sub and service call between two nodes
   using your backend
3. **ROS 2 interop** — if applicable, test communication with a ROS 2
   node running the equivalent RMW

Add a `just test-<backend>` recipe and place tests in
`packages/testing/nros-tests/tests/`.

## Error Mapping

Map your backend's native errors to `TransportError` variants:

| Scenario | TransportError variant |
|----------|----------------------|
| Connection refused / timeout | `ConnectionFailed` |
| Session closed unexpectedly | `Disconnected` |
| Entity creation fails | `PublisherCreationFailed`, etc. |
| Send fails | `PublishFailed` / `ServiceRequestFailed` |
| Buffer too small for message | `BufferTooSmall` |
| Message exceeds static capacity | `MessageTooLarge` |
| Serialization failure | `SerializationError` |
| Deserialization failure | `DeserializationError` |
| Keepalive / lease timeout | `KeepaliveFailed` |

## `no_std` Compatibility

All traits in `nros-rmw` are `#![no_std]`. Your backend should also be
`no_std`-compatible:

- Use `heapless` collections instead of `Vec`/`String`
- Gate heap-dependent code behind `#[cfg(feature = "alloc")]`
- Use static buffers with compile-time sizing via environment variables
  (see [Environment Variables](../reference/environment-variables.md))
- Avoid `std::io`, `std::net`, etc. — use raw FFI or embedded network stacks

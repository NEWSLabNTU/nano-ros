# RMW API

This chapter is a reference for the `nros-rmw` trait layer and its two backend
implementations (`nros-rmw-zenoh`, `nros-rmw-xrce`). For architectural
motivation and comparisons with the ROS 2 rmw interface, see
[RMW API Design](../design/rmw.md).

**Crate:** `nros-rmw` (re-exported via `nros::internals`)

## Trait Hierarchy

```text
Rmw                  -- factory, opens a Session
  Session            -- connection lifecycle, creates handles
    Publisher        -- publish serialized or typed messages
    Subscriber       -- receive messages (polling or async waker)
    ServiceServerTrait -- handle incoming service requests
    ServiceClientTrait -- send requests, poll for replies
```

All traits live in `nros_rmw::traits` and are re-exported from the crate root.

---

## `Rmw`

Factory trait for compile-time middleware selection. Each backend provides one
implementation.

```rust
pub trait Rmw {
    type Session: Session;
    type Error: core::fmt::Debug;

    fn open(config: &RmwConfig) -> Result<Self::Session, Self::Error>;
}
```

`open()` maps the middleware-agnostic `RmwConfig` to backend-specific
initialization (zenoh session or XRCE-DDS session+participant).

---

## `Session`

Manages connection lifecycle and creates communication handles. All associated
types are resolved at compile time -- no dynamic dispatch.

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

    /// Drive transport I/O. Default is no-op for push-based backends.
    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error> { .. }
}
```

Both zenoh-pico and XRCE-DDS are pull-based, so their `drive_io()`
implementations read from the network socket and dispatch incoming data to
internal buffers. The executor calls this periodically.

---

## `Publisher`

```rust
pub trait Publisher {
    type Error;

    /// Publish pre-serialized CDR bytes.
    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error>;

    /// Serialize `msg` into `buf` and publish. Default implementation
    /// calls `publish_raw` after CDR serialization via nros-serdes.
    fn publish<M: RosMessage>(&self, msg: &M, buf: &mut [u8]) -> Result<(), Self::Error> { .. }

    /// Return a buffer-too-small error (backend-specific).
    fn buffer_error(&self) -> Self::Error;

    /// Return a serialization error (backend-specific).
    fn serialization_error(&self) -> Self::Error;
}
```

The typed `publish()` has a default implementation. Backends only need to
implement `publish_raw()`, `buffer_error()`, and `serialization_error()`.

---

## `Subscriber`

```rust
pub trait Subscriber {
    type Error;

    /// Non-destructive readiness check. Conservative default returns true.
    fn has_data(&self) -> bool { true }

    /// Non-blocking receive of raw CDR bytes. Returns byte count or None.
    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;

    /// Non-blocking typed receive. Default deserializes via nros-serdes.
    fn try_recv<M: RosMessage>(&mut self, buf: &mut [u8]) -> Result<Option<M>, Self::Error> { .. }

    /// Zero-copy in-place processing. Calls `f` with a reference to the
    /// internal receive buffer. Messages arriving during `f` are dropped.
    fn process_raw_in_place(&mut self, f: impl FnOnce(&[u8])) -> Result<bool, Self::Error> { .. }

    /// Receive raw bytes with publisher metadata (GID, timestamp).
    fn try_recv_raw_with_info(
        &mut self, buf: &mut [u8],
    ) -> Result<Option<(usize, Option<MessageInfo>)>, Self::Error> { .. }

    /// Receive with E2E safety validation (CRC-32 + sequence tracking).
    /// Requires `safety-e2e` feature.
    #[cfg(feature = "safety-e2e")]
    fn try_recv_validated(
        &mut self, buf: &mut [u8],
    ) -> Result<Option<(usize, IntegrityStatus)>, Self::Error> { .. }

    /// Register a `core::task::Waker` for async notification on data arrival.
    fn register_waker(&self, waker: &core::task::Waker) {}

    /// Return a deserialization error (backend-specific).
    fn deserialization_error(&self) -> Self::Error;
}
```

**Required methods:** `try_recv_raw()`, `deserialization_error()`.
Everything else has a default.

---

## `ServiceServerTrait`

```rust
pub trait ServiceServerTrait {
    type Error;

    /// Non-destructive readiness check. Conservative default returns true.
    fn has_request(&self) -> bool { true }

    /// Non-blocking receive of a raw service request.
    fn try_recv_request<'a>(
        &mut self, buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error>;

    /// Send a CDR-encoded reply keyed by sequence number.
    fn send_reply(&mut self, sequence_number: i64, data: &[u8]) -> Result<(), Self::Error>;

    /// Typed request handling: deserialize request, call handler, serialize
    /// and send reply. Returns Ok(true) if a request was handled.
    fn handle_request<S: RosService>(
        &mut self,
        req_buf: &mut [u8],
        reply_buf: &mut [u8],
        handler: impl FnOnce(&S::Request) -> S::Reply,
    ) -> Result<bool, Self::Error>
    where Self::Error: From<TransportError> { .. }

    /// Same as handle_request but handler returns Box<S::Reply>.
    /// For services with large response types. Requires `alloc` feature.
    #[cfg(feature = "alloc")]
    fn handle_request_boxed<S: RosService>(
        &mut self,
        req_buf: &mut [u8],
        reply_buf: &mut [u8],
        handler: impl FnOnce(&S::Request) -> Box<S::Reply>,
    ) -> Result<bool, Self::Error>
    where Self::Error: From<TransportError> { .. }
}
```

**Required methods:** `try_recv_request()`, `send_reply()`.

`ServiceRequest` carries the CDR bytes and a `sequence_number: i64` used to
correlate replies.

---

## `ServiceClientTrait`

```rust
pub trait ServiceClientTrait {
    type Error;

    /// Blocking call (deprecated -- use Executor + Promise instead).
    #[deprecated]
    fn call_raw(
        &mut self, request: &[u8], reply_buf: &mut [u8],
    ) -> Result<usize, Self::Error> { .. }

    /// Send a request without waiting (non-blocking).
    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error>;

    /// Poll for a reply (non-blocking). Returns byte count or None.
    fn try_recv_reply_raw(
        &mut self, reply_buf: &mut [u8],
    ) -> Result<Option<usize>, Self::Error>;

    /// Typed non-blocking send. Serializes into req_buf, then calls send_request_raw.
    fn send_request<S: RosService>(
        &mut self, request: &S::Request, req_buf: &mut [u8],
    ) -> Result<(), Self::Error>
    where Self::Error: From<TransportError> { .. }

    /// Typed non-blocking reply poll. Deserializes if data is available.
    fn try_recv_reply<S: RosService>(
        &mut self, reply_buf: &mut [u8],
    ) -> Result<Option<S::Reply>, Self::Error>
    where Self::Error: From<TransportError> { .. }

    /// Register a Waker for async notification on reply arrival.
    fn register_waker(&self, waker: &core::task::Waker) {}

    /// Typed blocking call (serializes, sends, polls, deserializes).
    fn call<S: RosService>(
        &mut self,
        request: &S::Request,
        req_buf: &mut [u8],
        reply_buf: &mut [u8],
    ) -> Result<S::Reply, Self::Error>
    where Self::Error: From<TransportError> { .. }
}
```

**Required methods:** `send_request_raw()`, `try_recv_reply_raw()`.

The recommended pattern is `send_request_raw()` + executor `spin_once()` +
`try_recv_reply_raw()`, wrapped by `nros-node` as `Client::call()` returning
a `Promise`.

---

## Configuration Types

### `RmwConfig`

Middleware-agnostic session configuration passed to `Rmw::open()`.

```rust
pub struct RmwConfig<'a> {
    pub locator: &'a str,       // e.g. "tcp/192.168.1.1:7447"
    pub mode: SessionMode,      // Client or Peer
    pub domain_id: u32,         // ROS 2 domain ID (default 0)
    pub node_name: &'a str,     // e.g. "talker"
    pub namespace: &'a str,     // e.g. "" or "/ns1"
}
```

### `TransportConfig`

Lower-level configuration with backend-specific properties. Used by the legacy
`Transport` trait; new code should prefer `RmwConfig`.

```rust
pub struct TransportConfig<'a> {
    pub locator: Option<&'a str>,
    pub mode: SessionMode,
    pub properties: &'a [(&'a str, &'a str)],
}
```

Recognized zenoh-pico properties include `"multicast_scouting"`,
`"scouting_timeout_ms"`, `"multicast_locator"`, `"listen"`, and
`"add_timestamp"`.

### `SessionMode`

```rust
pub enum SessionMode {
    Client,  // Connect to a router (default)
    Peer,    // Peer-to-peer, no router required
}
```

---

## Information Types

### `TopicInfo`

```rust
pub struct TopicInfo<'a> {
    pub name: &'a str,          // "/chatter"
    pub type_name: &'a str,     // "std_msgs::msg::dds_::String_"
    pub type_hash: &'a str,
    pub domain_id: u32,
    pub node_name: Option<&'a str>,
    pub namespace: &'a str,
}
```

Builder methods: `with_domain()`, `with_node_name()`, `with_namespace()`.

### `ServiceInfo`

Same fields as `TopicInfo`. Builder methods: `with_domain()`,
`with_node_name()`, `with_namespace()`.

### `ActionInfo`

```rust
pub struct ActionInfo<'a> {
    pub name: &'a str,          // "/fibonacci"
    pub type_name: &'a str,
    pub type_hash: &'a str,
    pub domain_id: u32,
}
```

Key generation methods (const generic `N` for buffer size):

| Method | Returns |
|--------|---------|
| `send_goal_key::<N>()` | `/fibonacci/_action/send_goal` |
| `cancel_goal_key::<N>()` | `/fibonacci/_action/cancel_goal` |
| `get_result_key::<N>()` | `/fibonacci/_action/get_result` |
| `feedback_key::<N>()` | `/fibonacci/_action/feedback` |
| `status_key::<N>()` | `/fibonacci/_action/status` |

---

## `TransportError`

All backends use this shared error enum.

```rust
pub enum TransportError {
    ConnectionFailed,
    Disconnected,
    PublisherCreationFailed,
    SubscriberCreationFailed,
    ServiceServerCreationFailed,
    ServiceClientCreationFailed,
    PublishFailed,
    ServiceRequestFailed,
    ServiceReplyFailed,
    SerializationError,
    DeserializationError,
    BufferTooSmall,
    MessageTooLarge,
    Timeout,
    InvalidConfig,
    TaskStartFailed,
    PollFailed,
    KeepaliveFailed,
    JoinFailed,
}
```

`TransportError` is `Copy`, `Clone`, `Debug`, `PartialEq`, and `Eq`.

---

## QoS Settings

```rust
pub struct QosSettings {
    pub history: QosHistoryPolicy,        // KeepLast (default) | KeepAll
    pub reliability: QosReliabilityPolicy, // Reliable | BestEffort (default)
    pub durability: QosDurabilityPolicy,   // Volatile (default) | TransientLocal
    pub depth: u32,                        // history depth (default 10)
}
```

### Standard Profiles

| Constant | Reliability | Durability | Depth |
|----------|------------|------------|-------|
| `QOS_PROFILE_DEFAULT` | Reliable | Volatile | 10 |
| `QOS_PROFILE_SENSOR_DATA` | BestEffort | Volatile | 5 |
| `QOS_PROFILE_SERVICES_DEFAULT` | Reliable | Volatile | 10 |
| `QOS_PROFILE_PARAMETERS` | Reliable | TransientLocal | 1000 |
| `QOS_PROFILE_PARAMETER_EVENTS` | Reliable | Volatile | KeepAll |
| `QOS_PROFILE_ACTION_STATUS_DEFAULT` | Reliable | TransientLocal | 1 |
| `QOS_PROFILE_SYSTEM_DEFAULT` | Reliable | Volatile | 1 |
| `QOS_PROFILE_CLOCK` | BestEffort | Volatile | 1 |

Static constructors: `topics_default()`, `sensor_data_default()`,
`services_default()`, `parameters_default()`, `parameter_events_default()`,
`system_default()`, `action_status_default()`, `clock_default()`.

Builder methods: `keep_last(depth)`, `keep_all()`, `reliable()`,
`best_effort()`, `volatile()`, `transient_local()`, `reliability(policy)`,
`durability(policy)`, `history(policy)`, `depth(n)`.

---

## Locator Utilities

```rust
pub fn locator_protocol(locator: &str) -> LocatorProtocol;
pub fn validate_locator(locator: &str) -> Result<(), &'static str>;

pub enum LocatorProtocol { Tcp, Serial, Unknown }
```

`validate_locator()` checks format before passing to the transport backend:
- TCP: `tcp/<host>:<port>`
- Serial: `serial/<device>#baudrate=<rate>`

---

## Compile-Time Backend Dispatch

The `nros` facade crate resolves concrete types based on the active feature
flag. Application code never names backend types directly.

```rust
// In nros::internals (simplified):
#[cfg(feature = "rmw-zenoh")]
pub type RmwSession = nros_rmw_zenoh::ZenohSession;
#[cfg(feature = "rmw-zenoh")]
pub type RmwPublisher = nros_rmw_zenoh::ZenohPublisher;
// ...

#[cfg(feature = "rmw-xrce")]
pub type RmwSession = nros_rmw_xrce::XrceSession;
#[cfg(feature = "rmw-xrce")]
pub type RmwPublisher = nros_rmw_xrce::XrcePublisher;
// ...
```

Feature flags are mutually exclusive. Enabling both `rmw-zenoh` and `rmw-xrce`
is a compile error.

---

## Zenoh Backend (`nros-rmw-zenoh`)

**Crate:** `nros-rmw-zenoh`  
**Feature flag:** `rmw-zenoh`

### `ZenohRmw`

```rust
pub struct ZenohRmw;

impl Rmw for ZenohRmw {
    type Session = ZenohSession;
    type Error = TransportError;

    fn open(config: &RmwConfig) -> Result<ZenohSession, TransportError>;
}
```

### `ZenohSession`

Wraps a zenoh-pico C session. Requires manual polling -- there are no
background threads.

```rust
pub struct ZenohSession { /* private */ }

impl ZenohSession {
    pub fn new(config: &TransportConfig) -> Result<Self, TransportError>;
    pub fn is_open(&self) -> bool;
    pub fn uses_polling(&self) -> bool;  // always true
    pub fn poll(&self, timeout_ms: u32) -> Result<i32, TransportError>;
    pub fn spin_once(&self, timeout_ms: u32) -> Result<i32, TransportError>;
    pub fn zid(&self) -> Result<ZenohId, TransportError>;
    pub fn declare_liveliness(&self, keyexpr: &[u8]) -> Result<LivelinessToken, TransportError>;
    pub fn declare_node_liveliness(
        &self, domain_id: u32, namespace: &str, node_name: &str,
    ) -> Option<LivelinessToken>;
}

impl Session for ZenohSession {
    type Error = TransportError;
    type PublisherHandle = ZenohPublisher;
    type SubscriberHandle = ZenohSubscriber;
    type ServiceServerHandle = ZenohServiceServer;
    type ServiceClientHandle = ZenohServiceClient;
    // ...
}
```

`spin_once()` combines `poll()` and keepalive in a single call. This is the
recommended way to drive the session from a main loop or RTOS task.

### `ZenohPublisher`

Implements `Publisher`. Includes RMW attachment support (GID + sequence number)
for `rmw_zenoh_cpp` interoperability.

### `ZenohSubscriber`

Implements `Subscriber`. Supports `register_waker()` for async notification,
`process_raw_in_place()` for zero-copy receive, and
`try_recv_raw_with_info()` for publisher metadata extraction.

### `ZenohServiceServer`

Implements `ServiceServerTrait`. Backed by a zenoh queryable. Incoming queries
are buffered in a static slot; `send_reply()` responds to the stored query.

### `ZenohServiceClient`

Implements `ServiceClientTrait`. Uses `z_get` queries to send requests and
receive replies. Supports `register_waker()` for async polling.

### `ZenohId`

```rust
pub struct ZenohId {
    pub id: [u8; 16],
}
```

A 16-byte session identifier used in liveliness token key expressions for
ROS 2 discovery.

### `LivelinessToken`

```rust
pub struct LivelinessToken {
    handle: i32,
}
```

An opaque handle to a declared liveliness token. The token is undeclared when
dropped. Used for ROS 2 graph discovery via `rmw_zenoh` liveliness protocol.

---

## XRCE-DDS Backend (`nros-rmw-xrce`)

**Crate:** `nros-rmw-xrce`  
**Feature flag:** `rmw-xrce`

### Architecture

All state is held in module-level statics (single-session model, matching
XRCE-DDS's design). Board crates register transport callbacks via
`init_transport()`, then call `XrceRmw::open()` to create the DDS session
and participant.

### `XrceRmw`

```rust
pub struct XrceRmw;

impl Rmw for XrceRmw {
    type Session = XrceSession;
    type Error = TransportError;

    fn open(config: &RmwConfig) -> Result<XrceSession, TransportError>;
}
```

### `init_transport()`

Must be called before `XrceRmw::open()`.

```rust
pub unsafe fn init_transport(
    open: xrce_sys::open_custom_func,
    close: xrce_sys::close_custom_func,
    write: xrce_sys::write_custom_func,
    read: xrce_sys::read_custom_func,
);
```

Registers custom transport callbacks for the XRCE-DDS client library.
Platform-specific transport modules provide these:

- `nros_rmw_xrce::posix_udp` -- POSIX UDP sockets (feature `posix-udp`)
- `nros_rmw_xrce::posix_serial` -- POSIX serial (feature `posix-serial`)
- `nros_rmw_xrce::zephyr` -- Zephyr sockets (feature `platform-zephyr`)
- `xrce-smoltcp` crate -- smoltcp integration for bare-metal

### `XrceSession`

```rust
pub struct XrceSession;  // zero-size, all state in statics
```

Implements `Session`. `drive_io()` calls `uxr_run_session_time()` to process
XRCE-DDS I/O and dispatch topic/service callbacks to static buffer slots.

### Handle Types

| Type | Trait | Internal State |
|------|-------|---------------|
| `XrcePublisher` | `Publisher` | `datawriter_id` |
| `XrceSubscriber` | `Subscriber` | slot index into static subscriber array |
| `XrceServiceServer` | `ServiceServerTrait` | slot index + replier ID + last sample ID |
| `XrceServiceClient` | `ServiceClientTrait` | slot index + requester ID |

Subscriber and service slots use atomic flags for callback-to-consumer data
flow, matching the pattern used by the zenoh backend.

### Configuration Constants

Controlled via `XRCE_*` environment variables at build time (processed by
`build.rs`):

| Env Var | Default | Description |
|---------|---------|-------------|
| `XRCE_MAX_SUBSCRIBERS` | 4 | Max concurrent subscribers |
| `XRCE_MAX_SERVICE_SERVERS` | 2 | Max concurrent service servers |
| `XRCE_MAX_SERVICE_CLIENTS` | 2 | Max concurrent service clients |
| `XRCE_BUFFER_SIZE` | 512 | Per-slot receive buffer (bytes) |
| `XRCE_STREAM_HISTORY` | 4 | Reliable stream history depth |
| `XRCE_ENTITY_CREATION_TIMEOUT_MS` | 1000 | Entity creation timeout |

### FFI Reentrancy Guard

When the `ffi-sync` feature is enabled, all XRCE-DDS FFI calls are wrapped in
`critical_section::with()` to prevent concurrent access from mixed-priority
RTOS tasks. Without the feature, the guard is a zero-cost passthrough.

---

## Legacy `Transport` Trait

Retained for backward compatibility. New code should use `Rmw`.

```rust
pub trait Transport {
    type Error;
    type Session: Session;

    fn open(config: &TransportConfig) -> Result<Self::Session, Self::Error>;
}
```

The difference from `Rmw` is that `Transport::open()` takes a
`TransportConfig` (backend-specific properties) while `Rmw::open()` takes an
`RmwConfig` (middleware-agnostic).

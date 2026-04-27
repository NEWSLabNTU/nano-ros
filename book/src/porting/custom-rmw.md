# Porting a Custom RMW Backend

nano-ros ships with two RMW backends -- zenoh-pico and Micro-XRCE-DDS. To
add your own transport (DDS, MQTT, a proprietary bus, etc.), implement a
small set of traits or fill in a C function table.

Two paths are available:

- **Rust path** -- implement the `nros-rmw` traits directly.
- **C/C++ path** -- fill in `nros_rmw_vtable_t` and register it at
  startup via `nros-rmw-cffi`.

## What you implement

Your backend provides concrete types for six traits:

```text
Rmw                    -- factory: opens a Session from RmwConfig
  Session              -- connection lifecycle, creates handles
    Publisher          -- send CDR-encoded messages
    Subscriber         -- non-blocking receive (poll-based)
    ServiceServerTrait -- receive requests, send replies
    ServiceClientTrait -- send requests, poll for replies
```

Most methods have default implementations. The required methods per trait are:

| Trait | Required methods |
|-------|-----------------|
| `Rmw` | `open()` |
| `Session` | `create_publisher()`, `create_subscriber()`, `create_service_server()`, `create_service_client()`, `close()` |
| `Publisher` | `publish_raw()`, `buffer_error()`, `serialization_error()` |
| `Subscriber` | `try_recv_raw()`, `deserialization_error()` |
| `ServiceServerTrait` | `try_recv_request()`, `send_reply()` |
| `ServiceClientTrait` | `send_request_raw()`, `try_recv_reply_raw()` |

For full trait signatures and associated types, see
[RMW API Reference](../reference/rmw-api.md).

## Use nros-platform for networking

Call `ConcretePlatform::tcp_open()` / `udp_bind()` from `nros-platform`
rather than OS sockets directly. This makes your backend portable across
every platform (POSIX, Zephyr, FreeRTOS, NuttX, ThreadX, bare-metal). If
your transport library already abstracts networking (like zenoh-pico does),
you can use its own I/O layer instead.

---

## Rust path

### 1. Create the crate

Create `packages/myproto/nros-rmw-myproto/` with `nros-rmw` and
`nros-core` as dependencies (both `default-features = false` for
`no_std` support). Follow the `std`/`alloc` feature forwarding pattern
used by the existing backends.

### 2. Implement the traits

```rust
#![no_std]
use nros_rmw::*;

#[derive(Default)]
pub struct MyProtoRmw {
    // Optional pre-open configuration (agent address, TLS CA, serial
    // device handle, …) can live here and move into the Session at
    // `open` time. Every backend in-repo implements `Default` so the
    // caller can spell the common case as
    // `MyProtoRmw::default().open(&config)`.
}
impl Rmw for MyProtoRmw {
    type Session = MyProtoSession;
    type Error = TransportError;
    fn open(self, config: &RmwConfig) -> Result<MyProtoSession, TransportError> {
        todo!() // Parse config.locator, connect, map config.domain_id
    }
}

pub struct MyProtoSession { /* connection state */ }
impl Session for MyProtoSession {
    type Error = TransportError;
    type PublisherHandle = MyProtoPub;
    type SubscriberHandle = MyProtoSub;
    type ServiceServerHandle = MyProtoServer;
    type ServiceClientHandle = MyProtoClient;

    fn create_publisher(&mut self, t: &TopicInfo, q: QosSettings)
        -> Result<MyProtoPub, TransportError> { todo!() }
    fn create_subscriber(&mut self, t: &TopicInfo, q: QosSettings)
        -> Result<MyProtoSub, TransportError> { todo!() }
    fn create_service_server(&mut self, s: &ServiceInfo)
        -> Result<MyProtoServer, TransportError> { todo!() }
    fn create_service_client(&mut self, s: &ServiceInfo)
        -> Result<MyProtoClient, TransportError> { todo!() }
    fn close(&mut self) -> Result<(), TransportError> { todo!() }
    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), TransportError> {
        let _ = timeout_ms; Ok(()) // poll network, dispatch to buffers
    }
}

pub struct MyProtoPub;
impl Publisher for MyProtoPub {
    type Error = TransportError;
    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> { todo!() }
    fn buffer_error(&self) -> TransportError { TransportError::BufferTooSmall }
    fn serialization_error(&self) -> TransportError { TransportError::SerializationError }
}

pub struct MyProtoSub;
impl Subscriber for MyProtoSub {
    type Error = TransportError;
    fn try_recv_raw(&mut self, buf: &mut [u8])
        -> Result<Option<usize>, TransportError> { todo!() }
    fn deserialization_error(&self) -> TransportError { TransportError::DeserializationError }
}

pub struct MyProtoServer;
impl ServiceServerTrait for MyProtoServer {
    type Error = TransportError;
    fn try_recv_request<'a>(&mut self, buf: &'a mut [u8])
        -> Result<Option<ServiceRequest<'a>>, TransportError> { todo!() }
    fn send_reply(&mut self, seq: i64, data: &[u8])
        -> Result<(), TransportError> { todo!() }
}

pub struct MyProtoClient;
impl ServiceClientTrait for MyProtoClient {
    type Error = TransportError;
    fn send_request_raw(&mut self, req: &[u8])
        -> Result<(), TransportError> { todo!() }
    fn try_recv_reply_raw(&mut self, buf: &mut [u8])
        -> Result<Option<usize>, TransportError> { todo!() }
}
```

### Factory shape (Phase 84.E2)

`Rmw::open` consumes `self`, not a `&self`. That shape asks every
backend to treat its factory type as a **value** that carries any
pre-open configuration (agent address, serial device, TLS CA, …)
and *moves* that state into the returned `Session`:

```rust,ignore
// Default constructor (picks config from `&RmwConfig`):
let session = MyProtoRmw::default().open(&config)?;

// Explicit constructor when the backend has pre-open state
// that isn't in the middleware-agnostic `RmwConfig`:
let session = MyProtoRmw::with_endpoint("10.0.0.1", 7447).open(&config)?;
```

Conventions:

- **Every backend implements `Default`.** Keeps the common call
  site short and lets generic code build a factory without
  knowing the backend type.
- **Provide `new(...)` / `with_*(...)` helpers for backend-specific
  pre-open state.** Don't bake it into `RmwConfig` — that type is
  the middleware-agnostic contract. If your backend needs an agent
  IP, a serial device path, or a certificate slot, take it on the
  factory constructor.
- **Read your own environment variables in `<Backend>::from_env()`**
  if you want zero-boilerplate POSIX configuration. The shipped
  `ExecutorConfig::from_env()` only reads the middleware-agnostic
  `NROS_LOCATOR` / `NROS_SESSION_MODE` / `ROS_DOMAIN_ID`; anything
  backend-specific (e.g. `NROS_XRCE_AGENT`) stays on the backend
  side.
- **Post-open state lives in `Session`, never in `static mut`.**
  The `open(self, …)` signature makes it natural to move the
  configured transport into the `Session` return value, which
  then owns the connection for the rest of its lifetime. A backend
  that still uses `static mut` session-global state will fail any
  multi-session test (`backend.open(...)` twice in one process
  should succeed).

### 3. Wire into nros

Three changes are needed to integrate the new backend:

**a)** In `nros/Cargo.toml`, add a feature and optional dependency:

```toml
rmw-myproto = ["dep:nros-rmw-myproto", "nros-node/rmw-myproto"]
```

**b)** In `nros-node`, add the concrete session type alias:

```rust
#[cfg(feature = "rmw-myproto")]
pub type ConcreteSession = nros_rmw_myproto::MyProtoSession;
```

**c)** Add `compile_error!` guards to enforce mutual exclusivity with the
other backends (see existing guards in `nros-node/src/session.rs`).

Applications then select your backend with
`nros = { features = ["rmw-myproto", "platform-posix"] }`.

---

## C/C++ path

If your transport library is C or C++, use `nros-rmw-cffi` — a vtable
of C function pointers that map one-to-one onto the Rust trait methods.

The hand-written header lives at
`packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`. Browse the
rendered reference at [/api/rmw-cffi/](../api/rmw-cffi/index.html) for
per-field return-value, threading, and blocking conventions.

### 1. Fill in the vtable

```c
#include <nros/rmw_vtable.h>

// -- Session lifecycle --
static nros_rmw_handle_t my_open(const char *locator, uint8_t mode,
                                 uint32_t domain_id, const char *node_name) {
    /* connect, return non-NULL session handle (or NULL on failure). */
}
static int32_t my_close(nros_rmw_handle_t session) { /* ... */ }
static int32_t my_drive_io(nros_rmw_handle_t session, int32_t timeout_ms) {
    /* dispatch network I/O for up to timeout_ms; return 0 on success. */
}

// -- Publisher --
static nros_rmw_handle_t my_create_publisher(nros_rmw_handle_t session,
        const char *topic, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_cffi_qos_t *qos) { /* ... */ }
static void    my_destroy_publisher(nros_rmw_handle_t publisher) { /* ... */ }
static int32_t my_publish_raw(nros_rmw_handle_t publisher,
        const uint8_t *data, size_t len) { /* ... */ }

// -- Subscriber --
static nros_rmw_handle_t my_create_subscriber(nros_rmw_handle_t session,
        const char *topic, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_cffi_qos_t *qos) { /* ... */ }
static void    my_destroy_subscriber(nros_rmw_handle_t subscriber) { /* ... */ }
static int32_t my_try_recv_raw(nros_rmw_handle_t subscriber,
        uint8_t *buf, size_t buf_len) {
    /* positive = bytes received, 0 = no data, negative = error. */
}
static int32_t my_has_data(nros_rmw_handle_t subscriber) { /* 1 = yes, 0 = no */ }

// -- Service Server --
static nros_rmw_handle_t my_create_service_server(nros_rmw_handle_t session,
        const char *service, const char *type_name, const char *type_hash,
        uint32_t domain_id) { /* ... */ }
static void    my_destroy_service_server(nros_rmw_handle_t server) { /* ... */ }
static int32_t my_try_recv_request(nros_rmw_handle_t server,
        uint8_t *buf, size_t buf_len, int64_t *seq_out) { /* ... */ }
static int32_t my_has_request(nros_rmw_handle_t server) { /* 1 = yes, 0 = no */ }
static int32_t my_send_reply(nros_rmw_handle_t server,
        int64_t seq, const uint8_t *data, size_t len) { /* ... */ }

// -- Service Client --
static nros_rmw_handle_t my_create_service_client(nros_rmw_handle_t session,
        const char *service, const char *type_name, const char *type_hash,
        uint32_t domain_id) { /* ... */ }
static void    my_destroy_service_client(nros_rmw_handle_t client) { /* ... */ }
static int32_t my_call_raw(nros_rmw_handle_t client,
        const uint8_t *request, size_t req_len,
        uint8_t *reply_buf, size_t reply_buf_len) { /* ... */ }

static const nros_rmw_vtable_t MY_RMW = {
    .open                   = my_open,
    .close                  = my_close,
    .drive_io               = my_drive_io,
    .create_publisher       = my_create_publisher,
    .destroy_publisher      = my_destroy_publisher,
    .publish_raw            = my_publish_raw,
    .create_subscriber      = my_create_subscriber,
    .destroy_subscriber     = my_destroy_subscriber,
    .try_recv_raw           = my_try_recv_raw,
    .has_data               = my_has_data,
    .create_service_server  = my_create_service_server,
    .destroy_service_server = my_destroy_service_server,
    .try_recv_request       = my_try_recv_request,
    .has_request            = my_has_request,
    .send_reply             = my_send_reply,
    .create_service_client  = my_create_service_client,
    .destroy_service_client = my_destroy_service_client,
    .call_raw               = my_call_raw,
};
```

### 2. Register before opening a session

```c
int main(void) {
    nros_rmw_cffi_register(&MY_RMW);    // before any nros call
    /* now use the nano-ros C or C++ API normally */
}
```

Build the static library with the matching feature combo:

```bash
cargo build -p nros-c --features rmw-cffi,platform-posix,ros-humble
```

### 3. Lifecycle and threading contract

The Rust traits behind this vtable
([`nros_rmw::Session`](../api/rust/nros_rmw/index.html),
[`Publisher`](../api/rust/nros_rmw/index.html), …) document the
per-method contract: thread safety, buffer ownership, blocking
allowance. The C vtable inherits the same rules:

- The vtable itself is registered once and read concurrently. Function
  pointers must be safe to invoke from any executor thread.
- `drive_io` may block up to `timeout_ms`; it must not hold
  application-visible locks across the wait.
- `publish_raw`, `try_recv_raw`, and `send_reply` may run concurrently
  from different executor threads — your backend handles serialisation.
- `try_recv_raw` and `try_recv_request` are non-blocking: return
  `0` if no data is ready. The executor will retry after `drive_io`.
- `call_raw` is the deprecated blocking client path. In-tree backends
  route blocking waits through the executor instead. Implement it as
  a polling loop only if you need to support legacy callers.

All strings are null-terminated. Handles are opaque `nros_rmw_handle_t`
(`void*`). Return convention: `0` = success / no data, positive =
byte count, negative = error.

---

## Example: local echo RMW

Loops published messages back to subscribers -- no real transport. Only
pub/sub shown; service types are no-op stubs.

```rust
static mut ECHO_BUF: [u8; 1024] = [0; 1024];
static mut ECHO_LEN: usize = 0;

pub struct EchoPub;
impl Publisher for EchoPub {
    type Error = TransportError;
    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
        unsafe {
            let len = data.len().min(ECHO_BUF.len());
            ECHO_BUF[..len].copy_from_slice(&data[..len]);
            ECHO_LEN = len;
        }
        Ok(())
    }
    fn buffer_error(&self) -> TransportError { TransportError::BufferTooSmall }
    fn serialization_error(&self) -> TransportError { TransportError::SerializationError }
}

pub struct EchoSub;
impl Subscriber for EchoSub {
    type Error = TransportError;
    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        unsafe {
            if ECHO_LEN == 0 { return Ok(None); }
            let len = ECHO_LEN;
            buf[..len].copy_from_slice(&ECHO_BUF[..len]);
            ECHO_LEN = 0;
            Ok(Some(len))
        }
    }
    fn deserialization_error(&self) -> TransportError { TransportError::DeserializationError }
}
```

Wire `EchoPub`/`EchoSub` into an `EchoSession` the same way as the
skeleton above -- `create_publisher` returns `Ok(EchoPub)`, etc. The
`Rmw::open()` impl just returns `Ok(EchoSession)` unconditionally.

---

## What the ROS 2 ecosystem expects

Implementing the six traits compiles and runs, but a backend that stops
there will not interoperate cleanly with `ros2 CLI`, RQt, or
`rmw_zenoh_cpp` nodes. Real ROS 2 interop requires four extra
invariants the traits do not express:

### 1. Discovery / liveliness tokens

`ros2 node list`, `ros2 topic list`, `ros2 service list` rely on
discovery traffic. How you emit it depends on the backend protocol:

- **Zenoh-flavoured backends**: publish a liveliness token per endpoint
  under `@ros2_lv/<domain>/<zid>/<entity_kind>/<id>/…`. See
  [rmw-zenoh-protocol.md](../internals/rmw-zenoh-protocol.md) for the
  exact key grammar.
- **DDS-flavoured backends**: use the SPDP/SEDP discovery traffic that
  your DDS stack provides, plus the ROS 2–specific USER_DATA payload
  (node name, namespace, enclave).

If your backend is brand new (not wire-compatible with zenoh or DDS),
you still need *some* discovery channel for `ros2 CLI` tools to find
your endpoints. The traits currently do not cover this — discovery
happens inside `create_publisher` / `create_subscriber` /
`create_service_*` as a side effect.

### 2. RMW attachments (per-message metadata)

Every published message carries ROS 2 metadata that consumers read
through `MessageInfo`:

| Field | Size | Meaning |
|-------|------|---------|
| `sequence_number` | 8 bytes | `int64` LE — monotonic per publisher |
| `timestamp`       | 8 bytes | `int64` LE — source nanoseconds |
| `gid`             | 16 bytes | random per publisher, constant over its lifetime |

- **Zenoh**: the attachment rides alongside the payload as a zenoh
  `Attachment`. Humble uses a simple concatenation; Jazzy onward uses a
  VLE-encoded `gid_length` prefix.
- **DDS**: sample identity and source timestamp fall out of the DDS
  sample info — your backend only has to forward them.

If you skip this, `add_subscription_with_info()` on consumers always
reports `MessageInfo::default()`, and downstream features (safety-e2e
checks, source-timestamp ordering) silently degrade.

### 3. Actions decompose into five underlying channels

ROS 2 actions are not a transport primitive — they are a pattern built
on services and topics. Each action server exposes:

| Sub-entity | Kind | Name suffix |
|------------|------|-------------|
| `send_goal` | Service | `_action/send_goal` |
| `cancel_goal` | Service | `_action/cancel_goal` |
| `get_result` | Service | `_action/get_result` |
| `feedback` | Topic (pub) | `_action/feedback` |
| `status` | Topic (pub) | `_action/status` |

A backend that implements the six core traits automatically supports
actions — `nros-node` composes the five channels itself. The only
backend-specific piece is the key / topic construction, which the
`Session::create_service_server` and `create_publisher` methods already
handle.

### 4. Type hashes

| ROS 2 distro | Type hash |
|--------------|-----------|
| Humble | literal string `"TypeHashNotSupported"` |
| Iron / Jazzy / Rolling | `RIHS01_<sha256_hex>` computed from the IDL |

nano-ros currently targets Humble (see [Phase
41](../../../docs/roadmap/phase-41-iron-type-hash-support.md) for Iron
support). A new backend that aims at a newer distro must compute the
right hash string — the `TopicInfo::type_hash` field is already plumbed
through.

### 5. QoS mapping

ROS 2 QoS (reliability, durability, history, depth) maps differently
onto each backend:

- **Zenoh** has reliable/best-effort and a `KEEP_LAST(N)` / `KEEP_ALL`
  buffering policy — direct mapping.
- **DDS** has native QoS — almost 1:1.
- **Custom backends** must either honour the requested QoS or document
  which fields are ignored. A `best_effort` publisher matched with a
  `reliable` subscriber is a QoS mismatch in ROS 2 — the transport must
  refuse the subscription (or flag it at runtime) rather than silently
  lose messages.

---

## Further reading

- [RMW API Reference](../reference/rmw-api.md) -- full trait signatures,
  QoS profiles, error types, configuration structs.
- [RMW API Design](../design/rmw.md) -- architectural motivation and
  comparison with the ROS 2 rmw interface.
- [Zenoh-pico Symbol Reference](../internals/porting-platform/zenoh-pico.md)
  -- FFI symbol mapping for the zenoh-pico backend (useful as a reference
  for how an existing backend is structured).

# Porting a Custom RMW Backend

nano-ros ships with two RMW (ROS Middleware) backends -- zenoh-pico and
Micro-XRCE-DDS. If your system uses a different transport (DDS, MQTT, a
proprietary bus, etc.), you can implement your own backend by satisfying a
small set of Rust traits.

There are two paths:

- **Rust path** -- implement the `nros-rmw` traits directly.
- **C/C++ path** -- fill in a C function table (`nros_rmw_vtable_t`) and
  register it at startup. The `nros-rmw-cffi` crate bridges those function
  pointers into the Rust trait system.

## What you implement

The trait hierarchy lives in `nros-rmw`. Your backend must provide concrete
types for six traits:

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

Your RMW backend should call `ConcretePlatform::tcp_open()`,
`ConcretePlatform::udp_bind()`, etc. from `nros-platform` rather than
using OS sockets directly. This makes your backend portable across every
platform that implements the platform traits (POSIX, Zephyr, FreeRTOS,
NuttX, ThreadX, bare-metal with smoltcp).

If your transport library already abstracts networking (like zenoh-pico
or the XRCE-DDS client do), you can skip this and use the library's own
I/O layer instead.

---

## Rust path

### 1. Create the crate

```text
packages/myproto/nros-rmw-myproto/
  Cargo.toml
  src/
    lib.rs
```

```toml
# Cargo.toml
[package]
name = "nros-rmw-myproto"
version = "0.1.0"
edition = "2024"

[features]
default = ["std"]
std = ["alloc", "nros-rmw/std", "nros-core/std"]
alloc = ["nros-rmw/alloc", "nros-core/alloc"]

[dependencies]
nros-rmw = { version = "0.1.0", path = "../../core/nros-rmw", default-features = false }
nros-core = { version = "0.1.0", path = "../../core/nros-core", default-features = false }
```

### 2. Implement the traits

```rust
// src/lib.rs
#![no_std]
use nros_rmw::{
    Publisher, QosSettings, Rmw, RmwConfig, ServiceClientTrait,
    ServiceInfo, ServiceRequest, ServiceServerTrait, Session,
    Subscriber, TopicInfo, TransportError,
};

pub struct MyProtoRmw;
impl Rmw for MyProtoRmw {
    type Session = MyProtoSession;
    type Error = TransportError;
    fn open(config: &RmwConfig) -> Result<MyProtoSession, TransportError> {
        // Parse config.locator, establish connection, map config.domain_id
        todo!()
    }
}

pub struct MyProtoSession { /* connection state */ }
impl Session for MyProtoSession {
    type Error = TransportError;
    type PublisherHandle = MyProtoPub;
    type SubscriberHandle = MyProtoSub;
    type ServiceServerHandle = MyProtoServer;
    type ServiceClientHandle = MyProtoClient;

    fn create_publisher(&mut self, topic: &TopicInfo, qos: QosSettings)
        -> Result<MyProtoPub, TransportError> { todo!() }
    fn create_subscriber(&mut self, topic: &TopicInfo, qos: QosSettings)
        -> Result<MyProtoSub, TransportError> { todo!() }
    fn create_service_server(&mut self, service: &ServiceInfo)
        -> Result<MyProtoServer, TransportError> { todo!() }
    fn create_service_client(&mut self, service: &ServiceInfo)
        -> Result<MyProtoClient, TransportError> { todo!() }
    fn close(&mut self) -> Result<(), TransportError> { todo!() }
    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), TransportError> {
        // Read from network, dispatch to subscriber/service buffers.
        // The executor calls this on every spin iteration.
        let _ = timeout_ms; Ok(())
    }
}

pub struct MyProtoPub { /* handle */ }
impl Publisher for MyProtoPub {
    type Error = TransportError;
    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
        todo!() // Send CDR bytes over the wire
    }
    fn buffer_error(&self) -> TransportError { TransportError::BufferTooSmall }
    fn serialization_error(&self) -> TransportError { TransportError::SerializationError }
}

pub struct MyProtoSub { /* handle + receive buffer */ }
impl Subscriber for MyProtoSub {
    type Error = TransportError;
    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        todo!() // Copy next queued message into buf; Ok(None) if empty
    }
    fn deserialization_error(&self) -> TransportError { TransportError::DeserializationError }
}

pub struct MyProtoServer { /* handle */ }
impl ServiceServerTrait for MyProtoServer {
    type Error = TransportError;
    fn try_recv_request<'a>(&mut self, buf: &'a mut [u8])
        -> Result<Option<ServiceRequest<'a>>, TransportError> { todo!() }
    fn send_reply(&mut self, seq: i64, data: &[u8])
        -> Result<(), TransportError> { todo!() }
}

pub struct MyProtoClient { /* handle */ }
impl ServiceClientTrait for MyProtoClient {
    type Error = TransportError;
    fn send_request_raw(&mut self, request: &[u8])
        -> Result<(), TransportError> { todo!() }
    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8])
        -> Result<Option<usize>, TransportError> { todo!() }
}
```

### 3. Wire into nros

Add your backend as an optional dependency and feature flag in
`packages/core/nros/Cargo.toml`:

```toml
[features]
rmw-myproto = ["dep:nros-rmw-myproto", "nros-node/rmw-myproto"]

[dependencies]
nros-rmw-myproto = { version = "0.1.0", path = "../../myproto/nros-rmw-myproto", default-features = false, optional = true }
```

Then add the type alias in `nros-node` so the executor resolves your
session type when the feature is active:

```rust
#[cfg(feature = "rmw-myproto")]
pub type ConcreteSession = nros_rmw_myproto::MyProtoSession;
```

Enforce mutual exclusivity with the other backends:

```rust
#[cfg(all(feature = "rmw-myproto", feature = "rmw-zenoh"))]
compile_error!("Only one RMW backend can be enabled at a time");
```

Applications select your backend with:

```toml
[dependencies]
nros = { version = "0.1.0", features = ["rmw-myproto", "platform-posix"] }
```

---

## C/C++ path

If your transport library is written in C or C++, use the `nros-rmw-cffi`
crate instead of implementing Rust traits directly. It provides a vtable
of C function pointers that map one-to-one onto the Rust trait methods.

### 1. Fill in the vtable

```c
#include <nros/rmw_vtable.h>

static void *my_open(const char *locator, uint8_t mode,
                     uint32_t domain_id, const char *node_name) {
    // Initialize your transport, return an opaque session handle
    // (or NULL on failure)
}

static int my_close(void *session) {
    // Tear down the session. Return 0 on success, -1 on error.
}

static int my_drive_io(void *session, int timeout_ms) {
    // Read from network, dispatch to internal buffers.
    return 0;
}

static void *my_create_publisher(void *session,
        const char *topic, const char *type_name,
        const char *type_hash, uint32_t domain_id,
        const nros_cffi_qos_t *qos) {
    // Return an opaque publisher handle (or NULL on failure)
}

static void  my_destroy_publisher(void *pub_handle) { /* cleanup */ }

static int my_publish_raw(void *pub_handle,
        const uint8_t *data, size_t len) {
    // Send CDR bytes. Return 0 on success, -1 on error.
}

/* ... implement remaining function pointers for subscriber,
       service server, and service client ... */

static nros_rmw_vtable_t my_vtable = {
    .open               = my_open,
    .close              = my_close,
    .drive_io           = my_drive_io,
    .create_publisher   = my_create_publisher,
    .destroy_publisher  = my_destroy_publisher,
    .publish_raw        = my_publish_raw,
    .create_subscriber  = my_create_subscriber,
    .destroy_subscriber = my_destroy_subscriber,
    .try_recv_raw       = my_try_recv_raw,
    .has_data           = my_has_data,
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
    nros_rmw_cffi_register(&my_vtable);
    // Now use the normal nros C API -- it routes through your vtable
    nros_executor_t exec;
    nros_executor_open(&exec, "tcp/127.0.0.1:9000", 0);
    // ...
}
```

Enable the `rmw-cffi` feature when building the nros C library:

```bash
cargo build -p nros-c --features "rmw-cffi,platform-posix,ros-humble"
```

All strings passed to vtable functions are null-terminated. Handles are
opaque `void *` pointers -- the Rust side never inspects them. Return
values follow the convention: 0 = success / no data, positive = byte
count, negative = error.

---

## Example: local echo RMW

A minimal backend that loops published messages back to subscribers on the
same session, with no network transport. Useful for unit testing. Only the
publisher and subscriber are shown -- service types are stubbed as no-ops
(return `Ok(None)` / `Ok(())`).

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

## Further reading

- [RMW API Reference](../reference/rmw-api.md) -- full trait signatures,
  QoS profiles, error types, configuration structs.
- [Internals: Adding a New RMW Backend](../internals/adding-rmw-backend.md)
  -- contributor-oriented details on discovery, key expression format, and
  testing infrastructure.
- [Zenoh-pico Symbol Reference](../internals/porting-platform/zenoh-pico.md)
  -- FFI symbol mapping for the zenoh-pico backend (useful as a reference
  for how an existing backend is structured).

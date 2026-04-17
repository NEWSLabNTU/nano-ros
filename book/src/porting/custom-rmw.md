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

pub struct MyProtoRmw;
impl Rmw for MyProtoRmw {
    type Session = MyProtoSession;
    type Error = TransportError;
    fn open(config: &RmwConfig) -> Result<MyProtoSession, TransportError> {
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

If your transport library is C or C++, use `nros-rmw-cffi` -- a vtable of
C function pointers that map one-to-one onto the Rust trait methods.

### 1. Fill in the vtable

The vtable has 18 function pointers. Key signatures:

```c
#include <nros/rmw_vtable.h>

static void *my_open(const char *locator, uint8_t mode,
                     uint32_t domain_id, const char *node_name);
static int   my_close(void *session);
static int   my_drive_io(void *session, int timeout_ms);
static void *my_create_publisher(void *session, const char *topic,
    const char *type_name, const char *type_hash,
    uint32_t domain_id, const nros_cffi_qos_t *qos);
static void  my_destroy_publisher(void *pub_handle);
static int   my_publish_raw(void *pub_handle, const uint8_t *data, size_t len);
// ... subscriber, service server, service client follow the same pattern.

static nros_rmw_vtable_t my_vtable = {
    .open = my_open, .close = my_close, .drive_io = my_drive_io,
    .create_publisher = my_create_publisher,
    .destroy_publisher = my_destroy_publisher,
    .publish_raw = my_publish_raw,
    /* ... fill all 18 fields (see nros/rmw_vtable.h) ... */
};
```

### 2. Register before opening a session

```c
nros_rmw_cffi_register(&my_vtable);  // before any nros API call
```

Build with `cargo build -p nros-c --features "rmw-cffi,platform-posix,ros-humble"`.

All strings are null-terminated. Handles are opaque `void *`. Return
convention: 0 = success/no data, positive = byte count, negative = error.

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

## Further reading

- [RMW API Reference](../reference/rmw-api.md) -- full trait signatures,
  QoS profiles, error types, configuration structs.
- [RMW API Design](../internals/rmw-api-design.md) -- architectural
  motivation and comparison with the ROS 2 rmw interface.
- [Zenoh-pico Symbol Reference](../internals/porting-platform/zenoh-pico.md)
  -- FFI symbol mapping for the zenoh-pico backend (useful as a reference
  for how an existing backend is structured).

# RMW API Design: nros-rmw vs ros2/rmw

nano-ros defines its own RMW (ROS Middleware) abstraction in the `nros-rmw` crate. While it serves the same purpose as the official [ros2/rmw](https://github.com/ros2/rmw) interface -- decoupling the client library from the transport backend -- it is designed for `no_std` embedded systems and uses a fundamentally different approach.

This page documents the architectural differences and trade-offs.

## Architectural Pattern

| Aspect | ROS 2 `rmw` | `nros-rmw` |
|--------|-------------|------------|
| Language | C API (`rmw/rmw.h`) | Rust traits |
| Dispatch | Runtime plugin loading (shared library via `rmw_implementation`) | Compile-time monomorphization (Rust generics) |
| `no_std` | No (requires libc, heap, POSIX) | Yes (zero heap in core path) |
| Error model | `rmw_ret_t` integer codes | `TransportError` enum + associated `type Error` per trait |

ROS 2 selects the RMW backend at runtime by loading a shared library (e.g., `rmw_fastrtps_cpp.so`). This enables switching backends without recompilation but requires dynamic linking and heap allocation.

nros-rmw selects the backend at compile time via Cargo feature flags. The `Session` trait uses associated types, so the compiler monomorphizes all transport calls -- no vtables, no dynamic dispatch, no heap. This is critical for MCUs with 16--256 KB of RAM.

## Object Model

### ROS 2

ROS 2 rmw has a deep initialization hierarchy:

```
rmw_init() → rmw_context_t
  → rmw_create_node() → rmw_node_t
    → rmw_create_publisher() → rmw_publisher_t*
    → rmw_create_subscription() → rmw_subscription_t*
    → rmw_create_service() → rmw_service_t*
    → rmw_create_client() → rmw_client_t*
```

Nodes are first-class RMW objects. Each `rmw_node_t` carries its own context, name, namespace, and security credentials. The RMW layer is responsible for node lifecycle and graph participation.

### nros-rmw

nros-rmw is flatter -- there is no node at the RMW level:

```
Rmw::open(&RmwConfig) → Session
  → session.create_publisher(&TopicInfo, QosSettings) → Self::PublisherHandle
  → session.create_subscriber(&TopicInfo, QosSettings) → Self::SubscriberHandle
  → session.create_service_server(&ServiceInfo) → Self::ServiceServerHandle
  → session.create_service_client(&ServiceInfo) → Self::ServiceClientHandle
```

`Node` lives one layer up in `nros-node`. It is purely a namespace and liveliness concern -- it borrows the session from the executor and creates typed communication handles. The RMW layer only knows about sessions and communication endpoints.

## Serialization Boundary

This is the most significant design difference.

**ROS 2:** The rmw layer operates on pre-serialized data. `rcl` and `rosidl` handle CDR serialization before calling `rmw_publish()` with an `rmw_serialized_message_t`. The rmw layer never sees typed messages -- it only moves byte buffers. Type metadata is passed separately via `rosidl_message_type_support_t` structs.

**nros-rmw:** The traits include both raw and typed methods:

```rust,ignore
pub trait Publisher {
    // Raw: caller handles serialization
    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error>;

    // Typed: serialize + publish in one call
    fn publish<M: RosMessage>(&self, msg: &M, buf: &mut [u8]) -> Result<(), Self::Error>;
}

pub trait Subscriber {
    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;
    fn try_recv<M: RosMessage>(&mut self, buf: &mut [u8]) -> Result<Option<M>, Self::Error>;
}
```

The typed methods have default implementations that call the raw methods with CDR serialization/deserialization from `nros-serdes`. This keeps the RMW layer self-contained -- no separate serialization layer is needed.

Type metadata uses simple structs (`TopicInfo { name, type_name, type_hash }`) instead of C type support function tables.

## I/O and Readiness Model

**ROS 2:** Uses `rmw_wait()` with a wait set (`rmw_wait_set_t`) containing subscriptions, services, clients, guard conditions, and events. The caller constructs a wait set, adds handles, and blocks until any handle is ready. This is similar to `select()`/`epoll()`.

**nros-rmw:** Uses a single `drive_io(timeout_ms)` method on the `Session` trait:

```rust,ignore
pub trait Session {
    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error> {
        let _ = timeout_ms;
        Ok(())
    }
}
```

This is a pull-based model: the executor calls `drive_io()` to poll the network and dispatch incoming data to internal subscriber buffers, then checks each entity with `has_data()`. There is no wait set -- the executor iterates its dispatch table directly.

For async integration, subscribers and service clients expose `register_waker(&Waker)` instead of guard conditions. The transport backend calls `waker.wake()` from its C receive callback, bridging to Rust `Future` waking without the wait set abstraction.

## Memory Model

**ROS 2:** Heap-allocates handles, messages, and serialization buffers. `rmw_serialized_message_t` wraps a dynamically-sized `rcutils_uint8_array_t`. Loaned message APIs (`rmw_borrow_loaned_message`, `rmw_take_loaned_message`) provide optional zero-copy for transports that support shared memory.

**nros-rmw:** Uses caller-provided `&mut [u8]` buffers everywhere. All receive and serialize operations write into stack-allocated or statically-allocated buffers:

```rust,ignore
// Caller provides the buffer
let mut buf = [0u8; 512];
let msg: Option<MyMsg> = subscriber.try_recv(&mut buf)?;
```

Zero-copy receive is supported via `process_raw_in_place()`, which invokes a closure with a reference to the subscriber's internal receive buffer, avoiding the copy into a caller-provided buffer. This is gated behind the `unstable-zenoh-api` feature.

## QoS Settings

ROS 2 `rmw_qos_profile_t` includes:

| Field | ROS 2 | nros-rmw |
|-------|-------|----------|
| History (keep last/all) | Yes | Yes |
| Depth | Yes | Yes |
| Reliability (reliable/best-effort) | Yes | Yes |
| Durability (volatile/transient local) | Yes | Yes |
| Deadline | Yes | No |
| Lifespan | Yes | No |
| Liveliness (automatic/manual) | Yes | No |
| `avoid_ros_namespace_conventions` | Yes | No |

nros-rmw provides the four QoS policies that zenoh-pico and XRCE-DDS can actually enforce. The time-based policies (deadline, lifespan, liveliness) are omitted because the supported transports do not implement them.

Standard QoS profiles (`QOS_PROFILE_DEFAULT`, `QOS_PROFILE_SENSOR_DATA`, `QOS_PROFILE_SERVICES_DEFAULT`, etc.) match their ROS 2 equivalents for interoperability.

## Service Client Model

**ROS 2:** Service clients are always asynchronous at the rmw level. `rmw_send_request()` sends a request and returns a sequence number. The reply is retrieved later via `rmw_take_response()`, typically driven by `rmw_wait()`.

**nros-rmw:** Provides both models:

```rust,ignore
pub trait ServiceClientTrait {
    // Blocking: send request and wait for reply
    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, Self::Error>;

    // Async: send request, poll for reply separately
    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error>;
    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;
}
```

The blocking `call_raw()` is convenient for simple embedded applications. The async split (`send_request_raw` + `try_recv_reply_raw`) is used by the executor for non-blocking dispatch.

## APIs Present in ROS 2 rmw but Absent in nros-rmw

| ROS 2 rmw API | Purpose | Why absent |
|----------------|---------|-----------|
| `rmw_node_t` / `rmw_create_node()` | Node lifecycle at RMW level | Node is above the RMW layer in `nros-node` |
| `rmw_wait_set_t` / `rmw_wait()` | Multiplexed readiness waiting | Replaced by `drive_io()` + per-entity `has_data()` |
| `rmw_guard_condition_t` | Wake wait set from application code | Replaced by `register_waker(&Waker)` |
| `rmw_event_t` | QoS event callbacks (deadline missed, etc.) | QoS events not supported |
| `rmw_get_topic_names_and_types()` | Graph introspection | Discovery via zenoh liveliness, not exposed at trait level |
| `rmw_get_node_names()` | Node discovery | Same as above |
| `rmw_count_publishers()` / `rmw_count_subscribers()` | Graph statistics | Not exposed |
| `rosidl_message_type_support_t` | C type support tables for serialization | Replaced by `TopicInfo` string metadata |
| `rmw_serialize()` / `rmw_deserialize()` | Standalone serialization | CDR handled by `nros-serdes` |
| `rmw_borrow_loaned_message()` | Zero-copy shared memory publish | Not supported (smoltcp/zenoh-pico don't use shared memory) |
| Content-filtered topics | Server-side topic filtering | Not supported |

## APIs Present in nros-rmw but Absent in ROS 2 rmw

| nros-rmw API | Purpose |
|--------------|---------|
| `Publisher::publish<M>(msg, buf)` | Typed publish with built-in CDR serialization |
| `Subscriber::try_recv<M>(buf)` | Typed receive with built-in CDR deserialization |
| `Subscriber::process_raw_in_place(f)` | Zero-copy in-place processing via closure |
| `Subscriber::try_recv_validated()` | E2E safety validation (CRC-32 + sequence tracking) |
| `ServiceClientTrait::call_raw()` | Blocking request/reply (ROS 2 rmw is async-only) |
| `ServiceServerTrait::handle_request<S>()` | Typed request handling with automatic CDR roundtrip |
| `Session::drive_io(timeout_ms)` | Explicit network polling (ROS 2 rmw relies on middleware threads) |

## Summary

The core difference is that ROS 2 rmw is a **C plugin interface** designed for desktop systems with dynamic linking, heap allocation, and OS threading. nros-rmw is a **Rust trait hierarchy** designed for MCUs with static dispatch, stack allocation, and cooperative scheduling. The trade-off is flexibility (ROS 2 can swap backends at runtime) vs efficiency (nros eliminates all abstraction overhead at compile time).

Despite these differences, the two are **wire-compatible** when using the same transport. An nros node using `rmw-zenoh` communicates with a ROS 2 node using `rmw_zenoh_cpp` through the same `zenohd` router, with matching QoS profiles and CDR encoding.

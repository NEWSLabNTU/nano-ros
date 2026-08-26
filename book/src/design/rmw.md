# RMW API Design: nros-rmw vs ros2/rmw

nano-ros defines its own RMW (ROS Middleware) abstraction in the `nros-rmw` crate. While it serves the same purpose as the official [ros2/rmw](https://github.com/ros2/rmw) interface -- decoupling the client library from the transport backend -- it is designed for `no_std` embedded systems and uses a fundamentally different approach.

This page documents the architectural differences and trade-offs. For trait signatures and the available backends, see [RMW API Reference](../reference/rmw-api.md). For implementing a new backend, see [Custom RMW Backend](../porting/custom-rmw.md).

## Why We Revised `rmw.h`

`rmw.h` was designed for ROS 2 on Linux: a process with a libc heap, an OS scheduler, dynamic loaders, and middleware-owned background threads. None of those assumptions hold on a Cortex-M3 with 64 KB of RAM. Each constraint below drove a specific change.

### Heap availability

`rmw.h` heap-allocates everywhere -- handles, serialized message buffers, wait sets, type support tables. Bare-metal targets often have no allocator; RTOS targets have allocators with hard total budgets (~16-256 KB) that must cover the application as well.

nros-rmw moves all I/O buffers to the caller. `publish_raw(&[u8])` and `try_recv_raw(&mut [u8])` operate on slices that the caller stack- or statically-allocates. Type metadata is a string-only `TopicInfo` struct, not a pointer-laden `rosidl_message_type_support_t` table. What the abstraction itself allocates is nothing. What the BACKENDS allocate is
enumerated by `scripts/rmw-alloc-sites.py`, which reports each site with its
file and line and — the distinction that decides whether you care — whether it
sits on the STEADY-STATE path (per message, so it is latency and a heap that
must sustain traffic) or in CREATE/INIT (bounded, so it lands in startup):

| backend | steady-state | create / init |
| --- | ---: | ---: |
| Cyclone DDS | 6 | 6 |
| XRCE-DDS | 0 | 9 |
| uORB | 0 | 3 |

Run the script rather than trusting this table; it is the re-runnable source and
this is a snapshot of it. Two caveats it states and this page inherits: it
counts nano-ros's OWN sites, so allocations inside Cyclone below `dds_write` and
inside zenoh-pico's `z_malloc` are real but not listed (they are not ours to
remove), and zenoh-pico's internal transport buffers (~64 KB) reach the image
through `PlatformAlloc`, where a bump allocator suffices on bare-metal.

### Threading model

`rmw.h` assumes the middleware owns threads. `rmw_wait()` blocks the calling thread on a wait set; some implementations also spawn internal dispatch threads that fire callbacks asynchronously. Bare-metal has no scheduler; cooperative RTOS configurations can't tolerate hidden threads.

nros-rmw replaces `rmw_wait` with `Session::drive_io(timeout_ms)` -- a single call the executor invokes from its own (and only) thread. There is no wait set object, and no entity is implicitly polled by the middleware. The application drives all I/O explicitly. For async runtimes, subscribers and service clients expose `register_waker(&Waker)` so the transport's C receive callback can wake a Rust future without a wait set abstraction.

### Single-threaded callback dispatch

`rmw.h` permits multi-threaded executors and reentrant callbacks. Cooperative single-threaded targets cannot guarantee atomicity around RMW state without locks they don't have.

nros-rmw assumes a single-threaded executor that owns the session for its lifetime. Callbacks run sequentially on the executor thread; no callback can preempt another. This eliminates the need for internal locking around publisher state, subscriber buffers, or service queues -- a measurable code-size and runtime win on MCUs.

### No dynamic discovery tables

`rmw.h` provides `rmw_get_topic_names_and_types()`, `rmw_count_publishers()`, `rmw_get_node_names()`, and similar graph-introspection APIs. These require maintaining a dynamic discovery cache, which costs heap and CPU continuously even when nothing reads it.

nros-rmw carried none of these for most of its life: discovery happened at the transport layer (zenoh liveliness, XRCE-DDS session establishment) and was never surfaced as queryable graph state.

That is changing. Phase-376 W4 added the graph-enumeration slots to the ABI —
`get_node_names`, `get_topic_names_and_types`, `count_publishers` and the
by-node variants — as *visitor* callbacks rather than allocated
names-and-types arrays, so a backend walks its own discovery data and the
caller never owns a table. The slots are declared; backend wiring is the
in-flight part. The generated
[Per-RMW Feature Matrix](../reference/rmw-feature-matrix.md) is the current
per-backend truth, and it is derived from the vtables rather than from this
page.

### Compile-time backend selection

`rmw.h` selects backends at runtime via `dlopen()` of `librmw_*.so`. This requires a dynamic loader (no embedded MCU has one) and forces every call through a vtable.

nros-rmw selects the backend at **link time** via Cargo features, and reaches
it through a **C ABI vtable** (`nros_rmw_vtable_t`, RFC-0054) that the backend
hands the runtime once via `nros_rmw_cffi_register()` before any session is
created. There is no loader, no `.so`, and no path search: the only backends
reachable are the ones linked into the image.

This page used to claim the opposite — "no vtables, no dynamic dispatch",
monomorphized through Rust generics. That was true before RFC-0054 made the C
headers the SSoT so a backend could be written in C or C++ (Cyclone DDS is);
`nros-rmw`'s Rust traits are still the surface a Rust backend implements, and
`rust_adapter.rs` in `nros-rmw-cffi` is what turns such an impl into the
vtable the runtime calls. The cost is one indirect call per operation; what it
buys is a backend boundary that is not Rust-only.

## Architectural Pattern

| Aspect | ROS 2 `rmw` | `nros-rmw` |
|--------|-------------|------------|
| Language | C API (`rmw/rmw.h`) | Rust traits |
| Dispatch | Runtime plugin loading (shared library via `rmw_implementation`) | Link-time selection; calls cross a C ABI vtable registered at startup (RFC-0054) |
| `no_std` | No (requires libc, heap, POSIX) | Yes — but "no heap" is a property of the BACKEND, not of the abstraction (see below) |
| Error model | `rmw_ret_t` integer codes | `nros_rmw_ret_t` at the ABI, using upstream rmw's VALUES (phase-376 W3.d); `TransportError` on the Rust side |

ROS 2 selects the RMW backend at runtime by loading a shared library (e.g.,
`rmw_fastrtps_cpp.so`). This enables switching backends without recompilation
but requires a dynamic loader — which no MCU has.

nros-rmw links exactly the backends the image was built with and dispatches
through a registered vtable. What that removes is the loader and the
relocation work at startup, not the indirect call.

**On heap:** the abstraction adds none — I/O buffers are caller-owned, and
entity handles are inline. The BACKENDS are another matter, and this page
overstated it for years ("no heap", flatly). Issue 0777 established that;
`scripts/rmw-alloc-sites.py` now answers it precisely and repeatably. Only
Cyclone DDS allocates on the steady-state path in nano-ros's own code (6 sites);
XRCE and uORB allocate at entity/transport setup only. Plan the heap budget from
your backend's row in that report — and remember it excludes what the middleware
libraries do underneath, which for Cyclone and zenoh-pico is a general allocator
call per message regardless.

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
  → session.create_subscription(&TopicInfo, QosSettings) → Self::SubscriptionHandle
  → session.create_service(&ServiceInfo, QosSettings) → Self::ServiceHandle
  → session.create_client(&ServiceInfo, QosSettings) → Self::ClientHandle
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

pub trait Subscription {
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

**nros-rmw:** The same async split (phase-301 deleted the deprecated blocking `call_raw` path — like upstream, there is no blocking call at the RMW level):

```rust,ignore
pub trait ClientTrait {
    // Async: send request, poll for reply separately
    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error>;
    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;
}
```

Blocking waits are composed above the RMW by the executor (which keeps driving `drive_io` between polls).

## APIs Present in ROS 2 rmw but Absent in nros-rmw

Phase-376 is closing much of this table: several rows below have gained ABI
slots, and the ones that are DECLINED now carry a written reason rather than
silence. Treat the generated
[Per-RMW Feature Matrix](../reference/rmw-feature-matrix.md) as current and
this table as the design rationale behind the original omissions.

| ROS 2 rmw API | Purpose | Why absent |
|----------------|---------|-----------|
| `rmw_node_t` / `rmw_create_node()` | Node lifecycle at RMW level | **No longer absent** (phase-376 W5/B1): entities are created ON a node, upstream's shape — `create_publisher` and its siblings take a `const rmw_node_t *`. Node lifecycle still lives above the RMW in `nros-node`. |
| `rmw_wait_set_t` / `rmw_wait()` | Multiplexed readiness waiting | Replaced by `drive_io()` + per-entity `has_data()` |
| `rmw_guard_condition_t` | Wake wait set from application code | Replaced by `register_waker(&Waker)` |
| `rmw_event_t` | QoS event callbacks (deadline missed, etc.) | Partly present: `subscription_event_init` / `publisher_event_init` are ABI slots and zenoh wires them; the other backends leave them NULL. |
| `rmw_get_topic_names_and_types()` | Graph introspection | Slot declared (phase-376 W4) as a VISITOR callback — the backend walks its own discovery data, the caller owns no table. Backend wiring in flight. |
| `rmw_get_node_names()` | Node discovery | Same — declared as a visitor. |
| `rmw_count_publishers()` / `rmw_count_subscribers()` | Graph statistics | Same — declared, wiring in flight. |
| `rosidl_message_type_support_t` | C type support tables for serialization | Replaced by `TopicInfo` string metadata |
| `rmw_serialize()` / `rmw_deserialize()` | Standalone serialization | CDR handled by `nros-serdes` |
| `rmw_borrow_loaned_message()` | Zero-copy shared memory publish | Not supported (smoltcp/zenoh-pico don't use shared memory) |
| Content-filtered topics | Server-side topic filtering | Not supported |

## APIs Present in nros-rmw but Absent in ROS 2 rmw

| nros-rmw API | Purpose |
|--------------|---------|
| `Publisher::publish<M>(msg, buf)` | Typed publish with built-in CDR serialization |
| `Subscription::try_recv<M>(buf)` | Typed receive with built-in CDR deserialization |
| `Subscription::process_raw_in_place(f)` | Zero-copy in-place processing via closure |
| `Subscription::try_recv_validated()` | E2E safety validation (CRC-32 + sequence tracking) |
| `ServiceTrait::handle_request<S>()` | Typed request handling with automatic CDR roundtrip |
| `Session::drive_io(timeout_ms)` | Explicit network polling (ROS 2 rmw relies on middleware threads) |

## Summary

The core difference is that ROS 2 rmw is a **C plugin interface** designed for desktop systems with dynamic linking, heap allocation, and OS threading. nros-rmw is a **Rust trait hierarchy** designed for MCUs with static dispatch, stack allocation, and cooperative scheduling. The trade-off is flexibility (ROS 2 can swap backends at runtime) vs efficiency (nros eliminates all abstraction overhead at compile time).

Despite these differences, the two are **wire-compatible** when using the same transport. An nros node using `nros-rmw-zenoh` communicates with a ROS 2 node using `rmw_zenoh_cpp` through the same router, with matching QoS profiles and CDR encoding.

### The zenoh pairing (phase-362 / RFC-0075)

The router is **the one ROS ships** — `ros2 run rmw_zenoh_cpp rmw_zenohd`. It
links the same `libzenohc.so` that `rmw_zenoh_cpp` does, so it cannot drift from
the RMW you are actually talking to, and it is what a ROS 2 deployment runs.
nano-ros no longer ships a router of its own.

The table below is **data to diff a future failure against, not a constraint we
enforce**. `rmw_zenoh_cpp` lives on your machine, installed by your distro; we
cannot pin it, and the interesting number is not the ROS package version anyway.

| side | component | observed |
| --- | --- | --- |
| host | `rmw_zenoh_cpp` router + RMW | zenoh-c **1.6.2** (ROS 2 Humble, measured 2026-08-16) |
| firmware | `zenoh-pico` | **1.7.2** |

Read the zenoh version from the header, never from the package manager:

```
/opt/ros/<distro>/opt/zenoh_cpp_vendor/include/zenoh_configure.h   #define ZENOH_C "…"
packages/rmw/zenoh/zpico-sys/zenoh-pico/version.txt
```

The ROS package version (`ros-humble-rmw-zenoh-cpp 0.1.9`) is a **wrapper**
version and says nothing about the zenoh inside it — issue 0609 measured that
same package moving its vendored zenoh 1.2.0 → 1.8.0 in a patch-level bump, and
reading the package version instead of the header is what produced a wrong
version claim in that issue's first filing.

The two sides need not match. Under zenoh's 1.x wire guarantee a firmware pin
should move for its own reasons — footprint, features, fixes — rather than to
chase a host package.

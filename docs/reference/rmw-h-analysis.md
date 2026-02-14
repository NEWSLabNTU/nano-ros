# ROS 2 rmw.h Analysis for Embedded Middleware Abstraction

## Overview

ROS 2's `rmw.h` defines the C function interface (~60 functions) that every middleware implementation must provide. This document analyzes rmw.h's suitability as a reference for nano-ros's `nros-rmw` trait abstraction, identifying what to adopt and what to avoid for `no_std` embedded targets.

## rmw.h Function Categories

### Entity Lifecycle

```
rmw_create_node / rmw_destroy_node
rmw_create_publisher / rmw_destroy_publisher
rmw_create_subscription / rmw_destroy_subscription
rmw_create_service / rmw_destroy_service
rmw_create_client / rmw_destroy_client
rmw_create_wait_set / rmw_destroy_wait_set
rmw_create_guard_condition / rmw_destroy_guard_condition
```

All return heap-allocated pointers (`rmw_publisher_t*`, etc.) and require corresponding `destroy` calls to free memory.

### Data Operations

```
rmw_publish(publisher, ros_message, allocation)
rmw_publish_serialized_message(publisher, serialized_message, allocation)
rmw_take(subscription, ros_message, taken, allocation)
rmw_take_with_info(subscription, ros_message, taken, message_info, allocation)
rmw_take_serialized_message(subscription, serialized_message, taken, allocation)
rmw_take_request(service, request_header, ros_request, taken)
rmw_send_response(service, request_header, ros_response)
rmw_send_request(client, ros_request, sequence_id)
rmw_take_response(client, request_header, ros_response, taken)
```

Two modes: typed (`void* ros_message` + runtime type support) and serialized (`rmw_serialized_message_t`). The serialized mode maps directly to nano-ros's `publish_raw`/`try_recv_raw`.

### Central Polling

```
rmw_wait(subscriptions, guard_conditions, services, clients, events, wait_set, timeout)
```

Blocks until at least one entity has data or timeout expires. On return, entries without data are nulled out in the arrays.

### Graph Introspection

```
rmw_get_topic_names_and_types
rmw_get_service_names_and_types
rmw_get_node_names / rmw_get_node_names_with_enclaves
rmw_count_publishers / rmw_count_subscribers
rmw_publisher_count_matched_subscriptions
rmw_subscription_count_matched_publishers
```

All require dynamic-length string arrays. Not feasible in `no_std` without alloc.

### QoS

```
rmw_publisher_get_actual_qos
rmw_subscription_get_actual_qos
```

`rmw_qos_profile_t` has 10+ fields: reliability, durability, history, depth, deadline, lifespan, liveliness, lease_duration, avoid_ros_namespace_conventions, etc.

## Limitations for Embedded / no_std

### 1. Heap allocation in every create/destroy pair

Every `rmw_create_*` function returns a heap-allocated pointer. `rmw_create_publisher` allocates `rmw_publisher_t` on the heap and returns `rmw_publisher_t*`. On bare-metal with a bump allocator, `rmw_destroy_publisher` cannot actually reclaim memory -- leading to leaks if entities are created/destroyed dynamically.

**nros-rmw adaptation:** Return handles (integer indices) or use caller-provided storage. No heap allocation in trait methods. The current traits use associated types (`Session::Publisher`) that can be stack-allocated or statically placed.

### 2. rmw_wait is blocking and multi-entity

`rmw_wait` takes arrays of subscriptions, services, clients, guard conditions, and events, then blocks until any has data. This design assumes:
- A multi-threaded executor that can afford to block one thread
- Dynamic-length arrays of entities (allocated on heap)
- A kernel-level waiting mechanism (epoll, select, condition variables)

On single-threaded bare-metal, blocking means the entire system freezes. No kernel wait primitives are available.

**nros-rmw adaptation:** `spin_once(timeout_ms)` with non-blocking polling. The executor checks each entity in a loop, returning immediately when work is found or timeout expires. This is what the current `traits.rs` already does.

### 3. Runtime type support via function pointers

`rmw_publish` takes `const rosidl_message_type_support_t* type_support` -- a vtable of function pointers for runtime serialization/deserialization. This enables language-agnostic message handling (C, C++, Python share the same rmw calls).

nano-ros uses compile-time generics: `Publisher<M: RosMessage>` monomorphizes at build time. No function pointer overhead, no runtime type dispatch, compatible with `no_std`.

**nros-rmw adaptation:** Trait methods operate on raw bytes (`&[u8]`). Typed serialization (`publish<M>`, `try_recv<M>`) happens at a higher layer (board crates or `nros-node`), not in the RMW traits. This separates CDR concerns from middleware concerns.

### 4. Graph introspection requires alloc

`rmw_get_topic_names_and_types` returns variable-length arrays of strings. No fixed-size bound is possible -- the graph can have arbitrary many topics. This is inherently incompatible with `no_std` without alloc.

**nros-rmw adaptation:** Graph introspection is optional, gated behind `#[cfg(feature = "alloc")]` or omitted entirely for embedded. Discovery (liveliness tokens, SPDP) is middleware-specific and belongs in the RMW backend (`nros-rmw-zenoh`), not the trait interface.

### 5. Full QoS model is overkill

`rmw_qos_profile_t` exposes the complete DDS QoS model: deadline, lifespan, liveliness lease duration, etc. Most embedded applications need only reliability (best-effort vs reliable) and optionally history depth.

**nros-rmw adaptation:** `QosSettings` with a minimal subset: reliability, durability, history policy, depth. Middleware backends map these to their native QoS. Additional QoS can be added later without breaking the trait interface.

### 6. No serialization boundary

`rmw_publish` accepts either `void* ros_message` (typed) or `rmw_serialized_message_t` (pre-serialized). The typed path requires the rmw implementation to know how to serialize -- coupling serialization to the middleware layer.

**nros-rmw adaptation:** RMW traits only handle raw bytes. Serialization is above the RMW layer (in `nros-core` via `CdrWriter`/`CdrReader`). This is a cleaner boundary: the middleware just moves bytes, it doesn't interpret them.

## What to Adopt from rmw.h

### Function decomposition

rmw.h's create/destroy/publish/take/wait decomposition is the right granularity. Each entity has a clear lifecycle and data operations. The nros-rmw traits mirror this:

| rmw.h | nros-rmw |
|-------|----------|
| `rmw_create_publisher` | `Session::create_publisher` |
| `rmw_publish` | `Publisher::publish_raw` |
| `rmw_create_subscription` | `Session::create_subscriber` |
| `rmw_take` | `Subscriber::try_recv_raw` |
| `rmw_create_service` | `Session::create_service_server` |
| `rmw_take_request` + `rmw_send_response` | `ServiceServer::try_recv_request` + `send_reply` |
| `rmw_create_client` | `Session::create_service_client` |
| `rmw_send_request` + `rmw_take_response` | `ServiceClient::call_raw` |
| `rmw_wait` | `Session::spin_once` |

### Raw bytes interface

`rmw_publish_serialized_message` and `rmw_take_serialized_message` operate on pre-serialized CDR bytes. This is the right abstraction boundary for embedded: the middleware moves bytes without interpreting them.

### Separation of node from session

rmw.h has `rmw_node_t` separate from the middleware context. For embedded (1 node per MCU), these can be merged, but the conceptual separation is useful for desktop targets running multiple nodes.

## Current Zenoh-Specific Leaks in traits.rs

Five items in the current `traits.rs` are zenoh-specific and must move to `nros-rmw-zenoh` during the Phase 33 transport split (33.2):

| Item | Current location | Problem | Fix |
|------|-----------------|---------|-----|
| `TopicInfo::to_key()` | traits.rs | Formats zenoh keyexpr `<domain>/<topic>/<type>/TypeHashNotSupported` | Move to `nros-rmw-zenoh/keyexpr.rs` |
| `TopicInfo::to_key_wildcard()` | traits.rs | Same, with wildcard suffix | Same |
| `ServiceInfo::to_key()` / `to_key_wildcard()` | traits.rs | Same pattern for services | Same |
| `QosSettings::to_qos_string()` | traits.rs | Formats zenoh liveliness QoS string `2:2:1,1:,:,:,,` | Same |
| `validate_locator()` / `locator_protocol()` | traits.rs | Parses zenoh locator format `tcp/...` | Same |

Everything else (trait signatures, QoS enums, error types, `ServiceRequest`) is genuinely middleware-agnostic.

# RMW API: Differences from upstream `rmw.h`

This page is for developers who already know upstream ROS 2's
[`rmw/rmw.h`](https://github.com/ros2/rmw) and want to write — or just
understand — a backend for nano-ros. Side-by-side: what `rmw.h` looks
like, what `nros-rmw-cffi` looks like, and why nano-ros diverges.

The C signatures shown for nano-ros come from
[`<nros/rmw_vtable.h>`](../api/rmw-cffi/index.html). The Rust trait
counterparts are an alternative entry point for porters who already
work in Rust; this page sticks to the C-vtable surface throughout.

## TL;DR

| Concern | upstream `rmw.h` | `nros-rmw-cffi` |
|---------|------------------|------------------|
| Plugin loading | `dlopen("librmw_*.so")` at runtime | Single vtable registered at init |
| Init sequence | `rmw_init_options_t` → `rmw_context_t` → entities | One `open()` call, returns the session |
| Entity types | `rmw_publisher_t` / `rmw_subscription_t` / `rmw_service_t` / `rmw_client_t` | All entities are `nros_rmw_handle_t` (opaque `void*`) |
| Wait | `rmw_wait(waitset, timeout)` blocks the caller | `drive_io(session, timeout_ms)` drives I/O once |
| Serialization | typesupport-driven (rosidl) | Pre-serialized CDR bytes only |
| Graph queries | `rmw_get_topic_names_and_types`, … | None |
| QoS profiles | Full DDS profile match | Backend-defined minimal subset |
| DDS events | `rmw_event_t` (`rmw_take_event`) | None |
| Loaned messages | Optional `rmw_borrow_loaned_message` | First-class `loan_publish` / `loan_recv` |
| Error returns | `rmw_ret_t` (`RMW_RET_OK`, …) | `nros_rmw_ret_t` (`NROS_RMW_RET_OK`, …) — same named-constant style |

## 1. Plugin loading vs. compile-time backend

**Upstream.** `rmw_implementation` resolves the backend at process
start by opening a shared library:

```c
RCUTILS_DLLIMPORT
const char * rmw_get_implementation_identifier(void);
```

The runtime calls `dlopen` against the path encoded by
`RMW_IMPLEMENTATION` and binds every `rmw_*` symbol through the loader.

**nano-ros.** The backend is linked into the binary. A C backend
registers its vtable once before any nros call:

```c
#include <nros/rmw_vtable.h>

static const nros_rmw_vtable_t MY_VTABLE = { ... };

int main(void) {
    nros_rmw_cffi_register(&MY_VTABLE);
    nros_init(...);
}
```

**Why.** Most embedded targets have no dynamic loader. Even where
`dlopen` exists, a 32 KB Flash budget can't afford to link every
backend's C client and pick at runtime. Compile-time selection cuts
the binary by 60–80 % and lets the linker drop unused entity paths.

## 2. Init sequence

**Upstream.** Three-step init:

```c
rmw_init_options_t options = rmw_get_zero_initialized_init_options();
rmw_init_options_init(&options, allocator);
rmw_context_t context = rmw_get_zero_initialized_context();
rmw_init(&options, &context);
/* ... use context to create rmw_node_t, then rmw_publisher_t, … */
rmw_shutdown(&context);
```

**nano-ros.** One step — a session covers what upstream splits across
options + context:

```c
nros_rmw_handle_t session = vtable->open(locator, mode, domain_id, node_name);
/* ... use session to create_publisher / create_subscriber / … */
vtable->close(session);
```

**Why.** Upstream's split is useful when an application owns multiple
RMW instances with different options. Nano-ros assumes one session per
process — one transport, one wire-protocol, one set of QoS defaults —
so the options/context separation buys nothing.

## 3. Entity handles

**Upstream.** Every entity is a typed C struct with backend-private
state hidden behind `void * data`:

```c
typedef struct RMW_PUBLIC_TYPE rmw_publisher_t {
  const char * implementation_identifier;
  void * data;
  const char * topic_name;
  rmw_publisher_options_t options;
  bool can_loan_messages;
} rmw_publisher_t;
```

**nano-ros.** All entities are an opaque `void *`:

```c
typedef void* nros_rmw_handle_t;

nros_rmw_handle_t (*create_publisher)(nros_rmw_handle_t session,
                                       const char * topic_name,
                                       const char * type_name,
                                       const char * type_hash,
                                       nros_rmw_cffi_qos_t qos);
```

**Why.** `rmw_publisher_t` exists so the upstream client library can
read fields (e.g., `topic_name`, `can_loan_messages`) without calling
into the backend. The cost is that every backend has to maintain those
fields in sync with its own state. Nano-ros's runtime never reads
backend-internal state directly — it asks the vtable. Collapsing
entities to opaque pointers removes that synchronisation burden.

## 4. `drive_io` vs. `rmw_wait`

**Upstream.** Clients block on a waitset that aggregates entities:

```c
rmw_ret_t rmw_wait(
  rmw_subscriptions_t * subscriptions,
  rmw_guard_conditions_t * guard_conditions,
  rmw_services_t * services,
  rmw_clients_t * clients,
  rmw_events_t * events,
  rmw_wait_set_t * wait_set,
  const rmw_time_t * wait_timeout);
```

The middleware can also spawn its own background threads that fire
callbacks asynchronously.

**nano-ros.** The executor calls a single drive-I/O entry point:

```c
int32_t (*drive_io)(nros_rmw_handle_t session, int32_t timeout_ms);
```

The backend dispatches whatever receive / send / wakeup work is
pending and returns within `timeout_ms`. There is no waitset, no
guard-condition aggregation, no implicit middleware thread.

**Why.** Cooperative single-task runtimes (the bare-metal and
RTOS variants of nano-ros) have only one execution context. A waitset
abstraction doesn't fit — the executor *is* the waitset. Replacing
`rmw_wait` with `drive_io` makes the cooperative model explicit and
removes the multi-thread complexity from every backend that doesn't
need it.

## 5. Serialization: CDR bytes, not typesupport

**Upstream.** Publish/take APIs accept *typed* messages plus
typesupport pointers, and each backend implements
serialization-from-typesupport itself:

```c
rmw_ret_t rmw_publish(
  const rmw_publisher_t * publisher,
  const void * ros_message,
  rmw_publisher_allocation_t * allocation);
```

The backend dereferences `ros_message` according to a
`rosidl_message_type_support_t` table to compute the wire bytes.

**nano-ros.** The runtime serializes upstream of the RMW. Backends
receive *already-CDR-encoded* bytes:

```c
int32_t (*publish_raw)(nros_rmw_handle_t publisher,
                       const uint8_t * data, size_t len);

int32_t (*try_recv_raw)(nros_rmw_handle_t subscriber,
                        uint8_t * buf, size_t len);
```

**Why.** rosidl typesupport is heavy: dynamic dispatch through a
function-pointer table per field type, dependence on dynamic
allocators for nested sequences, megabytes of generated symbol tables
linked even for one message. Splitting the layers means:

- Codegen (`cargo nano-ros generate-c`) writes a fixed-shape C struct
  per message and a single `<MsgType>_serialize_cdr(...)` function.
- The runtime calls it before handing bytes to the RMW.
- The RMW backend stays transport-only — no rosidl, no typesupport,
  no allocator coupling.

## 6. No graph cache

**Upstream.** Graph introspection sits in the RMW:

```c
rmw_ret_t rmw_get_topic_names_and_types(...);
rmw_ret_t rmw_count_publishers(...);
rmw_ret_t rmw_get_node_names(...);
```

Implementations maintain a discovery cache that costs heap and CPU
continuously even when nothing reads it.

**nano-ros.** None of the above. Backends do whatever discovery their
transport mandates (zenoh liveliness, XRCE-DDS session establishment),
but no `get_topic_names` / `count_publishers` / `node_names` exists in
the vtable.

**Why.** Graph introspection is fundamentally a host-side debugging
need. An MCU has no terminal, no `ros2 topic list`. Wire-protocol
interop with `rmw_zenoh_cpp` means standard ROS 2 tools running on a
laptop can introspect the same domain as the MCU — at zero cost on
the MCU.

## 7. QoS: minimal subset, no profile matching

**Upstream.** Full DDS QoS profile family with profile *matching*
between endpoints:

```c
typedef struct RMW_PUBLIC_TYPE rmw_qos_profile_t {
  rmw_qos_history_policy_t history;
  size_t depth;
  rmw_qos_reliability_policy_t reliability;
  rmw_qos_durability_policy_t durability;
  rmw_time_t deadline;
  rmw_time_t lifespan;
  rmw_qos_liveliness_policy_t liveliness;
  rmw_time_t liveliness_lease_duration;
  bool avoid_ros_namespace_conventions;
} rmw_qos_profile_t;
```

The backend negotiates compatibility (`rmw_qos_profile_check_compatible`)
and surfaces violations as events.

**nano-ros.** Each backend defines its own minimal subset; the vtable
QoS struct is small:

```c
typedef struct nros_rmw_cffi_qos_t {
    uint8_t reliability;   /* RELIABLE | BEST_EFFORT */
    uint8_t durability;    /* VOLATILE | TRANSIENT_LOCAL */
    uint8_t history;       /* KEEP_LAST | KEEP_ALL */
    uint16_t depth;
} nros_rmw_cffi_qos_t;
```

No deadline / lifespan / liveliness fields. No profile matching at the
RMW layer — backends honor the subset they natively implement.

**Why.** `rmw_qos_profile_t` was DDS-shaped from day one. Zenoh-pico
has no concept of "liveliness lease" the way DDS does, and no
backend besides DDS implements `deadline` enforcement. Promising QoS
features that a backend can't enforce is worse than not promising
them; constraining the surface to four fields each backend actually
honors makes the contract real.

## 8. No DDS event API

**Upstream.** Event APIs surface DDS notifications:

```c
rmw_ret_t rmw_take_event(
  const rmw_event_t * event_handle,
  void * event_info,
  bool * taken);
```

`rmw_event_t` types include `RMW_EVENT_REQUESTED_DEADLINE_MISSED`,
`RMW_EVENT_LIVELINESS_LOST`, `RMW_EVENT_OFFERED_DEADLINE_MISSED`, etc.

**nano-ros.** None of these. The vtable has no `take_event` or
`event_handle`.

**Why.** Most events are DDS-only. Nano-ros backends include zenoh-pico,
XRCE-DDS, dust-DDS, and uORB; only dust-DDS has native equivalents,
and it surfaces them through its own callback registration outside the
RMW. Adding the upstream event API to the vtable would force every
non-DDS backend to stub it.

## 9. Loaned messages first-class

**Upstream.** Optional, often unimplemented:

```c
rmw_ret_t rmw_borrow_loaned_message(
  const rmw_publisher_t * publisher,
  const rosidl_message_type_support_t * type_support,
  void ** ros_message);

rmw_ret_t rmw_return_loaned_message_from_publisher(
  const rmw_publisher_t * publisher,
  void * loaned_message);
```

A backend that doesn't implement these returns
`RMW_RET_UNSUPPORTED` and the client falls back to a copying path.

**nano-ros.** Lending is a separate vtable surface (Phase 99). When
the backend supports zero-copy publish, it implements
`loan_publish_*` / `commit_publish_*`; when it supports zero-copy
receive, it implements `loan_recv_*` / `release_recv_*`. The runtime
checks the function pointers for non-NULL once at session open and
takes the lending path for the lifetime of the session.

**Why.** Zero-copy isn't optional on embedded — every avoidable copy
is a copy of a CDR-encoded sensor frame from a 64 KB heap. Promoting
the lending surface to first-class makes it visible at compile time
(missing pointer = compile error against the lending build) and lets
the runtime pre-arrange arena slots once instead of probing on every
publish.

## 10. Error returns

**Upstream.** Single error type for everything:

```c
typedef int32_t rmw_ret_t;
#define RMW_RET_OK              0
#define RMW_RET_ERROR           1
#define RMW_RET_TIMEOUT         2
#define RMW_RET_UNSUPPORTED     3
/* ... */
```

Pointer-returning calls indicate failure with `NULL` *and* set a
thread-local error string via `rmw_set_error_string`.

**nano-ros.** Same named-constant style (`<nros/rmw_ret.h>`),
different sign convention, no thread-local error string:

```c
typedef int32_t nros_rmw_ret_t;
#define NROS_RMW_RET_OK                       0
#define NROS_RMW_RET_ERROR                   -1
#define NROS_RMW_RET_TIMEOUT                 -2
#define NROS_RMW_RET_BAD_ALLOC               -3
#define NROS_RMW_RET_INVALID_ARGUMENT        -4
#define NROS_RMW_RET_UNSUPPORTED             -5
#define NROS_RMW_RET_INCOMPATIBLE_QOS        -6
#define NROS_RMW_RET_TOPIC_NAME_INVALID      -7
#define NROS_RMW_RET_NODE_NAME_NON_EXISTENT  -8
#define NROS_RMW_RET_LOAN_NOT_SUPPORTED      -9
#define NROS_RMW_RET_NO_DATA                -10
#define NROS_RMW_RET_WOULD_BLOCK            -11
#define NROS_RMW_RET_BUFFER_TOO_SMALL       -12
#define NROS_RMW_RET_MESSAGE_TOO_LARGE      -13
```

Two return-shape conventions, picked by call shape:

| Returns | Success | Failure |
|---------|---------|---------|
| `nros_rmw_handle_t` (`open`, `create_publisher`, …) | non-NULL | `NULL` |
| `nros_rmw_ret_t` (`drive_io`, `publish_raw`, `commit_slot`, …) | `NROS_RMW_RET_OK` | negative named constant |
| `int32_t` byte count (`try_recv_raw`, `try_recv_request`, `call_raw`) | `>= 0` (bytes received) | negative `nros_rmw_ret_t` |

**Differences from upstream.**

- **Negative for error.** Upstream uses positive integer codes
  (`RMW_RET_ERROR = 1`); nano-ros uses negative so the byte-count
  convention can be unified into the same `int32_t` return.
- **No thread-local error string.** No `rmw_set_error_string`, no
  `rmw_get_error_string`. The thread-local heap allocation that
  pattern needs is unaffordable on embedded targets. Backends log
  verbose diagnostics at the failure site through the platform's
  `printk` equivalent — never buffered, never thread-local.
- **Smaller code-set.** 13 codes total (vs upstream's ~25). Phase
  102.1 audit started from upstream's set and dropped codes that
  don't apply (e.g., DDS event codes, `RMW_RET_NODE_INVALID`).
  Adding a code is a `<nros/rmw_ret.h>` header change only.

**Why same style.** Phase 102 deliberately moved closer to upstream
on this point. Named constants make `switch` statements possible at
call sites; bare negative ints don't.

## See also

- [RMW API Design](rmw.md) — deeper architectural rationale (heap,
  threading, dispatch model) shared across all the points above.
- [Custom RMW Backend](../porting/custom-rmw.md) — step-by-step guide
  to writing a backend in C against this vtable.
- [`<nros/rmw_vtable.h>` Doxygen](../api/rmw-cffi/index.html) — full
  C reference for every function pointer above.
- [`packages/zpico/nros-rmw-zenoh`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/zpico/nros-rmw-zenoh)
  — canonical reference port. Read the source for a worked example.

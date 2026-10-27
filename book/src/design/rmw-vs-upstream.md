# RMW API: Differences from upstream `rmw.h`

This page is for developers who already know upstream ROS 2's
[`rmw/rmw.h`](https://github.com/ros2/rmw) and want to write — or just
understand — a backend for nano-ros. Side-by-side: what `rmw.h` looks
like, what `nros-rmw-cffi` looks like, and why nano-ros diverges.

The C signatures shown for nano-ros come from
[`<nros/rmw_vtable.h>`](../api/rmw-cffi/index.html). The Rust trait
counterparts are an alternative entry point for porters who already
work in Rust; this page sticks to the C-vtable surface throughout.

> **Phase-301 rename.** The vtable historically diverged from rmw's
> vocabulary (`subscriber`, `service_server`, `service_client`,
> `open`/`close`). Phase-301 (issues 0240/0241) aligned it: the entity
> types are now `rmw_subscription_t` / `rmw_service_t` /
> `rmw_client_t`, the session slots are `create_session` /
> `destroy_session`, and the deprecated blocking `call_raw` slot is
> deleted (`send_request` + `take_response` is the one
> request/reply path). Transport hints (`tx_express`,
> `rx_buffer_hint`) moved out of `rmw_qos_profile_t` into per-create
> options structs. This page uses the new names throughout.

> **Looking for the exhaustive list?** This page is the rationale — the
> divergences worth understanding before writing a backend. The complete
> table, every upstream symbol with both signatures side by side and the
> reason for each difference, is
> [RMW API vs upstream — every symbol](../reference/rmw-api-comparison.md),
> and it is GENERATED, so it cannot drift from the headers the way prose can.

## TL;DR

| Concern | upstream `rmw.h` | `nros-rmw-cffi` |
|---------|------------------|------------------|
| Plugin loading | `dlopen("librmw_*.so")` at runtime | Single vtable registered at init |
| Init sequence | `rmw_init_options_t` → `rmw_context_t` → entities | One `create_session()` call, returns the session |
| Entity types | `rmw_publisher_t` / `rmw_subscription_t` / `rmw_service_t` / `rmw_client_t` | Typed-with-opaque-tail: `rmw_publisher_t` / `_subscription_t` / `_service_t` / `_client_t` (visible metadata + opaque `backend_data`) |
| Wait | `rmw_wait(waitset, timeout)` blocks the caller | `drive_io(session, timeout_ms)` drives I/O once |
| Serialization | typesupport-driven (rosidl) | Pre-serialized CDR bytes only |
| Graph queries | `rmw_get_topic_names_and_types`, … | None |
| QoS profiles | Full DDS profile match between endpoints | Same field set; per-backend support advertised; synchronous `IncompatibleQos` on create instead of runtime mismatch event |
| DDS events | `rmw_event_t` (`rmw_take_event`) | None |
| Loaned messages | Optional `rmw_borrow_loaned_message` | First-class `loan_publish` / `loan_recv` |
| Error returns | `rmw_ret_t` (`RMW_RET_OK`, …) | `rmw_ret_t` (`NROS_RMW_RET_OK`, …) — same named-constant style |

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

int main(int argc, char** argv) {
    nros_rmw_cffi_register_named("my_rmw", &MY_VTABLE);
    rclc_support_init(&support, argc, argv, &allocator);
}
```

**Asking which backend you are on.** Upstream answers
`rmw_get_implementation_identifier()` once per process. Here the answer
is per node, because an image may register several backends:
`Node::rmw_implementation_identifier()` (Rust),
`rclc_node_get_rmw_implementation_identifier(node)` (C) and
`rclcpp::Node::rmw_implementation_identifier()` (C++) read the
backend's `get_implementation_identifier` slot, or the name the vtable
was registered under when the slot is NULL. The answer is the nano-ros
backend name — `"zenoh"`, `"cyclonedds"`, `"xrce"`, `"uorb"` — never a
stock `rmw_zenoh_cpp`-style name: upstream's identifier is a
process-local tag that never crosses the wire, so a stock name would buy
no interop and would mislead code that branches on it.

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
rmw_session_t session = {0};
rmw_ret_t ret = vtable->create_session(locator, mode, domain_id, node_name, &session);
/* ... use &session to create_publisher / create_subscription / … */
vtable->destroy_session(&session);
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

**nano-ros.** Hybrid: typed-with-opaque-tail.
Each entity is a typed C struct exposing the metadata the runtime
actually reads (topic name, type name, QoS, lending capabilities)
inline; backend-private state stays behind an opaque `backend_data`
pointer.

```c
typedef struct rmw_publisher_t {
    const char *   topic_name;          /* borrowed; outlives the publisher */
    const char *   type_name;           /* borrowed */
    rmw_qos_profile_t qos;
    bool           can_loan_messages;   /* matches upstream's field of the same name */
    uint8_t        _reserved[7];        /* forward-compat; must be zero */
    void *         backend_data;        /* opaque */
} rmw_publisher_t;

rmw_ret_t (*create_publisher)(
    rmw_session_t * session,
    const char * topic_name, const char * type_name, const char * type_hash,
    uint32_t domain_id, const rmw_qos_profile_t * qos,
    const rmw_publisher_options_t * options,   /* transport hints; NULL = defaults */
    rmw_publisher_t * out);   /* runtime-allocated; backend fills */
```

Same shape for `rmw_subscription_t` (whose
`rmw_subscription_options_t` carries `rx_buffer_hint`, as the
publisher's carries `tx_express`). Service entities
(`rmw_service_t`, `rmw_client_t`) and
`rmw_session_t` have no `qos` and no `can_loan_messages` —
service-level QoS doesn't generalise across non-DDS backends
(see [QoS, Section 7](#7-qos-minimal-subset-not-full-dds-profiles))
and service request/reply uses the byte-buffer API rather than the
loan primitive.

**Forward-compat reserved bytes.** Each entity carries an explicit
`_reserved[N]` byte block (sized to fill the natural alignment slot
before `backend_data`). New fields up to N bytes can be added later
without changing struct size or the offset of any field after
`backend_data`. Backends and runtime keep these bytes zero.

**Storage ownership.** The runtime allocates the entity-struct shell;
the backend writes its `backend_data` into the runtime-supplied
out-parameter at `create_*` time (`can_loan_messages` is derived by the
runtime — see below). The backend never `malloc`s a
struct shell — embedded targets cannot afford a per-entity heap
allocation. `destroy_*` releases only the backend's `backend_data`;
the shell stays valid until the runtime drops its owner.

**Differences from upstream's `rmw_publisher_t`.**

- **Borrowed strings, not backend-owned copies.** Upstream's
  `topic_name` points to a backend-allocated string copied at
  `create_publisher` time. Ours points to caller (runtime) storage
  that outlives the publisher — no allocation per entity.
- **No `implementation_identifier` field.** Backend selection is
  compile-time (see [Section 1](#1-plugin-loading-vs-compile-time-backend));
  there's no plugin loader to dispatch through, so no need to
  identify which backend owns a struct.
- **`can_loan_messages` matches upstream in shape, but the runtime
  DERIVES it** (issue 0814). Same bool, same name, same meaning —
  `true` if the backend exposes the loan primitive (the CDR-byte
  zero-copy loan path) — except that a backend does not write it. Its
  value is exactly `vtable->borrow_loaned_message != NULL`, because that
  is the same fact stated twice, and the two spellings had drifted in
  both directions: nothing outside a test ever wrote `true` (so a zenoh
  build whose loans worked advertised that it could not loan), while a
  backend writing `true` with a NULL slot was believed. This paragraph
  used to claim the runtime "dispatches the publish path with no
  per-call branch" from the flag; it never did — dispatch reads the
  vtable slot on every call.
- **`depth: uint16_t`.** Upstream uses 32-bit; embedded queue depths
  are 1–100, the 16-bit width saves 2 bytes × N entities.
- **Explicit `_reserved[N]` bytes.** Upstream uses an embedded
  `rmw_publisher_options_t` struct as the extension point; we
  reserve raw bytes inline. Same forward-compat property — new
  fields up to N bytes don't break ABI — without the indirection.

**Why this shape.** Fully-opaque `void *` (the previous nano-ros
design) forced every introspection through a vtable callback.
Upstream's "expose every field, backend keeps them in sync" forces
duplicated state. The typed-with-opaque-tail middle ground exposes
exactly the fields the runtime reads — no callback indirection — and
keeps backend implementation state private. The struct layout is
ABI; adding or reordering fields is a major-version bump.

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
rmw_ret_t (*drive_io)(rmw_session_t * session, int32_t timeout_ms);
```

The backend dispatches whatever receive / send / wakeup work is
pending and returns within `timeout_ms`. There is no waitset, no
guard-condition aggregation, no implicit middleware thread.

### How the two models differ in practice

The two designs distribute work across different layers:

| Phase | Upstream `rmw_wait` | nano-ros `drive_io` |
|-------|---------------------|---------------------|
| **Build the wait set** | Executor rebuilds a waitset every `spin_once`, adds every entity | Executor registers entities once at construction; no per-spin rebuild |
| **Block** | `rmw_wait` blocks the thread on a kernel waitable (DDS WaitSet, condvar, kqueue) until any entity is ready or timeout | `drive_io` blocks (sleep-model backends) or polls (poll-model backends) for up to `timeout_ms` |
| **Signal readiness** | Wait primitive raises per-entity ready flags; backend writes flags into the waitset's status arrays | Backend's RX worker pulls bytes; the data is "ready" by virtue of being received |
| **Dispatch user callback** | Executor's `spin_once` picks one ready entity, calls `rmw_take`, fires its user callback | Backend's RX worker / drive_io loop fires user callbacks while it has work to do |
| **Per `spin_once`** | Exactly one user callback runs (single-threaded executor) | All ready callbacks run before drive_io returns |

The upstream model separates **wait** (kernel-blockable) from
**dispatch** (executor-controlled). nano-ros today fuses them — the
backend handles both inside one call.

### What this buys, what this costs

Fusing wins on:

- **No per-spin waitset rebuild.** Upstream's
  `add_handles_to_wait_set` allocates and walks every entity each
  iteration. On a 100-entity executor at 1 kHz that's 100 000
  per-second add/remove operations plus heap churn. nano-ros
  registers entities once; the backend tracks them statically.
- **No kernel-waitable per entity.** Upstream's per-entity ready
  flags need a per-entity wakeable resource (DDS Condition, eventfd,
  pipe). nano-ros's backend uses one wait primitive per session;
  per-entity tracking is in user-space backend state.
- **Backend RX worker fires callbacks directly.** zenoh-pico's
  `_z_session_read_task` invokes user callbacks during its read.
  nano-ros's `drive_io` drains it; upstream's
  `rmw_zenoh` would still have to round-trip through `rmw_take`.

Fusing costs on **scheduling control**. Upstream's "one callback per
`spin_once`" rule gives the executor an opportunity between every
two callbacks to:

- Re-check timer expirations
- Re-check guard conditions
- Yield to higher-priority work (multi-threaded executor)
- Apply per-callback priority ordering

nano-ros's `drive_io` runs all ready callbacks back-to-back, then
the spin loop processes timers + GCs *between* `drive_io` calls.
For a 100 ms `drive_io` call that fires 10 sub callbacks in 80 ms,
a timer that should have fired 5 ms in is delayed 75 ms.

### Where this fits each RTOS execution model

| Execution model | Fits drive_io today? |
|-----------------|----------------------|
| Cooperative single-task (one task does ROS, no priority competition) | Yes — no other task to preempt; entity scheduling fairness is moot |
| Async / tokio / Embassy (futures, wakers) | Yes — `spin_async` drives futures; `drive_io` not used in the hot path |
| Preemptive priority RTOS, ROS at one priority (FreeRTOS / ThreadX / Zephyr typical) | **Partial** — kernel preemption from higher-priority tasks works; ROS-internal entity scheduling is batch-FIFO, timer expiries can be delayed by long sub callbacks at the same priority |
| WCET-bounded real-time (RTIC, DO-178C) | **No** — `drive_io` has unbounded execution time; callers needing per-callback WCET use the async path with explicit Waker integration instead |
| Time-triggered cyclic | **No** — no way to bound `drive_io` to a fixed wall-clock budget |

The "Yes" rows are where nano-ros ships today. The "Partial" / "No"
rows are addressed by the work below.

### How RTOS cooperation will improve

Three forward-looking knobs land incrementally as the
`drive_io` interface is extended. None breaks the default behaviour;
each is opt-in for apps that need it.

1. **Backend-internal-deadline visibility.** `Session::next_deadline_ms()`
   tells the executor when the backend's next internal event
   (lease keepalive, heartbeat, ACK retransmit) is due. The executor
   caps `drive_io`'s timeout against it so the call doesn't return
   sooner than expected on otherwise-quiet links. Saves one round-trip
   per quiet period.

2. **Per-call user-callback cap.** `drive_io` accepts
   `max_callbacks`: an upper bound on user callbacks fired per call.
   Setting it to `1` reproduces upstream's "one callback per
   `spin_once`" pattern. The runtime spin loop calls `drive_io` again
   to drain pending work, with timer / GC checks between iterations.
   Closes the priority-inversion footgun for preemptive priority RTOS.

3. **Wall-clock budget per call.** `drive_io` accepts
   `time_budget_ms`: a wall-clock cap that bounds total time spent
   firing callbacks. Time-triggered cyclic apps configure a fixed
   slot per cycle; `drive_io` yields when the slot expires even if
   `max_callbacks` isn't reached.

Once the cap (knob 2) ships, an additional refinement moves timer
and guard-condition dispatch *into* the backend's `drive_io` loop so
the cap applies uniformly across all callback sources, not just
backend-RX-driven ones. This unifies the dispatch path and makes
`max_callbacks = 1` mean "exactly one callback per `spin_once`,
regardless of whether it's a sub, service, timer, or guard
condition."

For the per-RTOS-model recommendations (which knobs to set, which
defaults to use), see the [RTOS Cooperation](../concepts/rtos-cooperation.md)
concepts page.

**Why drive_io and not rmw_wait.** Cooperative single-task runtimes
(bare-metal, single-threaded RTOS) have one execution context. A
waitset abstraction with kernel-blockable per-entity resources
doesn't fit — there is no kernel to provide them. `drive_io` makes
the cooperative model explicit and lets backends pick the most
efficient wait primitive available on their target (kernel block on
multi-threaded; cooperative yield + WFI on bare-metal). The
scheduling-control trade-off the upstream waitset gives up is
recovered through the optional knobs above for apps that need it.

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
rmw_ret_t (*publish)(rmw_publisher_t * publisher,
                     const uint8_t * data, size_t len);

int32_t (*take)(rmw_subscription_t * subscription,
                uint8_t * buf, size_t len);
```

**Why.** rosidl typesupport is heavy: dynamic dispatch through a
function-pointer table per field type, dependence on dynamic
allocators for nested sequences, megabytes of generated symbol tables
linked even for one message. Splitting the layers means:

- Codegen (`nros_generate_interfaces()` from CMake) writes a fixed-shape C struct
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

## 7. QoS: full DDS-shaped profile, per-backend support advertised

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

The backend negotiates compatibility
(`rmw_qos_profile_check_compatible`) and surfaces mismatches as
runtime events.

**nano-ros.** Same field set, packed into 24 bytes:

```c
typedef struct rmw_qos_profile_t {
    uint8_t  reliability;
    uint8_t  durability;
    uint8_t  history;
    uint8_t  liveliness_kind;
    uint16_t depth;
    uint16_t _reserved0;
    uint32_t deadline_ms;             /* 0 = infinite */
    uint32_t lifespan_ms;             /* 0 = infinite */
    uint32_t liveliness_lease_ms;     /* 0 = infinite */
    uint8_t  avoid_ros_namespace_conventions;
    uint8_t  _reserved1[3];
} rmw_qos_profile_t;
```

Standard profile constants
(`NROS_RMW_QOS_PROFILE_DEFAULT`, `_SENSOR_DATA`,
`_SERVICES_DEFAULT`, `_SYSTEM_DEFAULT`, `_PARAMETERS`) match
upstream `rmw_qos_profile_*` field-for-field, so applications
porting from rclcpp / rclrs can pull the equivalent profile
constant unchanged.

`_SYSTEM_DEFAULT` is the one where "field-for-field" is worth
spelling out: like upstream's, it names **no concrete policy at
all**. Every field is the sentinel — `NROS_RMW_{RELIABILITY,
DURABILITY,HISTORY,LIVELINESS}_SYSTEM_DEFAULT` are all `0`, and
the depth sentinel is `0` too, matching
`RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT`. The backend resolves it at
entity-create time, and **the answer differs per backend**, which
is the whole point of the name: cyclonedds resolves to
RELIABLE / VOLATILE / KEEP_LAST(1), mirroring what
`rmw_cyclonedds_cpp` does with the same sentinel; zenoh resolves
the depth to the subscriber ring the shim actually enforces; xrce
resolves the three policies the same way and leaves the depth
absent from the wire for the Agent to fill. Upstream's own two
reference RMWs disagree here as well — `rmw_cyclonedds_cpp` picks
depth 1, `rmw_zenoh_cpp` picks 42 — so a profile constant that
baked a number would be wrong for every backend but one. Ask for
`_DEFAULT` if you want a stated reliable / volatile /
keep-last(10) profile. (issue 0829)

### Per-backend support, no silent downgrade

Each backend advertises which policies it can honour via
`Session::supported_qos_policies()`, returning a `QoSPolicyMask`
bitfield. Policies a backend can't enforce are explicit.

```rust
pub trait Session {
    fn supported_qos_policies(&self) -> QoSPolicyMask {
        QoSPolicyMask::NONE     // "I have not said", which is not "I honour the basics"
    }
}
```

The default is EMPTY, and that is the whole point of the mechanism
(phase-428 W9). It was `QoSPolicyMask::CORE` — four policies granted
by the trait to any implementation that had written no code at all —
until measurement found the zenoh shim honouring none of the four. A
bit is now *earned*: `check-qos-mask-derivation` requires each one to
be backed by a claim sited on the code that reads the mapped profile
field and either applies the request or refuses a value it cannot
serve.

A backend reached through the C ABI answers the same question through
`nros_rmw_vtable_t::supported_qos_policies`, per backend (issue 1329).
The cffi route used to answer the UNION of what any nano-ros-supported
RMW honours, which over-claims for each of them; a NULL slot there
means the backend has not said, and carries the same meaning as the
trait default above.

The runtime validates the requested QoS against the backend's mask
at entity-create time. Requesting a policy the backend doesn't
support returns `TransportError::IncompatibleQos`
(`NROS_RMW_RET_INCOMPATIBLE_QOS` at the C boundary) **synchronously**.
There is no silent degradation — applications either get the
requested QoS or a hard error.

Apps that need cross-backend portability check the mask at startup:

```rust
if session.supported_qos_policies()
    .contains(QoSPolicyMask::DEADLINE)
{
    pub.create_with_qos(...deadline_ms = 100, ...);
} else {
    // app-side fallback: timeout monitoring in user code
}
```

### Manual liveliness assertion

For `LIVELINESS_MANUAL_BY_TOPIC` and `LIVELINESS_MANUAL_BY_NODE`,
publishers call `assert_liveliness()` explicitly:

```rust
pub.assert_liveliness()?;   // refresh this publisher's lease
```

C side: `rcl_publisher_assert_liveliness(&pub)`. C++ side:
`pub.assert_liveliness()`. Nothing to do for `AUTOMATIC` and `NONE` kinds,
which answer `Ok` on every backend.

Under a manual kind the answer is per backend, and `Ok` means the assertion
reached the wire: cyclonedds performs it (`dds_assert_liveliness`), while
zenoh-pico, XRCE-DDS and uORB have no per-topic lease and answer
`UNSUPPORTED` (phase-467 RMW gap-closure study, Row 7).

### Differences from upstream's matching

Upstream surfaces QoS mismatches via runtime events
(`RMW_EVENT_REQUESTED_INCOMPATIBLE_QOS`). nano-ros surfaces them
synchronously at create time as `IncompatibleQos`. The mismatch
is a configuration error visible at startup; the runtime path
doesn't need to handle it.

Two related choices:

- **No profile matching between publisher and subscriber.** Each
  endpoint requests the QoS it wants from its backend; the backend
  enforces locally. Cross-endpoint compatibility is the wire
  protocol's concern — DDS endpoints negotiate via DDS Discovery,
  zenoh endpoints communicate intent via the topic-key encoding,
  uORB endpoints share an in-process queue. nano-ros's executor
  doesn't run a profile-matching pass.
- **Wire metadata per backend.** Lifespan needs per-sample
  timestamps; liveliness needs a keepalive mechanism. Each backend
  uses its native attachment / sample-info mechanism (Zenoh
  attachments, DDS RTPS sample-info, XRCE session pings) — no
  cross-backend metadata header.

### Per-backend QoS coverage

The mask actually advertised by each backend's
`Session::supported_qos_policies()`:

| Backend | Reliability + Durability + History/Depth | Deadline | Lifespan | Liveliness Automatic | Liveliness Manual | Liveliness Lease | `avoid_ros_namespace_conventions` |
|---------|-------------------------------------------|----------|----------|----------------------|--------------------|------------------|-----------------------------------|
| Cyclone DDS | ✅ Native | ✅ Native | ✅ Native | ✅ Native | ✅ Native | ✅ Native | ✅ honoured |
| XRCE-DDS | ✅ Native (binary `uxrQoS_t`) | ✅ Shim-side clock check (sub: `RequestedDeadlineMissed`; pub: `OfferedDeadlineMissed`) + agent-side via FastDDS XML profile | ✅ Agent-side via FastDDS XML profile | ✅ Native | ✅ Configured via XML | ✅ Configured via XML | ✅ honoured |
| zenoh-pico | ✅ Shim-emulated | ✅ Clock-based check (sub + pub) | ✅ Subscriber-side filter using attachment timestamp | ✅ Trivial via session keepalive | ✅ Shim-side keepalive timer at pub publish path | ✅ Honoured | n/a (no `/rt/` prefix) |
| uORB | ✅ CORE only (intra-process, no wire) | ❌ No rate concept | ❌ No expiry concept | ❌ No wire-level liveliness | ❌ | ❌ | n/a |

**Key takeaways**:

- The default QoS profile (RELIABLE + VOLATILE + KEEP_LAST(10) + AUTOMATIC) works on every backend.
- Apps that need extended QoS but want to stay backend-portable: check `supported_qos_policies()` at startup and degrade gracefully.
- For full DDS QoS: Cyclone DDS is the native DDS path; XRCE-DDS
  auto-routes through FastDDS XML profile when extended policies are
  set. Zenoh-pico fills the gap with shim-side emulation. uORB is
  intra-process only.

See [Status events](../concepts/status-events.md) for how the
deadline / liveliness / message-lost policies translate into
runtime events, and [User guide → Configuration](../user-guide/configuration.md)
for code examples.

## 8. Status events: callback-on-entity, Tier-1 subset

**Upstream.** Event APIs surface DDS-shaped notifications via a
waitset-take pattern:

```c
rmw_ret_t rmw_subscription_event_init(
  rmw_event_t * event, const rmw_subscription_t * sub,
  rmw_event_type_t type);

/* Add event_handle to a waitset alongside subscriptions. */
rmw_ret_t rmw_wait(...);

/* Poll fired events. */
rmw_ret_t rmw_take_event(
  const rmw_event_t * event_handle,
  void * event_info, bool * taken);
```

Eleven event types covering liveliness, deadline, QoS-incompatibility,
match, message-lost, type-incompatibility — all dispatched through
the waitset.

**nano-ros.** Callback-on-entity for a Tier-1 subset (liveliness
changes, deadline misses, message lost) plus Iron's matched and
incompatible-type kinds (issue 1972). Skips the waitset. Skips
`QOS_INCOMPATIBLE` — see "What's skipped" below.

```rust
sub.on_liveliness_changed(|status| {
    if status.alive_count == 0 { trigger_failover(); }
})?;

sub.on_requested_deadline_missed(
    Duration::from_millis(15),
    |status| metric_inc(&LATE_SAMPLE_COUNT, status.total_count_change),
)?;

sub.on_message_lost(|status| log::warn!("dropped {}", status.total_count_change))?;
```

C side mirrors with `nros_subscription_set_*_callback` functions.

### What lands

| Event | Producer | Subscriber callback / Publisher callback |
|-------|----------|------------------------------------------|
| Liveliness changed | sub | `on_liveliness_changed(LivelinessChangedStatus)` |
| Liveliness lost | pub | `on_liveliness_lost(DeadlineMissedStatus)` |
| Requested deadline missed | sub | `on_requested_deadline_missed(deadline, DeadlineMissedStatus)` |
| Offered deadline missed | pub | `on_offered_deadline_missed(deadline, DeadlineMissedStatus)` |
| Message lost | sub | `on_message_lost(MessageLostStatus)` |
| Publication / subscription matched (Iron+) | pub / sub | `on_matched(MatchedStatus)` |
| Publisher / subscription incompatible type (Iron+) | pub / sub | `on_incompatible_type(IncompatibleTypeStatus)` |

The matched pair and the incompatible-type pair are values 5..8 of
`rmw_event_type_t`, APPENDED: upstream inserted them mid-enum, and ours was
never value-compatible with upstream's, so appending keeps every existing
value. The payloads are upstream's `rmw_matched_status_t` (four counts, in
upstream's order) and `rmw_incompatible_type_status_t`, at fixed `uint32_t`
widths.

### What's skipped

- **`QOS_INCOMPATIBLE`** — surfaces at create time, not as a runtime
  event: `NROS_RMW_RET_INCOMPATIBLE_QOS` carries the diagnostic
  synchronously from `create_publisher` / `create_subscription`.

`MATCHED` was skipped here until issue 1972 ruled it ADOPT: "start
publishing once somebody listens" is the event an embedded node most often
wants, and without it the node polls the matched count. `INCOMPATIBLE_TYPE`
came with it — the same graph machinery, and a peer with another type on
the same name is a runtime condition no create-time code can report.

### Dispatch — callback-on-entity, not waitset-take

Events fire from inside the existing `drive_io` callback-dispatch
path. The backend's RX worker detects an event in the same place it
detects messages; runs the registered callback; loops. No separate
waitset, no per-call take.

This reuses the message-callback dispatch model; events count
against the `max_callbacks` cap from Section 4 the same way message
callbacks do.

**Why callback-on-entity instead of waitset-take.** The waitset-take
pattern requires a waitset abstraction nano-ros deliberately doesn't
have (see Section 4). Replacing it with per-entity callbacks reuses
existing machinery, matches the message-callback ergonomics users
already know, and keeps the bounded-storage property — each
registered event-callback is a fixed-size struct embedded in the
entity, no per-call allocation.

The trade-off: users can't bulk-poll all events at once. For the
Tier-1 events this isn't load-bearing — events are rare, callbacks
are cheap.

### Backend coverage

Coverage is uneven and surfaces through `Subscription::supports_event`
(Rust) / `register_*_event` returning `NROS_RMW_RET_UNSUPPORTED`
(C). Apps must handle "not supported" — not every backend will
generate every event:

| Backend | Liveliness | Deadline | Message lost |
|---------|-----------|----------|--------------|
| Cyclone DDS | ✅ Polled: `dds_get_liveliness_changed_status` (sub) / `_liveliness_lost_status` (pub), drained by the runtime from the entity's data path | ✅ Polled: `dds_get_requested_deadline_missed_status` / `_offered_deadline_missed_status`. The deadline is the entity's QoS `deadline_ms`; the `on_*` argument is not forwarded on this path | ✅ Polled: `dds_get_sample_lost_status` |
| XRCE-DDS | ❌ XRCE protocol carries no session→client liveliness callback | ❌ No deadline tracking in the C backend (no `*_event_init` / `*_take_event` fill); a `DEADLINE` QoS is refused at create, since `uxrQoS_t` has no field for it | ❌ `topic_callback` carries no per-sample sequence |
| zenoh-pico | ✅ Sub: per-publisher count via `zpico_liveliness_get_count` + wildcard keyexpr query; pub: shim-side keepalive timer fires `LivelinessLost` from `publish` when `MANUAL_BY_*` lease expires. | ✅ Clock-based check at sub + pub, rate-limited to ≤ 1 fire per deadline period | ✅ Sequence-gap detection from RMW attachment |
| uORB | ❌ No wire-level liveliness | ❌ No rate concept | ❌ Not wired: the C++ backend fills no event slot (the retired Rust backend's publish-counter delta did not carry over) |

Matched / incompatible type (issue 1972):

| Backend | Matched | Incompatible type |
|---------|---------|-------------------|
| Cyclone DDS | ✅ Polled: `dds_get_{publication,subscription}_matched_status` — QoS-compatible endpoints only, as upstream counts them. The change fields are computed against what the event last reported, because other readers of the same status reset Cyclone's own change counters | ✅ Polled: endpoints on the same DDS topic name with another type name, read from the builtin `DCPSPublication` / `DCPSSubscription` topics. NOT `DDS_INCONSISTENT_TOPIC_STATUS` (what `rmw_cyclonedds_cpp` maps it to): the pinned 0.10.5 never raises it |
| zenoh-pico | ✅ From the session's graph cache: liveliness tokens of the opposite kind with the same topic name and type, as `rmw_zenoh_cpp` derives it. Snapshot-based, so `total_count` counts increases seen between polls; QoS is not consulted | ✅ Same walk: same name, another type |
| XRCE-DDS | ❌ No event slot | ❌ |
| uORB | ❌ No event slot | ❌ |

Both producers are POLLED from the entity's data path, so a publisher
observes a new match on its next `publish` or `assert_liveliness` — the
latter is the poll point for a publisher that waits for a listener before
publishing.

`assert_liveliness()` (manual): **cyclonedds** performs a real assertion
(`dds_assert_liveliness`). **zenoh-pico, XRCE-DDS and uORB** have no
per-topic lease, so under `MANUAL_BY_TOPIC` / `MANUAL_BY_NODE` they answer
`UNSUPPORTED`; under `AUTOMATIC` / `NONE` they answer `Ok(())`, because
there was nothing to assert. This said the unsupported default was `Ok(())`
whatever the kind, which reported an assertion that never happened
(phase-467 RMW gap-closure study, Row 7). zenoh's shim-side watchdog is
unchanged and still fires `LivelinessLost` on expiry — it is LOCAL, and the
return value is about what a peer can see.

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

**nano-ros.** Lending is a separate vtable surface. When the backend
supports zero-copy publish, it implements `loan_publish_*` /
`commit_publish_*`; when it supports zero-copy receive, it implements
`loan_recv_*` / `release_recv_*`. The runtime checks the function
pointers for non-NULL once at session open and takes the lending
path for the lifetime of the session.

**Why.** Zero-copy isn't optional on embedded — every avoidable copy
is a copy of a CDR-encoded sensor frame from a 64 KB heap. Promoting
the lending surface to first-class makes it visible at compile time
(missing pointer = compile error against the lending build) and lets
the runtime pre-arrange arena slots once instead of probing on every
publish.

## 9b. XRCE-DDS: what each NULL slot means

The XRCE backend fills fewer than half of the vtable slots (`just check rmw-slot-producers` prints the count).
A NULL slot answers `NROS_RMW_RET_UNSUPPORTED` (or its documented fallback), so
every gap below is a declared "cannot tell you", never a wrong answer. They
fall into two kinds, and the difference decides whether a gap is permanent:

* **Protocol cannot** — micro-XRCE-DDS-Client cannot learn the answer. The
  Agent owns the DDS participant and every entity on it; the client gets back
  per-request status codes (`uxrOnStatusFunc`) and the payloads it asked for
  (`uxrOnTopicFunc`, `uxrOnRequestFunc`, `uxrOnReplyFunc`), and nothing else —
  no discovery state, no builtin topics, no DDS status, no `SampleInfo` on a
  topic sample.
* **Unwritten** — the client has, or can get, what the slot asks for, and
  nobody has written the body. Each has an issue.

What stock [`rmw_microxrcedds`](https://github.com/micro-ROS/rmw_microxrcedds)
(Humble) does is in the last column; it is the same protocol, so its choices
are evidence for what the protocol allows.

| family | slots | ruling | why | stock `rmw_microxrcedds` |
| --- | --- | --- | --- | --- |
| graph: names, types, endpoint info, counts | `get_topic_names_and_types`, `get_service_names_and_types`, the four `*_names_and_types_by_node`, `get_{publishers,subscriptions}_info_by_topic`, `count_{publishers,subscribers,clients,services}` | protocol cannot | discovery lives on the Agent's participant; the client has no builtin-topic readers and no enumeration request | UNSUPPORTED unless built with `RMW_UXRCE_GRAPH`, which reads the micro-ROS Agent's own `ros_to_microros_graph` topic — not published by the stock Micro XRCE-DDS Agent nano-ros uses |
| graph: node names + change edge | `get_node_names`, `node_get_graph_guard_condition` | unwritten — [issue 2106](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/2106-xrce-node-enumeration-from-ros-discovery-info.md) | node names are DATA on `ros_discovery_info`, an ordinary topic the client can subscribe to through the Agent (this backend already publishes it, issue 1292); DDS peers only | as above |
| matched counts | `publisher_count_matched_subscriptions`, `subscription_count_matched_publishers` | protocol cannot | matching happens between the Agent's entities and remote ones; nothing reports it to the client | `RMW_UXRCE_GRAPH` only, from the graph topic |
| granted QoS | the six `*_get_actual_qos` | unwritten — [issue 2107](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/2107-xrce-get-actual-qos.md) | no read-back exists, but every honoured policy is sent explicitly at create and an unhonoured one is refused, so the created profile (with `*_UNKNOWN` where unsure) is an honest partial answer | returns the stored creation profile |
| status events: liveliness, incompatible QoS, matched; manual liveliness | `subscription_event_init`, `publisher_event_init`, `subscription_take_event`, `publisher_take_event`, `publisher_assert_liveliness` | protocol cannot | DDS status is on the Agent's entities; no XRCE submessage carries it; the Agent's writer asserts its own liveliness | `rmw_*_event_init` UNSUPPORTED |
| status events: deadline | the same slots for the two DEADLINE kinds, plus `next_deadline_ms`, `event_type_is_supported` | unwritten — [issue 2108](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/2108-xrce-deadline-event-emulation.md) | both deadlines are local facts (time since last arrival / last publish); emulating them is also what would earn the DEADLINE QoS bit XRCE refuses today | UNSUPPORTED |
| session wake | `set_wake_callback` | protocol cannot | the client has no background thread; nothing arrives outside `drive_io`, which the executor itself calls | no equivalent |
| per-entity new-data callbacks | `subscription_set_on_new_message_callback`, `service_set_on_new_request_callback`, `client_set_on_new_response_callback` | unwritten — [issue 2109](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/2109-xrce-per-entity-new-data-callbacks.md) | every arrival callback names its entity (`uxrObjectId`), so it can fire the registered callback from inside `drive_io` | UNSUPPORTED |
| take with info | `take_with_info` | protocol cannot | `uxrOnTopicFunc` carries no writer GUID and no source timestamp; the only fields left are local stamps, not worth a slot | ignores `message_info` |
| request info | `take_request_with_info` | **filled** | `uxrOnRequestFunc` DOES carry the request's `SampleIdentity` (issue 2081, measured live in issue 2088) | fills the request id |
| identity | `get_gid_for_publisher`, `get_gid_for_client` | unwritten — [issue 2110](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/2110-xrce-gid-for-publisher-and-client.md) | the protocol hands back no GUID, but `src/graph.c` already derives each tracked endpoint's DDS GID (probe + Fast-DDS entity counter, issue 1292) | copies the XRCE object id into the gid (matches no DDS GID) |
| server availability | `service_server_is_available` | protocol cannot | needs matched-replier state the Agent keeps; a `create_requester` ack says only that the client's own endpoints exist (see `vtable.c`) | UNSUPPORTED |
| loans | `borrow_loaned_message`, `publish_loaned_message`, `return_loaned_message_from_publisher`, `take_loaned_message`, `return_loaned_message_from_subscription`, `take_loaned_message_with_info` | protocol cannot (in the loan's shape) | a stream buffer is reused by the next `uxr_run_session_*`, so it cannot back a view that outlives the call; the zero-copy capability ships as `publish_streamed` and `process_raw_in_place` instead | UNSUPPORTED |
| batch take | `take_sequence` | not needed | one sample per arrival into a single-slot inbox; the runtime's `take` loop is the same work | loops `take_with_info` |
| acks | `publisher_wait_for_all_acked` | protocol cannot | reliable-stream acks are client↔Agent only (`uxr_run_session_until_confirm_delivery`), not from DDS readers; also inert ABI-wide | UNSUPPORTED |
| log level | `set_log_severity` | protocol cannot | the client library has no runtime log control | UNSUPPORTED |
| capability probe | `feature_supported` | n/a | inert ABI-wide (no reader); XRCE would answer false for both features, as upstream does | false for both |

Nothing on this list was trivial enough to land with the rulings: the closest,
`*_get_actual_qos`, needs a measurement first (whether an Agent-side profile
can override a BIN-created entity's QoS), which is in issue 2107.

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
typedef int32_t rmw_ret_t;
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
| `rmw_ret_t` + entity-struct out-param (`create_session`, `create_publisher`, `create_subscription`, …) | `NROS_RMW_RET_OK`, `out->backend_data` non-NULL | negative named constant |
| `rmw_ret_t` (`destroy_session`, `drive_io`, `publish`, `send_response`, …) | `NROS_RMW_RET_OK` | negative named constant |
| `int32_t` byte count (`take`, `take_request`, `take_response`) | `>= 0` (bytes received) | negative `rmw_ret_t` |

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
  set started from upstream's and dropped codes that don't apply
  (e.g., DDS event codes, `RMW_RET_NODE_INVALID`). Adding a code is
  a `<nros/rmw_ret.h>` header change only.

**Why same style.** Named constants make `switch` statements
possible at call sites; bare negative ints don't.

## Publisher GID, message-info & service info: functional-parity, different shape (issue 0242)

Three upstream `rmw.h` capabilities exist in nano-ros in a **different
shape** — the functionality is present, but the upstream-exact C entry
points are deliberately not mirrored. Recorded here so the divergence
reads as a decision, and updated (2026-07-27) to describe what nano-ros
actually provides today.

### Message-info: surfaced via a builder, not a `take_with_info` slot

Upstream's `rmw_take_with_info` fills an `rmw_message_info_t` (source +
received timestamp, publisher GID, publication + reception sequence
numbers) alongside the payload.

nano-ros provides the **same information** — `MessageInfo` carries all five
fields — but through a different seam:

- **Application API (present).** A subscription opts into message-info
  with the `message_info()` builder:
  ```rust
  node.subscription::<Msg>("/topic")
      .message_info()
      .build(|msg, info: Option<&MessageInfo>| {
          if let Some(i) = info {
              log::info!("from {:?} at {:?}", i.publisher_gid(), i.source_timestamp());
          }
      })?;
  ```
  C mirrors this with `rclc_executor_add_subscription_with_info`
  (the callback receives the payload plus the attachment/metadata
  arguments directly — there is no info struct on the C side), which
  registers into a `rcl_subscription_t` that
  `rclc_executor_remove_subscription` takes back out (issue 1668).
- **Two backend channels.** The hot `take` vtable slot returns bytes
  only. A backend that fills the optional `take_with_info` slot reports
  `rmw_message_info_t` in caller-owned storage, and the runtime dispatches
  it whenever it is present — that is how a C/C++ backend (Cyclone)
  reports metadata (issue 1495). A Rust backend behind the adapter writes a
  `MessageInfoSlot` side-channel instead, which the runtime pairs with the
  payload. Either way the `message_info()` builder sees the same
  `MessageInfo`.

### Publisher identity: one GID, from one source

Upstream exposes `rmw_get_gid_for_publisher(pub, out_gid)` and stamps
every sample with the writer's GID, so a subscriber can name the
publisher that sent a message. nano-ros does both, and — since issue
1495 — the two answers are the same bytes:

| backend | `get_gid()` and `MessageInfo::publisher_gid` |
| --- | --- |
| zenoh-pico | XXH3-128 of the entity's liveliness keyexpr, 16 bytes zero-extended to 24 — `rmw_zenoh_cpp`'s own derivation, so a stock peer's `ros2 topic info --verbose` prints the same GID |
| cyclonedds | the DDS writer GUID, 16 bytes zero-extended; the take path reports it through the `take_with_info` slot |
| XRCE-DDS, uORB | `UNSUPPORTED`; no metadata on a take |

The field is **24 bytes**, upstream's `RMW_GID_STORAGE_SIZE`. The 16 is
zenoh's *wire* width and stays there: `rmw_zenoh_cpp`'s attachment
reader rejects any other length.

One upstream divergence worth knowing when comparing against a stock
peer: a Humble `rmw_cyclonedds_cpp` subscriber puts the 8-byte
publication *handle* in `message_info.publisher_gid`, not the writer
GUID it reports everywhere else, so its take-side gid matches no gid
query. Ours reports the GUID.

### Service request info: a sibling slot, server side only (issue 2081)

Upstream's `rmw_take_request` fills an `rmw_service_info_t` beside the
request: the client's `request_id` (writer GUID + the sequence number that
client assigned) and the `source_timestamp` / `received_timestamp` pair.

nano-ros splits that take across two vtable slots:

- **`take_request`** returns the payload bytes and `int64_t *seq_out`, the
  backend's reply-correlation token that `send_response` takes back. That is
  everything a server needs to answer, and it is all this slot returns.
- **`take_request_with_info`** (optional, appended) is the same take plus a
  caller-owned `rmw_service_info_t *`, declared in `<nros/rmw_entity.h>`
  field for field as upstream's. `seq_out` stays beside the struct instead
  of becoming its `request_id.sequence_number`: a backend's token (a
  reply-slot index on Cyclone, XRCE and zenoh) is not the client's sequence
  number, and the runtime needs both. When a backend fills this slot the
  runtime takes every request through it; when it does not, the runtime
  falls back to `take_request`, and only a Rust backend's side table can
  then report anything.

A field the backend cannot report is **zero** (all-zero `writer_guid` = no
request id, timestamp 0 = not stamped), and the runtime turns each zero into
`None`, never into a value:

| backend | channel | `request_id` | `source_timestamp` | `received_timestamp` |
| --- | --- | --- | --- | --- |
| zenoh-pico | Rust adapter side table (issue 1942) | from the query attachment (`rmw_zenoh_cpp` layout) | from the attachment | not reported |
| Cyclone DDS | `take_request_with_info` | request-header GUID + publication handle, header sequence (stock `rmw_cyclonedds_cpp` layout) | the DDS sample's source timestamp | not reported |
| XRCE-DDS | `take_request_with_info` | the Agent's `SampleIdentity` (client request-writer GUID + sequence), as `rmw_microxrcedds` fills it | not reported — no timestamp reaches the client library | not reported |
| uORB | — | uORB has no services | — | — |

**No backend reports `received_timestamp`.** The field exists for upstream
parity and is always zero here.

What an application sees:

- **Rust.** `create_service_with_info` (or the builder's
  `.service_info()`) hands the callback a `nros::ServiceInfo` — rclrs's
  shape, except every field is an `Option`:
  ```rust
  executor.node_mut(nid).create_service_with_info::<AddTwoInts, _>(
      "/add_two_ints",
      |req: &AddTwoIntsRequest, info: &nros::ServiceInfo| {
          if let Some(id) = info.request_id {
              log::info!("request #{} from {:?}", id.sequence_number, id.writer_guid);
          }
          AddTwoIntsResponse { sum: req.a + req.b }
      },
  )?;
  ```
- **C.** `rclc_executor_add_service_with_request_id` (typed) and
  `rclc_executor_add_service_raw_with_request_id` (CDR bytes) pass the
  callback an `rmw_request_id_t *`, rclc's shape. The timestamps are not
  surfaced in C.
- **C++.** An rclcpp service handler taking three arguments —
  `(std::shared_ptr<rmw_request_id_t>, request, response)` — receives the
  request header; the two-argument handler does not. As in C, the
  timestamps are not surfaced.

**The client side gets none of this.** `take_response` returns the reply
bytes and the `sequence_id` that `send_request` handed out, which is what the
runtime matches a reply against (issue 0778). There is no
`take_response_with_info` slot, no backend reports the reply's timestamps or
the server's identity, and no client API surfaces them — upstream
`rmw_take_response`'s `request_header` has no counterpart.

## See also

- [RMW API Design](rmw.md) — deeper architectural rationale (heap,
  threading, dispatch model) shared across all the points above.
- [Custom RMW Backend](../porting/custom-rmw.md) — step-by-step guide
  to writing a backend in C against this vtable.
- [`<nros/rmw_vtable.h>` Doxygen](../api/rmw-cffi/index.html) — full
  C reference for every function pointer above.
- [`packages/rmw/zenoh/nros-rmw-zenoh`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/rmw/zenoh/nros-rmw-zenoh)
  — canonical reference port. Read the source for a worked example.

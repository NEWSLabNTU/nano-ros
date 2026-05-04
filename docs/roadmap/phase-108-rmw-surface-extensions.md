# Phase 108 — RMW Surface Extensions (Status Events + Full QoS)

**Goal:** Two API + FFI surface additions to the RMW layer, both ship as API-only first (no backend wiring), with backends opting in per-policy in follow-up phases.

- **108.A — Status events** — trait + C-vtable + user-facing surface for transport-level status events (liveliness changes, deadline misses, message loss). Adopts callback-on-entity dispatch (matches existing message-callback path) instead of upstream's `rmw_event_t + rmw_take_event + waitset` machinery.
- **108.B — Full DDS-shaped QoS profile** — extends `nros_rmw_qos_t` to carry deadline, lifespan, liveliness, and the namespace-convention flag in addition to the existing reliability / durability / history / depth subset. Backends advertise per-policy support via a bitmask; unsupported policies return `NROS_RMW_RET_INCOMPATIBLE_QOS` synchronously (no silent degradation).

Both bundled because they share the `nros_rmw_qos_t` / `nros_rmw_event_t` C header, both ship API-only, and Phase 108.A's deadline/liveliness events depend on Phase 108.B's QoS fields to be meaningful.

**Status:** v1 surface complete (108.A + 108.B). dust-DDS fully wired (QoS + assert_liveliness + sub/pub events). Lightweight follow-ups landed for XRCE-DDS / zenoh-pico / uORB (CORE QoS + AVOID_ROS_NAMESPACE_CONVENTIONS for XRCE). Heavy follow-ups (XRCE listener events, zenoh shim emulation, uORB MessageLost via px4-uorb extension, E2E test matrix) deferred — see § 108.C below.

**Priority:** Medium — surfaces let users start writing code; backend wiring follows in per-backend sub-phases below.

**Depends on:** Phase 102 (typed entity structs, `nros_rmw_ret_t`), Phase 110 (Activator + ReadySet — events count as ready callbacks under `DrainMode::Latched` and against the dispatch loop's count cap; `OptUs` newtype + sentinel-`0` ABI convention reused for time fields).

---

## Background

### Event tier vocabulary

| Tier | Events | Why grouped |
|------|--------|-------------|
| **Tier-1 (this phase)** | LivelinessChanged, RequestedDeadlineMissed, MessageLost, LivelinessLost, OfferedDeadlineMissed | Steady-state runtime events that fire repeatedly during normal operation; drive RTOS-side fail-over / alarm / drop logic. |
| **Tier-2 (deferred)** | Matched, QosIncompatible, IncompatibleType | Discovery-time events that fire once at startup in static-topology embedded apps. Surface via existing `nros_rmw_ret_t` codes at create-time instead. Re-evaluate if dynamic-discovery apps appear. |
| **Tier-3 (deferred)** | SampleRejected, RequestedIncompatibleQos (DDS-spec extras) | DDS-spec events with no clear RTOS use case. Skip indefinitely. |

Tier-1 use cases:

| Event                                     | Use case                                                                                                                |
|-------------------------------------------|-------------------------------------------------------------------------------------------------------------------------|
| **Liveliness changed (sub) / lost (pub)** | Safety-island fail-over: when a remote control node goes silent, trigger MRM. Drone bridge: detect PX4 commander stall. |
| **Deadline missed (sub / pub)**           | Periodic-pubsub safety: 100 Hz sensor topic; if a sample doesn't arrive within deadline, alarm or fail-over.            |
| **Message lost (sub)**                    | Slow-consumer diagnostic: ring buffer overflow signals the app to drop / coalesce / log.                                |

### 108.A — Why callbacks, not waitset-take

Upstream uses `rmw_event_t` handles in a waitset; `rmw_wait` returns when an event fires; `rmw_take_event` pulls the payload. Two-phase, per-call. Adopting it would require a waitset abstraction we deliberately don't have. Replace with **callback-on-entity** — backend's RX worker detects event, runs registered callback inline. Reuses existing `drive_io` callback dispatch path; events count as ready callbacks in Phase 110's `ReadySet` (one ready bit per event-callback subscription, drained alongside message callbacks); matches message-callback ergonomics.

### Why no `Box<dyn FnMut>` callbacks

nano-ros is no_std + heapless across all backends. Phase 110 explicitly forbids alloc-style indirection at the executor surface. Phase 108 follows suit: callbacks are raw function pointers + user-context `void*`, identical between Rust and C. Rust generic helpers wrap closures into static trampolines at call sites (same pattern as today's `add_subscription` for typed messages).

### 108.B — Why full QoS now

Today's `nros_rmw_qos_t` is a deliberate subset of `rmw_qos_profile_t`:

```c
typedef struct nros_rmw_qos_t {
    uint8_t  reliability;
    uint8_t  durability;
    uint8_t  history;
    uint8_t  _reserved0;
    uint16_t depth;
    uint16_t _reserved1;
} nros_rmw_qos_t;            // 8 bytes
```

Two factors changed the calculus:

1. **`rmw_zenoh_cpp` proved every DDS policy is implementable on non-DDS backends.** Bounded cost per policy.
2. **Real RTOS use cases need them.** Drone bridge needs liveliness for fail-over. 100 Hz sensor safety apps need deadline. Slow-consumer apps need lifespan + message-lost (108.A covers event side; need QoS side too).

So the "subset" framing is no longer load-bearing. Surface the full shape; let backends opt in per-policy.

---

## Design

### 108.A — Status events trait surface

```rust
// packages/core/nros-rmw/src/event.rs (new)

#[non_exhaustive]
#[repr(u8)]
pub enum EventKind {
    LivelinessChanged       = 0,  // subscriber
    RequestedDeadlineMissed = 1,  // subscriber
    MessageLost             = 2,  // subscriber
    LivelinessLost          = 3,  // publisher
    OfferedDeadlineMissed   = 4,  // publisher
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct LivelinessChangedStatus {
    pub alive_count: u16,
    pub not_alive_count: u16,
    pub alive_count_change: i16,
    pub not_alive_count_change: i16,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CountStatus {
    pub total_count: u32,
    pub total_count_change: u32,
}
// Used for RequestedDeadlineMissed, MessageLost, LivelinessLost,
// OfferedDeadlineMissed — same shape.

// EventPayload is laid out as a tagged union shared with the C
// vtable (kind + payload union). See `EventPayload` in nros-rmw-cffi.

/// Raw callback signature. Identical Rust + C ABI. No alloc.
/// `payload_kind` selects the variant of `payload_ptr`. `user_ctx`
/// is opaque application state passed at registration.
pub type EventCallback = unsafe extern "C" fn(
    payload_kind: EventKind,
    payload_ptr: *const core::ffi::c_void,
    user_ctx: *mut core::ffi::c_void,
);

pub trait Subscriber {
    fn supports_event(&self, _kind: EventKind) -> bool { false }

    /// Register a raw callback. `deadline_ms` consulted only for
    /// `RequestedDeadlineMissed`; `0` = use SC-bound deadline.
    /// Returns `Err(Unsupported)` if backend doesn't generate this
    /// event for this entity. SAFETY: `cb` must remain valid for
    /// entity lifetime; `user_ctx` likewise.
    unsafe fn register_event_callback(
        &mut self,
        kind: EventKind,
        deadline_ms: u32,
        cb: EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), Self::Error>;
}
```

Same shape on `Publisher`. Default `supports_event = false`; default `register_event_callback = Err(Unsupported)`. Backends override per event kind.

Closure ergonomics on `nros-node`: typed wrappers store the closure in a per-callback slot (allocated from the executor arena, not the heap), generate a static `unsafe extern "C" fn` trampoline that downcasts `user_ctx` and invokes the closure. Same pattern as today's `add_subscription::<M, F>(topic, closure)` — closure lifetime tied to the entity, no heap.

C vtable extension:

```c
typedef enum nros_rmw_event_kind_t {
    NROS_RMW_EVENT_LIVELINESS_CHANGED         = 0,
    NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED  = 1,
    NROS_RMW_EVENT_MESSAGE_LOST               = 2,
    NROS_RMW_EVENT_LIVELINESS_LOST            = 3,
    NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED    = 4,
} nros_rmw_event_kind_t;

typedef struct nros_rmw_liveliness_changed_status_t {
    uint16_t alive_count;
    uint16_t not_alive_count;
    int16_t  alive_count_change;
    int16_t  not_alive_count_change;
} nros_rmw_liveliness_changed_status_t;

typedef struct nros_rmw_count_status_t {
    uint32_t total_count;
    uint32_t total_count_change;
} nros_rmw_count_status_t;

typedef union nros_rmw_event_payload_t {
    nros_rmw_liveliness_changed_status_t liveliness_changed;
    nros_rmw_count_status_t              count;
} nros_rmw_event_payload_t;

typedef void (*nros_rmw_event_callback_t)(
    nros_rmw_event_kind_t kind,
    const nros_rmw_event_payload_t *payload,
    void *user_context);

typedef struct nros_rmw_vtable_t {
    /* … */
    nros_rmw_ret_t (*register_subscriber_event)(
        nros_rmw_subscriber_t *sub,
        nros_rmw_event_kind_t  kind,
        uint32_t               deadline_ms,
        nros_rmw_event_callback_t cb,
        void                  *user_context);

    nros_rmw_ret_t (*register_publisher_event)(
        nros_rmw_publisher_t *pub,
        nros_rmw_event_kind_t kind,
        uint32_t              deadline_ms,
        nros_rmw_event_callback_t cb,
        void                 *user_context);
} nros_rmw_vtable_t;
```

NULL function pointer = no events. Specific-kind unsupported = `NROS_RMW_RET_UNSUPPORTED`.

User-facing API (`nros-node`):

```rust
impl<M: Message> Subscription<M> {
    /// Closure stored in entity arena; trampoline generated at compile time.
    /// No heap allocation. Returns `Err(Unsupported)` if backend doesn't
    /// generate this event for this entity.
    pub fn on_liveliness_changed<F>(&mut self, cb: F) -> Result<()>
    where F: FnMut(LivelinessChangedStatus) + 'static;

    pub fn on_requested_deadline_missed<F>(
        &mut self,
        deadline: core::time::Duration,
        cb: F,
    ) -> Result<()> where F: FnMut(CountStatus) + 'static;

    pub fn on_message_lost<F>(&mut self, cb: F) -> Result<()>
    where F: FnMut(CountStatus) + 'static;
}

impl<M: Message> Publisher<M> {
    pub fn on_liveliness_lost<F>(...);
    pub fn on_offered_deadline_missed<F>(deadline: Duration, cb: F);
}
```

`'static` (not `Send`) — entity is single-thread-owned by its Executor; closure inherits.

Async equivalents (`next_liveliness_change().await` etc.) — Future variant via Phase 99's waker plumbing. Single shared `EventFuture` poll path; no heap allocation; same machinery as `Subscription::recv().await`. Optional, behind `feature = "async"`.

### 108.B — Full QoS shape

```c
typedef enum nros_rmw_liveliness_kind_t {
    NROS_RMW_LIVELINESS_NONE              = 0,
    NROS_RMW_LIVELINESS_AUTOMATIC         = 1,
    NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC   = 2,
    NROS_RMW_LIVELINESS_MANUAL_BY_NODE    = 3,
} nros_rmw_liveliness_kind_t;

typedef struct nros_rmw_qos_t {
    /* ---- Existing 8-byte subset, layout-preserved. ---- */
    uint8_t  reliability;
    uint8_t  durability;
    uint8_t  history;
    uint8_t  liveliness_kind;            /* repurposed from former _reserved0 */
    uint16_t depth;
    uint16_t _reserved0;                 /* renamed from former _reserved1 */

    /* ---- 108.B extensions (16 bytes). ---- */
    uint32_t deadline_ms;                /* 0 = infinite */
    uint32_t lifespan_ms;                /* 0 = infinite */
    uint32_t liveliness_lease_ms;        /* 0 = infinite */
    uint8_t  avoid_ros_namespace_conventions;  /* 0 = false, nonzero = true */
    uint8_t  _reserved1[3];
} nros_rmw_qos_t;                        /* 24 bytes */
```

Avoid C99 `_Bool` for ABI stability — `sizeof(_Bool)` is impl-defined; use `uint8_t` w/ documented `0/nonzero` convention. Sentinel `0` for time fields = "policy off / infinite" matches Phase 110 `OptUs` ABI convention.

Rust mirror uses `OptUs` (Phase 110) for the time fields:

```rust
#[repr(C)]
pub struct NrosRmwQos {
    pub reliability: u8,
    pub durability: u8,
    pub history: u8,
    pub liveliness_kind: u8,
    pub depth: u16,
    pub _reserved0: u16,
    pub deadline: OptUs,           // ← Phase 110 newtype
    pub lifespan: OptUs,           // ← Phase 110 newtype
    pub liveliness_lease: OptUs,   // ← Phase 110 newtype
    pub avoid_ros_namespace_conventions: u8,
    pub _reserved1: [u8; 3],
}
```

Single shared newtype across both phases — one definition site, consistent semantics.

Standard profile constants matching upstream (`NROS_RMW_QOS_PROFILE_DEFAULT`, `_SENSOR_DATA`, `_SERVICES_DEFAULT`, `_SYSTEM_DEFAULT`, `_PARAMETERS`).

Backend support advertised via bitmask:

```rust
pub trait Session {
    fn supported_qos_policies(&self) -> QosPolicyMask { QosPolicyMask::CORE }
}

bitflags! {
    pub struct QosPolicyMask: u32 {
        const RELIABILITY                       = 1 << 0;
        const DURABILITY_VOLATILE               = 1 << 1;
        const DURABILITY_TRANSIENT_LOCAL        = 1 << 2;
        const HISTORY                           = 1 << 3;
        const DEPTH                             = 1 << 4;
        const DEADLINE                          = 1 << 5;
        const LIFESPAN                          = 1 << 6;
        const LIVELINESS_AUTOMATIC              = 1 << 7;
        const LIVELINESS_MANUAL_BY_TOPIC        = 1 << 8;
        const LIVELINESS_MANUAL_BY_NODE         = 1 << 9;
        const LIVELINESS_LEASE                  = 1 << 10;
        const AVOID_ROS_NAMESPACE_CONVENTIONS   = 1 << 11;

        const CORE = Self::RELIABILITY.bits()
                   | Self::DURABILITY_VOLATILE.bits()
                   | Self::HISTORY.bits()
                   | Self::DEPTH.bits();
    }
}
```

`Session::create_publisher` / `create_subscriber` validate requested QoS against the mask; unsupported policy → `Err(IncompatibleQos)` synchronously. **No silent degradation.**

Liveliness API surface — for `MANUAL_BY_*`, app must explicitly assert:

```rust
impl<M: Message> Publisher<M> {
    pub fn assert_liveliness(&self) -> Result<(), NodeError>;
}
```

C side: `nros_publisher_assert_liveliness(pub) -> nros_ret_t`. Default impl no-op; backends override.

### Storage budget

Per registered event-callback (64-bit pointer target):
- 8 B function pointer (`EventCallback`)
- 8 B `user_ctx` opaque pointer
- 8 B status counters (`CountStatus` = 8 B; `LivelinessChangedStatus` = 8 B; same size)
- 4 B `deadline_ms`
- 4 B padding / discriminant

≈ **32 B per event-callback registration**.

Subscriber w/ all three sub-side events (liveliness + deadline + message-lost): **~96 B**. Publisher w/ both pub-side events: **~64 B**. Bounded; fits in executor arena. Apps not registering events pay zero.

On 32-bit platforms (Cortex-M / RV32) function/ctx pointers are 4 B each → ~24 B per event, ~72 B for full subscriber, ~48 B for full publisher.

### Wire-level requirements (108.B-driven)

| Policy | Backend mechanism |
|--------|-------------------|
| Lifespan | per-sample timestamp; subscriber compares to `now` |
| Liveliness | backend's keepalive (Zenoh tokens, DDS PARTICIPANT msgs, XRCE session pings) |
| Deadline | tracked at entity, no wire metadata |
| Durability TL | backend's late-joiner replay (Zenoh pub cache, DDS reader history) |

Each backend uses native attachment mechanism. nano-ros doesn't define a cross-backend attachment header.

### Cross-feature interaction

- **108.A events depend on 108.B QoS.** `register_event_callback(RequestedDeadlineMissed)` on a subscription with `deadline_ms = 0` returns `Err(IncompatibleQos)` — policy isn't enabled.
- **Phase 110 `ReadySet` interaction.** Each event-callback registration is a separate ready bit in the executor's `ReadySet`. Events drain alongside message callbacks under `DrainMode::Latched` and against the dispatch loop's optional count cap. No special-casing in the executor.
- **Phase 110 `OptUs` newtype reused** for QoS time fields. Single newtype definition site (`packages/core/nros-node/src/executor/sched_context.rs` from Phase 110) — Phase 108 imports it, doesn't redefine.
- **Tier-2/3 events deferred.** See vocabulary table above. `EventKind` is `#[non_exhaustive]`, additive.
- **No upstream `rmw_event_t` ABI compat.** Apps porting from rclcpp rewrite event-handling code; message path stays compatible.

---

## Work Items

### v1 — 108.A (Status events surface) — **COMPLETE** (commit `2ae8fbaf` + leak fix `f9d2267f`)

- [x] **108.A.1 — Rust trait + payload types.** `nros-rmw` crate: new `event.rs` module with `EventKind`, payload structs, `EventPayload<'a>` borrow-shaped union, `EventCallback` raw fn pointer typedef. Trait extensions on `Subscriber` and `Publisher` w/ default `Unsupported` impls.
  **Files:** `packages/core/nros-rmw/src/event.rs`, `packages/core/nros-rmw/src/traits.rs`, `packages/core/nros-rmw/src/lib.rs`.
- [x] **108.A.2 — C vtable + payload header.** `<nros/rmw_event.h>` w/ event types. Vtable adds `register_subscriber_event` + `register_publisher_event` function pointers.
  **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_event.h`, `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`.
- [x] **108.A.3 — Rust mirror in `nros-rmw-cffi`.** `#[repr(C)]` mirrors. `NrosRmwVtable` w/ event fn pointers. `CffiSubscriber`/`CffiPublisher` route through vtable.
  **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`.
- [x] **108.A.4 — User-facing API on `nros-node`.** `Subscription<M>::on_liveliness_changed` / `on_requested_deadline_missed` / `on_message_lost`. `Publisher<M>::on_liveliness_lost` / `on_offered_deadline_missed`. Closure boxed → trampoline; `Err(Unsupported)` until backend wiring lands per kind.
  **Files:** `packages/core/nros-node/src/executor/handles.rs`.
- [x] **108.A.5 — C / C++ thin wrappers.** `nros-c`: `nros_subscription_set_*_callback` family. `nros-cpp`: `nros::Subscription<M>::on_*` w/ std::function overloads under `NROS_CPP_STD`.
  **Files:** `packages/core/nros-c/src/event.rs`, `packages/core/nros-cpp/include/nros/{publisher,subscription}.hpp`.
- [x] **108.A.6 — Book chapter.** `book/src/concepts/status-events.md` (244 lines).
- [x] **108.A.7 — Closure leak on entity drop fix.** `heapless::Vec<EventReg, 3>` per entity + type-erased Box destructor; Drop walks the registry. No-alloc builds: ZST + no-op (commit `f9d2267f`).
  **Files:** `packages/core/nros-node/src/executor/handles.rs`, `packages/core/nros-node/src/executor/node.rs`.

### v1 — 108.B (Full QoS surface) — **COMPLETE** (commit `c5ef9fdc`)

- [x] **108.B.1 — Update C header `<nros/rmw_entity.h>`.** `nros_rmw_qos_t` = 24 bytes; `nros_rmw_liveliness_kind_t` enum; standard profile constants; `bool` → `uint8_t` for ABI stability.
- [x] **108.B.2 — Update Rust mirror in `nros-rmw-cffi`.** `NrosRmwQos` grows; `LivelinessKind` enum; `pub const`s for standard profiles; `avoid_ros_namespace_conventions: u8`.
- [x] **108.B.3 — Update `nros-rmw` `QosSettings` + add `QosPolicyMask`.** Extended fields; bitflags; `Session::supported_qos_policies()` trait method; `QosSettings::required_policies()` / `validate_against()` helpers.
- [x] **108.B.4 — `Node::create_*` validates QoS against mask.** Synchronous `Err(IncompatibleQos)`; no silent downgrade. (Validation lives in `nros-node` `Node::create_*_with_qos`, not in the `Session` trait — keeps backends transport-only.)
- [x] **108.B.5 — `Publisher::assert_liveliness()` trait method.** Default no-op. C vtable: `assert_publisher_liveliness` function pointer.
- [x] **108.B.6 — `nros-node` user-facing surface.** `Publisher<M>::assert_liveliness()` + `EmbeddedRawPublisher::assert_liveliness()`. `create_*_with_qos` validates. `nros::qos::{DEFAULT, SENSOR_DATA, SERVICES_DEFAULT, PARAMETERS, SYSTEM_DEFAULT}` profile constants re-exported.

### Post-v1 — 108.C (Per-backend wiring)

Backends opt into specific QoS bits + event kinds one at a time. Each landing flips bits in the backend's `supported_qos_policies()` mask and overrides the relevant trait methods.

#### dust-DDS — `108.C.dds` — **COMPLETE** (commit `d74aa834`)

- [x] **108.C.dds.1 — Full QoS mapping.** `map_writer_qos` / `map_reader_qos` translate `QosSettings` → dust-dds `DataWriterQos` / `DataReaderQos`. Reliability, durability (V/TL), history, deadline, lifespan (writer), liveliness (Auto/ManualByTopic/ManualByParticipant), liveliness lease.
- [x] **108.C.dds.2 — `Session::supported_qos_policies()` override.** Returns CORE | DURABILITY_TL | DEADLINE | LIFESPAN | LIVELINESS_AUTOMATIC | LIVELINESS_MANUAL_BY_TOPIC | LIVELINESS_MANUAL_BY_NODE | LIVELINESS_LEASE.
- [x] **108.C.dds.3 — `Publisher::assert_liveliness`** routes through dust-dds `DataWriter::assert_liveliness` (sync) / `DataWriterAsync` (no_std).
- [x] **108.C.dds.4 — Status events (sub side).** `DataReaderListener` bridge fires `LivelinessChanged` / `RequestedDeadlineMissed` / `MessageLost` (commit `861fc2cf`).
- [x] **108.C.dds.5 — Status events (pub side).** `DataWriterListener` bridge fires `LivelinessLost` / `OfferedDeadlineMissed` (commit `861fc2cf`).

#### XRCE-DDS — `108.C.xrce`

- [x] **108.C.xrce.1 — `Session::supported_qos_policies()` override.** Returns CORE | DURABILITY_TL | AVOID_ROS_NAMESPACE_CONVENTIONS (commits `95df4d39`, `<this commit>`). XRCE C client surface (`uxrQoS_t`) doesn't expose deadline / lifespan / liveliness; agent-side enforcement only.
- [x] **108.C.xrce.1b — `avoid_ros_namespace_conventions` honoured at topic-name encoding.** `naming::dds_topic_name` skips the `rt/` prefix when the flag is set; XRCE is the only backend that meaningfully implements this flag (others pass topic names through unchanged).
- [x] **108.C.xrce.2 — Shim-emulated DEADLINE.** Same shape as 108.C.zenoh.2: clock-based check on every receive / publish poll. Sub side: `SubscriberSlot` captures `deadline_ms` from QoS at create, bumps `last_msg_at_ms` from inside `topic_callback` after each payload copy, runs `check_deadline_and_fire` from `XrceSubscriber::has_data` / `try_recv_raw` / `process_raw_in_place` (rate-limited to ≤ 1 fire per deadline period). Pub side: `XrcePublisher` carries the same fields plus an `AtomicCallback`; `publish_raw` calls `check_offered_deadline` before the wire write, then bumps `last_publish_at_ms` only on `Ok`. Both paths short-circuit when `now_ms() == 0` (gated behind the `platform-udp` feature). `Subscriber::supports_event(RequestedDeadlineMissed)` / `Publisher::supports_event(OfferedDeadlineMissed)` now return `true`; `supported_qos_policies()` advertises `DEADLINE`. `LivelinessChanged` / `LivelinessLost` / `MessageLost` remain unsupported (xrce-dds-client API doesn't expose session-level liveliness events to topic readers, and `topic_callback` carries no per-sample sequence). `AtomicCallback` (raw `cb` + `ctx` AtomicPtr pair) is used instead of `Cell<Option<EventReg>>` because XRCE entities live in a `static mut` slot array. (`<this commit>`)
- [ ] **108.C.xrce.3 — Optional: full QoS via XML. DEFERRED.** `uxr_buffer_create_*_xml` accepts agent-side QoS XML w/ deadline / lifespan / liveliness. Requires agent w/ XML support + larger payload. Defer until requested.

#### zenoh-pico — `108.C.zenoh`

- [x] **108.C.zenoh.1 — `Session::supported_qos_policies()` override.** Returns CORE only (commit `95df4d39`).
- [x] **108.C.zenoh.2 — Shim-emulated DEADLINE.** Sub side: `ZenohSubscriber` captures `deadline_ms` from QoS at create, tracks `last_msg_at_ms` (updated on every successful `try_recv_raw`), checks gap against `now_ms()` from `<P as PlatformClock>::clock_ms` on each `try_recv_raw` / `has_data`, fires `RequestedDeadlineMissed` with rate-limit (≤ 1 fire per deadline period). Pub side: same pattern on `ZenohPublisher::publish_raw` for `OfferedDeadlineMissed`. (`<this commit>`)
- [x] **108.C.zenoh.3 — Shim-emulated LIFESPAN.** `try_recv_raw` parses the publisher timestamp out of the RMW attachment (bytes 8..16 = i64 ns LE, already populated since Phase 91), compares to `now_ms()`, drops the sample (returns `Ok(None)`) when `now > sent_ts + lifespan_ms`. (`<this commit>`)
- [x] **108.C.zenoh.4 — Shim-emulated LIVELINESS via zenoh tokens.** Sub side `LivelinessChanged`: `ZenohSubscriber` builds a wildcard publisher liveliness keyexpr (`Ros2Liveliness::publisher_keyexpr_wildcard` — wildcards on zid, namespace, node, type_hash, qos) at create. A poll loop in `has_data` issues `liveliness_get_start` against that keyexpr every `LIVELINESS_POLL_DEFAULT_MS = 1000ms` (clamped to half the QoS lease when set), checks the result on subsequent polls via `liveliness_get_check`, and fires `LivelinessChanged` on alive↔not-alive transitions. Approximation: alive_count is `{0, 1}` (any matching publisher), not the per-publisher DDS count — sufficient for "any publisher present" semantics; per-publisher counting needs a long-lived `z_liveliness_declare_subscriber` shim, deferred. Pub side `LivelinessLost`: surface only, never fires (needs per-publisher keepalive timer for MANUAL_BY_TOPIC/NODE; AUTOMATIC kind has no "lost" event since the zenoh runtime keeps the token until session close). ~280 LOC. (`<this commit>`)
- [x] **108.C.zenoh.5 — Shim-emulated MESSAGE_LOST.** Subscriber parses publisher seq from the existing RMW attachment, tracks `next_expected_seq`, and fires `MessageLost { total_count, total_count_change }` on the registered callback when `seq > expected`. Out-of-order / duplicate samples count as zero loss. First-message synchronisation: initial `next_expected_seq = 0` means we sync to the publisher's first observed seq w/o reporting a gap. (commit `0c8e24ee`)
- [x] **108.C.zenoh.6 — Update `supported_qos_policies()` mask.** Now `CORE | DEADLINE | LIFESPAN`. Liveliness deferred until 108.C.zenoh.4's bridge lands.

Bonus side-effect: `ZenohPublisher::current_timestamp` switched from a per-publisher monotonic counter to the platform clock (`<P as PlatformClock>::clock_ms()` × 1_000_000 ns). Existing rmw_zenoh interop keeps working (timestamps still monotonic per publisher, now also globally meaningful).

#### uORB — `108.C.uorb`

- [x] **108.C.uorb.1 — `Session::supported_qos_policies()` override.** Returns CORE only (commit `95df4d39`).
- [x] **108.C.uorb.2 — `MessageLost` event.** Both std host (mock broker) and real-target paths complete. The C wrapper's `RustSubscriptionCallback` (`px4-sys/wrapper.cpp`) tracks publish-callback fires in an atomic and computes the delta on each successful `update()`. New `px4_rs_sub_cb_lost_take` extern "C" surface drains the accumulator. `RawSubscription::missed_count()` routes there; `UorbSubscriber` polls it on every `try_recv_raw` and fires the registered `MessageLost` callback when non-zero. RawSubscription gains `on_message_lost` / `on_liveliness_changed` / `on_requested_deadline_missed` ergonomic wrappers (also reusable by other typeless backends). Test `message_lost_event_fires_on_dropped_messages` verifies cumulative + delta counts on host. (commits `c9748ad9` host, `<this>` real-target submodule bump.)

#### Cross-backend integration

- [ ] **108.C.x.1 — Test matrix. DEFERRED.** Per-backend × per-policy E2E test (e.g. dust-DDS publisher exceeds deadline → subscriber fires `RequestedDeadlineMissed`). ~5-10 tests × backends w/ wiring (today: dust-DDS only). Open as separate phase once at least 2 backends have full wiring.
- [x] **108.C.x.2 — Per-backend support matrix in book.** `book/src/concepts/status-events.md` § "Backend support is uneven" updated to reflect dust-DDS full wiring + others 🟡 planned. (commit `<this commit>`).
- [x] **108.C.x.3 — `AVOID_ROS_NAMESPACE_CONVENTIONS` flag.** XRCE-DDS `naming::dds_topic_name` honours the flag (`true` skips `rt/` prefix). dust-DDS / zenoh-pico / uORB pass topic names through unchanged (no prefix added either way) — flag has no observable effect on those backends and is therefore not advertised in their `supported_qos_policies()`. (commit `<this commit>`).
  **Files:** `packages/core/nros-node/src/`, `packages/core/nros/src/lib.rs`.
- [ ] **108.B.7 — C / C++ user-facing wrappers.** `nros-c` extends `nros_qos_t`. `nros-cpp` extends `nros::QoS` builder + adds `Publisher<M>::assert_liveliness()`. Profile constants in both.
  **Files:** `packages/core/nros-c/src/qos.rs`, `packages/core/nros-c/include/nros/types.h`, `packages/core/nros-cpp/include/nros/qos.hpp`.
- [ ] **108.B.8 — Book + Doxygen updates.** Rewrite `book/src/design/rmw-vs-upstream.md` § 7 (was "QoS subset"; now full DDS surface w/ per-backend mask + synchronous IncompatibleQos check + assert_liveliness). Update `book/src/concepts/ros2-comparison.md`. Cross-link to status-events doc.

---

## Acceptance Criteria

### v1 (108.A + 108.B API surface)

- [ ] `cargo build -p nros-rmw -p nros-rmw-cffi -p nros-node -p nros -p nros-c -p nros-cpp` clean.
- [ ] cbindgen regenerates `nros_generated.h` w/ new event callback typedefs + extended QoS struct.
- [ ] Doxygen `<nros/rmw_event.h>` + extended `<nros/rmw_entity.h>` site renders clean.
- [ ] `mdbook build` clean.
- [ ] `cargo test -p nros-rmw-cffi --lib tests::typed_struct_roundtrip` passes after QoS struct grows to 24 bytes.
- [ ] All standard QoS profile constants match upstream `rmw_qos_profile_*` field-by-field.
- [ ] Calling `subscription.on_liveliness_changed(...)` on every backend returns `Err(Unsupported)` (no backend wiring lands here).
- [ ] Calling `node.create_publisher_with_qos(topic, nros::qos::SENSOR_DATA)` returns `Err(IncompatibleQos)` because no backend has wired up DEADLINE / LIFESPAN / LIVELINESS_* yet.
- [ ] `nros::qos::DEFAULT` (CORE-only) paths unaffected.

---

## Notes

### Backend wiring progress (108.C)

Tracked as sub-phases above. Current status:

| Backend | QoS surface | assert_liveliness | Status events | Avoid-ROS-prefix |
|---------|-------------|---------------------|------------------|---------------------|
| dust-DDS | ✅ full (commit `d74aa834`) | ✅ native | ✅ full (commit `861fc2cf`) | n/a (no prefix) |
| XRCE-DDS | ✅ CORE+TL+DEADLINE (commit `95df4d39`, `<this commit>`) | n/a | 🟡 DEADLINE (sub+pub) shim-emulated; LIVELINESS/MessageLost not feasible at this layer | ✅ honoured |
| zenoh-pico | ✅ CORE+DEADLINE+LIFESPAN+LIVELINESS_AUTOMATIC+LIVELINESS_LEASE | n/a | ✅ MessageLost + RequestedDeadlineMissed + OfferedDeadlineMissed + LivelinessChanged (poll-based, alive_count ∈ {0,1}); 🟡 LivelinessLost surface only | n/a (no prefix) |
| uORB | ✅ CORE (commit `95df4d39`) | n/a | ✅ MessageLost (host mock + real-PX4 wired) | n/a (no DDS naming) |

### No upstream ABI compat

`nros_rmw_qos_t` ABI break is one-shot; pre-publish so no version-bump migration. Apps recompile against the new header; in-tree backends recompile cleanly because they don't honour any new policies yet (default mask = CORE).

### uORB QoS

uORB has limited QoS by design — intra-process pubsub, no wire-level reliability or durability. Adapted: `RELIABLE` always (queue-bounded), `VOLATILE` always, no deadline / lifespan / liveliness in the DDS sense. uORB's `supported_qos_policies()` returns `RELIABILITY | DURABILITY_VOLATILE | HISTORY | DEPTH` only.

### Zero default = policy off

Apps that don't care about deadline / lifespan / liveliness pay zero validation cost (mask only matters when policy explicitly requested w/ non-zero value). Sentinel `0` matches Phase 110 `OptUs` convention.

### Async path symmetry

Phase 99's `Subscription::recv().await` machinery extends to event-Futures; same waker registration, same drive path.

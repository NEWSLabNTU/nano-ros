# Phase 108 — RMW Surface Extensions (Status Events + Full QoS)

**Goal:** Two API + FFI surface additions to the RMW layer, both ship as API-only first (no backend wiring), with backends opting in per-policy in follow-up phases.

- **108.A — Status events** — trait + C-vtable + user-facing surface for transport-level status events (liveliness changes, deadline misses, message loss). Adopts callback-on-entity dispatch (matches existing message-callback path) instead of upstream's `rmw_event_t + rmw_take_event + waitset` machinery.
- **108.B — Full DDS-shaped QoS profile** — extends `nros_rmw_qos_t` to carry deadline, lifespan, liveliness, and the namespace-convention flag in addition to the existing reliability / durability / history / depth subset. Backends advertise per-policy support via a bitmask; unsupported policies return `NROS_RMW_RET_INCOMPATIBLE_QOS` synchronously (no silent degradation).

Both bundled because they share the `nros_rmw_qos_t` / `nros_rmw_event_t` C header, both ship API-only, and Phase 108.A's deadline/liveliness events depend on Phase 108.B's QoS fields to be meaningful.

**Status:** Not Started

**Priority:** Medium — surfaces let users start writing code; backend wiring follows in per-backend phases.

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

### v1 — 108.A (Status events surface)

- [ ] **108.A.1 — Rust trait + payload types.** `nros-rmw` crate: new `event.rs` module with `EventKind`, payload structs, `EventPayload<'a>` borrow-shaped union, `EventCallback` boxed closure typedef. Trait extensions on `Subscriber` and `Publisher` w/ default `Unsupported` impls.
  **Files:** `packages/core/nros-rmw/src/event.rs` (new), `packages/core/nros-rmw/src/traits.rs`, `packages/core/nros-rmw/src/lib.rs`.
- [ ] **108.A.2 — C vtable + payload header.** New `<nros/rmw_event.h>`. Vtable extension: two new optional function pointers. Doxygen on lifetime + threading + dispatch context.
  **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_event.h` (new), `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`.
- [ ] **108.A.3 — Rust mirror in `nros-rmw-cffi`.** `#[repr(C)]` mirrors. `NrosRmwVtable` adds two new function pointers. `CffiSubscriber`/`CffiPublisher` thread `register_event_callback` calls through.
  **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`.
- [ ] **108.A.4 — User-facing API on `nros-node`.** `Subscription<M>::on_liveliness_changed` / `on_requested_deadline_missed` / `on_message_lost`. `Publisher<M>::on_liveliness_lost` / `on_offered_deadline_missed`. Async `next_*` Future variants. Each method `Err(Unsupported)` until backend wiring lands.
  **Files:** `packages/core/nros-node/src/subscription.rs`, `packages/core/nros-node/src/publisher.rs`.
- [ ] **108.A.5 — C / C++ thin wrappers.** `nros-c`: `nros_subscription_set_*_callback` family. `nros-cpp`: `nros::Subscription<M>::on_*` w/ `std::function` overloads under `NROS_CPP_STD`.
  **Files:** `packages/core/nros-c/src/`, `packages/core/nros-cpp/include/nros/`, `packages/core/nros-cpp/src/`.
- [ ] **108.A.6 — Book chapter.** New `book/src/concepts/status-events.md`: five Tier-1 event kinds + use cases; callback-on-entity vs upstream waitset-take rationale; per-backend support matrix; Tier-2/3 skipped + rationale; per-RTOS recommendations (drone bridge, 100 Hz sensor pattern). Cross-link from `book/src/design/rmw-vs-upstream.md` § 8.

### v1 — 108.B (Full QoS surface)

- [ ] **108.B.1 — Update C header `<nros/rmw_entity.h>`.** Extend `nros_rmw_qos_t` to 24 bytes. Add `nros_rmw_liveliness_kind_t` enum. Standard profile constants. Doxygen on each new field.
  **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_entity.h`.
- [ ] **108.B.2 — Update Rust mirror in `nros-rmw-cffi`.** `NrosRmwQos` grows. `LivelinessKind` enum. `pub const`s for standard profiles. `From<QosSettings> for NrosRmwQos` extended.
  **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`.
- [ ] **108.B.3 — Update `nros-rmw` `QosSettings` + add `QosPolicyMask`.** Extend QosSettings w/ deadline, lifespan, liveliness_kind, liveliness_lease_duration, avoid_ros_namespace_conventions. Default values preserve current semantics (zero = "off"). Add `QosPolicyMask` bitflags. Add `Session::supported_qos_policies()` trait method (default `CORE`).
  **Files:** `packages/core/nros-rmw/src/traits.rs`, `packages/core/nros-rmw/src/lib.rs`.
- [ ] **108.B.4 — `Session::create_*` validates QoS against mask.** Default-implemented validation. Unsupported policy → `IncompatibleQos`. No silent downgrade.
  **Files:** `packages/core/nros-rmw/src/traits.rs`.
- [ ] **108.B.5 — `Publisher::assert_liveliness()` trait method.** Default no-op. C vtable adds optional `assert_publisher_liveliness` function pointer.
  **Files:** `packages/core/nros-rmw/src/traits.rs`, `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`, `packages/core/nros-rmw-cffi/src/lib.rs`.
- [ ] **108.B.6 — `nros-node` user-facing surface.** `Publisher<M>::assert_liveliness()`. `create_*_with_qos` accepts extended QosSettings. Profile constants re-exported at `nros::qos::*`.
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

### Backend wiring follow-up phases

108 lands the surface only. Per-backend wiring follows in numbered sub-phases (concrete numbers TBD when 108 lands; will use the 109 / 111+ slots freed by Phase 105/107/109 archive/merge):

- dust-DDS event + QoS wiring (native; ~80 LOC for events, ~120 LOC for full QoS opt-in)
- XRCE-DDS event + QoS wiring (native via uxr listener; ~80 LOC events, ~120 LOC QoS)
- zenoh-pico event + QoS wiring (shim-tracked, ~150 LOC events, ~200 LOC QoS — biggest because Zenoh has no native QoS, all policies emulated)
- uORB event + QoS wiring (Tier-1 partial coverage; ~50 LOC; only RELIABILITY/DURABILITY_VOLATILE/HISTORY/DEPTH supported per uORB QoS section below)

### No upstream ABI compat

`nros_rmw_qos_t` ABI break is one-shot; pre-publish so no version-bump migration. Apps recompile against the new header; in-tree backends recompile cleanly because they don't honour any new policies yet (default mask = CORE).

### uORB QoS

uORB has limited QoS by design — intra-process pubsub, no wire-level reliability or durability. Adapted: `RELIABLE` always (queue-bounded), `VOLATILE` always, no deadline / lifespan / liveliness in the DDS sense. uORB's `supported_qos_policies()` returns `RELIABILITY | DURABILITY_VOLATILE | HISTORY | DEPTH` only.

### Zero default = policy off

Apps that don't care about deadline / lifespan / liveliness pay zero validation cost (mask only matters when policy explicitly requested w/ non-zero value). Sentinel `0` matches Phase 110 `OptUs` convention.

### Async path symmetry

Phase 99's `Subscription::recv().await` machinery extends to event-Futures; same waker registration, same drive path.

# Phase 108 — RMW Surface Extensions (Status Events + Full QoS)

**Goal:** Two API + FFI surface additions to the RMW layer, both ship as API-only first (no backend wiring), with backends opting in per-policy in follow-up phases.

- **108.A — Status events** — trait + C-vtable + user-facing surface for transport-level status events (liveliness changes, deadline misses, message loss). Adopts callback-on-entity dispatch (matches existing message-callback path) instead of upstream's `rmw_event_t + rmw_take_event + waitset` machinery.
- **108.B — Full DDS-shaped QoS profile** — extends `nros_rmw_qos_t` to carry deadline, lifespan, liveliness, and the namespace-convention flag in addition to the existing reliability / durability / history / depth subset. Backends advertise per-policy support via a bitmask; unsupported policies return `NROS_RMW_RET_INCOMPATIBLE_QOS` synchronously (no silent degradation).

Both bundled because they share the `nros_rmw_qos_t` / `nros_rmw_event_t` C header, both ship API-only, and Phase 108.A's deadline/liveliness events depend on Phase 108.B's QoS fields to be meaningful.

**Status:** Not Started

**Priority:** Medium — surfaces let users start writing code; backend wiring follows in per-backend phases.

**Depends on:** Phase 102 (typed entity structs, `nros_rmw_ret_t`), Phase 105 (`max_callbacks` cap; events count against it the same as message callbacks).

---

## Background

### 108.A — Why callbacks, not waitset-take

Three classes of transport-level status are useful on RTOS:

| Event | Use case |
|-------|----------|
| **Liveliness changed (sub) / lost (pub)** | Safety-island fail-over: when a remote control node goes silent, trigger MRM. Drone bridge: detect PX4 commander stall. |
| **Deadline missed (sub / pub)** | Periodic-pubsub safety: 100 Hz sensor topic; if a sample doesn't arrive within deadline, alarm or fail-over. |
| **Message lost (sub)** | Slow-consumer diagnostic: ring buffer overflow signals the app to drop / coalesce / log. |

Three more (`MATCHED`, `QOS_INCOMPATIBLE`, `INCOMPATIBLE_TYPE`) exist in upstream but are mostly diagnostic — fire once at startup in static-topology embedded apps. Skip them for now; surface via existing `nros_rmw_ret_t` codes at create-time instead.

Upstream uses `rmw_event_t` handles in a waitset; `rmw_wait` returns when an event fires; `rmw_take_event` pulls the payload. Two-phase, per-call. Adopting it would require a waitset abstraction we deliberately don't have. Replace with **callback-on-entity** — backend's RX worker detects event, runs registered callback inline. Reuses existing `drive_io` callback dispatch path; counts against Phase 105's `max_callbacks` cap; matches message-callback ergonomics.

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

pub enum EventKind {
    LivelinessChanged,            // subscriber
    RequestedDeadlineMissed,      // subscriber
    MessageLost,                  // subscriber
    LivelinessLost,               // publisher
    OfferedDeadlineMissed,        // publisher
}

#[derive(Debug, Clone, Copy)]
pub struct LivelinessChangedStatus {
    pub alive_count: u16,
    pub not_alive_count: u16,
    pub alive_count_change: i16,
    pub not_alive_count_change: i16,
}

#[derive(Debug, Clone, Copy)]
pub struct DeadlineMissedStatus {
    pub total_count: u32,
    pub total_count_change: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct MessageLostStatus {
    pub total_count: u32,
    pub total_count_change: u32,
}

pub enum EventPayload<'a> {
    LivelinessChanged(&'a LivelinessChangedStatus),
    RequestedDeadlineMissed(&'a DeadlineMissedStatus),
    MessageLost(&'a MessageLostStatus),
    LivelinessLost(&'a DeadlineMissedStatus),
    OfferedDeadlineMissed(&'a DeadlineMissedStatus),
}

pub trait Subscriber {
    fn supports_event(&self, _kind: EventKind) -> bool { false }

    fn register_event_callback(
        &mut self,
        kind: EventKind,
        deadline_ms: u32,
        cb: EventCallback,
    ) -> Result<(), Self::Error>;
}

pub type EventCallback = alloc::boxed::Box<dyn FnMut(EventPayload<'_>) + Send>;
```

Same shape on `Publisher`. Default `supports_event = false`; default `register_event_callback = Err(Unsupported)`. Backends override per event kind.

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
    pub fn on_liveliness_changed<F>(&mut self, cb: F) -> Result<()>
    where F: FnMut(LivelinessChangedStatus) + Send + 'static;

    pub fn on_requested_deadline_missed<F>(
        &mut self,
        deadline: core::time::Duration,
        cb: F,
    ) -> Result<()> where F: FnMut(DeadlineMissedStatus) + Send + 'static;

    pub fn on_message_lost<F>(&mut self, cb: F) -> Result<()>
    where F: FnMut(MessageLostStatus) + Send + 'static;
}

impl<M: Message> Publisher<M> {
    pub fn on_liveliness_lost<F>(...);
    pub fn on_offered_deadline_missed<F>(deadline: Duration, cb: F);
}
```

Async equivalents (`next_liveliness_change()` etc.) return `Future`, mirror message-future path.

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
    uint8_t  liveliness_kind;
    uint16_t depth;
    uint16_t _reserved0;

    /* ---- 108.B extensions (16 bytes). ---- */
    uint32_t deadline_ms;                /* 0 = infinite */
    uint32_t lifespan_ms;                /* 0 = infinite */
    uint32_t liveliness_lease_ms;        /* 0 = infinite */
    bool     avoid_ros_namespace_conventions;
    uint8_t  _reserved1[3];
} nros_rmw_qos_t;                        /* 24 bytes */
```

Sentinel `0` = "policy off / infinite" matches Phase 110 `OptUs` ABI convention.

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

Per registered event-callback: 8 B function ptr + 8 B context + 16-32 B status counters ≈ 32-48 B inline. Subscriber w/ liveliness + deadline + message-lost ≈ 96 B. Bounded; fits in executor arena. Apps not registering events pay zero.

### Wire-level requirements (108.B-driven)

| Policy | Backend mechanism |
|--------|-------------------|
| Lifespan | per-sample timestamp; subscriber compares to `now` |
| Liveliness | backend's keepalive (Zenoh tokens, DDS PARTICIPANT msgs, XRCE session pings) |
| Deadline | tracked at entity, no wire metadata |
| Durability TL | backend's late-joiner replay (Zenoh pub cache, DDS reader history) |

Each backend uses native attachment mechanism. nano-ros doesn't define a cross-backend attachment header.

### Cross-feature interaction

- **108.A events depend on 108.B QoS.** `register_event_callback(RequestedDeadlineMissed)` on a subscription with `deadline_ms = 0` is a no-op (`Err(IncompatibleQos)` since the policy isn't enabled).
- **Phase 105 `max_callbacks` interaction.** Event callbacks count against `max_callbacks_per_spin` like message callbacks. No special-casing.
- **Tier-2/3 events deferred.** `MATCHED`, `QOS_INCOMPATIBLE`, `INCOMPATIBLE_TYPE` not in API. `EventKind` is `#[non_exhaustive]`, additive.
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

108 lands the surface only. Per-backend wiring follows in numbered sub-phases (numbering reused after archive — concrete numbers TBD when 108 lands):

- dust-DDS event wiring (native; ~50 LOC)
- XRCE-DDS event wiring (native via uxr listener; ~50 LOC)
- zenoh-pico event wiring (shim-tracked, ~100 LOC)
- uORB event wiring (Tier-1 partial coverage; ~50 LOC)
- Per-backend QoS policy opt-ins (deadline, liveliness, lifespan, durability TL, namespace flag — flipped one bit at a time per backend)

### No upstream ABI compat

`nros_rmw_qos_t` ABI break is one-shot; pre-publish so no version-bump migration. Apps recompile against the new header; in-tree backends recompile cleanly because they don't honour any new policies yet (default mask = CORE).

### uORB QoS

uORB has limited QoS by design — intra-process pubsub, no wire-level reliability or durability. Adapted: `RELIABLE` always (queue-bounded), `VOLATILE` always, no deadline / lifespan / liveliness in the DDS sense. uORB's `supported_qos_policies()` returns `RELIABILITY | DURABILITY_VOLATILE | HISTORY | DEPTH` only.

### Zero default = policy off

Apps that don't care about deadline / lifespan / liveliness pay zero validation cost (mask only matters when policy explicitly requested w/ non-zero value). Sentinel `0` matches Phase 110 `OptUs` convention.

### Async path symmetry

Phase 99's `Subscription::recv().await` machinery extends to event-Futures; same waker registration, same drive path.

# Phase 108 — RMW status events: API + FFI surface (no backend wiring)

**Goal:** define the trait + C-vtable + user-facing surface for
transport-level status events (liveliness changes, deadline misses,
message loss) and document the design. Adopts a callback-on-entity
dispatch model rather than upstream's `rmw_event_t + rmw_take_event +
waitset` machinery — the events fire from inside the existing
`drive_io` callback-dispatch path. No backend implementations land in
this phase; backends advertise "supported / not supported" per event
kind via the trait, and `register_event_callback` returns
`Unsupported` until follow-up phases wire up specific backends.

**Status:** Not Started.
**Priority:** Medium. The API + FFI surface lets users start writing
code against status events; the book chapter lets contributors and
porters understand the design before any backend commits to wiring.
Actual backend wiring follows in per-backend phases.
**Depends on:** Phase 102 (typed entity structs), Phase 105
(`max_callbacks` cap; events count against it the same as message
callbacks).

## Background

Three classes of transport-level status are useful on RTOS:

| Event | Use case |
|-------|----------|
| **Liveliness changed (sub) / lost (pub)** | Safety-island fail-over: when a remote control node goes silent, trigger MRM. Drone bridge: detect PX4 commander stall. |
| **Deadline missed (sub / pub)** | Periodic-pubsub safety: 100 Hz sensor topic; if a sample doesn't arrive within deadline, alarm or fail-over. |
| **Message lost (sub)** | Slow-consumer diagnostic: ring buffer overflow signals the app to drop / coalesce / log. |

Three more (`MATCHED`, `QOS_INCOMPATIBLE`, `INCOMPATIBLE_TYPE`)
exist in upstream but are mostly diagnostic — fire once at startup
in static-topology embedded apps. Skip them for now; surface via
existing `nros_rmw_ret_t` codes at create-time instead. Re-evaluate
if dynamic-discovery apps appear.

The dispatch model follows nano-ros's existing message-callback
pattern: register callback at entity construction; callback fires
from inside `drive_io` when the backend's RX worker detects the
event. No new dispatch machinery; events are just another kind of
work `drive_io` drains.

## Design

### Rust trait surface

```rust
// packages/core/nros-rmw/src/event.rs (new)

pub enum EventKind {
    /// Subscriber: a tracked publisher's liveliness state changed.
    LivelinessChanged,
    /// Subscriber: an expected sample didn't arrive within the
    /// configured deadline.
    RequestedDeadlineMissed,
    /// Subscriber: backend dropped a sample (overflow, etc.).
    MessageLost,
    /// Publisher: I missed my own liveliness assertion.
    LivelinessLost,
    /// Publisher: I promised X Hz, fell behind.
    OfferedDeadlineMissed,
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
    LivelinessLost(&'a DeadlineMissedStatus),  // shape-compatible
    OfferedDeadlineMissed(&'a DeadlineMissedStatus),
}
```

Trait extension on `Subscriber` and `Publisher`:

```rust
pub trait Subscriber {
    /* existing methods … */

    /// `true` if the backend can generate this event for this
    /// subscriber. Default returns `false` — backends override per
    /// event kind they support.
    fn supports_event(&self, _kind: EventKind) -> bool { false }

    /// Register a callback to fire when the named event occurs.
    /// `deadline_ms` is consulted for `RequestedDeadlineMissed`
    /// events only; ignored otherwise.
    ///
    /// Returns `Err(Unsupported)` if the backend doesn't generate
    /// this event for this subscriber.
    fn register_event_callback(
        &mut self,
        kind: EventKind,
        deadline_ms: u32,
        cb: EventCallback,
    ) -> Result<(), Self::Error>;
}

pub type EventCallback = alloc::boxed::Box<dyn FnMut(EventPayload<'_>) + Send>;
```

Same shape on `Publisher` (with the publisher-side event kinds).

Default trait-method implementations: `supports_event` returns
`false`; `register_event_callback` returns
`Err(Unsupported)`. Backends override only for events they generate.

### C vtable extension

```c
// <nros/rmw_event.h> (new)

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
/* Used for MessageLost, RequestedDeadlineMissed, LivelinessLost,
 * OfferedDeadlineMissed — same shape. */

typedef union nros_rmw_event_payload_t {
    nros_rmw_liveliness_changed_status_t liveliness_changed;
    nros_rmw_count_status_t              count;
} nros_rmw_event_payload_t;

/** User callback invoked when an event fires.
 *  `kind` identifies which member of `payload` is valid. */
typedef void (*nros_rmw_event_callback_t)(
    nros_rmw_event_kind_t kind,
    const nros_rmw_event_payload_t *payload,
    void *user_context);
```

Vtable extension:

```c
typedef struct nros_rmw_vtable_t {
    /* existing entries … */

    /** Optional. NULL = backend doesn't generate any events.
     *  Returns NROS_RMW_RET_UNSUPPORTED if the specific kind isn't
     *  supported by this backend for this entity. */
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

NULL function pointer = "no events." Specific-kind unsupported on a
backend that does support some events = `NROS_RMW_RET_UNSUPPORTED`
return.

### User-facing API on `nros-node`

```rust
// packages/core/nros-node/src/subscription.rs

impl<M: Message> Subscription<M> {
    /// Register a callback that fires when a tracked publisher's
    /// liveliness changes. Returns `Err(Unsupported)` if the active
    /// backend doesn't track liveliness.
    pub fn on_liveliness_changed<F>(&mut self, cb: F) -> Result<()>
    where
        F: FnMut(LivelinessChangedStatus) + Send + 'static;

    /// Register a callback that fires when an expected message
    /// doesn't arrive within `deadline`.
    pub fn on_requested_deadline_missed<F>(
        &mut self,
        deadline: core::time::Duration,
        cb: F,
    ) -> Result<()>
    where
        F: FnMut(DeadlineMissedStatus) + Send + 'static;

    /// Register a callback that fires when the backend drops a
    /// sample (overflow, etc.).
    pub fn on_message_lost<F>(&mut self, cb: F) -> Result<()>
    where
        F: FnMut(MessageLostStatus) + Send + 'static;
}

impl<M: Message> Publisher<M> {
    pub fn on_liveliness_lost<F>(...);
    pub fn on_offered_deadline_missed<F>(deadline: Duration, cb: F);
}
```

Async equivalents return `Future`:

```rust
impl<M: Message> Subscription<M> {
    pub async fn next_liveliness_change(&mut self) -> LivelinessChangedStatus;
    pub async fn next_deadline_miss(&mut self) -> DeadlineMissedStatus;
}
```

C user-facing API (`nros-c`):

```c
typedef void (*nros_event_liveliness_changed_cb_t)(
    nros_subscription_t *sub,
    nros_liveliness_changed_status_t status,
    void *user_context);

nros_ret_t nros_subscription_set_liveliness_changed_callback(
    nros_subscription_t *sub,
    nros_event_liveliness_changed_cb_t cb,
    void *user_context);

/* Same shape for the other four events. Returns
 * NROS_RMW_RET_UNSUPPORTED if active backend doesn't generate. */
```

C++ thin wrapper in `nros-cpp`:

```cpp
namespace nros {
template <typename M> class Subscription {
    /* existing … */
    Result on_liveliness_changed(std::function<void(LivelinessChangedStatus)> cb);
    Result on_requested_deadline_missed(std::chrono::milliseconds deadline,
                                         std::function<void(DeadlineMissedStatus)> cb);
    Result on_message_lost(std::function<void(MessageLostStatus)> cb);
};
}
```

`std::function` overloads only with `NROS_CPP_STD` — same pattern as
existing `on_message` callback.

### Why callbacks, not waitset-take

Upstream uses `rmw_event_t` handles in a waitset; `rmw_wait` returns
when an event fires; `rmw_take_event` pulls the payload. Two-phase,
per-call.

Adopting that pattern would require a waitset abstraction we
deliberately don't have (see [RMW vs upstream](../../book/src/design/rmw-vs-upstream.md)
Section 4). Replacing it with callback-on-entity:

- Reuses the existing `drive_io` callback dispatch path. Backend's
  RX worker detects the event in the same place it detects
  messages; runs the registered callback; loops.
- Counts against Phase 105's `max_callbacks` cap automatically —
  events are just another callback source.
- Matches the message-callback ergonomics users already know.
- Keeps the bounded-storage property: each event subscription is a
  fixed-size struct embedded in the entity, no per-call allocation.

Trade-off: users can't bulk-poll all events at once (the way
`rmw_wait` returns "any of these are ready"). For the Tier-1 events
this isn't load-bearing — events are rare, callbacks are cheap.

### Storage

Each registered event-callback holds:
- One `nros_rmw_event_callback_t` function pointer (8 bytes)
- One user-context `void *` (8 bytes)
- Backend's status counters (16–32 bytes per event kind)

A subscriber that wants liveliness + deadline + message-lost
callbacks pays ~96 bytes of inline storage. Bounded; fits in the
executor arena. Apps that don't register events pay zero.

## Work Items

- [ ] **108.1 — Rust trait + payload types.**
      `nros-rmw` crate: new `event.rs` module with `EventKind` enum,
      `LivelinessChangedStatus` / `DeadlineMissedStatus` /
      `MessageLostStatus` payloads, `EventPayload<'a>` borrow-shaped
      union, `EventCallback` boxed closure typedef. Trait extensions
      on `Subscriber` and `Publisher` with default `Unsupported`
      impls.
      **Files:** `packages/core/nros-rmw/src/event.rs` (new),
      `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-rmw/src/lib.rs`.

- [ ] **108.2 — C vtable + payload header.**
      New `<nros/rmw_event.h>` with `nros_rmw_event_kind_t` enum,
      payload structs, `nros_rmw_event_callback_t` typedef. Vtable
      extension: two new optional function pointers
      (`register_subscriber_event`, `register_publisher_event`).
      Doxygen docs cover lifetime + threading + dispatch context.
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_event.h`
      (new), `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/Doxyfile`.

- [ ] **108.3 — Rust mirror in `nros-rmw-cffi`.**
      `#[repr(C)]` mirrors of the C event types
      (`NrosRmwEventKind`, `NrosRmwLivelinessChangedStatus`,
      `NrosRmwCountStatus`). `NrosRmwVtable` gets the two new
      function pointers. `CffiSubscriber` /  `CffiPublisher` thread
      `register_event_callback` calls through.
      **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **108.4 — User-facing API on `nros-node`.**
      `Subscription<M>::on_liveliness_changed` /
      `on_requested_deadline_missed` / `on_message_lost`.
      `Publisher<M>::on_liveliness_lost` /
      `on_offered_deadline_missed`. Async `next_*` Future variants.
      Each method returns `Err(Unsupported)` until backend wiring
      lands.
      **Files:** `packages/core/nros-node/src/subscription.rs`,
      `packages/core/nros-node/src/publisher.rs`.

- [ ] **108.5 — C / C++ thin wrappers.**
      `nros-c`: `nros_subscription_set_liveliness_changed_callback`
      and the four other `set_*_callback` calls. `nros-cpp`:
      `nros::Subscription<M>::on_liveliness_changed` etc., with
      `std::function` overloads under `NROS_CPP_STD`. cbindgen
      regenerates the C header from the Rust side.
      **Files:** `packages/core/nros-c/src/`,
      `packages/core/nros-cpp/include/nros/`,
      `packages/core/nros-cpp/src/`.

- [ ] **108.6 — Book chapter.**
      New `book/src/concepts/status-events.md` covers:
      - The five Tier-1 event kinds + use cases
      - The callback-on-entity dispatch model + why it differs
        from upstream `rmw_event_t + waitset`
      - Per-backend support matrix (dust-DDS / XRCE-DDS native;
        zenoh-pico tracked at shim; uORB partial)
      - Tier-2 / Tier-3 events deliberately skipped + rationale
      - Per-RTOS recommendations: drone-bridge fail-over example,
        100 Hz sensor deadline pattern
      Cross-link from `book/src/design/rmw-vs-upstream.md` Section 8
      (DDS event API). SUMMARY.md updated.

- [ ] **108.7 — Update `rmw-vs-upstream.md` Section 8.**
      The "DDS event API — present vs absent" section currently
      reads "absent." Update to: "present in callback form, Tier-1
      subset" with the rationale for skipping Tier-2 / Tier-3 and
      for callback-on-entity vs waitset-take.

## Acceptance Criteria

- [ ] `cargo build -p nros-rmw -p nros-rmw-cffi -p nros-node -p nros`
      clean.
- [ ] `cargo build -p nros-c -p nros-cpp` clean.
- [ ] cbindgen regenerates `nros_generated.h` with the new event
      callback typedefs.
- [ ] Doxygen `<nros/rmw_event.h>` site renders clean.
- [ ] `mdbook build` clean.
- [ ] Calling `subscription.on_liveliness_changed(...)` on every
      backend in the workspace returns `Err(Unsupported)` (since no
      backend wiring lands in this phase).
- [ ] No regression on any existing test.

## Notes

- **Backend wiring is per-phase per-backend.** Once 108 lands the
  surface, follow-up phases pick up backends one at a time:
  - Phase 109 — dust-DDS event wiring (native; ~50 LOC).
  - Phase 110 — XRCE-DDS event wiring (native via uxr listener; ~50 LOC).
  - Phase 111 — zenoh-pico event wiring (shim-tracked, ~100 LOC).
  - Phase 112 — uORB event wiring (Tier-1 partial coverage; ~50 LOC).
  These phases re-evaluate in sequence; doc-only commits don't block.
- **No upstream `rmw_event_t` shape compatibility.** We deliberately
  ship a different API. Apps porting from rclcpp need to rewrite
  event-handling code; the message-callback path stays compatible.
- **Async path mirrors the message path.** Phase 99's
  `Subscription::recv().await` machinery extends to event-Futures;
  same waker registration, same drive path.
- **Tier-2 / Tier-3 events.** `MATCHED`, `QOS_INCOMPATIBLE`,
  `INCOMPATIBLE_TYPE` are intentionally not in the API. If a use
  case shows up, add the kind + payload + callback method —
  additive, no ABI break (`EventKind` enum is `#[non_exhaustive]`,
  C side is integer-valued so unknown values pass through).
- **Phase 105 interaction.** Event callbacks count against
  `max_callbacks_per_spin` the same as message callbacks. No
  special-casing in Phase 105's executor logic.

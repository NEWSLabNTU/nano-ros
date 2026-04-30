# Phase 109 — Full DDS-shaped QoS profile: API surface (no backend wiring)

**Goal:** extend the entity-creation QoS surface to carry the full
DDS `rmw_qos_profile_t` field set — deadline, lifespan, liveliness,
and the namespace-convention flag in addition to the existing
reliability / durability / history / depth subset. Backends advertise
per-policy support; until per-backend wiring lands (Phase 110+),
backends that can't honour a requested policy return
`NROS_RMW_RET_INCOMPATIBLE_QOS` at create time.

This phase is **API + book + Rust runtime only**. Per-backend
implementation lands in follow-on phases:

* Phase 110 — deadline QoS wiring (zenoh-pico, XRCE-DDS, dust-DDS, uORB)
* Phase 111 — liveliness QoS wiring + `Publisher::assert_liveliness()`
* Phase 112 — durability TRANSIENT_LOCAL + lifespan wiring
* Phase 113 — `avoid_ros_namespace_conventions` topic-name encoding

**Status:** Not Started.
**Priority:** Medium. Matches user-facing API to upstream's
`rmw_qos_profile_t` so ROS 2 apps port cleanly. Backend wiring is a
follow-up; the API alone is useful immediately because applications
can construct full-shaped QoS profiles and the runtime returns
`IncompatibleQos` synchronously when the active backend can't honour
them — no silent degradation.

**Depends on:** Phase 102 (typed entity structs, `nros_rmw_ret_t`),
Phase 108 (status events surface — deadline / liveliness fire as
events).

## Background

Today's `nros_rmw_qos_t` carries a deliberate subset of `rmw_qos_profile_t`:

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

The book's `rmw-vs-upstream.md` Section 7 explicitly framed this as a
"minimal subset, not full DDS profiles" choice. Two factors changed
the calculus:

1. **`rmw_zenoh_cpp` proved every DDS policy is implementable on
   non-DDS backends.** It maps reliability → zenoh congestion-control,
   durability TL → zenoh publication cache, deadline → shim timer,
   lifespan → sample metadata + filter, liveliness → zenoh liveliness
   tokens. Bounded cost per policy; nothing exotic.

2. **Real RTOS use cases need them.** Drone bridge needs liveliness
   for fail-over. 100 Hz sensor safety apps need deadline. Slow-
   consumer apps need lifespan + message-lost (Phase 108 already
   covers the event side; need the QoS side too).

So the "subset" framing is no longer load-bearing. Surface the full
shape; let backends opt in per-policy.

## Design

### New `nros_rmw_qos_t` shape (24 bytes)

```c
typedef enum nros_rmw_liveliness_kind_t {
    /** No liveliness assertion / tracking. Default. */
    NROS_RMW_LIVELINESS_NONE              = 0,
    /** Backend's keepalive task asserts liveliness automatically. */
    NROS_RMW_LIVELINESS_AUTOMATIC         = 1,
    /** Application calls `assert_liveliness()` per topic explicitly. */
    NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC   = 2,
    /** Application calls `assert_liveliness()` at the node level. */
    NROS_RMW_LIVELINESS_MANUAL_BY_NODE    = 3,
} nros_rmw_liveliness_kind_t;

typedef struct nros_rmw_qos_t {
    /* ---- Existing 8-byte subset, layout-preserved. ---- */
    uint8_t  reliability;        /* NROS_RMW_RELIABILITY_* */
    uint8_t  durability;         /* NROS_RMW_DURABILITY_*  */
    uint8_t  history;            /* NROS_RMW_HISTORY_*     */
    uint8_t  liveliness_kind;    /* NROS_RMW_LIVELINESS_*  */
    uint16_t depth;
    uint16_t _reserved0;

    /* ---- Phase 109 extensions (16 bytes). ---- */
    /** Subscriber: max acceptable inter-arrival.
     *  Publisher: max acceptable inter-publish (offered rate).
     *  0 = no deadline (default; effectively infinite). */
    uint32_t deadline_ms;

    /** Sample expiry. Subscriber filters samples older than this.
     *  0 = infinite (default). */
    uint32_t lifespan_ms;

    /** Liveliness lease. Publisher must refresh within this period
     *  or be considered dead.
     *  0 = infinite (default). */
    uint32_t liveliness_lease_ms;

    /** If `true`, topic name encoding skips the `/rt/` ROS prefix
     *  and uses raw application names. Matches upstream semantics. */
    bool     avoid_ros_namespace_conventions;
    uint8_t  _reserved1[3];
} nros_rmw_qos_t;                  /* 24 bytes */
```

The first 8 bytes remain layout-equivalent to the current struct so
intermediate work-in-progress builds compile (the new fields default
to zero = "policy off / infinite"). Once Phase 109 lands the layout
is fixed at 24 bytes.

`liveliness_kind` reuses the spare byte at offset 3 (was `_reserved0`
in the previous shape); `liveliness_kind = 0` (NONE) preserves the
"no liveliness" default for apps that don't set it.

### Default profile (matching `rmw_qos_profile_default`)

```rust
pub const NROS_RMW_QOS_PROFILE_DEFAULT: NrosRmwQos = NrosRmwQos {
    reliability: NROS_RMW_RELIABILITY_RELIABLE,
    durability: NROS_RMW_DURABILITY_VOLATILE,
    history: NROS_RMW_HISTORY_KEEP_LAST,
    liveliness_kind: NROS_RMW_LIVELINESS_AUTOMATIC,
    _reserved0: 0,
    depth: 10,
    _reserved1_inner: 0,
    deadline_ms: 0,                          // infinite
    lifespan_ms: 0,                          // infinite
    liveliness_lease_ms: 0,                  // infinite
    avoid_ros_namespace_conventions: false,
    _reserved1: [0; 3],
};
```

Plus the standard ROS 2 profiles (sensor data, services, system
default, parameters) as named constants matching upstream. ROS apps
that pull `rmw_qos_profile_sensor_data` get the same effective
profile here.

### Compatibility convention

Backends advertise per-policy support via a new method:

```rust
pub trait Session {
    /* existing methods … */

    /// Report which QoS policies the backend actually honours.
    /// Returned bitmask uses [`QosPolicyMask`] flags.
    /// Default: `core` — only reliability + durability=VOLATILE +
    /// history + depth.
    fn supported_qos_policies(&self) -> QosPolicyMask {
        QosPolicyMask::CORE
    }
}

bitflags! {
    pub struct QosPolicyMask: u32 {
        const RELIABILITY        = 1 << 0;
        const DURABILITY_VOLATILE = 1 << 1;
        const DURABILITY_TRANSIENT_LOCAL = 1 << 2;
        const HISTORY            = 1 << 3;
        const DEPTH              = 1 << 4;
        const DEADLINE           = 1 << 5;
        const LIFESPAN           = 1 << 6;
        const LIVELINESS_AUTOMATIC = 1 << 7;
        const LIVELINESS_MANUAL_BY_TOPIC = 1 << 8;
        const LIVELINESS_MANUAL_BY_NODE  = 1 << 9;
        const LIVELINESS_LEASE   = 1 << 10;
        const AVOID_ROS_NAMESPACE_CONVENTIONS = 1 << 11;

        const CORE = Self::RELIABILITY.bits()
                   | Self::DURABILITY_VOLATILE.bits()
                   | Self::HISTORY.bits()
                   | Self::DEPTH.bits();
    }
}
```

`Session::create_publisher` / `create_subscriber` validate the
requested QoS against the supported mask. If the request specifies
a policy the backend doesn't support (`deadline_ms != 0` on a
backend without `DEADLINE` in its mask, etc.), the call returns
`Err(TransportError::IncompatibleQos)` synchronously. **No silent
degradation** — the runtime never quietly "downgrades" a requested
policy.

C side: same check happens in the cffi `register_*_event` /
`create_*` paths, returns `NROS_RMW_RET_INCOMPATIBLE_QOS`.

### Until backend wiring lands

This phase ships only the API. No backend's `supported_qos_policies`
returns anything beyond `CORE`. Apps requesting deadline / lifespan /
liveliness / TL durability / namespace-conventions flag get
`IncompatibleQos` until the backend phase lands.

Backends gradually opt in:
* Phase 110: zenoh-pico + dust-DDS + XRCE-DDS gain `DEADLINE`.
* Phase 111: backends that can do liveliness gain the four
  `LIVELINESS_*` flags.
* Phase 112: backends gain `DURABILITY_TRANSIENT_LOCAL` and
  `LIFESPAN`.
* Phase 113: namespace-conventions flag.

Apps that test their backend's support at startup can call
`session.supported_qos_policies().contains(QosPolicyMask::DEADLINE)`
and either request the policy or fall back.

### Liveliness API surface

For `MANUAL_BY_TOPIC` and `MANUAL_BY_NODE`, the application must
explicitly assert liveliness. Today no API for that exists.

```rust
impl<M: Message> Publisher<M> {
    /* existing methods … */

    /// Assert this publisher's liveliness manually. Required for
    /// publishers configured with
    /// `LivelinessKind::ManualByTopic`. No-op (returns `Ok(())`) for
    /// other liveliness kinds. Returns `Err(IncompatibleQos)` if
    /// the active backend doesn't support manual liveliness.
    pub fn assert_liveliness(&self) -> Result<(), NodeError>;
}
```

C side adds `nros_publisher_assert_liveliness(pub) -> nros_ret_t`.
C++ adds `Publisher<M>::assert_liveliness()`.

### Wire-level requirements

Three policies need wire-level metadata that backends carry in their
own attachment / sample-info:

* **Lifespan**: per-sample timestamp; subscriber compares to `now`.
* **Liveliness**: backend's keepalive mechanism (Zenoh tokens, DDS
  PARTICIPANT messages, XRCE session pings).
* **Deadline**: tracked at the entity, no wire metadata needed.
* **Durability TL**: backend's late-joiner replay (Zenoh pub cache,
  DDS DataReader history, etc.).

Each backend handles its own metadata using its native attachment
mechanism. nano-ros doesn't define a cross-backend attachment
header — backends are free to choose what's most efficient on their
transport.

### What's deliberately not on the entity struct

Per-policy bookkeeping state lives in the backend's `backend_data`
slot, not in the visible entity struct. The struct carries the QoS
profile (configuration) — it does NOT carry runtime state like
"last_received_at," "deadline timer fd," or "cache contents."

Rationale: state lifetime is per-creation; layout is per-create
config. Keeping them separate lets the entity struct stay small
(24-byte QoS + the struct's other fields ≈ 56 bytes total) while
backends grow `backend_data` freely.

## Work Items

- [ ] **109.1 — Update C header `<nros/rmw_entity.h>`.**
      Extend `nros_rmw_qos_t` to 24 bytes per the design above. Add
      `nros_rmw_liveliness_kind_t` enum. Add named macro constants
      for the standard QoS profiles
      (`NROS_RMW_QOS_PROFILE_DEFAULT`,
      `NROS_RMW_QOS_PROFILE_SENSOR_DATA`,
      `NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT`,
      `NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT`,
      `NROS_RMW_QOS_PROFILE_PARAMETERS`).
      Doxygen on each new field.
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_entity.h`.

- [ ] **109.2 — Update Rust mirror in `nros-rmw-cffi`.**
      `NrosRmwQos` grows to match. `LivelinessKind` enum.
      `pub const`s for the standard profiles. `From<QosSettings> for
      NrosRmwQos` extended.
      **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **109.3 — Update `nros-rmw` `QosSettings` + add `QosPolicyMask`.**
      Extend `QosSettings` with `deadline`, `lifespan`,
      `liveliness_kind`, `liveliness_lease_duration`,
      `avoid_ros_namespace_conventions` fields. Default values
      preserve current semantics (zero = "off"). Add
      `QosPolicyMask` bitflags. Add
      `Session::supported_qos_policies()` trait method with default
      returning `CORE`.
      **Files:** `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-rmw/src/lib.rs`.

- [ ] **109.4 — `Session::create_*` validates QoS against mask.**
      Default-implemented validation: if requested policy isn't in
      the backend's `supported_qos_policies()`, return
      `IncompatibleQos` synchronously. No silent downgrade.
      **Files:** `packages/core/nros-rmw/src/traits.rs`.

- [ ] **109.5 — `Publisher::assert_liveliness()` trait method.**
      Default impl returns `Ok(())` (no-op). Backends override when
      they support manual liveliness. C vtable adds optional
      `assert_publisher_liveliness` function pointer (NULL = no-op).
      **Files:** `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **109.6 — `nros-node` user-facing surface.**
      `Publisher<M>::assert_liveliness()`. Subscription/Publisher
      `create_*_with_qos` accepts the extended QosSettings.
      QoS profile constants re-exported at `nros::qos::*` for
      ergonomic use:
      ```rust
      let pub = node.create_publisher_with_qos::<MyMsg>(
          "/topic",
          nros::qos::SENSOR_DATA,
      )?;
      ```
      **Files:** `packages/core/nros-node/src/`,
      `packages/core/nros/src/lib.rs`.

- [ ] **109.7 — C / C++ user-facing wrappers.**
      `nros-c` extends `nros_qos_t` to match. `nros-cpp` extends
      `nros::QoS` builder + adds `Publisher<M>::assert_liveliness()`.
      Standard profile constants exposed in both languages.
      **Files:** `packages/core/nros-c/src/qos.rs`,
      `packages/core/nros-c/include/nros/types.h`,
      `packages/core/nros-cpp/include/nros/qos.hpp`,
      `packages/core/nros-cpp/src/`.

- [ ] **109.8 — Book + Doxygen updates.**
      Rewrite `book/src/design/rmw-vs-upstream.md` Section 7 — was
      "QoS subset, not full DDS profiles"; now describes full DDS
      QoS surface with the per-backend `supported_qos_policies()`
      mask, the synchronous `IncompatibleQos` check on create,
      and the `assert_liveliness()` API.
      Update `book/src/concepts/ros2-comparison.md` "QoS subset"
      paragraph similarly.
      Update `book/src/concepts/status-events.md` to cross-link
      QoS-driven event sources (deadline, liveliness).
      Doxygen on the new C header content.
      **Files:** `book/src/design/rmw-vs-upstream.md`,
      `book/src/concepts/ros2-comparison.md`,
      `book/src/concepts/status-events.md`,
      `packages/core/nros-rmw-cffi/Doxyfile` (no change; existing
      header pickup).

## Acceptance Criteria

- [ ] `cargo build -p nros-rmw -p nros-rmw-cffi -p nros-node -p nros
      -p nros-c -p nros-cpp` clean.
- [ ] `cargo test -p nros-rmw-cffi --lib`
      `tests::typed_struct_roundtrip` passes after struct grows
      to 24 bytes (test asserts that the QoS struct round-trips
      through the C boundary).
- [ ] All standard QoS profile constants
      (`NROS_RMW_QOS_PROFILE_DEFAULT` etc.) match the equivalent
      upstream `rmw_qos_profile_*` field-by-field.
- [ ] Calling `node.create_publisher_with_qos(topic,
      nros::qos::SENSOR_DATA)` against any in-tree backend returns
      `Err(NodeError::Transport(IncompatibleQos))` because no
      backend has wired up `DEADLINE` / `LIFESPAN` /
      `LIVELINESS_*` yet (this is the "no silent degradation"
      contract; verified by per-backend integration tests in 110+
      that flip individual flags on).
- [ ] No regression on existing `nros::qos::DEFAULT` paths (which
      use only CORE policies).
- [ ] Book + Doxygen build clean.

## Notes

- **No backward compatibility shim.** The `nros_rmw_qos_t` ABI
  break is one-shot; pre-publish so no version-bump migration.
  Apps recompile against the new header; the in-tree four backends
  recompile cleanly because they don't honour any of the new
  policies yet (default mask = CORE).
- **Why not split into `qos_basic` + `qos_extended`?** Path B from
  the design discussion. Rejected per user direction — keep the
  upstream-shaped struct so ROS 2 apps and porters see one
  familiar type.
- **Why default policies = "off"?** Zero values mean "no deadline,"
  "no lifespan," etc. Apps that don't care about those policies
  pay no validation cost (the backend's `supported_qos_policies`
  mask only matters when a policy is explicitly requested with a
  non-zero value).
- **Phase 110+ phasing.** Each per-backend wiring phase flips
  specific bits in that backend's `supported_qos_policies` mask
  and implements the policy. Apps that need a specific policy can
  watch the phase docs to know when their backend supports it.
- **uORB has limited QoS coverage** by design — intra-process
  pubsub doesn't have wire-level reliability or durability.
  Adapted semantics: `RELIABLE` always (queue-bounded), `VOLATILE`
  always, no deadline / lifespan / liveliness in the DDS sense.
  uORB's `supported_qos_policies()` returns
  `RELIABILITY | DURABILITY_VOLATILE | HISTORY | DEPTH` only.
- **Status-event interaction.** Phase 108's deadline / liveliness
  events only fire if the corresponding QoS policy is set on the
  entity. `register_event_callback(RequestedDeadlineMissed)` on a
  subscription with `deadline_ms = 0` is a no-op
  (`Err(IncompatibleQos)` since the policy isn't enabled).

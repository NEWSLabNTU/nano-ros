---
id: 1164
title: "a Cyclone application cannot observe a status event through either half of the surface — `*_event_init` is NULL and `*_take_event` has no caller"
status: resolved
type: bug
area: rmw
severity: medium
found: 2026-09-06
related: [0800, 1137]
---

# Both halves are missing, and each one alone would be enough

`packages/rmw/cyclonedds/*/src/vtable.cpp:258`:

```cpp
// Phase 108 event hooks left NULL until a follow-up phase wires
// Cyclone listeners through to the runtime's status-event surface.
constexpr rmw_ret_t (*kRegisterSubscriptionEvent)(...) = nullptr;
constexpr rmw_ret_t (*kRegisterPublisherEvent)(...)    = nullptr;
```

Both are installed into `kVtable` as those NULL constants. So on Cyclone there
is **no way to register** a status-event callback.

The other half is `*_take_event`, which Cyclone *does* implement — it reads real
`dds_get_liveliness_changed_status` counters. But it has **no caller anywhere in
the tree** outside cyclonedds' own `tests/status_events.cpp`: the Rust adapter
hardcodes both slots to `None`, and `nros-node` exposes no poll API. So the
implemented half is unreachable too.

Either gap alone would make status events unobservable on Cyclone. Both are
present.

## Why `check-rmw-slot-producers` still says `produced`

Because zenoh fills the slots. `produced` is a statement about the SLOT, not
about a backend — which is the overstatement issue 0800 measured and the reason
the gate now prints the split. This issue is the per-backend version of that
question, and it argues the split should go one level further: a slot filled by
one backend and NULL in another is not the same thing as a slot that works, and
today nothing distinguishes them.

Note the same shape one family over: issue 1137 found Cyclone's graph reader
answering `Unsupported` on nine of twelve `produced` slots, live.

## Not a live-peer finding, deliberately

phase-433 W6 set out to make a QoS event fire against a stock ROS 2 peer and
succeeded on zenoh (`qos_event_interop.rs`). Cyclone is recorded there as
`native-qos-event-rust-cyclone-r2n-CARVED` rather than as a failing cell,
because **a live peer changes nothing about this**: the missing piece is a
registration path and a runtime poll path on our side. A test would only ever
report the absence we already know from reading the source.

## Fix

Two independent pieces, and the first is cheap:

1. Wire `*_take_event` to a poll path — the implementation exists and reads real
   counters; what is missing is a caller and an `nros-node` surface.
2. Implement `*_event_init` over Cyclone listeners, which is what the phase-108
   comment deferred.

Either makes Cyclone status events observable for the first time. Doing both
makes the surface match zenoh's.

## Resolved 2026-09-09 — by the FIRST piece only, and the second is now a decision

The headline claim is no longer true: a Cyclone application can observe a status
event, through the same `on_*` API it uses on zenoh.

**What landed** is piece 1, in `8b8fdb2cf` (*"the poll half of the status-event
surface had no caller"*). `nros-rmw-cffi` records a `register_event_callback`
whose backend has no `*_event_init`, drains the poll slot from the entity's
ordinary data path, and fires the callback on the caller's own thread. The two
claims that had to hold are proved in two different places, deliberately:

* *the backend's `take_event` reports a counter the DDS stack actually moved* —
  `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/status_events.cpp`, which
  provokes a real `REQUESTED_DEADLINE_MISSED` out of a live Cyclone reader and
  fails if the change count is zero.
* *the runtime polls that slot and delivers to the registered callback* —
  `packages/rmw/cffi/tests/take_event_poll.rs`, against a stub written to
  Cyclone's contract, because what is under test is the wiring above the slot.

**What did NOT land** is piece 2, `*_event_init` over Cyclone listeners. Both
constants are still `nullptr` — and that is now a recorded decision rather than
the phase-108 deferral this issue was filed about. The reason is at the slot
(`vtable.cpp`): Cyclone's `drive_io` is a sleep, so it has nowhere safe to call
a callback from, and the runtime's poll path supplies the same observable
behaviour without one. The note there also states its own scope — it covers the
two `*_event_init` constants and nothing else, which is what issue 1231 was
filed about when `kAssertPublisherLiveliness` sat under it as an unexplained
third NULL.

So this issue closes on its own stated terms: *"Either makes Cyclone status
events observable for the first time."* Listener-based events remain a
legitimate thing to want — they would drop the poll latency — but that is a new
argument for a new issue, not this one's remainder.

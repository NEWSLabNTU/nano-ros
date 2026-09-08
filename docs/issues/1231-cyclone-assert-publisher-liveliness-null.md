---
id: 1231
title: "Cyclone's `publisher_assert_liveliness` is `nullptr` with no recorded reason, while zenoh implements it"
status: open
type: bug
area: rmw
severity: low
found: 2026-09-08
related: [1164, 0800]
---

# A NULL slot that never said why

`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp:282`:

```cpp
constexpr rmw_ret_t (*kAssertPublisherLiveliness)(
    const rmw_publisher_t *) = nullptr;
```

It sits in the same block as the two `*_event_init` hooks, under one shared
comment that explains only those:

> Phase 108 event hooks left NULL until a follow-up phase wires Cyclone
> listeners through to the runtime's status-event surface.

`publisher_assert_liveliness` is not an event hook. It is the publisher-side
half of MANUAL_BY_TOPIC liveliness — the call an application makes to say "I am
still here" without publishing — and the comment above it does not cover it.
Issue 1164's work established the reason for the event hooks (Cyclone has no
safe callback context, so wiring listeners needs a lock issue 0780 removed);
that argument says nothing about this slot, which needs no callback at all.

**zenoh implements it.** `nros-rmw-zenoh/src/shim/publisher.rs:416`,
`fn assert_liveliness(&self)`, with a tracked last-assertion timestamp at :58.
So this is a per-backend gap, not an ABI-wide decline.

## Why it is low severity and still worth a file

Nothing in the tree calls it on Cyclone, so no application is currently broken
by it. What is wrong is the RECORD: a `nullptr` with no per-slot reason is
indistinguishable from an oversight, and this campaign has now found three
separate cases where an unexplained absence was read as a decision (issues
1137, 1164, and the `produced`-vs-per-backend confusion in
`check-rmw-slot-producers`).

Either state applies:

* **It is deliberate** — Cyclone's `dds_assert_liveliness` exists in the pinned
  0.10.5, so a reason would have to be about our layer, not the DDS one. Write
  it at the slot, the way the declined RMW symbols carry theirs.
* **It is an oversight** — implement it. `dds_assert_liveliness(entity)` is a
  direct mapping and the zenoh side shows the shape.

## What has NOT been established

Whether `dds_assert_liveliness` behaves usefully under our writer setup, and
whether any consumer wants MANUAL_BY_TOPIC on Cyclone at all. Both are reasons
the answer might legitimately be "declined" — but neither is written down, and
that is the defect this issue names.

Found while auditing what phase-433 left behind; the slot was noted in passing
by issue 1164's work as "a separate gap" and had no file.

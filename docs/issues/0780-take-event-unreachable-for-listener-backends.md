---
id: 780
title: "`rmw_take_event` is declined on a premise this ABI contradicts, and the
  one backend that would need it has no other way to deliver a status event"
status: open
type: bug
area: rmw
related: [phase-376, issue-0460]
---

## Problem

`rmw_take_event` is `declined` with two clauses. Both fail.

**Clause 1 — "upstream polls an event because the WAIT SET said one was ready,
and the wait set is declined here, so a poll would be blind."** This ABI is a
poll-without-a-wait-set design, by construction. `has_data`'s own doc says so:

> RTOS addition: upstream has no equivalent, because a hosted caller reaches
> for a wait-set. This is the poll a loop with no wait-set needs, and it
> allocates nothing.

A poll with no wait set is the model here, not a blindness.

**Clause 2 — "our callback already runs on the safe context, from inside
`drive_io` on the executor thread, never an ISR or a transport thread."** This
is a per-backend fact recorded ABI-wide — the same shape as the network-flow
decline that W5 already flipped.

- Only zenoh implements status events at all. Cyclone
  (`nros-rmw-cyclonedds/src/vtable.cpp`) and XRCE
  (`nros-rmw-xrce/src/vtable.c`) leave both `*_event_init` slots NULL.
- Zenoh does not do what the clause says: it fires from `try_recv_raw` and
  `has_data` (`zenoh/nros-rmw-zenoh/src/shim/subscriber.rs`), never from
  `drive_io`.
- The backend that breaks it is cyclone. DDS listeners fire on Cyclone's own
  worker thread, and its `drive_io` is a sleep with no callback path
  (`session.cpp` — the comment there claiming "listener trampolines wake the
  runtime's `Activator`" refers to code that does not exist; grepping
  `Activator` over that backend returns only the comment). A cyclone status
  event therefore has no safe context to defer onto, and buffer-plus-poll IS
  `take_event`.

## Why it matters

Cyclone is the backend a ROS-interop image uses, and QoS status events
(deadline missed, liveliness lost, incompatible QoS) are how an application
finds out its data flow broke. Today it cannot deliver one at all, and the ABI
records that absence as a design decision.

## Fix

A slot, NULL where a backend has no listener thread:

```c
rmw_ret_t (*subscription_take_event)(const rmw_subscription_t *subscription,
    rmw_event_type_t kind, rmw_event_payload_t *out, bool *taken);
```

plus the publisher twin, matching `*_event_init`'s split. No allocator, no
`rmw_event_t`. Cost is two pointers, NULL in four backends of five.

## Two contract sentences to fix in the same pass

Both currently make the decline sound like a contract rather than an
observation, and neither is true:

- `rmw_event.h` — "Invoked from inside `drive_io` on the executor thread." No
  backend does this.
- `rmw_vtable.h`, `has_data` — "A backend must not mutate subscription state
  here; the probe is logically read-only." Zenoh's `has_data` fires deadline
  and liveliness callbacks and writes cells.

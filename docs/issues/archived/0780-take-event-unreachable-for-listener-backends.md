---
id: 780
title: "`rmw_take_event` is declined on a premise this ABI contradicts, and the
  one backend that would need it has no other way to deliver a status event"
status: resolved
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

## Landed 2026-08-25 — the ABI half

Two slots, split per entity kind because upstream's `rmw_event_t` carries the
entity and ours is declined:

```c
rmw_ret_t (*subscription_take_event)(const rmw_subscription_t *subscription,
    rmw_event_type_t kind, rmw_event_payload_t *out, bool *taken);
rmw_ret_t (*publisher_take_event)(const rmw_publisher_t *publisher,
    rmw_event_type_t kind, rmw_event_payload_t *out, bool *taken);
```

`rmw_take_event` is recorded as GROUPED onto the subscription one;
`publisher_take_event` is its declared twin. Parity: declined 14 → 13,
contract 72 → 73, undeclared 0.

Both false clauses are now recorded AS false where the decision lives, so the
next reader meets the correction rather than the claim.

### The two contract sentences, fixed

* `rmw_event.h` said the callback is "invoked from inside `drive_io` on the
  executor thread". True of no backend, and load-bearing — it was the stated
  reason `take_event` could be declined. Replaced with what is actually
  guaranteed (nothing about the context) plus the consequence: a backend with
  no safe delivery context should leave `*_event_init` NULL and implement
  `*_take_event`.
* `rmw_vtable.h`'s `has_data` said "a backend must not mutate subscription
  state here — the probe is logically read-only". zenoh's fires callbacks and
  writes cells from inside it; cyclonedds' now peeks its reader. The rule that
  is actually keepable, and now recorded, is narrower: a probe may not CONSUME
  a message.

## Still open: cyclonedds has no status events at all

The slots are NULL in every backend, which for cyclonedds is honest rather than
lazy — it registers no DDS listeners, so it has nothing buffered to hand out.
Implementing means listener registration for the five kinds, a per-entity
buffer, and a drain in `take_event`. That is a feature, not a plumbing change,
and it is what actually closes this issue.

What the ABI change buys is that the feature is now a BACKEND change. Before
it, a cyclonedds status event had no route to a caller at any price.

## Found on the way: the positional-initialiser hazard, gated

Inserting two slots mid-header shifted every later entry of cyclonedds'
POSITIONAL vtable initialiser. The compiler caught it — but only because the
shifted function-pointer types disagreed. Adjacent slots with the same
signature (`destroy_service` / `destroy_client`, the two new `*_take_event`
slots, several `get_*` graph slots) would have swapped SILENTLY.

Issue 0773's write-up proposed exactly this check and deferred it. Added now as
`scripts/check-vtable-positional-order.py` (`just check-rmw-vtable-order`, on
the fast line): the `/*slot*/` comment sequence must be an ordered subsequence
of the header's field order — subsequence because an initialiser may stop early
and let C++ zero-fill, ordered because it may never name them out of sequence.
Proven to fire by swapping two adjacent entries.

## Resolved 2026-08-25 — cyclonedds delivers status events, by POLLING

`subscription_take_event` and `publisher_take_event` are implemented in the
cyclonedds backend. No listeners, no buffer, no lock.

Cyclone offers both listeners and status getters, and for this backend the
getters are strictly better:

* **`dds_get_*_status` RESETS its `*_change` counters as it reads them.** That
  IS take semantics. The event is consumed by the read, so there is nothing to
  buffer and no depth to bound.
* **A listener would fire on Cyclone's own worker thread**, and this backend
  has nowhere safe to hand that to — its `drive_io` is a sleep. That fact is
  precisely what made the decline wrong; solving it with a listener would have
  reintroduced the same problem plus a buffer and a lock.

So `*_event_init` stays NULL here and the two poll slots carry the whole
surface. A caller polls them the way it already polls `has_data`.

Mapping:

| kind | entity | Cyclone getter |
| --- | --- | --- |
| `LIVELINESS_CHANGED` | reader | `dds_get_liveliness_changed_status` |
| `REQUESTED_DEADLINE_MISSED` | reader | `dds_get_requested_deadline_missed_status` |
| `MESSAGE_LOST` | reader | `dds_get_sample_lost_status` |
| `LIVELINESS_LOST` | writer | `dds_get_liveliness_lost_status` |
| `OFFERED_DEADLINE_MISSED` | writer | `dds_get_offered_deadline_missed_status` |

An entity asked for the other side's kind gets `INVALID_ARGUMENT`, not
`taken = false`: answering "no events" would let the caller's mistake run
forever looking like quiet.

**Saturating conversions, deliberately.** `rmw_liveliness_changed_status_t` is
16-bit where DDS is 32-bit, and `rmw_count_status_t::total_count_change` is
UNSIGNED where DDS's is signed. Counts saturate rather than wrap — a wrapped
count reads as a plausible small number, which is worse than a pegged one that
visibly means "at least this many".

## The test provokes a real event

`status_events.cpp` sets a 50 ms deadline on both ends, publishes ONCE and then
stops, and waits for `REQUESTED_DEADLINE_MISSED`. It then asserts the same
event is NOT taken twice — that the read consumed it — and that both
cross-entity kind errors are reported.

A test that only checked "the slot exists and answers `taken = false`" would
have passed against the NULL slot it replaces. 19/19 in the cyclonedds suite.

## Not changed

zenoh keeps `*_event_init` and leaves these NULL: it delivers through the
callback and has a context to do it from, so a poll would be redundant there.
That is the per-backend choice the slot exists to allow.


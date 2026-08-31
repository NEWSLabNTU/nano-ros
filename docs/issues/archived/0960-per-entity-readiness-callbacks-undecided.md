---
id: 960
title: "The per-entity readiness callback trio is declared, unfilled, and
  undecided — and `set_wake_callback` does not answer it"
status: resolved
type: tech-debt
area: rmw
related: [phase-406, phase-407, 0956, 0800, 0827]
---

## Problem

Three vtable slots are inert — declared, written and read by nothing:

```
subscription_set_on_new_message_callback
service_set_on_new_request_callback
client_set_on_new_response_callback
```

phase-406 first classified them `re-mapped` to `set_wake_callback`, on the
reading that the session-level wake is this ABI's answer to upstream's
per-entity callbacks. **That classification was wrong, and the header had
already said so**, in the doc comment on the third slot:

> added with the other two rather than left recorded as "covered by
> `set_wake_callback`" — *it never was*: that slot is session-scoped and serves
> subscriptions, services and clients identically.

The signatures show what is lost:

```c
/* upstream, and what our three slots declare */
void (*rmw_event_callback_t)(const void *user_data, size_t number_of_events);
/* set_wake_callback */
void (*cb)(void *ctx);
```

Two facts drop. **Which** entity became ready — the wake is per-SESSION, so it
says only "something on this session moved". And **how many** events are
pending — upstream's `number_of_events` is load-bearing for a consumer that
wants to drain exactly that many without re-polling.

A consumer holding only the session wake must poll every entity to find the
ready one, which is what `has_data` / `has_request` already do. So nothing was
re-mapped: the session wake and the per-entity callback answer different
questions, and only the first is implemented.

## Why this is `not-implemented` and not `not-supported`

Both are defensible, which is exactly the problem — nobody has chosen.

The case for **not-supported**: this ABI's premise is that no backend runs a
background transport thread and the executor drives everything. A per-entity
callback carrying a pending count implies the backend maintains per-entity
queue depth and reports it, which is a different division of labour. If that
is the decision, the three slots should GO, not sit declared — a slot nothing
will ever fill is the shape issue 0800 exists to prevent.

The case for **not-implemented**: `check-rmw-slot-producers.py` records the
trio as "reserved for parity". Reserved is not decided-against; it is a
placeholder for a decision deferred.

phase-406's `status` axis has a rule for exactly this: an inert slot must
declare `re-mapped` (naming what answers it), `not-supported` (a decision), or
`not-implemented` (a gap, with an issue). Undecided-but-declared is the state
the rule exists to make visible, and `not-implemented` is where it goes,
because that bucket is the one that has to shrink.

## What would resolve this

Pick one, and make the tree say it:

1. **Implement.** Cyclone can drive per-reader listeners; zenoh-pico's
   subscription callbacks fire per-entity already. XRCE is poll-only and stays
   NULL, which the nullity contract covers.
2. **Decide against, and delete the three slots.** Record the constraint —
   per-entity queue depth is backend bookkeeping this ABI declines — and let
   `set_wake_callback` + `has_data` be the stated answer, with the parity map
   saying `not-supported` rather than pointing at a slot.

What must not happen is a fourth phase in which they are still declared, still
unfilled, and still described as covered by a slot the header says does not
cover them.

## Note on `feature_supported`

Flagged alongside these and deliberately left `re-mapped`. Its inert reason is
an argued replacement, not a placeholder: "the capabilities the runtime
actually branches on are each their own slot, answered by nullity or a
dedicated probe, which is a narrower and checkable mechanism." That names what
answers the capability. The callback trio's reason never did.

## Resolution, 2026-08-31 — DECLINED, and the three slots are deleted

Option 2. Readiness belongs to the executor here, and the measurement says the
gap costs almost nothing:

```rust
// packages/core/nros-node/src/executor/spin.rs — spin_once
for (i, meta) in self.entries.iter().enumerate() {
    if unsafe { (meta.has_data)(data_ptr) } { … }
}
```

One vtable call per entity per wake, on images whose entity count is single
digits. So implementing the trio would save a scan nobody has measured as
costly.

What it would COST is concrete. Upstream's callback carries `number_of_events`,
so honouring the contract means every backend tracking per-entity pending
DEPTH — new per-entity state on a target where static RAM is already the
pressure (issue 0827 measures a talker reserving 275 KB of service and
large-block arena). New state, for a scan-skip nobody needs, for a consumer that
does not exist.

The parity argument is real but conditional: rclcpp's `EventsExecutor` is built
on these callbacks. If that compatibility is ever wanted it is a phase with a
stated goal, not three slots kept warm against the possibility.

Recorded so the next reader does not re-litigate: `set_wake_callback` does NOT
answer these. It is session-scoped and carries no count, so a consumer holding
only it must poll every entity anyway. The two mechanisms answer different
questions; we implement one and decline the other, and the map now says exactly
that instead of pointing at a slot the header itself said does not cover them.

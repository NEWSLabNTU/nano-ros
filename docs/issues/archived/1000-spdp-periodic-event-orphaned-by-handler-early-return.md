---
id: 1000
title: "NOT A DEFECT: the `handle_xevk_spdp` / `handle_xevk_acknack` early returns do not orphan their events — the owning entity deletes them"
status: wontfix
area: [rmw, third-party]
severity: low
related: [0997, 1004]
resolved_in: "closed 2026-09-04, not a defect"
---

# Filed on a real invariant, aimed at the wrong code

Opened claiming `handle_xevk_spdp`'s two early returns orphan the PERIODIC spdp
event — off the fibheap, unmarked, unfreed, unreferenced — and proposed
`delete_xevent (ev)` on those paths. Two paths in `handle_xevk_acknack` were
later added to the same charge.

**All four are correct as written, and the proposed fix would have introduced
double frees.**

## Why the events are not orphaned

Both event types are owned by the entity they belong to, and deleted during its
teardown:

```c
ddsi_participant.c:688     if (pp->spdp_xevent)    delete_xevent (pp->spdp_xevent);
ddsi_entity_match.c:634    if (m->acknack_xevent)  delete_xevent (m->acknack_xevent);
```

So when `handle_xevk_spdp` cannot find the participant, or `handle_xevk_acknack`
cannot find the proxy writer or the reader match, the owning entity is already
gone or going and its teardown has already called `delete_xevent`. Returning
without rescheduling is the only correct action; a `delete_xevent (ev)` there
would delete each event a second time.

0.10.5 documents the handshake at the point of deletion
(`ddsi_participant.c:693`):

> SPDP relies on the WHC, but dispose-unregister will empty it. The event
> handler verifies the event has already been scheduled for deletion when it
> runs into an empty WHC

which is the `assert (ev->tsched.v == TSCHED_DELETE)` inside `handle_xevk_spdp`
— the handler asserting that teardown got there first. Upstream 11.x agrees: its
`ddsi_spdp_directed_xevent_cb` calls `ddsi_delete_xevent` only for the DIRECTED
event, which is the one with no owning entity.

## Two retractions, in order

1. **The causal claim went first.** This was filed as the mechanism behind
   [0997](0997-island-announces-spdp-once-then-lease-expires.md). Counters in
   `handle_xevk_spdp`, read over the gdb stub, gave `unknown_guid = 0` and
   `no_writer = 0` across three handler invocations — the paths were never
   taken.
2. **The defect claim went second, and more completely.** Ownership means that
   even if they were taken, nothing leaks. The measurement retired "this causes
   0997"; ownership retires "this is a bug".

## What survives, and where it went

The general observation was worth the filing and is real: `handle_xevents`
extracts an event and stamps `tsched = DDS_NEVER` before calling its handler, so
re-arming is the handler's responsibility and nothing verifies it happened. It is
simply not violated by these four paths.

It is now addressed where it genuinely applies, by backporting upstream's
invariant to the tracking branch:

```c
assert (!need_to_eventually_nack (aanr) || xevent_is_scheduled (ev));
```

cyclonedds fork `nano-ros` @ `67ff751`, pinned by PR #326, with the
`xevent_is_scheduled()` helper 0.10.5 lacked. That covers the acknack path where
the event IS the handler's to re-arm — which is where this family's one genuine
defect lived: `AANR_SUPPRESSED_NACK` re-arming from a stale `t_last_nack`, fixed
in `7f064c1` (PR #306).

## The lesson

The reasoning that produced this issue — "the dispatcher takes the event off the
heap, so a handler returning without re-arming loses it" — is sound, and every
step was verified in the code. What was never checked is whether anything ELSE
owned the event. A control-flow argument about a resource says nothing about its
lifetime until ownership is established; one more file would have prevented the
filing.

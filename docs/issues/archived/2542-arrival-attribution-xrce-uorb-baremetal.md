---
id: 2542
title: "XRCE, uORB and bare-metal backends do not say which entity an arrival is for, so their executors scan and fan out wakes"
status: resolved
type: limitation
severity: low
area: [executor, rmw-xrce, uorb, baremetal]
related: [rfc-0084, rfc-0106, rfc-0113, phase-555]
found: 2026-11-14
resolved: 2026-11-19
resolved_in: "test(nros-tests, issue 2542): an XRCE arrival is dispatched with no readiness probe; RFC-0084 says which slots stay NULL"
---

## What

RFC-0084 pushes readiness from the arrival path: a backend that fills the
per-entity vtable slots (`{subscription,service,client}_set_on_new_*_callback`)
names the entity and the executor that owns it, so an arrival sets one bit
and wakes one executor. zenoh and Cyclone DDS fill them. XRCE, uORB and the
bare-metal backends leave them `NULL` (RFC-0084 section 8). For those:

- the executor probes each unattributed slot with `has_data` once per pass
  (`REPROBE_EACH_CALLBACK_DEFAULT = false`, `executor/spin.rs`; the cost
  grows with slots, and a real probe crosses the rmw C ABI);
- a session shared by several executors wakes all of them on any arrival
  (RFC-0106 D6's fan-out; measured on zenoh before phase-397 at 6.8-9.4 us
  of CPU per spurious wake on a native host).

RFC-0113 D4 gave XRCE a stated readiness (`Waitable` socket or `Notify`),
which ends a park, but not a per-entity attribution.

## Why it is filed now

Phase-555 W3 flipped RFC-0084 and RFC-0106 to Stable. Both name this as the
remaining case of their design, and neither had an issue for it.

## Acceptance

For each of XRCE and uORB, either the backend fills the per-entity slots and
a test shows an arrival waking only its owning executor with no `has_data`
probe for that slot; or the backend's reason it cannot attribute (for
example, the client library delivers per session, not per entity) is stated
in RFC-0084's backend row. Bare metal is poll-only by RFC-0106 D5 and may be
closed on the second branch.

## Resolution (2026-11-19)

The premise was out of date when filed. RFC-0084 section 8 was the source
of it, and is corrected.

- **XRCE: first branch.** XRCE has filled all three per-entity slots since
  issue 2109.
  - Each uxr arrival callback routes by the reader, replier or requester
    `object_id` to its slot, and fires the slot's callback from `drive_io`
    (`vtable.c`, `subscriber.c`, `service.c`).
  - `nros-rmw-xrce/tests/new_data_callbacks.c` proves it at the slot level,
    and `xrce_entity_wake::each_xrce_arrival_marks_its_own_subscription`
    proves it in the executor's ready word.
  - New here, `a_pushed_xrce_spin_dispatches_without_probing_readiness`
    (through a live Agent):
    - The spin that dispatches an arrival makes **0** `has_data` calls, with
      the callback count as the lower bound.
    - An idle spin also makes **0**.
    - With the three vtable slots reverted to NULL, the slot is not
      attributed, and each makes **1**, i.e. one per unattributed slot per
      pass (measured on aarch64 Linux, from the executor's own
      `readiness_probe_count`).
- **uORB: first branch for subscriptions, second for the rest.**
  - The subscription slot is filled (phase-521 W8): the PX4 callback glue
    registers one broker callback per subscription.
  - The service and client slots stay NULL because uORB has no services or
    clients; `service_create` and the client's create return `UNSUPPORTED`.
  - The non-PX4 build (`NROS_RMW_UORB_BUILD_PX4_GLUE=OFF`) has no broker
    callback, states readiness NONE, and is refused at open (RFC-0113 D5), so
    no executor polls it.
  - Not proven at executor level: that needs a PX4 build. The manager ruled
    the slot-level proof plus the stated reason an honest close.
- **Bare metal: second branch.** It is not a backend but zenoh or XRCE over
  smoltcp, and poll-only by RFC-0106 D5.

All of this is in RFC-0084 section 8's "backends touched" row, plus a
measured XRCE row.


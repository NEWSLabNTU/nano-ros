---
id: 2542
title: "XRCE, uORB and bare-metal backends do not say which entity an arrival is for, so their executors scan and fan out wakes"
status: open
type: limitation
severity: low
area: [executor, rmw-xrce, uorb, baremetal]
related: [rfc-0084, rfc-0106, rfc-0113, phase-555]
found: 2026-11-14
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

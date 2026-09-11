---
id: 1327
title: "The four service/client `*_get_actual_qos` slots are FILLED and read by
  nothing — cyclonedds answers a question no diagnostic asks"
status: resolved
type: bug
area: rmw
related: [phase-428, issue-0823, issue-0800, issue-1329]
---

## What is true

Issue 0823 said the runtime treats the QoS it REQUESTED as the QoS it got, and
closed the publisher and subscription halves: `read_entity_qos` in cyclonedds'
`qos.cpp` is the inverse of `make_dds_qos`, and `cffi/src/lib.rs`'s
`create_publisher` reads `publisher_get_actual_qos` and calls
`report_qos_downgrade` when the grant differs from the request.

Its "Not done here" listed the four client/service variants as left for later.
They have since been FILLED — cyclonedds' `vtable.cpp` wires all four:

```
client_request_publisher_get_actual_qos
client_response_subscription_get_actual_qos
service_request_subscription_get_actual_qos
service_response_publisher_get_actual_qos
```

**And nothing reads any of them.** Measured 2026-09-11 by
`check-rmw-slot-producers` once phase-428 W8 made consumption the first
question: the four are `inert` — a backend body with no dispatch site. Under
the old ordering they read `produced`, which is the shape issue 0800 exists to
refuse, displaced one step: not a declared slot nobody fills, but a body
nobody calls.

## Why it matters

The bug 0823 describes is still live on the service side, and it is now live
with the fix sitting unreached in the tree. A client requesting RELIABLE
against a BEST_EFFORT service gets the same silence as a name typo, and every
diagnostic that prints a client's or service's QoS prints the request.

The cost of the current state is worse than the gap it replaced: four
implementations exist, are compiled, and are maintained, while answering
nobody.

## Direction

The consumer is the client/service half of `report_qos_downgrade` — the
publisher side is the model, three lines in `create_publisher`. It belongs at
`create_client` / `create_service` in `packages/rmw/cffi/src/lib.rs`, which is
where **phase-428 W9** is separately deciding what those two do with a QoS
profile at all (today: `let _ = qos;`). Doing it before W9 lands would write
the read-back against a path W9 is changing, so this is deliberately sequenced
behind it and not merely unscheduled.

Acceptance, mirroring 0823's: the test pre-loads the out-struct with values
the entity cannot have, so an echoing implementation fails it.

## Tracked by

`check-rmw-slot-producers.py`'s `granted-qos-service-side` inert family, whose
`defer = 1327` is checked against this file's `status:` on every run — so this
issue closing or being archived while the slots are still unread is itself a
gate failure.

## Resolved 2026-09-12

The consumer landed where the issue said it belonged: `create_service` and
`create_client` in `packages/rmw/cffi/src/lib.rs`, each reading BOTH directions
— the request endpoint and the response endpoint negotiate against different
peers and may be granted different things, and `create_*` takes one profile for
the two of them.

**Measured, which is the acceptance: `inert 11 -> 7`.** All four move from
`inert` to `produced` under `check-rmw-slot-producers`, and the
`granted-qos-service-side` family is DELETED rather than narrowed — a family
naming a slot that is no longer inert is the stale claim that table exists to
refuse.

### One helper, not four more copies

`create_publisher` carried the three steps inline (pre-load the out-struct with
the request, call the slot, compare), and this issue needed four more of them.
Six hand-copies of one sequence is how the sizes-header mirror got to six
sites, so all six now go through `report_granted_qos` — generic over the entity
type, since the slots differ only in which handle they take. It returns the
GRANTED profile rather than only logging, which is what lets a test see what
the backend answered.

### The test refuses to pass vacuously

Mirroring 0823's `actual_qos.cpp`: the scripted backend writes values the
request cannot have, so an implementation that echoes its input and one that
zeroes it both fail.

* `service_and_client_create_read_the_granted_qos_back` asserts all four slots
  are dispatched **exactly once** — `[1, 1, 1, 1]`, which is `[0, 0, 0, 0]`
  against the old code — and that each was handed the REQUEST rather than a
  zeroed struct. That second assertion is the slot's own contract: a field the
  backend cannot report is left as it arrived, so a zeroed out-struct would
  turn "unreported" into a confident grant of 0.
* `the_read_back_helper_returns_the_grant_not_the_request` asserts the helper
  returns the backend's answer, not the caller's request.

Negative control, run on the live tree and reverted: rewriting the four reads
away takes `check-rmw-slot-producers --check` to exit 1 naming all four as
"inert slot in no declared family", and cfg-ing the two consumer blocks out
fails `service_and_client_create_read_the_granted_qos_back`.

### Sequencing note, since the issue made a point of it

This was deliberately behind phase-428 W9, which was changing what
`create_service` / `create_client` do with a QoS profile at all. W9 landed; the
same PR also lands issue 1329, which is the other half of that seam — so the
read-back and the per-backend mask arrived together rather than one being
written against a path the other was moving.

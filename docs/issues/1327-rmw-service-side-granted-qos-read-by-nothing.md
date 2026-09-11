---
id: 1327
title: "The four service/client `*_get_actual_qos` slots are FILLED and read by
  nothing — cyclonedds answers a question no diagnostic asks"
status: open
type: bug
area: rmw
related: [phase-428, issue-0823, issue-0800]
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

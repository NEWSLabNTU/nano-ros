---
id: 2120
title: "An XRCE service or client admits a requested QoS it never applies: the client library drops it and the Agent uses a fixed profile"
status: open
type: bug
area: [rmw, xrce, qos]
severity: low
found: 2026-10-27
related: [2107, 1329]
---

## What

`create_service` / `create_client` on XRCE take a QoS profile, check it
against `xrce_supported_qos_policies` (reliability, durability, history, depth,
avoid-ROS), admit it, and pass it to `uxr_buffer_create_{replier,requester}_bin`.
That function discards it: micro-XRCE-DDS-Client 3.0.1's
`create_entities_bin.c` reads `(void) qos;` and serializes no QoS into
`OBJK_{Requester,Replier}_Binary`. The Agent (2.4.3) then builds both endpoints
from fixed attributes (`FastDDSMiddleware::create_{requester,replier}_by_bin`):
the writer from a default `PublisherAttributes`, the reader from a default
`SubscriberAttributes` with reliability forced RELIABLE.

Measured by issue 2107 with a stock `rmw_fastrtps_cpp` peer reading each
endpoint's discovered QoS (`get_{publishers,subscriptions}_info_by_topic`,
`no_mangle`), for a server and a client created with the services-default
profile (RELIABLE, VOLATILE):

| endpoint | reliability | durability |
| --- | --- | --- |
| request writer (client) | RELIABLE | **TRANSIENT_LOCAL** |
| reply reader (client) | RELIABLE | VOLATILE |
| request reader (server) | RELIABLE | VOLATILE |
| reply writer (server) | RELIABLE | **TRANSIENT_LOCAL** |

So a caller that asks for BEST_EFFORT, or for TRANSIENT_LOCAL on the readers, or
for a depth, is told OK and gets the table above. That is issue 1329's
"admitted, then ignored all the way to the wire", for services only — pub/sub
does carry its four fields.

Since issue 2107 the four service/client `*_get_actual_qos` slots report the
table, so the difference is visible to a caller who asks. Nothing refuses it.

## What would close it

Either refuse at service/client create any honoured policy that differs from
the fixed profile (the slot can still admit the services default), or make the
QoS reach the Agent (the client library would have to serialize it and the
Agent to apply it — both upstream, and the fork rule applies to the client
submodule). The refusal is local and cheap; the mask is per session, so it needs
a per-entity-kind check beside `xrce_supported_qos_policies`.
